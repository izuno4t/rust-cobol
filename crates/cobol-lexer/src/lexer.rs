use cobol_common::{FileId, SourceFormat, Span};
use smol_str::SmolStr;

use crate::source_reader::SourceReader;
use crate::token::{Token, TokenKind};

/// The COBOL lexer. Consumes source text and produces a stream of `Token`s.
///
/// Internally uses `SourceReader` to handle fixed/free format source layout,
/// then tokenizes the content area of each non-comment, non-blank line.
pub struct Lexer {
    #[allow(dead_code)]
    source: String,
    file_id: FileId,
    #[allow(dead_code)]
    format: SourceFormat,
    /// Flattened content built from all non-comment, non-blank lines.
    content: String,
    /// Mapping from byte positions in `content` to global source offsets.
    offset_map: Vec<u32>,
    /// Current byte position in `content`.
    pos: usize,
    /// Accumulated tokens.
    tokens: Vec<Token>,
    /// Whether we just emitted PIC/PICTURE and need to lex a picture string.
    picture_mode: bool,
    /// Whether we are at the start of a statement (for level number detection).
    at_statement_start: bool,
}

/// A segment of content extracted from one source line.
struct ContentSegment {
    text: String,
    global_offset: u32,
}

impl Lexer {
    /// Creates a new lexer for the given source text, file ID, and format.
    pub fn new(source: &str, file_id: FileId, format: SourceFormat) -> Self {
        let reader = SourceReader::new(source, format);

        // Build flattened content and offset map from non-comment, non-blank lines.
        let segments: Vec<ContentSegment> = reader
            .lines()
            .iter()
            .filter(|line| !line.is_comment() && !line.is_blank())
            .map(|line| {
                let text = line.content_text().to_string();
                let global_offset = line.global_offset + line.content_start as u32;
                ContentSegment {
                    text,
                    global_offset,
                }
            })
            .collect();

        let mut content = String::new();
        let mut offset_map: Vec<u32> = Vec::new();

        for (i, seg) in segments.iter().enumerate() {
            if i > 0 {
                // Insert a space between lines to separate tokens
                content.push(' ');
                // The inter-line space maps to the end of the previous segment
                let prev = &segments[i - 1];
                offset_map.push(prev.global_offset + prev.text.len() as u32);
            }
            let base = content.len();
            content.push_str(&seg.text);
            // Build offset map for each byte in this segment
            for j in 0..seg.text.len() {
                debug_assert_eq!(offset_map.len(), base + j);
                offset_map.push(seg.global_offset + j as u32);
            }
        }

        Self {
            source: source.to_string(),
            file_id,
            format,
            content,
            offset_map,
            pos: 0,
            tokens: Vec::new(),
            picture_mode: false,
            at_statement_start: true,
        }
    }

