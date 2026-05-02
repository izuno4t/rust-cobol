use cobol_common::{FileId, SourceFormat, Span};
use smol_str::SmolStr;

use crate::source_reader::SourceReader;
use crate::token::{Token, TokenKind};

/// The COBOL lexer. Consumes source text and produces a stream of `Token`s.
///
/// Internally uses `SourceReader` to handle fixed/free format source layout,
/// then tokenizes the content area of each non-comment, non-blank line.
pub struct Lexer {
    file_id: FileId,
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

/// A segment of content extracted from one or more source lines
/// (when continuation lines are merged).
struct ContentSegment {
    text: String,
    /// Global byte offset of the start of the original (non-continuation) line's content.
    global_offset: u32,
    /// Additional offset mappings from continuation lines that were merged into this
    /// segment. Each entry is `(byte_offset_in_text, global_offset)` marking where
    /// a continuation line's content begins within `text`.
    cont_offsets: Vec<(usize, u32)>,
}

impl Lexer {
    fn ends_inside_string(text: &str, quote: u8) -> bool {
        let bytes = text.as_bytes();
        let mut pos = 0;
        let mut in_string = false;

        while pos < bytes.len() {
            if bytes[pos] != quote {
                pos += 1;
                continue;
            }

            if in_string {
                if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                    pos += 2;
                } else {
                    in_string = false;
                    pos += 1;
                }
            } else {
                in_string = true;
                pos += 1;
            }
        }

        in_string
    }

