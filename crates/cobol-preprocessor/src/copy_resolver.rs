// Copy resolver: locates copybook files on the filesystem.
//
// Searches configurable directories for copybook files, trying multiple
// extensions and case variations.

use std::path::{Path, PathBuf};

use crate::PreprocessorConfig;

/// Extensions to try when resolving a copybook name to a file.
const EXTENSIONS: &[&str] = &[".cpy", ".cob", ".CPY", ".COB", ""];

/// Resolves copybook names to filesystem paths by searching configured
/// directories with multiple extension candidates.
#[derive(Debug)]
pub struct CopyResolver {
    /// Directories to search, in order of priority.
    search_paths: Vec<PathBuf>,
}

impl CopyResolver {
    /// Creates a resolver using the configured copy paths.
    ///
    /// The source file's parent directory is prepended to the search paths
    /// so that copybooks relative to the including file are found first.
    pub fn new(config: &PreprocessorConfig, source_file: &Path) -> Self {
        let mut search_paths = Vec::with_capacity(config.copy_paths.len() + 1);

        // The directory containing the source file gets highest priority.
        if let Some(parent) = source_file.parent() {
            search_paths.push(parent.to_path_buf());
        }

        for path in &config.copy_paths {
            // Avoid duplicating the source file's parent directory.
            if search_paths.iter().any(|sp| sp == path) {
                continue;
            }
            search_paths.push(path.clone());
        }

        Self { search_paths }
    }

    /// Returns the search paths used by this resolver.
    pub fn search_paths(&self) -> &[PathBuf] {
        &self.search_paths
    }

    /// Resolves a copybook name to a file path.
    ///
    /// If `library_name` is provided, searches within a subdirectory of
    /// each search path with that name.
    ///
    /// Tries multiple extensions for each candidate directory.
    pub fn resolve(&self, copybook_name: &str, library_name: Option<&str>) -> Option<PathBuf> {
        // First pass: search with the library name (as a subdirectory) if given,
        // or directly in each search path.
        for base_dir in &self.search_paths {
            let dir = if let Some(lib) = library_name {
                base_dir.join(lib)
            } else {
                base_dir.clone()
            };

            if let Some(path) = self.try_resolve_in_dir(&dir, copybook_name) {
                return Some(path);
            }
        }

        // Fallback: when a library name was specified but no subdirectory matched,
        // try resolving the copybook directly in each search path (ignoring the
        // library name). This handles environments where copybooks live in a flat
        // directory rather than being organised into library subdirectories (e.g.
        // NIST CCVS 85 COPYLIB).
        if library_name.is_some() {
            for base_dir in &self.search_paths {
                if let Some(path) = self.try_resolve_in_dir(base_dir, copybook_name) {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Tries to find a copybook file in a specific directory.
    fn try_resolve_in_dir(&self, dir: &Path, copybook_name: &str) -> Option<PathBuf> {
        // Try the name as given with each extension.
        for ext in EXTENSIONS {
            let filename = format!("{}{}", copybook_name, ext);
            let candidate = dir.join(&filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        // Try lowercase name with each extension.
        let lower_name = copybook_name.to_ascii_lowercase();
        if lower_name != copybook_name {
            for ext in EXTENSIONS {
                let filename = format!("{}{}", lower_name, ext);
                let candidate = dir.join(&filename);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        // Try uppercase name with each extension.
        let upper_name = copybook_name.to_ascii_uppercase();
        if upper_name != copybook_name && upper_name != lower_name {
            for ext in EXTENSIONS {
                let filename = format!("{}{}", upper_name, ext);
                let candidate = dir.join(&filename);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_resolve_with_cpy_extension() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();
        fs::write(dir.path().join("mybook.cpy"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("mybook", None);
        assert!(result.is_some(), "should find mybook.cpy");
    }

    #[test]
    fn test_resolve_with_cob_extension() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();
        fs::write(dir.path().join("mybook.cob"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("mybook", None);
        assert!(result.is_some(), "should find mybook.cob");
    }

    #[test]
    fn test_resolve_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();
        fs::write(dir.path().join("mybook"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("mybook", None);
        assert!(result.is_some(), "should find mybook (no extension)");
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();
        fs::write(dir.path().join("mybook.cpy"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        // Uppercase name should resolve to lowercase file.
        let result = resolver.resolve("MYBOOK", None);
        assert!(result.is_some(), "should find mybook.cpy via case fallback");
    }

    #[test]
    fn test_resolve_in_library() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();

        let lib_dir = dir.path().join("mylib");
        fs::create_dir(&lib_dir).unwrap();
        fs::write(lib_dir.join("libbook.cpy"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("libbook", Some("mylib"));
        assert!(result.is_some(), "should find libbook in mylib");
    }

    #[test]
    fn test_resolve_library_fallback_to_flat() {
        // When a library name is specified but no matching subdirectory exists,
        // the resolver should fall back to searching the flat directory.
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();

        // Copybook lives directly in the search path (no "mylib" subdirectory).
        fs::write(dir.path().join("ALTLB.cpy"), "content").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("ALTLB", Some("mylib"));
        assert!(
            result.is_some(),
            "should find ALTLB.cpy via flat-directory fallback"
        );
    }

    #[test]
    fn test_resolve_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let source_file = dir.path().join("test.cob");
        fs::write(&source_file, "").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("nonexistent", None);
        assert!(result.is_none(), "should not find nonexistent copybook");
    }

    #[test]
    fn test_source_dir_has_priority() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        let source_file = dir1.path().join("test.cob");
        fs::write(&source_file, "").unwrap();

        // Same copybook name in both directories with different content.
        fs::write(dir1.path().join("shared.cpy"), "from-dir1").unwrap();
        fs::write(dir2.path().join("shared.cpy"), "from-dir2").unwrap();

        let config = PreprocessorConfig {
            copy_paths: vec![dir2.path().to_path_buf()],
            ..Default::default()
        };
        let resolver = CopyResolver::new(&config, &source_file);
        let result = resolver.resolve("shared", None).unwrap();

        // Source file's directory should win.
        assert!(
            result.starts_with(dir1.path()),
            "source dir should have priority, got: {}",
            result.display()
        );
    }
}
