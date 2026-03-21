// COBOL Compiler - Preprocessor for COPY and REPLACE statements
//
// Runs before lexing to expand COPY statements (copybook inclusion) and
// apply REPLACE text substitutions. Handles nested COPY with circular
// dependency detection.

mod copy_resolver;
mod replacer;
mod scanner;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cobol_common::SourceFormat;
use cobol_diagnostics::{Diagnostic, DiagnosticReporter};

pub use copy_resolver::CopyResolver;

/// Result of preprocessing a COBOL source file.
#[derive(Debug)]
pub struct PreprocessedSource {
    /// The fully expanded source text (COPY inlined, REPLACE applied).
    pub source: String,
    /// Diagnostics emitted during preprocessing (missing copybooks, circular COPY, etc.).
    pub diagnostics: Vec<Diagnostic>,
    /// The effective source format after preprocessing. When the input is
    /// fixed format, the preprocessor strips columns 1-7 and 73-80, converting
    /// the text to free format for correct COPY/REPLACE handling.
    pub effective_source_format: SourceFormat,
}

/// Configuration for the preprocessor.
#[derive(Debug, Clone)]
pub struct PreprocessorConfig {
    /// Directories to search for copybook files, in order.
    pub copy_paths: Vec<PathBuf>,
    /// Maximum nesting depth for COPY statements (to guard against deep recursion).
    pub max_copy_depth: usize,
    /// Source format (Fixed, Free, Variable). In Fixed format, columns 73-80
    /// (the identification area) are stripped before scanning for COPY/REPLACE.
    pub source_format: SourceFormat,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self {
            copy_paths: vec![
                PathBuf::from("."),
                PathBuf::from("./copybooks"),
                PathBuf::from("./copy"),
            ],
            max_copy_depth: 64,
            source_format: SourceFormat::Free,
        }
    }
}

/// Preprocesses a COBOL source file, expanding COPY statements and applying
/// REPLACE directives.
///
/// # Arguments
/// * `source` - The raw COBOL source text.
/// * `file_path` - Path to the source file (used as base for relative copybook lookup).
/// * `config` - Preprocessor configuration (copy paths, depth limit, etc.).
///
/// # Returns
/// A `PreprocessedSource` containing the expanded text and any diagnostics.
pub fn preprocess(
    source: &str,
    file_path: &Path,
    config: &PreprocessorConfig,
) -> PreprocessedSource {
    let mut reporter = DiagnosticReporter::new();
    let mut include_stack: HashSet<PathBuf> = HashSet::new();

    // Add the source file itself to the include stack to detect self-inclusion.
    if let Ok(canonical) = file_path.canonicalize() {
        include_stack.insert(canonical);
    } else {
        include_stack.insert(file_path.to_path_buf());
    }

    // Strip identification area (columns 73-80) for fixed-format sources.
    // This prevents text like "SM2064.2" in columns 73-80 from corrupting
    // copybook names (e.g., "COPY K5SDA" + "SM2064.2" → "K5SDASM2064").
    let source = if config.source_format == SourceFormat::Fixed {
        strip_fixed_format_columns(source)
    } else {
        source.to_string()
    };

    let resolver = CopyResolver::new(config, file_path);

    // Phase 1: Expand all COPY statements (recursively).
    let expanded = expand_copy(
        &source,
        &resolver,
        &mut include_stack,
        &mut reporter,
        0,
        config,
    );

    // Phase 2: Apply REPLACE directives.
    let fixed = config.source_format == SourceFormat::Fixed;
    let replaced = replacer::apply_replace(&expanded, &mut reporter, fixed);

    PreprocessedSource {
        source: replaced,
        diagnostics: reporter.take_diagnostics(),
        effective_source_format: config.source_format,
    }
}

