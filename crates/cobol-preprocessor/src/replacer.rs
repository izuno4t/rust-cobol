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
            // Pseudo-text: literal substring replacement.
            result = result.replace(&pair.old_text, &pair.new_text);
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
pub fn apply_replace(source: &str, _reporter: &mut DiagnosticReporter) -> String {
    let directives = scanner::scan_replace_directives(source);

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
        let result = apply_replace(source, &mut reporter);
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
        let result = apply_replace(source, &mut reporter);
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
        let result = apply_replace(source, &mut reporter);

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
