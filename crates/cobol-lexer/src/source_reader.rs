use cobol_common::SourceFormat;

/// A single logical line from a COBOL source file, with metadata about
/// its column-based layout (fixed format) or free-form structure.
#[derive(Debug, Clone)]
pub struct SourceLine {
    /// 1-based line number in the source file.
    pub line_number: usize,
    /// Full raw text of the line (including trailing newline if present).
    pub raw: String,
    /// Column 7 indicator character (fixed format) or ' ' (free format).
    /// For free-format comment lines (`*>`), this is set to '*'.
    pub indicator: char,
    /// Byte offset within `raw` where the content area starts.
    pub content_start: usize,
    /// Byte offset within `raw` where the content area ends (exclusive).
    pub content_end: usize,
    /// Byte offset of this line's start within the full source text.
    pub global_offset: u32,
    /// The source format used to parse this line.
    format: SourceFormat,
}

impl SourceLine {
    /// Returns the content area text (the portion of the line that contains
    /// actual COBOL program text).
    pub fn content_text(&self) -> &str {
        &self.raw[self.content_start..self.content_end]
    }

    /// Returns `true` if this line is a comment (indicator `*` or `/`).
    pub fn is_comment(&self) -> bool {
        self.indicator == '*' || self.indicator == '/'
    }

    /// Returns `true` if this line is a continuation line (indicator `-`).
    pub fn is_continuation(&self) -> bool {
        self.indicator == '-'
    }

    /// Returns `true` if this line is a debug line (indicator `D` or `d`).
    pub fn is_debug(&self) -> bool {
        self.indicator == 'D' || self.indicator == 'd'
    }

    /// Returns `true` if the content area is blank (all whitespace).
    pub fn is_blank(&self) -> bool {
        self.content_text().trim().is_empty()
    }

    /// Returns the sequence number area (columns 1-6) for fixed format.
    /// For free format, returns an empty string.
    pub fn sequence_area(&self) -> &str {
        match self.format {
            SourceFormat::Fixed | SourceFormat::Variable => {
                let end = 6.min(self.raw.len());
                &self.raw[..end]
            }
            SourceFormat::Free => "",
        }
    }
}

/// Reads COBOL source text and splits it into logical `SourceLine`s,
/// handling the column-based layout of fixed format and the free-form
/// layout of free format.
pub struct SourceReader {
    lines: Vec<SourceLine>,
}

impl SourceReader {
    /// Creates a new `SourceReader` by parsing the given source text
    /// according to the specified format.
    pub fn new(source: &str, format: SourceFormat) -> Self {
        let lines = match format {
            SourceFormat::Fixed => Self::parse_fixed(source),
            SourceFormat::Free => Self::parse_free(source),
            SourceFormat::Variable => Self::parse_variable(source),
        };
        Self { lines }
    }

    /// Returns a slice of all parsed source lines.
    pub fn lines(&self) -> &[SourceLine] {
        &self.lines
    }

    /// Parse source in fixed format (COBOL-85 standard layout).
    ///
    /// - Columns 1-6: Sequence number area (ignored for compilation)
    /// - Column 7: Indicator area
    /// - Columns 8-11: Area A
    /// - Columns 12-72: Area B
    /// - Columns 73+: Identification area (ignored)
    fn parse_fixed(source: &str) -> Vec<SourceLine> {
        let mut result = Vec::new();
        let mut global_offset: u32 = 0;

        for (idx, raw_line) in source.split('\n').enumerate() {
            // Preserve the newline in the raw text if it was present in the
            // original source (all lines except possibly the last will have
            // had a newline that split() consumed).
            let has_newline = global_offset as usize + raw_line.len() < source.len();
            let raw = if has_newline {
                format!("{}\n", raw_line)
            } else {
                raw_line.to_string()
            };

            let line_len = raw_line.len();

            // Extract indicator from column 7 (0-indexed position 6)
            let indicator = if line_len >= 7 {
                raw_line.as_bytes()[6] as char
            } else {
                ' '
            };

            // Content area: columns 8-72 (0-indexed: bytes 7..72)
            let content_start = 7.min(line_len);
            let content_end = 72.min(line_len);

            result.push(SourceLine {
                line_number: idx + 1,
                raw,
                indicator,
                content_start,
                content_end,
                global_offset,
                format: SourceFormat::Fixed,
            });

            // Advance global offset past the raw line plus the newline
            global_offset += line_len as u32;
            if has_newline {
                global_offset += 1; // for the '\n'
            }
        }

        // Remove trailing empty line produced by a final newline
        if let Some(last) = result.last() {
            if last.raw.is_empty() {
                result.pop();
            }
        }

        result
    }

    /// Parse source in variable format (like fixed, but no right margin
    /// at column 72).
    fn parse_variable(source: &str) -> Vec<SourceLine> {
        let mut result = Vec::new();
        let mut global_offset: u32 = 0;

        for (idx, raw_line) in source.split('\n').enumerate() {
            let has_newline = global_offset as usize + raw_line.len() < source.len();
            let raw = if has_newline {
                format!("{}\n", raw_line)
            } else {
                raw_line.to_string()
            };

            let line_len = raw_line.len();

            let indicator = if line_len >= 7 {
                raw_line.as_bytes()[6] as char
            } else {
                ' '
            };

            // Variable format: content extends to end of line (no right margin)
            let content_start = 7.min(line_len);
            let content_end = line_len;

            result.push(SourceLine {
                line_number: idx + 1,
                raw,
                indicator,
                content_start,
                content_end,
                global_offset,
                format: SourceFormat::Variable,
            });

            global_offset += line_len as u32;
            if has_newline {
                global_offset += 1;
            }
        }

        if let Some(last) = result.last() {
            if last.raw.is_empty() {
                result.pop();
            }
        }

        result
    }