    /// Lexes the entire source and returns all tokens (including a trailing `Eof`).
    pub fn lex_all(&mut self) -> Vec<Token> {
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            self.tokens.push(token);
            if is_eof {
                break;
            }
        }
        self.tokens.clone()
    }

    /// Produces the next token from the source.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        if self.pos >= self.content.len() {
            return self.make_eof();
        }

        // If we're in picture mode, lex a picture string
        if self.picture_mode {
            self.picture_mode = false;
            // Skip optional IS keyword
            if self.remaining_upper().starts_with("IS") {
                let after_is = &self.content[self.pos + 2..];
                if after_is.starts_with(' ') || after_is.starts_with('\t') {
                    self.pos += 2;
                    self.skip_whitespace();
                }
            }
            return self.lex_picture_string();
        }

        let ch = self.current_byte();

        // String literals with prefix: X"...", B"...", N"..."
        if (ch == b'X' || ch == b'x' || ch == b'B' || ch == b'b' || ch == b'N' || ch == b'n')
            && self.pos + 1 < self.content.len()
        {
            let next = self.content.as_bytes()[self.pos + 1];
            if next == b'"' || next == b'\'' {
                return self.lex_prefixed_literal(ch);
            }
        }

        // String literals
        if ch == b'"' || ch == b'\'' {
            return self.lex_string_literal();
        }

        // Level numbers at statement start: 01-49, 66, 77, 88
        if ch.is_ascii_digit() && self.at_statement_start {
            if let Some(tok) = self.try_lex_level_number() {
                return tok;
            }
        }

        // Numbers and signed numbers
        if ch.is_ascii_digit() {
            return self.lex_number();
        }

        // Sign followed by digit: could be signed number literal
        if (ch == b'+' || ch == b'-') && self.pos + 1 < self.content.len() {
            let next = self.content.as_bytes()[self.pos + 1];
            if next.is_ascii_digit() && self.should_treat_as_sign() {
                return self.lex_number();
            }
        }

        // Operators and punctuation
        if let Some(tok) = self.try_lex_operator() {
            return tok;
        }

        // Keywords and identifiers
        if ch.is_ascii_alphabetic() {
            return self.lex_word();
        }

        // Unknown character - emit error token
        let start = self.pos;
        self.pos += 1;
        self.make_token(TokenKind::Error, start, self.pos)
    }

    // ── Helper methods ──────────────────────────────────────────────

    /// Returns the current byte at `self.pos`.
    fn current_byte(&self) -> u8 {
        self.content.as_bytes()[self.pos]
    }

    /// Returns the remaining content from `self.pos`, uppercased.
    fn remaining_upper(&self) -> String {
        self.content[self.pos..].to_uppercase()
    }

    /// Skips whitespace characters (spaces, tabs).
    fn skip_whitespace(&mut self) {
        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch == b' ' || ch == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Creates an EOF token.
    fn make_eof(&self) -> Token {
        let offset = if self.offset_map.is_empty() {
            0
        } else if self.pos < self.offset_map.len() {
            self.offset_map[self.pos]
        } else {
            *self.offset_map.last().unwrap() + 1
        };
        Token {
            kind: TokenKind::Eof,
            text: SmolStr::default(),
            span: Span::new(offset, offset, self.file_id),
        }
    }

    /// Creates a token from content byte range [start..end).
    fn make_token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        let text: SmolStr = self.content[start..end].into();
        let global_start = self.offset_map[start];
        let global_end = if end > 0 && end <= self.offset_map.len() {
            if end < self.offset_map.len() {
                self.offset_map[end]
            } else {
                *self.offset_map.last().unwrap() + 1
            }
        } else {
            global_start
        };
        Token {
            kind,
            text,
            span: Span::new(global_start, global_end, self.file_id),
        }
    }

    /// Determines whether a `+` or `-` should be treated as a sign prefix
    /// (part of a numeric literal) rather than an operator.
    ///
    /// Heuristic: treat as sign only when at the very start of input or
    /// when the preceding non-whitespace token position was an operator or
    /// opening paren.
    fn should_treat_as_sign(&self) -> bool {
        // Look backward in content for the preceding non-whitespace char
        let mut i = self.pos;
        while i > 0 {
            i -= 1;
            let ch = self.content.as_bytes()[i];
            if ch == b' ' || ch == b'\t' {
                continue;
            }
            // If preceded by an operator or open paren, it's a sign
            return matches!(ch, b'=' | b'+' | b'-' | b'*' | b'/' | b'(' | b',' | b';');
        }
        // At start of content - treat as sign
        true
    }

    /// Lexes a word (keyword or identifier).
    ///
    /// COBOL words start with a letter and can contain letters, digits,
    /// and hyphens. They cannot end with a hyphen.
    fn lex_word(&mut self) -> Token {
        let start = self.pos;
        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch.is_ascii_alphanumeric() || ch == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }

        // COBOL identifiers cannot end with a hyphen; trim trailing hyphens
        while self.pos > start && self.content.as_bytes()[self.pos - 1] == b'-' {
            self.pos -= 1;
        }

        let word = &self.content[start..self.pos];

        // Check if it's a keyword
        if let Some(kind) = TokenKind::from_keyword(word) {
            self.at_statement_start = false;
            // Enter picture mode after PIC/PICTURE
            if kind == TokenKind::Pic {
                self.picture_mode = true;
            }
            return self.make_token(kind, start, self.pos);
        }

        self.at_statement_start = false;
        self.make_token(TokenKind::Identifier, start, self.pos)
    }

    /// Tries to lex a level number at the current position.
    ///
    /// Returns `Some(Token)` if the current position contains a valid level
    /// number (01-49, 66, 77, 88) followed by whitespace or end of content.
    /// Returns `None` otherwise, allowing fallback to normal number lexing.
    fn try_lex_level_number(&mut self) -> Option<Token> {
        let start = self.pos;

        // Peek ahead to collect 1-2 digits
        let mut end = self.pos;
        while end < self.content.len()
            && self.content.as_bytes()[end].is_ascii_digit()
            && (end - start) < 3
        {
            end += 1;
        }

        let word = &self.content[start..end];
        if word.len() > 2 || word.is_empty() {
            return None;
        }

        // Must be followed by whitespace, end of content, or period
        if end < self.content.len() {
            let next_ch = self.content.as_bytes()[end];
            if next_ch != b' ' && next_ch != b'\t' && next_ch != b'.' {
                return None;
            }
        }

        match word.parse::<u32>() {
            Ok(n) if (1..=49).contains(&n) || n == 66 || n == 77 || n == 88 => {
                self.pos = end;
                self.at_statement_start = false;
                Some(self.make_token(TokenKind::LevelNumber, start, self.pos))
            }
            _ => None,
        }
    }

    /// Lexes a numeric literal (integer or decimal).
    fn lex_number(&mut self) -> Token {
        let start = self.pos;

        // Optional sign
        if self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch == b'+' || ch == b'-' {
                self.pos += 1;
            }
        }

        // Integer part
        while self.pos < self.content.len() && self.content.as_bytes()[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Check for decimal point: period followed by a digit
        if self.pos < self.content.len()
            && self.content.as_bytes()[self.pos] == b'.'
            && self.pos + 1 < self.content.len()
            && self.content.as_bytes()[self.pos + 1].is_ascii_digit()
        {
            self.pos += 1; // consume the '.'
            while self.pos < self.content.len()
                && self.content.as_bytes()[self.pos].is_ascii_digit()
            {
                self.pos += 1;
            }
            return self.make_token(TokenKind::DecimalLiteral, start, self.pos);
        }

        self.make_token(TokenKind::IntegerLiteral, start, self.pos)
    }

    /// Lexes a string literal enclosed in single or double quotes.
    fn lex_string_literal(&mut self) -> Token {
        let start = self.pos;
        let quote = self.content.as_bytes()[self.pos];
        self.pos += 1; // consume opening quote

        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch == quote {
                self.pos += 1;
                // Check for doubled quote (escape): "" or ''
                if self.pos < self.content.len() && self.content.as_bytes()[self.pos] == quote {
                    self.pos += 1; // consume the second quote
                    continue;
                }
                // End of string
                return self.make_token(TokenKind::StringLiteral, start, self.pos);
            }
            self.pos += 1;
        }

        // Unterminated string - emit as error
        self.make_token(TokenKind::Error, start, self.pos)
    }

    /// Lexes a prefixed literal: X"...", B"...", N"..."
    fn lex_prefixed_literal(&mut self, prefix: u8) -> Token {
        let start = self.pos;
        let upper_prefix = prefix.to_ascii_uppercase();
        self.pos += 1; // skip prefix letter

        let quote = self.content.as_bytes()[self.pos];
        self.pos += 1; // skip opening quote

        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch == quote {
                self.pos += 1;
                // Check for doubled quote
                if self.pos < self.content.len() && self.content.as_bytes()[self.pos] == quote {
                    self.pos += 1;
                    continue;
                }
                let kind = match upper_prefix {
                    b'X' => TokenKind::HexLiteral,
                    b'B' => TokenKind::BooleanLiteral,
                    b'N' => TokenKind::NationalLiteral,
                    _ => TokenKind::Error,
                };
                return self.make_token(kind, start, self.pos);
            }
            self.pos += 1;
        }

        // Unterminated
        self.make_token(TokenKind::Error, start, self.pos)
    }

    /// Lexes a PICTURE string. Called after PIC/PICTURE keyword.
    ///
    /// A picture string consists of characters like S, 9, X, A, V, P, Z,
    /// along with parenthesized repeat counts and special editing chars.
    fn lex_picture_string(&mut self) -> Token {
        self.skip_whitespace();

        if self.pos >= self.content.len() {
            return self.make_eof();
        }

        let start = self.pos;

        // Picture string continues until we hit whitespace, period followed
        // by space/end, or end of content
        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];

            if ch == b' ' || ch == b'\t' {
                break;
            }

            // Period handling: a period followed by space or end-of-content
            // is a sentence terminator, not part of the picture string.
            // But a period within the picture (like in Z,ZZZ.99) should be
            // included.
            if ch == b'.' {
                let next_pos = self.pos + 1;
                if next_pos >= self.content.len()
                    || self.content.as_bytes()[next_pos] == b' '
                    || self.content.as_bytes()[next_pos] == b'\t'
                {
                    break;
                }
            }

            self.pos += 1;
        }

        if self.pos == start {
            // No picture string found
            return self.make_token(TokenKind::Error, start, self.pos);
        }

        self.make_token(TokenKind::PictureString, start, self.pos)
    }

    /// Tries to lex an operator or punctuation character.
    fn try_lex_operator(&mut self) -> Option<Token> {
        let start = self.pos;
        let ch = self.content.as_bytes()[self.pos];
        let next = if self.pos + 1 < self.content.len() {
            Some(self.content.as_bytes()[self.pos + 1])
        } else {
            None
        };

        match ch {
            b'(' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::LeftParen, start, self.pos))
            }
            b')' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::RightParen, start, self.pos))
            }
            b'+' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::Plus, start, self.pos))
            }
            b'-' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::Minus, start, self.pos))
            }
            b'*' => {
                if next == Some(b'*') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::DoubleStar, start, self.pos))
                } else {
                    self.pos += 1;
                    Some(self.make_token(TokenKind::Star, start, self.pos))
                }
            }
            b'/' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::Slash, start, self.pos))
            }
            b'=' => {
                if next == Some(b'>') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::EqualGreater, start, self.pos))
                } else {
                    self.pos += 1;
                    Some(self.make_token(TokenKind::Equals, start, self.pos))
                }
            }
            b'>' => {
                if next == Some(b'=') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::GreaterEqual, start, self.pos))
                } else {
                    self.pos += 1;
                    Some(self.make_token(TokenKind::GreaterThan, start, self.pos))
                }
            }
            b'<' => {
                if next == Some(b'=') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::LessEqual, start, self.pos))
                } else if next == Some(b'>') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::NotEqual, start, self.pos))
                } else {
                    self.pos += 1;
                    Some(self.make_token(TokenKind::LessThan, start, self.pos))
                }
            }
            b'.' => {
                // Period: decimal point or sentence terminator.
                // If followed by a digit, it's a decimal point (handled in
                // lex_number). As an operator, it's always a sentence terminator.
                self.pos += 1;
                self.at_statement_start = true;
                Some(self.make_token(TokenKind::Period, start, self.pos))
            }
            b',' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::Comma, start, self.pos))
            }
            b';' => {
                self.pos += 1;
                Some(self.make_token(TokenKind::Semicolon, start, self.pos))
            }
            b':' => {
                if next == Some(b':') {
                    self.pos += 2;
                    Some(self.make_token(TokenKind::DoubleColon, start, self.pos))
                } else {
                    self.pos += 1;
                    Some(self.make_token(TokenKind::Colon, start, self.pos))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
        lexer
            .lex_all()
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .collect()
    }

    fn lex_free(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        lexer
            .lex_all()
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .collect()
    }

    #[test]
    fn test_lex_identification_division() {
        let src = "       IDENTIFICATION DIVISION.\
                                                            ";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::Identification);
        assert_eq!(tokens[1].kind, TokenKind::Division);
        assert_eq!(tokens[2].kind, TokenKind::Period);
    }

    #[test]
    fn test_lex_program_id() {
        let src = "       PROGRAM-ID. HELLO-WORLD.\
                                                            ";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::ProgramId);
        assert_eq!(tokens[1].kind, TokenKind::Period);
        assert_eq!(tokens[2].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].text.as_str(), "HELLO-WORLD");
        assert_eq!(tokens[3].kind, TokenKind::Period);
    }

    #[test]
    fn test_lex_level_number() {
        let src = "       01  WS-NAME PIC X(10).\
                                                              ";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::LevelNumber);
        assert_eq!(tokens[0].text.as_str(), "01");
    }

    #[test]
    fn test_lex_string_literal() {
        let src = "       MOVE \"HELLO\" TO WS-NAME.\
                                                            ";
        let tokens = lex(src);
        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .unwrap();
        assert!(str_tok.text.contains("HELLO"));
    }

    #[test]
    fn test_lex_integer_literal() {
        let src = "       MOVE 42 TO WS-COUNT.\
                                                              ";
        let tokens = lex(src);
        let num_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::IntegerLiteral)
            .unwrap();
        assert_eq!(num_tok.text.as_str(), "42");
    }

    #[test]
    fn test_lex_decimal_literal() {
        let src = "       COMPUTE WS-AMT = 3.14.\
                                                              ";
        let tokens = lex(src);
        let dec_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::DecimalLiteral)
            .unwrap();
        assert_eq!(dec_tok.text.as_str(), "3.14");
    }

    #[test]
    fn test_lex_operators() {
        let src = "       COMPUTE X = A + B * C / D - E ** 2.\
                                             ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Plus));
        assert!(kinds.contains(&TokenKind::Star));
        assert!(kinds.contains(&TokenKind::Slash));
        assert!(kinds.contains(&TokenKind::Minus));
        assert!(kinds.contains(&TokenKind::DoubleStar));
    }

    #[test]
    fn test_lex_picture_clause() {
        let src = "       01  WS-AMT PIC S9(7)V99.\
                                                            ";
        let tokens = lex(src);
        let pic_idx = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Pic)
            .unwrap();
        assert_eq!(tokens[pic_idx + 1].kind, TokenKind::PictureString);
        assert_eq!(tokens[pic_idx + 1].text.as_str(), "S9(7)V99");
    }

    #[test]
    fn test_lex_comparison_operators() {
        let src = "       IF A >= B AND C <= D.\
                                                              ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::GreaterEqual));
        assert!(kinds.contains(&TokenKind::LessEqual));
    }

    #[test]
    fn test_lex_parentheses() {
        let src = "       COMPUTE X = (A + B) * C.\
                                                             ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::LeftParen));
        assert!(kinds.contains(&TokenKind::RightParen));
    }

    #[test]
    fn test_lex_free_format() {
        let src = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TEST.\n";
        let tokens = lex_free(src);
        assert_eq!(tokens[0].kind, TokenKind::Identification);
        assert_eq!(tokens[1].kind, TokenKind::Division);
        assert_eq!(tokens[2].kind, TokenKind::Period);
    }

    #[test]
    fn test_lex_comment_skipped() {
        let src = "000100*THIS IS A COMMENT                                                  \n\
             000200 DISPLAY \"HI\".\
                                                                ";
        let tokens = lex(src);
        // Comment line should be skipped, first token should be DISPLAY
        assert_eq!(tokens[0].kind, TokenKind::Display);
    }

    #[test]
    fn test_lex_hex_literal() {
        let src = "       MOVE X\"0F\" TO WS-HEX.\
                                                               ";
        let tokens = lex(src);
        let hex_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::HexLiteral)
            .unwrap();
        assert!(hex_tok.text.contains("0F"));
    }

    #[test]
    fn test_lex_period_as_terminator() {
        let src = "       STOP RUN.\
                                                                        ";
        let tokens = lex(src);
        let last_meaningful = tokens.last().unwrap();
        assert_eq!(last_meaningful.kind, TokenKind::Period);
    }

    #[test]
    fn test_lex_boolean_literal() {
        let src = "       MOVE B\"0101\" TO WS-BOOL.\
                                                            ";
        let tokens = lex(src);
        let bool_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::BooleanLiteral)
            .unwrap();
        assert!(bool_tok.text.contains("0101"));
    }

    #[test]
    fn test_lex_national_literal() {
        let src = "       MOVE N\"TEXT\" TO WS-NAT.\
                                                             ";
        let tokens = lex(src);
        let nat_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::NationalLiteral)
            .unwrap();
        assert!(nat_tok.text.contains("TEXT"));
    }

    #[test]
    fn test_lex_not_equal() {
        let src = "       IF A <> B.\
                                                                       ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::NotEqual));
    }

    #[test]
    fn test_lex_double_colon() {
        let src = "       MOVE A::B TO C.\
                                                                   ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::DoubleColon));
    }

    #[test]
    fn test_lex_equal_greater() {
        let src = "       EVALUATE TRUE WHEN A => B.\
                                                          ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::EqualGreater));
    }

    #[test]
    fn test_lex_single_quoted_string() {
        let src = "       MOVE 'HELLO' TO WS-NAME.\
                                                            ";
        let tokens = lex(src);
        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .unwrap();
        assert!(str_tok.text.contains("HELLO"));
    }

    #[test]
    fn test_lex_level_numbers_various() {
        // Test that 01, 05, 10, 49, 66, 77, 88 are recognized
        for level in ["01", "05", "10", "49", "66", "77", "88"] {
            let src = format!(
                "       {}  FILLER PIC X.{}",
                level,
                " ".repeat(60 - level.len())
            );
            let tokens = lex(&src);
            assert_eq!(
                tokens[0].kind,
                TokenKind::LevelNumber,
                "Level {} should be recognized",
                level
            );
        }
    }

    #[test]
    fn test_lex_picture_with_is() {
        // PIC IS X(10) should also work
        let src = "       01  WS-NAME PIC IS X(10).\
                                                           ";
        let tokens = lex(src);
        let pic_idx = tokens
            .iter()
            .position(|t| t.kind == TokenKind::Pic)
            .unwrap();
        assert_eq!(tokens[pic_idx + 1].kind, TokenKind::PictureString);
        assert_eq!(tokens[pic_idx + 1].text.as_str(), "X(10)");
    }

    #[test]
    fn test_lex_empty_source() {
        let tokens = lex("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_lex_multiple_lines_fixed() {
        let src = "000100 IDENTIFICATION DIVISION.                                          \n\
             000200 PROGRAM-ID. HELLO.                                                \n";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::Identification);
        assert_eq!(tokens[1].kind, TokenKind::Division);
        assert_eq!(tokens[2].kind, TokenKind::Period);
        assert_eq!(tokens[3].kind, TokenKind::ProgramId);
    }

    #[test]
    fn test_statement_start_reset_after_period() {
        // After a period, the next number should be treated as a level number
        let src = "       STOP RUN.                                                          \n\
             000200 01  WS-A PIC X.                                                    \n";
        let tokens = lex(src);
        // Find the level number after STOP RUN.
        let level_tok = tokens.iter().find(|t| t.kind == TokenKind::LevelNumber);
        assert!(level_tok.is_some());
    }
}