/// Strips columns 73-80 (the identification area) from each line of fixed-format
/// COBOL source. Lines shorter than 73 characters are left unchanged.
/// This prevents the identification area from corrupting copybook names and other
/// tokens that the COPY/REPLACE scanner needs to parse.
fn strip_fixed_format_columns(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    for line in source.split('\n') {
        let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
        if line_no_cr.len() > 72 {
            result.push_str(&line_no_cr[..72]);
        } else {
            result.push_str(line_no_cr);
        }
        result.push('\n');
    }
    // Remove the trailing newline we added if source didn't end with one
    if !source.ends_with('\n') && !source.ends_with("\r\n") {
        result.pop(); // remove trailing '\n'
    }
    result
}

/// Recursively expands COPY statements in the given source text.
fn expand_copy(
    source: &str,
    resolver: &CopyResolver,
    include_stack: &mut HashSet<PathBuf>,
    reporter: &mut DiagnosticReporter,
    depth: usize,
    config: &PreprocessorConfig,
) -> String {
    if depth > config.max_copy_depth {
        reporter.report(Diagnostic::error(
            "PP001",
            format!(
                "COPY nesting depth exceeds maximum of {}",
                config.max_copy_depth
            ),
        ));
        return source.to_string();
    }

    let fixed = config.source_format == SourceFormat::Fixed;
    let copy_stmts = scanner::scan_copy_statements(source, fixed);

    if copy_stmts.is_empty() {
        return source.to_string();
    }

    let mut result = String::with_capacity(source.len());
    let mut last_end = 0;

    for stmt in &copy_stmts {
        // Append text before this COPY statement.
        result.push_str(&source[last_end..stmt.start]);

        // Resolve the copybook file.
        match resolver.resolve(&stmt.copybook_name, stmt.library_name.as_deref()) {
            Some(path) => {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

                if include_stack.contains(&canonical) {
                    reporter.report(
                        Diagnostic::error(
                            "PP002",
                            format!("circular COPY detected: '{}'", canonical.display()),
                        )
                        .with_note(format!(
                            "the copybook '{}' is already being included in the current chain",
                            stmt.copybook_name
                        )),
                    );
                } else {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            // Strip identification area from copybook
                            // content if in fixed format.
                            let content =
                                if config.source_format == SourceFormat::Fixed {
                                    strip_fixed_format_columns(
                                        &content,
                                    )
                                } else {
                                    content
                                };

                            // Apply REPLACING if specified.
                            let content = if stmt.replacings.is_empty() {
                                content
                            } else {
                                replacer::apply_replacing(&content, &stmt.replacings)
                            };

                            // Recursively expand nested COPY statements.
                            include_stack.insert(canonical.clone());
                            let nested = expand_copy(
                                &content,
                                resolver,
                                include_stack,
                                reporter,
                                depth + 1,
                                config,
                            );
                            include_stack.remove(&canonical);

                            result.push_str(&nested);
                        }
                        Err(e) => {
                            reporter.report(Diagnostic::error(
                                "PP003",
                                format!("cannot read copybook '{}': {}", path.display(), e),
                            ));
                        }
                    }
                }
            }
            None => {
                reporter.report(
                    Diagnostic::error(
                        "PP004",
                        format!("copybook '{}' not found", stmt.copybook_name),
                    )
                    .with_note(format!(
                        "searched in: {}",
                        resolver
                            .search_paths()
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
            }
        }

        last_end = stmt.end;
    }

    // Append any remaining text after the last COPY statement.
    result.push_str(&source[last_end..]);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn test_no_copy_passthrough() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = "IDENTIFICATION DIVISION.\nPROGRAM-ID. TEST1.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert_eq!(result.source, source);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_basic_copy_expansion() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        // Create a copybook file.
        let copybook_content = "       01 WS-NAME PIC X(20).\n";
        fs::write(dir.path().join("mybook.cpy"), copybook_content).unwrap();

        let source = "DATA DIVISION.\nWORKING-STORAGE SECTION.\nCOPY mybook.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("01 WS-NAME PIC X(20)."),
            "expanded source: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_copy_with_replacing() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let copybook_content = "       01 :PREFIX:-NAME PIC X(20).\n";
        fs::write(dir.path().join("mybook.cpy"), copybook_content).unwrap();

        let source = "COPY mybook REPLACING ==:PREFIX:== BY ==WS==.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("WS-NAME"),
            "expanded source: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_copy_with_word_replacing() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let copybook_content = "       01 OLD-NAME PIC X(20).\n";
        fs::write(dir.path().join("mybook.cpy"), copybook_content).unwrap();

        let source = "COPY mybook REPLACING OLD-NAME BY NEW-NAME.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("NEW-NAME"),
            "expanded source: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_nested_copy() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        // inner.cpy contains a data item.
        fs::write(
            dir.path().join("inner.cpy"),
            "       05 INNER-FIELD PIC 9.\n",
        )
        .unwrap();

        // outer.cpy copies inner.cpy.
        fs::write(
            dir.path().join("outer.cpy"),
            "       01 OUTER-REC.\nCOPY inner.\n",
        )
        .unwrap();

        let source = "COPY outer.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("OUTER-REC"),
            "missing outer content: {:?}",
            result.source
        );
        assert!(
            result.source.contains("INNER-FIELD"),
            "missing inner content: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_circular_copy_detection() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        // a.cpy copies b.cpy, and b.cpy copies a.cpy -> circular.
        fs::write(dir.path().join("a.cpy"), "COPY b.\n").unwrap();
        fs::write(dir.path().join("b.cpy"), "COPY a.\n").unwrap();

        let source = "COPY a.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "PP002" && d.message.contains("circular")),
            "expected circular COPY error, got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_missing_copybook_error() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = "COPY nonexistent.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.code == "PP004" && d.message.contains("not found")),
            "expected missing copybook error, got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn test_replace_statement() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = concat!(
            "REPLACE ==TEMP-NAME== BY ==REAL-NAME==.\n",
            "       01 TEMP-NAME PIC X(10).\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("REAL-NAME"),
            "REPLACE not applied: {:?}",
            result.source
        );
    }

    #[test]
    fn test_replace_off() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = concat!(
            "REPLACE ==OLD== BY ==NEW==.\n",
            "       01 OLD PIC X.\n",
            "REPLACE OFF.\n",
            "       01 OLD PIC 9.\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        // The first OLD should be replaced, the second should remain.
        let lines: Vec<&str> = result.source.lines().collect();
        let data_lines: Vec<&&str> = lines.iter().filter(|l| l.contains("PIC")).collect();
        assert!(
            data_lines.len() >= 2,
            "expected at least 2 data lines, got: {:?}",
            data_lines
        );
        assert!(
            data_lines[0].contains("NEW"),
            "first line should have NEW: {:?}",
            data_lines[0]
        );
        assert!(
            data_lines[1].contains("OLD"),
            "second line should have OLD: {:?}",
            data_lines[1]
        );
    }

    #[test]
    fn test_copy_of_library() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        // Create a library subdirectory.
        let lib_dir = dir.path().join("mylib");
        fs::create_dir(&lib_dir).unwrap();
        fs::write(lib_dir.join("libbook.cpy"), "       01 LIB-FIELD PIC X.\n").unwrap();

        let source = "COPY libbook OF mylib.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("LIB-FIELD"),
            "library COPY failed: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_copy_case_insensitive() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        fs::write(dir.path().join("mybook.cpy"), "       01 FIELD-A PIC X.\n").unwrap();

        // COPY keyword in lowercase.
        let source = "copy MYBOOK.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("FIELD-A"),
            "case-insensitive COPY failed: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn test_multiple_replacings() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let copybook_content = "       01 :PFX:-REC.\n          05 :PFX:-FIELD PIC :TYP:.\n";
        fs::write(dir.path().join("multi.cpy"), copybook_content).unwrap();

        let source = "COPY multi REPLACING ==:PFX:== BY ==WS== ==:TYP:== BY ==X(10)==.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("WS-REC"),
            "first replacing failed: {:?}",
            result.source
        );
        assert!(
            result.source.contains("X(10)"),
            "second replacing failed: {:?}",
            result.source
        );
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }
}