    /// Creates a new lexer for the given source text, file ID, and format.
    pub fn new(source: &str, file_id: FileId, format: SourceFormat) -> Self {
        let reader = SourceReader::new(source, format);

        // Build flattened content and offset map from non-comment, non-blank lines.
        // Continuation lines (indicator '-') are merged with the preceding line.
        let segments = Self::build_segments(reader.lines());

        let mut content = String::new();
        let mut offset_map: Vec<u32> = Vec::new();

        for (i, seg) in segments.iter().enumerate() {
            if i > 0 {
                // Preserve logical line boundaries so unterminated literals
                // cannot swallow following divisions or statements.
                content.push('\n');
                // The inter-line separator maps to the end of the previous segment's
                // original content (before any continuation was merged).
                let prev = &segments[i - 1];
                let prev_end = if let Some(&(start_in_text, base_global)) = prev.cont_offsets.last()
                {
                    // The previous segment had continuations; compute end from
                    // the last continuation region.
                    let cont_len = prev.text.len() - start_in_text;
                    base_global + cont_len as u32
                } else {
                    prev.global_offset + prev.text.len() as u32
                };
                offset_map.push(prev_end);
            }
            let base = content.len();
            content.push_str(&seg.text);

            // Build offset map for each byte in this segment, accounting for
            // continuation offsets that shift the global mapping partway through.
            let mut cont_idx = 0;
            let mut current_global_base = seg.global_offset;
            let mut current_text_start: usize = 0;

            for j in 0..seg.text.len() {
                // Check if we've entered a continuation region
                if cont_idx < seg.cont_offsets.len() && j >= seg.cont_offsets[cont_idx].0 {
                    current_text_start = seg.cont_offsets[cont_idx].0;
                    current_global_base = seg.cont_offsets[cont_idx].1;
                    cont_idx += 1;
                }
                debug_assert_eq!(offset_map.len(), base + j);
                offset_map.push(current_global_base + (j - current_text_start) as u32);
            }
        }

        Self {
            file_id,
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
        std::mem::take(&mut self.tokens)
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
            self.skip_whitespace();

            // Skip optional IS keyword
            if self.remaining_starts_with_ignore_case("IS") {
                let after_is_pos = self.pos + 2;
                let has_separator = self
                    .content
                    .as_bytes()
                    .get(after_is_pos)
                    .is_none_or(|b| b.is_ascii_whitespace());
                if has_separator {
                    self.pos = after_is_pos;
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

    // ── Continuation line merging ─────────────────────────────────

    /// Builds content segments from source lines, merging continuation lines
    /// with their preceding lines.
    ///
    /// In COBOL fixed format, a '-' in column 7 indicates that the line
    /// continues the previous line. For string literal continuations, the
    /// trailing quote of the previous line and the leading quote of the
    /// continuation line are removed so that the string content is seamlessly
    /// joined. For non-string continuations, the continuation line's content
    /// is appended after trimming trailing whitespace from the previous line.
    fn build_segments(lines: &[crate::source_reader::SourceLine]) -> Vec<ContentSegment> {
        let filtered: Vec<_> = lines
            .iter()
            .filter(|line| !line.is_comment() && !line.is_blank())
            .collect();

        let mut segments: Vec<ContentSegment> = Vec::new();

        for line in filtered {
            if line.is_continuation() {
                if let Some(prev) = segments.last_mut() {
                    let cont_text = line.content_text();
                    let cont_global = line.global_offset + line.content_start as u32;

                    // Determine if this is a string literal continuation by
                    // checking whether the continuation line's first non-space
                    // character is a quote.
                    let cont_bytes = cont_text.as_bytes();
                    let mut skip = 0;
                    while skip < cont_bytes.len() && cont_bytes[skip] == b' ' {
                        skip += 1;
                    }

                    let first_non_space = cont_bytes.get(skip).copied();
                    let is_string_continuation = matches!(first_non_space, Some(b'"' | b'\''));

                    if is_string_continuation {
                        // Skip the opening quote on the continuation line
                        let quote = first_non_space.unwrap();
                        skip += 1;

                        // Drop the previous line's trailing quote when present.
                        // If the previous line is an open literal without that
                        // marker, its trailing spaces are literal content.
                        let prev_trimmed_len = prev.text.trim_end().len();
                        // Drop exactly the trailing continuation quote marker.
                        // This preserves any escaped quotes that are part of
                        // the string content while still removing the single
                        // quote used to continue the literal onto the next
                        // physical line.
                        let mut dropped_prev_quote = false;
                        let trailing_spaces = prev.text.len().saturating_sub(prev_trimmed_len);
                        if prev_trimmed_len > 0
                            && prev.text.as_bytes().get(prev_trimmed_len - 1) == Some(&quote)
                            && Self::ends_inside_string(&prev.text[..prev_trimmed_len - 1], quote)
                        {
                            prev.text.truncate(prev_trimmed_len);
                            prev.text.pop();
                            dropped_prev_quote = true;
                        }
                        if !dropped_prev_quote {
                            prev.text.truncate(prev_trimmed_len);
                            if trailing_spaces == 1 {
                                prev.text.push(' ');
                            }
                        }

                        // When the previous physical line ended with a
                        // continuation marker quote, a second quote at the
                        // start of the continued text is part of the logical
                        // literal and must be preserved.
                        let continued_quote_run = cont_bytes[skip..]
                            .iter()
                            .take_while(|&&b| b == quote)
                            .count();
                        if dropped_prev_quote
                            && cont_bytes.get(skip).copied() == Some(quote)
                            && continued_quote_run == 1
                        {
                            prev.text.push(quote as char);
                        }

                        // Append the continuation content (after the quote)
                        let appended = &cont_text[skip..];
                        let append_start = prev.text.len();
                        prev.text.push_str(appended);
                        prev.cont_offsets
                            .push((append_start, cont_global + skip as u32));
                    } else {
                        // Non-string continuation: trim previous trailing spaces,
                        // then append continuation content (trimmed of leading
                        // spaces).
                        let prev_trimmed_len = prev.text.trim_end().len();
                        prev.text.truncate(prev_trimmed_len);

                        let cont_trimmed = cont_text.trim_start();
                        let leading_spaces = cont_text.len() - cont_trimmed.len();
                        if leading_spaces == 1 && !prev.text.is_empty() && !cont_trimmed.is_empty()
                        {
                            prev.text.push(' ');
                        }
                        let append_start = prev.text.len();
                        prev.text.push_str(cont_trimmed);
                        prev.cont_offsets
                            .push((append_start, cont_global + leading_spaces as u32));
                    }
                }
                // If no previous segment exists, skip the orphan continuation line
            } else {
                let text = line.content_text().to_string();
                let global_offset = line.global_offset + line.content_start as u32;
                segments.push(ContentSegment {
                    text,
                    global_offset,
                    cont_offsets: Vec::new(),
                });
            }
        }

        segments
    }

    // ── Helper methods ──────────────────────────────────────────────

    /// Returns the current byte at `self.pos`.
    fn current_byte(&self) -> u8 {
        self.content.as_bytes()[self.pos]
    }

    /// Checks whether the remaining content starting at `self.pos` begins
    /// with `target`, using ASCII case-insensitive comparison.
    fn remaining_starts_with_ignore_case(&self, target: &str) -> bool {
        let remaining = &self.content[self.pos..];
        if remaining.len() < target.len() {
            return false;
        }
        remaining[..target.len()].eq_ignore_ascii_case(target)
    }

    /// Skips whitespace characters, including logical line boundaries.
    fn skip_whitespace(&mut self) {
        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Skips horizontal whitespace but not logical line boundaries.
    fn skip_horizontal_whitespace(&mut self) {
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
            self.offset_map.last().unwrap_or(&0) + 1
        };
        Token {
            kind: TokenKind::Eof,
            text: SmolStr::default(),
            span: Span::new(offset, offset, self.file_id),
        }
    }

    /// Creates a token from content byte range [start..end).
    fn make_token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        if start >= self.offset_map.len() {
            return self.make_eof();
        }
        let text: SmolStr = self.content[start..end].into();
        let global_start = self.offset_map[start];
        let global_end = if end > 0 && end <= self.offset_map.len() {
            if end < self.offset_map.len() {
                self.offset_map[end]
            } else {
                self.offset_map.last().unwrap_or(&0) + 1
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

    /// Like `make_token`, but normalises the text to uppercase.
    /// Used for identifiers and keywords since COBOL is case-insensitive.
    fn make_token_upper(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        if start >= self.offset_map.len() {
            return self.make_eof();
        }
        let raw = &self.content[start..end];
        let text: SmolStr = if raw.bytes().any(|b| b.is_ascii_lowercase()) {
            SmolStr::from(raw.to_ascii_uppercase())
        } else {
            SmolStr::from(raw)
        };
        let global_start = self.offset_map[start];
        let global_end = if end > 0 && end <= self.offset_map.len() {
            if end < self.offset_map.len() {
                self.offset_map[end]
            } else {
                self.offset_map.last().unwrap_or(&0) + 1
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
    /// Heuristic: treat as sign only when at the very start of input, after
    /// whitespace, or when the preceding non-whitespace token position was an
    /// operator or opening paren. COBOL arithmetic operators are separated; a
    /// sign immediately attached to digits denotes a signed numeric literal.
    fn should_treat_as_sign(&self) -> bool {
        if self.pos > 0 {
            let prev = self.content.as_bytes()[self.pos - 1];
            if prev == b' ' || prev == b'\t' {
                return true;
            }
        }

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
            return self.make_token_upper(kind, start, self.pos);
        }

        self.at_statement_start = false;
        // COBOL is case-insensitive; normalise identifiers to uppercase.
        self.make_token_upper(TokenKind::Identifier, start, self.pos)
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
    ///
    /// Also handles COBOL words that start with digits (e.g., `25COUNT`,
    /// `3-DEM-TBL`). After consuming digits, if the next character is a
    /// letter or a hyphen followed by an alphanumeric character, the token
    /// is treated as an identifier instead of a number.
    fn lex_number(&mut self) -> Token {
        let start = self.pos;

        // Optional sign
        let has_sign = if self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];
            if ch == b'+' || ch == b'-' {
                self.pos += 1;
                true
            } else {
                false
            }
        } else {
            false
        };

        // Integer part
        while self.pos < self.content.len() && self.content.as_bytes()[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Check if this is actually a COBOL word starting with digits
        // (e.g., "25COUNT", "3-DEM-TBL"). Only applies when there is no
        // sign prefix — signed tokens are always numeric literals.
        if !has_sign && self.pos < self.content.len() {
            let next_ch = self.content.as_bytes()[self.pos];
            let is_cobol_word = next_ch.is_ascii_alphabetic()
                || (next_ch == b'-'
                    && self.pos + 1 < self.content.len()
                    && self.content.as_bytes()[self.pos + 1].is_ascii_alphanumeric());
            if is_cobol_word {
                // Continue consuming as a COBOL word (letters, digits, hyphens)
                while self.pos < self.content.len() {
                    let ch = self.content.as_bytes()[self.pos];
                    if ch.is_ascii_alphanumeric() || ch == b'-' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                // COBOL words cannot end with a hyphen; trim trailing hyphens
                while self.pos > start && self.content.as_bytes()[self.pos - 1] == b'-' {
                    self.pos -= 1;
                }
                self.at_statement_start = false;
                return self.make_token(TokenKind::Identifier, start, self.pos);
            }
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
            if ch == b'\n' {
                // In fixed/variable formats only explicit continuation lines
                // may continue a literal. If we hit a logical line boundary
                // here, terminate recovery at the newline instead of
                // swallowing subsequent divisions/statements.
                return self.make_token(TokenKind::Error, start, self.pos);
            }
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
            if ch == b'\n' {
                return self.make_token(TokenKind::Error, start, self.pos);
            }
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
        self.skip_horizontal_whitespace();

        if self.pos >= self.content.len() {
            return self.make_eof();
        }

        let start = self.pos;

        // Picture string continues until we hit whitespace, period followed
        // by whitespace/end, or end of content.
        while self.pos < self.content.len() {
            let ch = self.content.as_bytes()[self.pos];

            if ch.is_ascii_whitespace() {
                break;
            }

            // Period handling: a period followed by whitespace or
            // end-of-content is a sentence terminator, not part of the
            // picture string.
            // But a period within the picture (like in Z,ZZZ.99) should be
            // included.
            if ch == b'.' {
                let next_pos = self.pos + 1;
                if next_pos >= self.content.len()
                    || self.content.as_bytes()[next_pos].is_ascii_whitespace()
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
                // If followed by a digit, lex as a decimal numeric literal
                // (e.g., `.11111` means `0.11111` in COBOL).
                if next.is_some_and(|b| b.is_ascii_digit()) {
                    return Some(self.lex_number());
                }
                // Otherwise it's a sentence terminator period.
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
    fn test_lex_decimal_literal_starting_with_dot() {
        let src = "       VALUE .11111.\
                                                              ";
        let tokens = lex(src);
        let dec_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::DecimalLiteral)
            .expect("should lex .11111 as DecimalLiteral");
        assert_eq!(dec_tok.text.as_str(), ".11111");
        // The trailing dot should be a period (sentence terminator)
        let last = tokens.last().unwrap();
        assert_eq!(last.kind, TokenKind::Period);
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
    fn test_lex_picture_stops_at_newline() {
        let src = "       01  WS-NAME PIC X(20).\n       01  WS-COUNT PIC 9(5).\n";
        let tokens = lex_free(src);
        let pic_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::PictureString)
            .collect();
        assert_eq!(pic_tokens.len(), 2);
        assert_eq!(pic_tokens[0].text.as_str(), "X(20)");
        assert_eq!(pic_tokens[1].text.as_str(), "9(5)");
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

    // ── Continuation line tests ─────────────────────────────────

    /// Helper to build a fixed-format line padded/truncated to exactly 80 characters
    /// (72 content columns + 8 identification area, including the newline).
    /// `seq` is the 6-char sequence area, `ind` is the indicator character,
    /// and `content` is the Area A+B text (columns 8-72, up to 65 chars).
    fn fixed_line(seq: &str, ind: char, content: &str) -> String {
        // columns 1-6: sequence, column 7: indicator, columns 8-72: content
        // pad content to 65 chars (columns 8-72), then add 8 chars identification area
        let padded_content = format!("{:<65}", content);
        let ident_area = "        "; // 8 chars for columns 73-80
        format!("{}{}{}{}\n", seq, ind, padded_content, ident_area)
    }

    #[test]
    fn test_continuation_string_literal() {
        // Line 1: MOVE "THIS IS A VERY L
        // Line 2 (continuation): "ONG STRING" TO WS-VAR.
        let line1 = fixed_line("000100", ' ', r#"MOVE "THIS IS A VERY L"#);
        let line2 = fixed_line("000200", '-', r#"    "ONG STRING" TO WS-VAR."#);
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        // Find the string literal token
        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a string literal token");
        assert_eq!(
            str_tok.text.as_str(),
            r#""THIS IS A VERY LONG STRING""#,
            "string literal should be merged across continuation"
        );

        // Verify the surrounding tokens are correct
        assert_eq!(tokens[0].kind, TokenKind::Move);
        let to_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::To)
            .expect("should have TO keyword");
        assert_eq!(to_tok.text.as_str(), "TO");
    }

    #[test]
    fn test_continuation_multiple_lines() {
        // Three lines forming one long string:
        // "FIRST P" + "ART SECOND" + " PART THIRD PART"
        let line1 = fixed_line("000100", ' ', r#"MOVE "FIRST P"#);
        let line2 = fixed_line("000200", '-', r#"    "ART SECOND P"#);
        let line3 = fixed_line("000300", '-', r#"    "ART THIRD PART" TO X."#);
        let src = format!("{}{}{}", line1, line2, line3);
        let tokens = lex(&src);

        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a string literal token");
        assert_eq!(
            str_tok.text.as_str(),
            r#""FIRST PART SECOND PART THIRD PART""#,
            "string should be merged across multiple continuation lines"
        );
    }

    #[test]
    fn test_continuation_non_string() {
        // Non-string continuation: a long identifier or keyword sequence
        // Line 1: MOVE VERY-LONG-
        // Line 2 (continuation): VARIABLE-NAME TO X.
        let line1 = fixed_line("000100", ' ', "MOVE VERY-LONG-VARI");
        let line2 = fixed_line("000200", '-', "    ABLE-NAME TO X.");
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        // The continuation should merge into one identifier
        let ident_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Identifier)
            .expect("should have an identifier token");
        assert_eq!(
            ident_tok.text.as_str(),
            "VERY-LONG-VARIABLE-NAME",
            "identifier should be merged across continuation"
        );
    }

    #[test]
    fn test_lex_digit_starting_identifier() {
        // COBOL allows data names starting with digits like "25COUNT"
        let src = "MOVE 25COUNT TO WS-A.";
        let tokens = lex_free(src);
        let ident = tokens
            .iter()
            .find(|t| t.text.as_str() == "25COUNT")
            .expect("should lex 25COUNT as a single token");
        assert_eq!(ident.kind, TokenKind::Identifier);
    }

    #[test]
    fn test_lex_digit_hyphen_identifier() {
        // COBOL allows data names like "3-DEM-TBL"
        let src = "MOVE 3-DEM-TBL TO WS-A.";
        let tokens = lex_free(src);
        let ident = tokens
            .iter()
            .find(|t| t.text.as_str() == "3-DEM-TBL")
            .expect("should lex 3-DEM-TBL as a single token");
        assert_eq!(ident.kind, TokenKind::Identifier);
    }

    #[test]
    fn test_lex_digit_identifier_does_not_break_numbers() {
        // Plain numbers should still be lexed as IntegerLiteral
        let src = "MOVE 42 TO WS-A.";
        let tokens = lex_free(src);
        let num = tokens
            .iter()
            .find(|t| t.text.as_str() == "42")
            .expect("should lex 42");
        assert_eq!(num.kind, TokenKind::IntegerLiteral);
    }

    #[test]
    fn test_lex_digit_identifier_does_not_break_decimals() {
        // Decimal literals should still work
        let src = "COMPUTE X = 3.14.";
        let tokens = lex_free(src);
        let dec = tokens
            .iter()
            .find(|t| t.text.as_str() == "3.14")
            .expect("should lex 3.14");
        assert_eq!(dec.kind, TokenKind::DecimalLiteral);
    }

    #[test]
    fn test_lex_digit_identifier_signed_number_not_affected() {
        // Signed numbers should remain as numeric literals
        let src = "COMPUTE X = (+5).";
        let tokens = lex_free(src);
        let num = tokens
            .iter()
            .find(|t| t.text.as_str() == "+5")
            .expect("should lex +5");
        assert_eq!(num.kind, TokenKind::IntegerLiteral);
    }

    #[test]
    fn test_lex_attached_sign_after_space_as_signed_number() {
        let src = "COMPUTE X = FUNCTION RANGE(10.2 -0.2, 5.6, -15.6).";
        let tokens = lex_free(src);

        assert!(tokens
            .iter()
            .any(|t| t.kind == TokenKind::DecimalLiteral && t.text == "-0.2"));
        assert!(tokens
            .iter()
            .any(|t| t.kind == TokenKind::DecimalLiteral && t.text == "-15.6"));
    }

    #[test]
    fn test_continuation_single_quoted_string() {
        // Same as test_continuation_string_literal but with single quotes
        let line1 = fixed_line("000100", ' ', "MOVE 'HELLO WO");
        let line2 = fixed_line("000200", '-', "    'RLD' TO WS-VAR.");
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a string literal token");
        assert_eq!(
            str_tok.text.as_str(),
            "'HELLO WORLD'",
            "single-quoted string should be merged across continuation"
        );
    }

    #[test]
    fn test_continuation_quote_heavy_string_closes_before_next_statement() {
        let line1 = fixed_line("037600", ' ', r#"IF WRK-X = """"""""""""""""#);
        let line2 = fixed_line(
            "037700",
            '-',
            r#"    """"""""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line3 = fixed_line("037800", '-', r#"    """""" TO WRK-X"#);
        let line4 = fixed_line("037900", ' ', "PERFORM PASS");
        let src = format!("{}{}{}{}", line1, line2, line3, line4);
        let tokens = lex(&src);

        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::To),
            "TO should be tokenized after the long string"
        );
        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::Perform),
            "PERFORM should not be swallowed into the string literal"
        );
    }

    #[test]
    fn test_continuation_string_keeps_trailing_escaped_quote() {
        let line1 = fixed_line(
            "029900",
            ' ',
            r#"MOVE " IF NO OTHER REPORT LINES APPEAR BELOW, ""COPY K7SEA"""#,
        );
        let line2 = fixed_line("030000", '-', r#"         "FAILED." TO PRINT-REC."#);
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::To),
            "TO should be tokenized after the continued string"
        );
        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a string literal token");
        assert!(
            str_tok.text.contains(r#""COPY K7SEA""FAILED."#),
            "continued string should preserve the escaped quote before FAILED"
        );
    }

    #[test]
    fn test_continuation_quote_heavy_string_from_reflowed_replace_closes_before_to() {
        let line1 = fixed_line(
            "036800",
            ' ',
            r#"MOVE """"""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line2 = fixed_line(
            "036900",
            '-',
            r#""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line3 = fixed_line(
            "037000",
            '-',
            r#""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line4 = fixed_line(
            "037100",
            '-',
            r#""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line5 = fixed_line(
            "037200",
            '-',
            r#""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""#,
        );
        let line6 = fixed_line("037300", '-', r#""""""""""""""" TO WRK-XN-00322."#);
        let src = format!("{}{}{}{}{}{}", line1, line2, line3, line4, line5, line6);
        let tokens = lex(&src);

        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::To),
            "TO should be tokenized after the reflowed quote-heavy string"
        );
    }

    #[test]
    fn test_continuation_preserves_quote_at_start_of_next_line_content() {
        let line1 = fixed_line(
            "004900",
            ' ',
            r#"THE-BIG-OL-LITERAL-ALPHABET IS "A+0B-1C*2D/3E=4Fl5G,6H;7I.8J""#,
        );
        let line2 = fixed_line("005000", '-', r#"    ""9K(L)M>N<O PQRSTUVWXYZ"."#);
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a merged string literal token");
        assert!(
            str_tok.text.contains(r#"8J""9K"#),
            "continued literal should preserve the embedded quote at the start of the next line: {:?}",
            str_tok.text
        );
        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::Period),
            "period after the literal should still be tokenized"
        );
    }

    #[test]
    fn test_continuation_line_can_close_string_without_prev_trailing_quote() {
        let line1 = fixed_line("067500", ' ', r#"MOVE "LITERAL ENDS AT 72"#);
        let line2 = fixed_line("067600", '-', r#"    "" TO X."#);
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::To),
            "TO should still be tokenized after the closed continued literal"
        );
        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::StringLiteral)
            .expect("should have a closed string literal");
        assert_eq!(str_tok.text, r#""LITERAL ENDS AT 72""#);
    }

    #[test]
    fn test_picture_string_can_start_on_next_logical_line_after_is() {
        let line1 = fixed_line(
            "013500",
            ' ',
            "01  LONG-PICTURE                       PICTURE IS",
        );
        let line2 = fixed_line("013600", ' ', "    XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX.");
        let src = format!("{}{}", line1, line2);
        let tokens = lex(&src);

        let str_tok = tokens
            .iter()
            .find(|t| t.kind == TokenKind::PictureString)
            .expect("should have a picture string token");
        assert_eq!(str_tok.text, "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    }

    #[test]
    fn test_unterminated_quote_does_not_swallow_next_division_header() {
        let src = concat!(
            "000100 IDENTIFICATION DIVISION.                                          \n",
            "000200 PROGRAM-ID. SG104A.                                                \n",
            "000300 SECURITY.                                                          \n",
            "000400     THIS PROGRAM CHECKS THE COMPILER\"S ABILITY.                    \n",
            "000500 ENVIRONMENT DIVISION.                                              \n",
            "000600 DATA DIVISION.                                                     \n",
            "000700 PROCEDURE DIVISION.                                                \n",
        );
        let tokens = lex(src);

        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::Environment),
            "unterminated quote on one line must not swallow ENVIRONMENT DIVISION"
        );
        assert!(
            tokens.iter().any(|t| t.kind == TokenKind::Procedure),
            "unterminated quote on one line must not swallow PROCEDURE DIVISION"
        );
    }
}