    /// Parse source in free format (COBOL 2002+).
    ///
    /// - No column restrictions
    /// - Comments start with `*>` anywhere in the line
    /// - Entire line is content
    fn parse_free(source: &str) -> Vec<SourceLine> {
        let mut result = Vec::new();
        let mut global_offset: u32 = 0;

        for (idx, raw_line) in source.split('\n').enumerate() {
            let has_newline = global_offset as usize + raw_line.len() < source.len();
            let raw = if has_newline {
                format!("{}\n", raw_line)
            } else {
                raw_line.to_string()
            };

            let line_len = raw_line.len();

            // Check if this is a comment line: trimmed line starts with *>
            let trimmed = raw_line.trim_start();
            let indicator = if trimmed.starts_with("*>") { '*' } else { ' ' };

            let content_start = 0;
            let content_end = line_len;

            result.push(SourceLine {
                line_number: idx + 1,
                raw,
                indicator,
                content_start,
                content_end,
                global_offset,
                format: SourceFormat::Free,
            });

            global_offset += line_len as u32;
            if has_newline {
                global_offset += 1;
            }
        }

        if let Some(last) = result.last() {
            if last.raw.is_empty() {
                result.pop();
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_format_basic() {
        let src = "000100 IDENTIFICATION DIVISION.                                          \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        let lines = reader.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].indicator, ' ');
        assert_eq!(lines[0].content_text().trim(), "IDENTIFICATION DIVISION.");
    }

    #[test]
    fn test_fixed_format_comment() {
        let src = "000100*THIS IS A COMMENT                                                 \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert!(reader.lines()[0].is_comment());
    }

    #[test]
    fn test_fixed_format_continuation() {
        let src = "000100 MOVE \"HELLO                                                       \n\
                   000200-    \"WORLD\" TO WS-VAR.                                          \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        let lines = reader.lines();
        assert!(!lines[0].is_continuation());
        assert!(lines[1].is_continuation());
    }

    #[test]
    fn test_free_format_basic() {
        let src = "IDENTIFICATION DIVISION.\n";
        let reader = SourceReader::new(src, SourceFormat::Free);
        let lines = reader.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content_text().trim(), "IDENTIFICATION DIVISION.");
    }

    #[test]
    fn test_free_format_comment() {
        let src = "*> This is a free format comment\n";
        let reader = SourceReader::new(src, SourceFormat::Free);
        assert!(reader.lines()[0].is_comment());
    }

    #[test]
    fn test_sequence_area() {
        let src = "000100 IDENTIFICATION DIVISION.                                          \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert_eq!(reader.lines()[0].sequence_area(), "000100");
    }

    #[test]
    fn test_short_lines_fixed_format() {
        // Lines shorter than 7 chars should still be handled gracefully
        let src = "     \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert!(reader.lines()[0].is_blank());
    }

    #[test]
    fn test_empty_source() {
        let reader = SourceReader::new("", SourceFormat::Fixed);
        assert!(reader.lines().is_empty());
    }

    #[test]
    fn test_fixed_format_debug_line() {
        let src = "000100D    DISPLAY \"DEBUG INFO\".                                         \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert!(reader.lines()[0].is_debug());
    }

    #[test]
    fn test_fixed_format_page_eject() {
        let src = "000100/                                                                  \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert!(reader.lines()[0].is_comment());
    }

    #[test]
    fn test_multiple_lines() {
        let src = "000100 IDENTIFICATION DIVISION.                                          \n\
                   000200 PROGRAM-ID. TEST.                                                  \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        let lines = reader.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_number, 1);
        assert_eq!(lines[1].line_number, 2);
    }

    #[test]
    fn test_global_offset() {
        let src = "000100 IDENTIFICATION DIVISION.                                          \n\
                   000200 PROGRAM-ID. TEST.                                                  \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        let lines = reader.lines();
        assert_eq!(lines[0].global_offset, 0);
        // Second line starts after the first line's bytes + newline
        assert!(lines[1].global_offset > 0);
    }

    #[test]
    fn test_free_format_sequence_area_empty() {
        let src = "IDENTIFICATION DIVISION.\n";
        let reader = SourceReader::new(src, SourceFormat::Free);
        assert_eq!(reader.lines()[0].sequence_area(), "");
    }

    #[test]
    fn test_content_end_truncates_at_72() {
        // Verify that columns 73+ are excluded from content in fixed format
        let src =
            "000100 IDENTIFICATION DIVISION.                                          IGNORED\n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        let content = reader.lines()[0].content_text();
        // Content should not include "IGNORED" (which is past column 72)
        assert!(!content.contains("IGNORED"));
    }

    #[test]
    fn test_variable_format_no_right_margin() {
        // Variable format: no right margin at column 72
        let src =
            "000100 IDENTIFICATION DIVISION.                                          EXTRA-CONTENT\n";
        let reader = SourceReader::new(src, SourceFormat::Variable);
        let content = reader.lines()[0].content_text();
        // Content should include everything after column 7
        assert!(content.contains("EXTRA-CONTENT"));
    }

    #[test]
    fn test_is_blank_with_only_spaces_in_content() {
        // Content area is all spaces
        let src = "000100                                                                   \n";
        let reader = SourceReader::new(src, SourceFormat::Fixed);
        assert!(reader.lines()[0].is_blank());
    }
}
