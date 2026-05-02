// COBOL Semantic Analysis - PICTURE clause analyzer
//
// Parses PICTURE strings (e.g. "S9(7)V99", "X(20)", "Z,ZZZ,ZZ9.99")
// and computes the data category, total size, decimal positions,
// sign presence, and whether the picture is edited.

use cobol_ast::{PictureCategory, PictureClause};
use cobol_common::Span;
use smol_str::SmolStr;

/// Aggregated character counts from a PICTURE string expansion, used to
/// determine the picture category.
struct PictureCounts {
    digit: u32,
    alpha: u32,
    alphanumeric: u32,
    national: u32,
    boolean: u32,
    is_edited: bool,
    edit_numeric: u32,
    edit_misc: u32,
}

/// Analyzes PICTURE clause strings into structured `PictureClause` values.
pub struct PictureAnalyzer;

impl PictureAnalyzer {
    /// Analyzes a PICTURE string and returns a fully populated `PictureClause`.
    ///
    /// The input is the raw PIC string such as `"S9(7)V99"` or `"X(20)"`.
    /// This function expands repetition factors `(n)`, counts character
    /// positions, determines the picture category, and detects editing symbols.
    pub fn analyze(pic_str: &str, span: Span) -> PictureClause {
        let expanded = Self::expand(pic_str);

        let mut digit_count: u32 = 0;
        let mut alpha_count: u32 = 0;
        let mut alphanumeric_count: u32 = 0;
        let mut national_count: u32 = 0;
        let mut boolean_count: u32 = 0;
        let mut decimal_positions: u32 = 0;
        let mut is_signed = false;
        let mut has_decimal = false;
        let mut is_edited = false;

        // Edit symbol counters.
        let mut edit_z_count: u32 = 0;
        let mut edit_star_count: u32 = 0;
        let mut edit_plus_count: u32 = 0;
        let mut edit_minus_count: u32 = 0;
        let mut edit_dollar_count: u32 = 0;
        let mut edit_misc_count: u32 = 0;

        let chars: Vec<char> = expanded.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i].to_ascii_uppercase();
            match ch {
                '9' => {
                    digit_count += 1;
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                'X' => {
                    alphanumeric_count += 1;
                }
                'A' => {
                    alpha_count += 1;
                }
                'N' => {
                    national_count += 1;
                }
                '1' => {
                    boolean_count += 1;
                }
                'S' => {
                    is_signed = true;
                    // S does not contribute to storage size for display purposes.
                }
                'V' => {
                    has_decimal = true;
                    // V does not contribute to storage size.
                }
                'P' => {
                    // Scaling position: contributes to decimal adjustment
                    // but not to displayed size.
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                'Z' => {
                    edit_z_count += 1;
                    is_edited = true;
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                '*' => {
                    edit_star_count += 1;
                    is_edited = true;
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                '+' => {
                    edit_plus_count += 1;
                    is_edited = true;
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                '-' => {
                    edit_minus_count += 1;
                    is_edited = true;
                    if has_decimal {
                        decimal_positions += 1;
                    }
                }
                '$' | 'W' => {
                    edit_dollar_count += 1;
                    is_edited = true;
                }
                ',' | '.' | 'B' | '0' | '/' => {
                    edit_misc_count += 1;
                    is_edited = true;
                }
                'C' => {
                    // CR
                    if i + 1 < chars.len() && chars[i + 1].eq_ignore_ascii_case(&'R') {
                        edit_misc_count += 2;
                        is_edited = true;
                        i += 1; // skip 'R'
                    }
                }
                'D' => {
                    // DB
                    if i + 1 < chars.len() && chars[i + 1].eq_ignore_ascii_case(&'B') {
                        edit_misc_count += 2;
                        is_edited = true;
                        i += 1; // skip 'B'
                    }
                }
                _ => {
                    // Unknown character; ignored.
                }
            }
            i += 1;
        }

        let total_edit_numeric =
            edit_z_count + edit_star_count + edit_plus_count + edit_minus_count + edit_dollar_count;

        // Determine category.
        let counts = PictureCounts {
            digit: digit_count,
            alpha: alpha_count,
            alphanumeric: alphanumeric_count,
            national: national_count,
            boolean: boolean_count,
            is_edited,
            edit_numeric: total_edit_numeric,
            edit_misc: edit_misc_count,
        };
        let category = Self::determine_category(&counts);

        // Compute total size (number of character positions in storage).
        let size = match category {
            PictureCategory::Numeric => digit_count,
            PictureCategory::Alphabetic => alpha_count,
            PictureCategory::Alphanumeric => digit_count + alpha_count + alphanumeric_count,
            PictureCategory::NumericEdited => digit_count + total_edit_numeric + edit_misc_count,
            PictureCategory::AlphanumericEdited => {
                digit_count
                    + alpha_count
                    + alphanumeric_count
                    + total_edit_numeric
                    + edit_misc_count
            }
            PictureCategory::National => national_count,
            PictureCategory::NationalEdited => national_count + edit_misc_count,
            PictureCategory::Boolean => boolean_count,
        };

        PictureClause {
            raw_string: SmolStr::new(pic_str),
            category,
            size,
            decimal_positions,
            is_signed,
            is_edited,
            span,
        }
    }

    /// Expands repetition factors in a PICTURE string.
    ///
    /// For example, `"9(5)"` becomes `"99999"`, `"X(3)9(2)"` becomes `"XXX99"`.
    fn expand(pic_str: &str) -> String {
        let mut result = String::with_capacity(pic_str.len());
        let chars: Vec<char> = pic_str.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if i + 1 < chars.len() && chars[i + 1] == '(' {
                // Found a repetition factor: CHAR(n)
                let ch = chars[i];
                i += 2; // skip character and '('
                let mut num_str = String::new();
                while i < chars.len() && chars[i] != ')' {
                    num_str.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // skip ')'
                }
                // Note: Invalid repeat count in PIC string (e.g., PIC X(abc)) defaults to 1
                let count: u32 = num_str.parse().unwrap_or(1);
                for _ in 0..count {
                    result.push(ch);
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
    }

    /// Determines the PICTURE category from aggregated character counts.
    fn determine_category(c: &PictureCounts) -> PictureCategory {
        if c.boolean > 0 {
            return PictureCategory::Boolean;
        }

        if c.national > 0 {
            if c.is_edited {
                return PictureCategory::NationalEdited;
            }
            return PictureCategory::National;
        }

        if c.is_edited {
            // If there are alpha or X characters mixed with edit symbols,
            // it is alphanumeric-edited.
            if c.alpha > 0 || c.alphanumeric > 0 {
                return PictureCategory::AlphanumericEdited;
            }
            // Pure numeric with editing symbols.
            if c.digit > 0 || c.edit_numeric > 0 {
                return PictureCategory::NumericEdited;
            }
            // Only edit symbols (e.g. "B" insertion) with no data chars.
            if c.edit_misc > 0 {
                return PictureCategory::AlphanumericEdited;
            }
        }

        // Non-edited categories.
        if c.alphanumeric > 0 {
            return PictureCategory::Alphanumeric;
        }
        if c.digit > 0 && c.alpha > 0 {
            return PictureCategory::Alphanumeric;
        }
        if c.digit > 0 {
            return PictureCategory::Numeric;
        }
        if c.alpha > 0 {
            return PictureCategory::Alphabetic;
        }

        // Default fallback.
        PictureCategory::Alphanumeric
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze_picture(pic_str: &str) -> PictureClause {
        PictureAnalyzer::analyze(pic_str, Span::dummy())
    }

    #[test]
    fn test_expand_simple() {
        assert_eq!(PictureAnalyzer::expand("9(5)"), "99999");
        assert_eq!(PictureAnalyzer::expand("X(3)"), "XXX");
        assert_eq!(PictureAnalyzer::expand("S9(7)V99"), "S9999999V99");
    }

    #[test]
    fn test_picture_analysis_numeric() {
        let pic = analyze_picture("9(5)");
        assert_eq!(pic.category, PictureCategory::Numeric);
        assert_eq!(pic.size, 5);
        assert!(!pic.is_signed);
        assert_eq!(pic.decimal_positions, 0);
    }

    #[test]
    fn test_picture_analysis_signed_decimal() {
        let pic = analyze_picture("S9(7)V99");
        assert_eq!(pic.category, PictureCategory::Numeric);
        assert_eq!(pic.size, 9); // 7 + 2 = 9 digit positions
        assert!(pic.is_signed);
        assert_eq!(pic.decimal_positions, 2);
    }

    #[test]
    fn test_picture_analysis_alphanumeric() {
        let pic = analyze_picture("X(20)");
        assert_eq!(pic.category, PictureCategory::Alphanumeric);
        assert_eq!(pic.size, 20);
    }

    #[test]
    fn test_picture_analysis_alphabetic() {
        let pic = analyze_picture("A(10)");
        assert_eq!(pic.category, PictureCategory::Alphabetic);
        assert_eq!(pic.size, 10);
    }

    #[test]
    fn test_picture_analysis_edited() {
        let pic = analyze_picture("Z,ZZZ,ZZ9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
    }

    #[test]
    fn test_picture_analysis_edited_with_dollar() {
        let pic = analyze_picture("$$$,$$9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
    }

    #[test]
    fn test_picture_analysis_cr_db() {
        let pic = analyze_picture("9(5)CR");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);

        let pic = analyze_picture("9(5)DB");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
    }

    #[test]
    fn test_picture_analysis_mixed_ax() {
        let pic = analyze_picture("X(5)9(3)");
        assert_eq!(pic.category, PictureCategory::Alphanumeric);
        assert_eq!(pic.size, 8);
    }

    #[test]
    fn test_picture_analysis_sign_and_decimal_no_size() {
        let pic = analyze_picture("S9V9");
        assert_eq!(pic.category, PictureCategory::Numeric);
        assert_eq!(pic.size, 2); // two 9s
        assert!(pic.is_signed);
        assert_eq!(pic.decimal_positions, 1);
    }

    #[test]
    fn test_picture_analysis_star_edit() {
        let pic = analyze_picture("**,***,**9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
    }

    // -----------------------------------------------------------------------
    // Numeric edited PICTURE edge cases (Phase 6, item 4-2)
    // -----------------------------------------------------------------------

    #[test]
    fn test_numeric_edited_z_suppression() {
        // Z(4)9 = ZZZZ9  -> 4 Z + 1 digit = size 5
        let pic = analyze_picture("Z(4)9");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert_eq!(pic.size, 5);
        assert!(pic.is_edited);
        assert_eq!(pic.decimal_positions, 0);
    }

    #[test]
    fn test_numeric_edited_z_with_decimal() {
        // ZZZ.ZZ -> 3 Z before V, 2 Z after (implicit V at '.')
        let pic = analyze_picture("ZZZ.ZZ");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 5 Z + 1 edit_misc (.) = size 6
        assert_eq!(pic.size, 6);
    }

    #[test]
    fn test_numeric_edited_plus_sign_floating() {
        // +(4)9.99 -> ++++9.99
        let pic = analyze_picture("+(4)9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 4 + signs + 1 digit + 2 digits + 1 dot = 8
        // edit_numeric = 4, digit = 3, edit_misc = 1
        assert_eq!(pic.size, 8);
    }

    #[test]
    fn test_numeric_edited_minus_sign_floating() {
        // --,--9.99
        let pic = analyze_picture("--,--9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 4 minus + 1 digit + 2 digits + 1 comma + 1 dot = 9
        assert_eq!(pic.size, 9);
    }

    #[test]
    fn test_numeric_edited_cr_suffix() {
        // 9(5)CR -> 5 digits + 2 edit_misc (CR)
        let pic = analyze_picture("9(5)CR");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_db_suffix() {
        // 9(5)DB -> 5 digits + 2 edit_misc (DB)
        let pic = analyze_picture("9(5)DB");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_slash_insertion() {
        // 99/99/9999 -> date format
        let pic = analyze_picture("99/99/9999");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 8 digits + 2 slashes = 10
        assert_eq!(pic.size, 10);
    }

    #[test]
    fn test_numeric_edited_b_insertion() {
        // 9(3)B9(3) -> blank insertion
        let pic = analyze_picture("9(3)B9(3)");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 6 digits + 1 B = 7
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_zero_insertion() {
        // 9(3)09(3) -> zero insertion
        let pic = analyze_picture("9(3)09(3)");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 6 digits + 1 zero = 7
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_dollar_floating() {
        // $$$$.99 -> 4 dollar + 2 digits + 1 dot = 7
        let pic = analyze_picture("$$$$.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_star_check_protect() {
        // ***.99 -> 3 stars + 2 digits + 1 dot = 6
        let pic = analyze_picture("***.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        assert_eq!(pic.size, 6);
    }

    #[test]
    fn test_alphanumeric_edited_with_b() {
        // X(3)BX(3) -> alphanumeric with B insertion
        let pic = analyze_picture("X(3)BX(3)");
        assert_eq!(pic.category, PictureCategory::AlphanumericEdited);
        assert!(pic.is_edited);
        // 6 X + 1 B = 7
        assert_eq!(pic.size, 7);
    }

    #[test]
    fn test_numeric_edited_all_z_no_digits() {
        // ZZZZZ -> all suppression, no fixed digits
        let pic = analyze_picture("ZZZZZ");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        assert_eq!(pic.size, 5);
    }

    #[test]
    fn test_numeric_edited_complex_format() {
        // $$$,$$$,$$9.99CR -> complex currency format
        let pic = analyze_picture("$$$,$$$,$$9.99CR");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
        // 8 $ + 1 digit + 2 digits + 2 commas + 1 dot + 2 CR = 16
        assert_eq!(pic.size, 16);
    }
}
