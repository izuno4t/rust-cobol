// Scanner for COPY and REPLACE statements in COBOL source text.
//
// This is a lightweight text-level scanner that runs before full lexing.
// It identifies COPY statements (with optional REPLACING clauses) and
// REPLACE directives, recording their positions for the preprocessor.

/// A replacement pair: old text -> new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacePair {
    pub old_text: String,
    pub new_text: String,
    /// Whether the replacement uses pseudo-text delimiters (==...==).
    pub is_pseudo_text: bool,
}

/// A parsed COPY statement found in source text.
#[derive(Debug, Clone)]
pub struct CopyStatement {
    /// Byte offset of the start of the COPY statement (the 'C' of COPY).
    pub start: usize,
    /// Byte offset past the terminating period.
    pub end: usize,
    /// The copybook name.
    pub copybook_name: String,
    /// Optional library name (from OF/IN clause).
    pub library_name: Option<String>,
    /// REPLACING pairs.
    pub replacings: Vec<ReplacePair>,
}

/// A parsed REPLACE directive found in source text.
#[derive(Debug, Clone)]
pub struct ReplaceDirective {
    /// Byte offset of the start of the REPLACE directive.
    pub start: usize,
    /// Byte offset past the terminating period.
    pub end: usize,
    /// `true` if this is REPLACE OFF.
    pub is_off: bool,
    /// Replacement pairs (empty if `is_off` is true).
    pub replacings: Vec<ReplacePair>,
}

/// Scans source text for COPY statements.
///
/// Returns all COPY statements found, in order of appearance.
/// When `fixed_format` is true, the scanner skips fixed-format line prefixes
/// (columns 1-7: sequence area + indicator) when crossing line boundaries.
pub fn scan_copy_statements(source: &str, fixed_format: bool) -> Vec<CopyStatement> {
    let mut results = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        // Skip to potential COPY keyword (case-insensitive).
        if let Some(copy_start) = find_keyword(source, pos, "COPY") {
            let after_copy = copy_start + 4;
            if after_copy >= len || !is_word_boundary(bytes, copy_start, after_copy) {
                pos = copy_start + 1;
                continue;
            }

            if let Some(stmt) = parse_copy_statement(source, copy_start, after_copy, fixed_format) {
                pos = stmt.end;
                results.push(stmt);
            } else {
                pos = copy_start + 1;
            }
        } else {
            break;
        }
    }

    results
}

/// Scans source text for REPLACE directives.
///
/// Returns all REPLACE directives found, in order of appearance.
pub fn scan_replace_directives(source: &str, fixed_format: bool) -> Vec<ReplaceDirective> {
    let mut results = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        if let Some(replace_start) = find_keyword(source, pos, "REPLACE") {
            let after_replace = replace_start + 7;
            if after_replace >= len || !is_word_boundary(bytes, replace_start, after_replace) {
                pos = replace_start + 1;
                continue;
            }

            if let Some(directive) =
                parse_replace_directive(source, replace_start, after_replace, fixed_format)
            {
                pos = directive.end;
                results.push(directive);
            } else {
                pos = replace_start + 1;
            }
        } else {
            break;
        }
    }

    results
}

/// Finds the next occurrence of a keyword (case-insensitive) starting from `pos`.
/// Returns the byte offset of the start of the keyword, or None.
///
/// Skips occurrences that appear inside string literals (single or double quotes)
/// or inside comment lines (fixed format: column 7 is '*' or '/'; free format: `*>`).
fn find_keyword(source: &str, start: usize, keyword: &str) -> Option<usize> {
    let source_upper = source[start..].to_ascii_uppercase();
    let mut search_from = 0;
    while let Some(offset) = source_upper[search_from..].find(keyword) {
        let abs_pos = start + search_from + offset;

        // Check if this position is inside a comment line or string literal.
        if is_in_comment_line(source, abs_pos) || is_in_string_literal(source, abs_pos) {
            search_from = search_from + offset + 1;
            continue;
        }

        return Some(abs_pos);
    }
    None
}

