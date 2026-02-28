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
pub fn scan_copy_statements(source: &str) -> Vec<CopyStatement> {
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

            if let Some(stmt) = parse_copy_statement(source, copy_start, after_copy) {
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
pub fn scan_replace_directives(source: &str) -> Vec<ReplaceDirective> {
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

            if let Some(directive) = parse_replace_directive(source, replace_start, after_replace) {
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
fn find_keyword(source: &str, start: usize, keyword: &str) -> Option<usize> {
    let source_upper = source[start..].to_ascii_uppercase();
    source_upper.find(keyword).map(|offset| start + offset)
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
) -> Option<CopyStatement> {
    let mut pos = skip_whitespace(source, after_keyword);
    let len = source.len();

    if pos >= len {
        return None;
    }

    // Parse copybook name.
    let (copybook_name, next_pos) = parse_cobol_word(source, pos)?;
    pos = skip_whitespace(source, next_pos);

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
            pos = skip_whitespace(source, pos + 2);
            if let Some((lib_name, np)) = parse_cobol_word(source, pos) {
                library_name = Some(lib_name);
                pos = skip_whitespace(source, np);
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
                pos = skip_whitespace(source, after_replacing);
                // Parse replacement pairs until we hit the period.
                while pos < len && source.as_bytes()[pos] != b'.' {
                    if let Some((pair, np)) = parse_replace_pair(source, pos) {
                        replacings.push(pair);
                        pos = skip_whitespace(source, np);
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
) -> Option<ReplaceDirective> {
    let mut pos = skip_whitespace(source, after_keyword);
    let len = source.len();

    if pos >= len {
        return None;
    }

    // Check for REPLACE OFF.
    let upper_rest = source[pos..].to_ascii_uppercase();
    if upper_rest.starts_with("OFF") {
        let after_off = pos + 3;
        if after_off >= len || !is_cobol_word_char(source.as_bytes()[after_off]) {
            pos = skip_whitespace(source, after_off);
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
        if let Some((pair, np)) = parse_replace_pair(source, pos) {
            replacings.push(pair);
            pos = skip_whitespace(source, np);
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
fn parse_replace_pair(source: &str, start: usize) -> Option<(ReplacePair, usize)> {
    let len = source.len();
    let mut pos = start;

    if pos >= len {
        return None;
    }

    let (old_text, is_pseudo, next_pos) = if source[pos..].starts_with("==") {
        let (text, np) = parse_pseudo_text(source, pos)?;
        (text, true, np)
    } else {
        let (word, np) = parse_cobol_word(source, pos)?;
        (word, false, np)
    };

    pos = skip_whitespace(source, next_pos);

    // Expect BY keyword.
    let upper_rest = source[pos..].to_ascii_uppercase();
    if !upper_rest.starts_with("BY") {
        return None;
    }
    let after_by = pos + 2;
    if after_by < len && is_cobol_word_char(source.as_bytes()[after_by]) {
        return None;
    }
    pos = skip_whitespace(source, after_by);

    let (new_text, next_pos2) = if pos < len && source[pos..].starts_with("==") {
        let (text, np) = parse_pseudo_text(source, pos)?;
        (text, np)
    } else {
        parse_cobol_word(source, pos)?
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

/// Parses pseudo-text delimited by `==` ... `==`.
/// Returns the text between delimiters (trimmed) and position after closing `==`.
fn parse_pseudo_text(source: &str, start: usize) -> Option<(String, usize)> {
    if !source[start..].starts_with("==") {
        return None;
    }

    let content_start = start + 2;
    let rest = &source[content_start..];

    let close_pos = rest.find("==")?;
    let text = rest[..close_pos].trim().to_string();
    let after_close = content_start + close_pos + 2;

    Some((text, after_close))
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
fn skip_whitespace(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut pos = start;
    while pos < bytes.len()
        && (bytes[pos] == b' ' || bytes[pos] == b'\t' || bytes[pos] == b'\n' || bytes[pos] == b'\r')
    {
        pos += 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_simple_copy() {
        let source = "COPY mybook.\n";
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert!(stmts[0].library_name.is_none());
        assert!(stmts[0].replacings.is_empty());
    }

    #[test]
    fn test_scan_copy_of_library() {
        let source = "COPY mybook OF mylib.\n";
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert_eq!(stmts[0].library_name.as_deref(), Some("mylib"));
    }

    #[test]
    fn test_scan_copy_in_library() {
        let source = "COPY mybook IN mylib.\n";
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "mybook");
        assert_eq!(stmts[0].library_name.as_deref(), Some("mylib"));
    }

    #[test]
    fn test_scan_copy_with_pseudo_text_replacing() {
        let source = "COPY mybook REPLACING ==OLD== BY ==NEW==.\n";
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].replacings.len(), 1);
        assert_eq!(stmts[0].replacings[0].old_text, "OLD");
        assert_eq!(stmts[0].replacings[0].new_text, "NEW");
        assert!(stmts[0].replacings[0].is_pseudo_text);
    }

    #[test]
    fn test_scan_copy_with_word_replacing() {
        let source = "COPY mybook REPLACING OLD-NAME BY NEW-NAME.\n";
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].replacings.len(), 1);
        assert_eq!(stmts[0].replacings[0].old_text, "OLD-NAME");
        assert_eq!(stmts[0].replacings[0].new_text, "NEW-NAME");
        assert!(!stmts[0].replacings[0].is_pseudo_text);
    }

    #[test]
    fn test_scan_copy_multiple_replacings() {
        let source = "COPY mybook REPLACING ==:A:== BY ==X== ==:B:== BY ==Y==.\n";
        let stmts = scan_copy_statements(source);
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
        let stmts = scan_copy_statements(source);
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0].copybook_name, "MYBOOK");
    }

    #[test]
    fn test_scan_replace_directive() {
        let source = "REPLACE ==OLD== BY ==NEW==.\n";
        let directives = scan_replace_directives(source);
        assert_eq!(directives.len(), 1);
        assert!(!directives[0].is_off);
        assert_eq!(directives[0].replacings.len(), 1);
        assert_eq!(directives[0].replacings[0].old_text, "OLD");
        assert_eq!(directives[0].replacings[0].new_text, "NEW");
    }

    #[test]
    fn test_scan_replace_off() {
        let source = "REPLACE OFF.\n";
        let directives = scan_replace_directives(source);
        assert_eq!(directives.len(), 1);
        assert!(directives[0].is_off);
        assert!(directives[0].replacings.is_empty());
    }

    #[test]
    fn test_no_false_positive_on_copybook_word() {
        // "COPYBOOK" should not be recognized as a COPY statement.
        let source = "MOVE COPYBOOK TO WS-FIELD.\n";
        let stmts = scan_copy_statements(source);
        assert!(stmts.is_empty());
    }

    #[test]
    fn test_multiple_copy_statements() {
        let source = "COPY a.\nCOPY b.\n";
        let stmts = scan_copy_statements(source);
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
        assert_eq!(text, "HELLO WORLD");
    }
}
