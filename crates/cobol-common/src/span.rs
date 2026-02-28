use serde::{Deserialize, Serialize};

/// Unique identifier for a source file within the compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

/// A byte-offset range within a source file.
///
/// Used throughout the compiler to track the origin of tokens, AST nodes,
/// and diagnostics back to source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub file_id: FileId,
}

impl Span {
    /// Creates a new span with the given byte offsets and file id.
    pub fn new(start: u32, end: u32, file_id: FileId) -> Self {
        Self {
            start,
            end,
            file_id,
        }
    }

    /// Returns a zero-length span at offset 0 in file 0.
    ///
    /// Useful as a placeholder when no real source location is available.
    pub fn dummy() -> Self {
        Self {
            start: 0,
            end: 0,
            file_id: FileId(0),
        }
    }

    /// Returns a span that covers both `self` and `other`.
    ///
    /// The resulting span uses the minimum start, maximum end, and
    /// retains the file id of `self`.
    pub fn merge(&self, other: &Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            file_id: self.file_id,
        }
    }

    /// Returns the byte length of this span.
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if this span has zero length.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(10, 20, FileId(1));
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.file_id, FileId(1));
    }

    #[test]
    fn test_span_merge() {
        let a = Span::new(5, 15, FileId(1));
        let b = Span::new(10, 25, FileId(1));
        let merged = a.merge(&b);
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 25);
        assert_eq!(merged.file_id, FileId(1));

        // Merge in reverse order should also pick min/max
        let merged_rev = b.merge(&a);
        assert_eq!(merged_rev.start, 5);
        assert_eq!(merged_rev.end, 25);
    }

    #[test]
    fn test_span_len() {
        let span = Span::new(10, 20, FileId(0));
        assert_eq!(span.len(), 10);

        let zero = Span::new(5, 5, FileId(0));
        assert_eq!(zero.len(), 0);
    }

    #[test]
    fn test_span_is_empty() {
        let empty = Span::new(5, 5, FileId(0));
        assert!(empty.is_empty());

        let non_empty = Span::new(5, 10, FileId(0));
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_span_dummy() {
        let dummy = Span::dummy();
        assert_eq!(dummy.start, 0);
        assert_eq!(dummy.end, 0);
        assert_eq!(dummy.file_id, FileId(0));
    }
}