/// Returns true if `pos` falls on a comment line.
///
/// Fixed format: column 7 (0-indexed byte 6) of the line is '*' or '/'.
/// Free format: the line (after leading spaces) starts with `*>`.
fn is_in_comment_line(source: &str, pos: usize) -> bool {
    // Find the start of the current line.
    let line_start = match source[..pos].rfind('\n') {
        Some(nl) => nl + 1,
        None => 0,
    };
    let line_bytes = &source.as_bytes()[line_start..];

    // Fixed format check: column 7 (index 6) is '*' or '/'
    if line_bytes.len() > 6 && (line_bytes[6] == b'*' || line_bytes[6] == b'/') {
        return true;
    }

    // Free format check: line starts with optional whitespace then `*>`
    let trimmed = source[line_start..].trim_start();
    if trimmed.starts_with("*>") {
        return true;
    }

    false
}

/// Returns true if `pos` is inside a string literal on the same line.
///
/// Scans from the start of the line up to `pos`, tracking open/close quotes.
fn is_in_string_literal(source: &str, pos: usize) -> bool {
    let line_start = match source[..pos].rfind('\n') {
        Some(nl) => nl + 1,
        None => 0,
    };

    let bytes = source.as_bytes();
    let mut in_string = false;
    let mut quote_char: u8 = 0;
    let mut i = line_start;

    while i < pos {
        let b = bytes[i];
        if in_string {
            if b == quote_char {
                // Check for doubled quote (escape): e.g. "" or ''
                if i + 1 < bytes.len() && bytes[i + 1] == quote_char {
                    i += 2; // skip the escaped quote
                    continue;
                }
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote_char = b;
        }
        i += 1;
    }

    in_string
}

/// Checks whether the positions `start` and `end` form a word boundary:
/// the character before `start` (if any) is not alphanumeric/hyphen,
/// and the character at `end` (if any) is not alphanumeric/hyphen.
fn is_word_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !is_cobol_word_char(bytes[start - 1]);
    let after_ok = end >= bytes.len() || !is_cobol_word_char(bytes[end]);
    before_ok && after_ok
}

