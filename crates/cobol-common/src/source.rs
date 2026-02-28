use crate::span::FileId;

/// A source file with its content and precomputed line-start offsets.
///
/// Supports efficient conversion from byte offset to (line, column) pairs
/// for diagnostic reporting.
pub struct SourceFile {
    pub id: FileId,
    pub name: String,
    pub content: String,
    /// Byte offsets where each line begins (0-indexed offsets).
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Creates a new source file, computing line-start offsets from newlines.
    pub fn new(id: FileId, name: String, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);
        Self {
            id,
            name,
            content,
            line_starts,
        }
    }

    /// Returns the 1-based (line, column) for the given byte offset.
    ///
    /// If `offset` is beyond the content length, returns the position at
    /// the end of the last line.
    pub fn line_col(&self, offset: u32) -> (usize, usize) {
        let offset = offset as usize;
        // binary search for the line containing this offset
        let line_idx = match self.line_starts.binary_search(&(offset as u32)) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx] as usize;
        let line = line_idx + 1; // 1-based
        let col = offset - line_start + 1; // 1-based
        (line, col)
    }

    /// Returns the total number of lines in this source file.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Computes the byte offsets where each line starts.
    ///
    /// The first line always starts at offset 0. Each subsequent line starts
    /// at the byte immediately after a `\n`.
    fn compute_line_starts(content: &str) -> Vec<u32> {
        let mut starts = vec![0u32];
        for (i, byte) in content.bytes().enumerate() {
            if byte == b'\n' {
                starts.push((i + 1) as u32);
            }
        }
        starts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_file_creation() {
        let src = SourceFile::new(FileId(1), "test.cob".to_string(), "HELLO\nWORLD".to_string());
        assert_eq!(src.id, FileId(1));
        assert_eq!(src.name, "test.cob");
        assert_eq!(src.content, "HELLO\nWORLD");
    }

    #[test]
    fn test_line_col_first_line() {
        let src = SourceFile::new(FileId(0), "test.cob".to_string(), "HELLO\nWORLD".to_string());
        // 'H' is at offset 0 -> line 1, col 1
        assert_eq!(src.line_col(0), (1, 1));
        // 'O' is at offset 4 -> line 1, col 5
        assert_eq!(src.line_col(4), (1, 5));
    }

    #[test]
    fn test_line_col_second_line() {
        let src = SourceFile::new(FileId(0), "test.cob".to_string(), "HELLO\nWORLD".to_string());
        // 'W' is at offset 6 -> line 2, col 1
        assert_eq!(src.line_col(6), (2, 1));
        // 'D' is at offset 10 -> line 2, col 5
        assert_eq!(src.line_col(10), (2, 5));
    }

    #[test]
    fn test_line_count() {
        let single = SourceFile::new(FileId(0), "a.cob".to_string(), "HELLO".to_string());
        assert_eq!(single.line_count(), 1);

        let two = SourceFile::new(FileId(0), "b.cob".to_string(), "HELLO\nWORLD".to_string());
        assert_eq!(two.line_count(), 2);

        let three =
            SourceFile::new(FileId(0), "c.cob".to_string(), "A\nB\nC".to_string());
        assert_eq!(three.line_count(), 3);
    }
}
