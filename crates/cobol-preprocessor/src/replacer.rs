// Replacer: applies REPLACING and REPLACE text substitutions.
//
// REPLACING is applied per-COPY statement on the inlined copybook content.
// REPLACE is a directive that applies globally to the source text after
// COPY expansion.

use crate::scanner::{self, ReplacePair};
use cobol_diagnostics::DiagnosticReporter;

/// Applies REPLACING pairs to copybook content.
///
/// For pseudo-text replacements (==old== BY ==new==), performs literal
/// substring replacement. For word replacements, replaces whole-word
/// occurrences only.
pub fn apply_replacing(content: &str, replacings: &[ReplacePair]) -> String {
    let mut result = content.to_string();

    for pair in replacings {
        if pair.is_pseudo_text {
            result = replace_pseudo_text(&result, &pair.old_text, &pair.new_text);
        } else {
            // Word replacement: replace whole-word occurrences.
            result = replace_whole_word(&result, &pair.old_text, &pair.new_text);
        }
    }

    result
}

/// Applies REPLACE directives to expanded source text.
///
/// REPLACE directives are scanned and processed in order. Each REPLACE
/// directive activates a set of replacements that apply to subsequent text
/// until the next REPLACE or REPLACE OFF directive.
pub fn apply_replace(
    source: &str,
    _reporter: &mut DiagnosticReporter,
    fixed_format: bool,
) -> String {
    let directives = scanner::scan_replace_directives(source, fixed_format);

    if directives.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len());
    let mut last_end = 0;
    let mut active_replacings: &[ReplacePair] = &[];

    for directive in &directives {
        // Apply active replacements to the text between the previous directive
        // and this one.
        let segment = &source[last_end..directive.start];
        if active_replacings.is_empty() {
            result.push_str(segment);
        } else {
            result.push_str(&apply_replacing(segment, active_replacings));
        }

        // Update active replacements based on this directive.
        if directive.is_off {
            active_replacings = &[];
        } else {
            active_replacings = &directive.replacings;
        }

        last_end = directive.end;
    }

    // Apply active replacements to the remaining text after the last directive.
    let remaining = &source[last_end..];
    if active_replacings.is_empty() {
        result.push_str(remaining);
    } else {
        result.push_str(&apply_replacing(remaining, active_replacings));
    }

    result
}

/// Replaces whole-word occurrences of `old` with `new` in `text`.
///
/// A "word boundary" in COBOL terms means the character before/after is not
/// alphanumeric or hyphen.
fn replace_whole_word(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return text.to_string();
    }

    let bytes = text.as_bytes();
    let old_upper = old.to_ascii_uppercase();
    let text_upper = text.to_ascii_uppercase();
    let mut result = String::with_capacity(text.len());
    let mut pos = 0;

    while pos < text.len() {
        if let Some(found) = text_upper[pos..].find(&old_upper) {
            let match_start = pos + found;
            let match_end = match_start + old.len();

            let before_ok = match_start == 0 || !is_cobol_word_char(bytes[match_start - 1]);
            let after_ok = match_end >= bytes.len() || !is_cobol_word_char(bytes[match_end]);

            if before_ok && after_ok {
                result.push_str(&text[pos..match_start]);
                result.push_str(new);
                pos = match_end;
            } else {
                // Not a whole-word match; advance past this occurrence.
                result.push_str(&text[pos..match_start + 1]);
                pos = match_start + 1;
            }
        } else {
            result.push_str(&text[pos..]);
            break;
        }
    }

    result
}

fn replace_pseudo_text(text: &str, old: &str, new: &str) -> String {
    let old_norm = normalize_pseudo_text(old);
    if old_norm.is_empty() {
        return text.to_string();
    }

    let new_norm = normalize_pseudo_text(new);
    let token_only = is_single_cobol_token(&old_norm);

    let mut raw = text.to_string();
    loop {
        let normalized = normalize_for_matching(&raw);
        let Some((start_norm, end_norm)) = find_pseudo_match(&normalized.text, &old_norm, token_only)
        else {
            break;
        };

        let start_raw = normalized.map[start_norm];
        let end_raw = normalized.map[end_norm - 1] + 1;
        raw.replace_range(start_raw..end_raw, &new_norm);
    }

    raw
}

fn find_pseudo_match(text: &str, pattern: &str, token_only: bool) -> Option<(usize, usize)> {
    let text_upper = text.to_ascii_uppercase();
    let pattern_upper = pattern.to_ascii_uppercase();
    let text_bytes = text.as_bytes();
    let mut pos = 0;

    while let Some(found) = text_upper[pos..].find(&pattern_upper) {
        let start = pos + found;
        let end = start + pattern.len();
        if !token_only || is_token_boundary(text_bytes, start, end) {
            return Some((start, end));
        }
        pos = start + 1;
    }

    None
}

fn is_token_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !is_cobol_word_char(bytes[start - 1]);
    let after_ok = end >= bytes.len() || !is_cobol_word_char(bytes[end]);
    before_ok && after_ok
}

fn is_single_cobol_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty() && bytes.iter().copied().all(is_cobol_word_char)
}

struct NormalizedText {
    text: String,
    map: Vec<usize>,
}