fn is_cobol_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Parses a COPY statement starting at `copy_start` (position of 'C').
/// `after_keyword` is the position just after the "COPY" keyword.
fn parse_copy_statement(
    source: &str,
    copy_start: usize,
    after_keyword: usize,
    fixed_format: bool,
) -> Option<CopyStatement> {
    let mut pos = skip_whitespace(source, after_keyword, fixed_format);
    let len = source.len();

    if pos >= len {
        return None;
    }

    // Parse copybook name.
    let (copybook_name, next_pos) = parse_cobol_word(source, pos)?;
    pos = skip_whitespace(source, next_pos, fixed_format);

    // Check for optional OF/IN library-name.
    let mut library_name = None;
    if pos < len {
        let upper_rest = source[pos..].to_ascii_uppercase();
        if upper_rest.starts_with("OF ")
            || upper_rest.starts_with("OF\t")
            || upper_rest.starts_with("OF\n")
            || upper_rest.starts_with("OF\r")
            || upper_rest.starts_with("IN ")
            || upper_rest.starts_with("IN\t")
            || upper_rest.starts_with("IN\n")
            || upper_rest.starts_with("IN\r")
        {
            pos = skip_whitespace(source, pos + 2, fixed_format);
            if let Some((lib_name, np)) = parse_cobol_word(source, pos) {
                library_name = Some(lib_name);
                pos = skip_whitespace(source, np, fixed_format);
            }
        }
    }

    // Check for optional REPLACING clause.
    let mut replacings = Vec::new();
    if pos < len {
        let upper_rest = source[pos..].to_ascii_uppercase();
        if upper_rest.starts_with("REPLACING") {
            let after_replacing = pos + 9;
            if after_replacing >= len || !is_cobol_word_char(source.as_bytes()[after_replacing]) {
                pos = skip_whitespace(source, after_replacing, fixed_format);
                // Parse replacement pairs until we hit the period.
                while pos < len && source.as_bytes()[pos] != b'.' {
                    if let Some((pair, np)) = parse_replace_pair(source, pos, fixed_format) {
                        replacings.push(pair);
                        pos = skip_whitespace(source, np, fixed_format);
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // Expect terminating period.
    if pos < len && source.as_bytes()[pos] == b'.' {
        pos += 1; // consume the period
    }

    Some(CopyStatement {
        start: copy_start,
        end: pos,
        copybook_name,
        library_name,
        replacings,
    })
}

/// Parses a REPLACE directive starting at `replace_start`.
fn parse_replace_directive(
    source: &str,
    replace_start: usize,
    after_keyword: usize,
    fixed_format: bool,
) -> Option<ReplaceDirective> {
    let mut pos = skip_whitespace(source, after_keyword, fixed_format);
    let len = source.len();

    if pos >= len {
        return None;
    }

    // Check for REPLACE OFF.
    let upper_rest = source[pos..].to_ascii_uppercase();
    if upper_rest.starts_with("OFF") {
        let after_off = pos + 3;
        if after_off >= len || !is_cobol_word_char(source.as_bytes()[after_off]) {
            pos = skip_whitespace(source, after_off, fixed_format);
            // Expect terminating period.
            if pos < len && source.as_bytes()[pos] == b'.' {
                pos += 1;
            }
            return Some(ReplaceDirective {
                start: replace_start,
                end: pos,
                is_off: true,
                replacings: Vec::new(),
            });
        }
    }

    // Parse replacement pairs.
    let mut replacings = Vec::new();
    while pos < len && source.as_bytes()[pos] != b'.' {
        if let Some((pair, np)) = parse_replace_pair(source, pos, fixed_format) {
            replacings.push(pair);
            pos = skip_whitespace(source, np, fixed_format);
        } else {
            break;
        }
    }

    if replacings.is_empty() {
        return None;
    }

    // Expect terminating period.
    if pos < len && source.as_bytes()[pos] == b'.' {
        pos += 1;
    }

    Some(ReplaceDirective {
        start: replace_start,
        end: pos,
        is_off: false,
        replacings,
    })
}

/// Parses a single replacement pair: either `==old== BY ==new==`
/// (pseudo-text) or `old-word BY new-word` (identifier).
fn parse_replace_pair(
    source: &str,
    start: usize,
    fixed_format: bool,
) -> Option<(ReplacePair, usize)> {
    let len = source.len();
    let mut pos = start;

    if pos >= len {
        return None;
    }

    let (old_text, is_pseudo, next_pos) = if source[pos..].starts_with("==") {
        let (text, np) = parse_pseudo_text(source, pos)?;
        (text, true, np)
    } else if pos < len && (source.as_bytes()[pos] == b'"' || source.as_bytes()[pos] == b'\'') {
        // Handle string literal as old operand (e.g., REPLACING "PIG" BY "HORSE")
        let (text, np) = parse_string_literal(source, pos)?;
        (text, false, np)
    } else {
        let (word, np) = parse_cobol_word(source, pos)?;
        (word, false, np)
    };

    pos = skip_whitespace(source, next_pos, fixed_format);

    // Expect BY keyword.
    let upper_rest = source[pos..].to_ascii_uppercase();
    if !upper_rest.starts_with("BY") {
        return None;
    }
    let after_by = pos + 2;
    if after_by < len && is_cobol_word_char(source.as_bytes()[after_by]) {
        return None;
    }
    pos = skip_whitespace(source, after_by, fixed_format);

    let (new_text, next_pos2) = if pos < len && source[pos..].starts_with("==") {
        let (text, np) = parse_pseudo_text(source, pos)?;
        (text, np)
    } else if pos < len && (source.as_bytes()[pos] == b'"' || source.as_bytes()[pos] == b'\'') {
        // Handle string literal replacement values (e.g., "TRUE ")
        parse_string_literal(source, pos)?
    } else if pos < len
        && (source.as_bytes()[pos] == b'+' || source.as_bytes()[pos] == b'-')
        && pos + 1 < len
        && source.as_bytes()[pos + 1].is_ascii_digit()
    {
        // Handle signed numeric literal (e.g., +000004.99)
        parse_numeric_literal(source, pos)?
    } else if pos < len && source.as_bytes()[pos].is_ascii_digit() {
        // Handle numeric literal (e.g., 12345)
        parse_numeric_literal(source, pos)?
    } else {
        // Parse a COBOL word, possibly qualified with OF/IN and/or subscripted.
        // Examples: FIELD, FIELD OF GROUP, FIELD IN GRP (1), Z (2, 1, 1)
        parse_qualified_word(source, pos, fixed_format)?
    };

    Some((
        ReplacePair {
            old_text,
            new_text,
            is_pseudo_text: is_pseudo,
        },
        next_pos2,
    ))
}

/// Parses a COBOL word with optional qualification (OF/IN) and subscripts.
///
/// Handles patterns like:
/// - `FIELD`
/// - `FIELD OF GROUP`
/// - `FIELD IN GRP-1 IN GRP-2 (1)`
/// - `Z (2, 1, 1)`
///
/// Stops before the next replacement pair keyword (BY, period, ==) or end of input.
/// Returns the raw text (preserving spacing) and position after the last consumed token.
fn parse_qualified_word(source: &str, start: usize, fixed_format: bool) -> Option<(String, usize)> {
    let len = source.len();

    // Parse the initial COBOL word.
    let (_, word_end) = parse_cobol_word(source, start)?;
    let mut end = word_end;

    loop {
        let saved = end;
        let ws_end = skip_whitespace(source, end, fixed_format);

        // Check for subscript: (...)
        if ws_end < len && source.as_bytes()[ws_end] == b'(' {
            if let Some(close) = find_closing_paren(source, ws_end) {
                end = close + 1;
                continue;
            }
        }

        // Check for OF/IN qualification.
        if ws_end < len {
            let upper_rest = source[ws_end..].to_ascii_uppercase();
            let is_of_in = (upper_rest.starts_with("OF")
                && (ws_end + 2 >= len || !is_cobol_word_char(source.as_bytes()[ws_end + 2])))
                || (upper_rest.starts_with("IN")
                    && (ws_end + 2 >= len || !is_cobol_word_char(source.as_bytes()[ws_end + 2])));
            if is_of_in {
                let after_kw = skip_whitespace(source, ws_end + 2, fixed_format);
                if let Some((_, np)) = parse_cobol_word(source, after_kw) {
                    end = np;
                    continue;
                }
            }
        }

        end = saved;
        break;
    }

    Some((source[start..end].to_string(), end))
}

/// Finds the closing `)` matching the opening `(` at `start`.
/// Handles nested parentheses.
fn find_closing_paren(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    if start >= len || bytes[start] != b'(' {
        return None;
    }
    let mut depth = 0;
    let mut pos = start;
    while pos < len {
        match bytes[pos] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

/// Parses pseudo-text delimited by `==` ... `==`.
/// Returns the raw text between delimiters and position after closing `==`.
fn parse_pseudo_text(source: &str, start: usize) -> Option<(String, usize)> {
    if !source[start..].starts_with("==") {
        return None;
    }

    let content_start = start + 2;
    let rest = &source[content_start..];

    let close_pos = rest.find("==")?;
    let text = rest[..close_pos].to_string();
    let after_close = content_start + close_pos + 2;

    Some((text, after_close))
}

/// Parses a string literal (single or double quoted) starting at `pos`.
/// Returns the full string including quotes and the position after it.
fn parse_string_literal(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    if start >= len {
        return None;
    }

    let quote = bytes[start];
    if quote != b'"' && quote != b'\'' {
        return None;
    }

    let mut end = start + 1;
    while end < len {
        if bytes[end] == quote {
            // Check for doubled quote (escape)
            if end + 1 < len && bytes[end + 1] == quote {
                end += 2;
                continue;
            }
            end += 1; // past the closing quote
            return Some((source[start..end].to_string(), end));
        }
        end += 1;
    }
    None // unterminated string
}

/// Parses a numeric literal (optionally signed, with decimal point) starting at `pos`.
/// Returns the literal text and position after it.
///
/// A decimal point is only included if it is followed by at least one digit.
/// This prevents the COPY statement's terminating period from being consumed
/// as part of the numeric literal (e.g., `BY 4.` means literal `4` + period).
fn parse_numeric_literal(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let len = bytes.len();
    if start >= len {
        return None;
    }

    let mut end = start;
    // Optional sign
    if end < len && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    // Digits before decimal point
    let digit_start = end;
    while end < len && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digit_start {
        return None; // no digits
    }
    // Decimal point followed by digits (e.g., 4.99)
    if end < len && bytes[end] == b'.' && end + 1 < len && bytes[end + 1].is_ascii_digit() {
        end += 1; // consume '.'
        while end < len && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    Some((source[start..end].to_string(), end))
}

/// Parses a COBOL word (letters, digits, hyphens) starting at `pos`.
/// Returns the word and position after it.
fn parse_cobol_word(source: &str, start: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let len = bytes.len();

    if start >= len || !bytes[start].is_ascii_alphanumeric() {
        return None;
    }

    let mut end = start;
    while end < len && is_cobol_word_char(bytes[end]) {
        end += 1;
    }

    if end == start {
        return None;
    }

    Some((source[start..end].to_string(), end))
}

/// Skips whitespace and newlines starting from `pos`.
/// When `fixed_format` is true, also skips the fixed-format line prefix
/// (columns 1-7: 6-digit sequence number + 1 indicator character) after
/// crossing a newline boundary.
fn skip_whitespace(source: &str, start: usize, fixed_format: bool) -> usize {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = start;
    while pos < len {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' => pos += 1,
            b'\n' => {
                pos += 1;
                if fixed_format && pos < len {
                    // Skip fixed-format line prefix (columns 1-7).
                    // Find end of line to check its length.
                    let line_start = pos;
                    let mut line_end = pos;
                    while line_end < len && bytes[line_end] != b'\n' {
                        line_end += 1;
                    }
                    let line_len = line_end - line_start;
                    if line_len >= 7 {
                        let indicator = bytes[line_start + 6];
                        // If indicator is '*' or '/', this is a comment line.
                        // Skip the entire line.
                        if indicator == b'*' || indicator == b'/' {
                            pos = line_end;
                            continue;
                        }
                        // Skip the 7-character prefix (sequence + indicator).
                        pos = line_start + 7;
                    }
                }
            }
            _ => break,
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_simple_copy() {
        let source = "COPY mybook.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert!(stmts[0].library_name.is_none());
        assert!(stmts[0].replacings.is_empty());
    }

    #[test]
    fn test_scan_copy_of_library() {
        let source = "COPY mybook OF mylib.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert_eq!(stmts[0].library_name.as_deref(), Some("mylib"));
    }

    #[test]
    fn test_scan_copy_in_library() {
        let source = "COPY mybook IN mylib.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert_eq!(stmts[0].library_name.as_deref(), Some("mylib"));
    }

    #[test]
    fn test_scan_copy_with_pseudo_text_replacing() {
        let source = "COPY mybook REPLACING ==OLD== BY ==NEW==.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].replacings.len(), 1);
        assert_eq!(stmts[0].replacings[0].old_text, "OLD");
        assert_eq!(stmts[0].replacings[0].new_text, "NEW");
        assert!(stmts[0].replacings[0].is_pseudo_text);
    }

    #[test]
    fn test_scan_copy_with_word_replacing() {
        let source = "COPY mybook REPLACING OLD-NAME BY NEW-NAME.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].replacings.len(), 1);
        assert_eq!(stmts[0].replacings[0].old_text, "OLD-NAME");
        assert_eq!(stmts[0].replacings[0].new_text, "NEW-NAME");
        assert!(!stmts[0].replacings[0].is_pseudo_text);
    }

    #[test]
    fn test_scan_copy_multiple_replacings() {
        let source = "COPY mybook REPLACING ==:A:== BY ==X== ==:B:== BY ==Y==.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].replacings.len(), 2);
        assert_eq!(stmts[0].replacings[0].old_text, ":A:");
        assert_eq!(stmts[0].replacings[0].new_text, "X");
        assert_eq!(stmts[0].replacings[1].old_text, ":B:");
        assert_eq!(stmts[0].replacings[1].new_text, "Y");
    }

    #[test]
    fn test_scan_copy_case_insensitive() {
        let source = "copy MYBOOK.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "MYBOOK");
    }

    #[test]
    fn test_scan_replace_directive() {
        let source = "REPLACE ==OLD== BY ==NEW==.\n";
        let directives = scan_replace_directives(source, false);
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_off);
        assert_eq!(directives[0].replacings.len(), 1);
        assert_eq!(directives[0].replacings[0].old_text, "OLD");
        assert_eq!(directives[0].replacings[0].new_text, "NEW");
    }

    #[test]
    fn test_scan_replace_off() {
        let source = "REPLACE OFF.\n";
        let directives = scan_replace_directives(source, false);
        assert_eq!(directives.len(), 1);
        assert!(directives[0].is_off);
        assert!(directives[0].replacings.is_empty());
    }

    #[test]
    fn test_no_false_positive_on_copybook_word() {
        // "COPYBOOK" should not be recognized as a COPY statement.
        let source = "MOVE COPYBOOK TO WS-FIELD.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_multiple_copy_statements() {
        let source = "COPY a.\nCOPY b.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0].copybook_name, "a");
        assert_eq!(stmts[1].copybook_name, "b");
    }

    #[test]
    fn test_parse_pseudo_text_with_spaces() {
        let source = "==  HELLO WORLD  ==";
        let result = parse_pseudo_text(source, 0);
        assert!(result.is_some());
        let (text, _) = result.unwrap();
        assert_eq!(text, "  HELLO WORLD  ");
    }

    #[test]
    fn test_copy_in_string_literal_ignored() {
        // COPY inside a double-quoted string should not be detected.
        let source = "MOVE \"COPY FILE DESCR\" TO FEATURE.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(
            stmts.is_empty(),
            "COPY inside string literal should be ignored"
        );
    }

    #[test]
    fn test_copy_in_single_quoted_string_ignored() {
        let source = "MOVE 'COPY FILE DESCR' TO FEATURE.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(
            stmts.is_empty(),
            "COPY inside single-quoted string should be ignored"
        );
    }

    #[test]
    fn test_copy_in_fixed_format_comment_ignored() {
        // Fixed format: column 7 (index 6) is '*' → comment line.
        let source = "000100*    COPY MYBOOK.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(
            stmts.is_empty(),
            "COPY in fixed-format comment line should be ignored"
        );
    }

    #[test]
    fn test_copy_in_fixed_format_comment_slash_ignored() {
        // Fixed format: column 7 (index 6) is '/' → comment line.
        let source = "000100/    COPY MYBOOK.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(
            stmts.is_empty(),
            "COPY in fixed-format '/' comment line should be ignored"
        );
    }

    #[test]
    fn test_copy_in_free_format_comment_ignored() {
        let source = "*> COPY MYBOOK.\n";
        let stmts = scan_copy_statements(source, false);
        assert!(
            stmts.is_empty(),
            "COPY in free-format comment should be ignored"
        );
    }

    #[test]
    fn test_copy_after_string_still_found() {
        // COPY after a string literal should still be found.
        let source = "MOVE \"HELLO\" TO WS-FIELD.\nCOPY mybook.\n";
        let stmts = scan_copy_statements(source, false);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
    }

    #[test]
    fn test_replace_in_string_literal_ignored() {
        let source = "MOVE \"REPLACE OLD BY NEW\" TO WS-FIELD.\n";
        let directives = scan_replace_directives(source, false);
        assert!(
            directives.is_empty(),
            "REPLACE inside string literal should be ignored"
        );
    }
}
