use crate::source::SourceFile;
use crate::span::FileId;

/// A registry of source files used during compilation.
///
/// Each file added receives a unique [`FileId`] that can be used
/// to look up the file later (e.g., when rendering diagnostics).
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a source file and returns its assigned [`FileId`].
    pub fn add_file(&mut self, name: impl Into<String>, content: impl Into<String>) -> FileId {
        debug_assert!(
            self.files.len() < u32::MAX as usize,
            "Too many source files: FileId would overflow u32"
        );
        let id = FileId(self.files.len() as u32);
        self.files
            .push(SourceFile::new(id, name.into(), content.into()));
        id
    }

    /// Returns a reference to the source file with the given id, if it exists.
    pub fn get_file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    /// Returns the number of files registered in this source map.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_file() {
        let mut map = SourceMap::new();
        let id = map.add_file("main.cob", "IDENTIFICATION DIVISION.");

        let file = map.get_file(id).expect("file should exist");
        assert_eq!(file.id, id);
        assert_eq!(file.name, "main.cob");
        assert_eq!(file.content, "IDENTIFICATION DIVISION.");
        assert_eq!(map.file_count(), 1);
    }

    #[test]
    fn test_multiple_files() {
        let mut map = SourceMap::new();
        let id0 = map.add_file("a.cob", "FILE A");
        let id1 = map.add_file("b.cob", "FILE B");
        let id2 = map.add_file("c.cob", "FILE C");

        assert_eq!(id0, FileId(0));
        assert_eq!(id1, FileId(1));
        assert_eq!(id2, FileId(2));
        assert_eq!(map.file_count(), 3);

        assert_eq!(map.get_file(id0).unwrap().name, "a.cob");
        assert_eq!(map.get_file(id1).unwrap().name, "b.cob");
        assert_eq!(map.get_file(id2).unwrap().name, "c.cob");
    }

    #[test]
    fn test_get_nonexistent_file() {
        let map = SourceMap::new();
        assert!(map.get_file(FileId(0)).is_none());
        assert!(map.get_file(FileId(99)).is_none());
    }
}