fn normalize_for_matching(raw: &str) -> NormalizedText {
    let mut text = String::with_capacity(raw.len());
    let mut map = Vec::with_capacity(raw.len());
    let mut pending_space = false;
    let mut offset = 0;
    let mut previous_line_ended_with_space = false;

    for line in raw.split_inclusive('\n') {
        let has_newline = line.ends_with('\n');
        let line_no_nl = line.strip_suffix('\n').unwrap_or(line);
        let line_no_cr = line_no_nl.strip_suffix('\r').unwrap_or(line_no_nl);
        let visible = if line_no_cr.len() > 72 {
            &line_no_cr[..72]
        } else {
            line_no_cr
        };
        let bytes = visible.as_bytes();

        let (indicator, start_idx) = if bytes.len() >= 7 && bytes[..6].iter().all(u8::is_ascii_digit)
        {
            (bytes[6], 7)
        } else {
            (b' ', 0)
        };

        let mut suppress_leading_space = false;
        if indicator == b'-' {
            pending_space = previous_line_ended_with_space;
            suppress_leading_space = !previous_line_ended_with_space;
        } else if !text.is_empty() {
            pending_space = true;
        }

        let mut line_ended_with_space = false;
        for (idx, b) in bytes.iter().copied().enumerate().skip(start_idx) {
            if b == b' ' || b == b'\t' {
                if suppress_leading_space {
                    continue;
                }
                pending_space = true;
                line_ended_with_space = true;
                continue;
            }

            suppress_leading_space = false;
            if pending_space && !text.is_empty() {
                text.push(' ');
                map.push(offset + idx);
            }
            pending_space = false;
            line_ended_with_space = false;
            text.push(b as char);
            map.push(offset + idx);
        }

        previous_line_ended_with_space = line_ended_with_space;
        offset += line.len();
        if !has_newline {
            break;
        }
    }

    NormalizedText { text, map }
}

fn normalize_pseudo_text(raw: &str) -> String {
    normalize_for_matching(raw).text
}

fn is_cobol_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_replacing_pseudo_text() {
        let content = "01 :PREFIX:-NAME PIC X.";
        let replacings = vec![ReplacePair {
            old_text: ":PREFIX:".to_string(),
            new_text: "WS".to_string(),
            is_pseudo_text: true,
        }];
        let result = apply_replacing(content, &replacings);
        assert_eq!(result, "01 WS-NAME PIC X.");
    }

    #[test]
    fn test_apply_replacing_pseudo_text_respects_token_boundaries() {
        let content = "ADD 001005 TO WRK-DS-09V00-901.";
        let replacings = vec![ReplacePair {
            old_text: "001".to_string(),
            new_text: "3".to_string(),
            is_pseudo_text: true,
        }];
        let result = apply_replacing(content, &replacings);
        assert_eq!(result, content);
    }

    #[test]
    fn test_apply_replacing_word() {
        let content = "01 OLD-NAME PIC X.";
        let replacings = vec![ReplacePair {
            old_text: "OLD-NAME".to_string(),
            new_text: "NEW-NAME".to_string(),
            is_pseudo_text: false,
        }];
        let result = apply_replacing(content, &replacings);
        assert_eq!(result, "01 NEW-NAME PIC X.");
    }

    #[test]
    fn test_replace_whole_word_no_partial() {
        // "FIELD" should not match inside "SUBFIELD".
        let result = replace_whole_word("01 SUBFIELD PIC X.", "FIELD", "OTHER");
        assert_eq!(result, "01 SUBFIELD PIC X.");
    }

    #[test]
    fn test_replace_whole_word_case_insensitive() {
        let result = replace_whole_word("01 my-field PIC X.", "MY-FIELD", "NEW-FIELD");
        assert_eq!(result, "01 NEW-FIELD PIC X.");
    }

    #[test]
    fn test_replace_whole_word_multiple_occurrences() {
        let result = replace_whole_word("MOVE A TO B. MOVE A TO C.", "A", "X");
        assert_eq!(result, "MOVE X TO B. MOVE X TO C.");
    }

    #[test]
    fn test_apply_replace_basic() {
        let source = "REPLACE ==OLD== BY ==NEW==.\n01 OLD PIC X.\n";
        let mut reporter = DiagnosticReporter::new();
        let result = apply_replace(source, &mut reporter, false);
        assert!(result.contains("NEW"), "result: {:?}", result);
        assert!(!reporter.has_errors());
    }

    #[test]
    fn test_apply_replace_off() {
        let source = concat!(
            "REPLACE ==OLD== BY ==NEW==.\n",
            "01 OLD PIC X.\n",
            "REPLACE OFF.\n",
            "01 OLD PIC 9.\n",
        );
        let mut reporter = DiagnosticReporter::new();
        let result = apply_replace(source, &mut reporter, false);
        let lines: Vec<&str> = result.lines().collect();

        // First data line: OLD should be replaced with NEW.
        let first_data = lines.iter().find(|l| l.contains("PIC X")).unwrap();
        assert!(first_data.contains("NEW"), "first line: {:?}", first_data);

        // Second data line: OLD should remain.
        let second_data = lines.iter().find(|l| l.contains("PIC 9")).unwrap();
        assert!(
            second_data.contains("OLD"),
            "second line: {:?}",
            second_data
        );
    }

    #[test]
    fn test_apply_replace_successive() {
        let source = concat!(
            "REPLACE ==A== BY ==B==.\n",
            "MOVE A TO X.\n",
            "REPLACE ==X== BY ==Y==.\n",
            "MOVE A TO X.\n",
        );
        let mut reporter = DiagnosticReporter::new();
        let result = apply_replace(source, &mut reporter, false);

        // After first REPLACE: A -> B
        assert!(result.contains("MOVE B TO X."), "result: {:?}", result);
        // After second REPLACE: X -> Y (but A is no longer replaced)
        assert!(result.contains("MOVE A TO Y."), "result: {:?}", result);
    }

    #[test]
    fn test_replace_whole_word_at_boundaries() {
        assert_eq!(replace_whole_word("A", "A", "B"), "B");
        assert_eq!(replace_whole_word(" A ", "A", "B"), " B ");
        assert_eq!(replace_whole_word("A.", "A", "B"), "B.");
    }
}
