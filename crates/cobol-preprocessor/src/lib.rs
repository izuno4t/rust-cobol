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

use cobol_common::{FileId, SourceFormat, Span};
use cobol_diagnostics::{Diagnostic, DiagnosticReporter};

pub use copy_resolver::CopyResolver;

/// Result of preprocessing a COBOL source file.
#[derive(Debug)]
pub struct PreprocessedSource {
    /// The fully expanded source text (COPY inlined, REPLACE applied).
    pub source: String,
    /// Diagnostics emitted during preprocessing (missing copybooks, circular COPY, etc.).
    pub diagnostics: Vec<Diagnostic>,
    /// The effective source format after preprocessing.
    /// Fixed-format input is normalized into logical source lines so that
    /// downstream stages do not interpret fixed-format continuation twice.
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
        let source = strip_fixed_format_columns(source);
        filter_inactive_fixed_debug_lines(&source)
    } else {
        source.to_string()
    };

    report_nonconforming_source_manipulation_warnings(&source, &mut reporter, config.source_format);

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
    let (replaced, effective_source_format) = if fixed {
        let reflowed = reflow_fixed_format_source(&replaced);
        (normalize_fixed_format_source(&reflowed), SourceFormat::Free)
    } else {
        (replaced, config.source_format)
    };

    PreprocessedSource {
        source: replaced,
        diagnostics: reporter.take_diagnostics(),
        effective_source_format,
    }
}

fn report_nonconforming_source_manipulation_warnings(
    source: &str,
    reporter: &mut DiagnosticReporter,
    source_format: SourceFormat,
) {
    let fixed = source_format == SourceFormat::Fixed;

    for stmt in scanner::scan_copy_statements(source, fixed) {
        if stmt.replacings.is_empty() {
            continue;
        }
        let span = Span::new(stmt.start as u32, (stmt.start + 4) as u32, FileId(0));
        reporter.report(
            Diagnostic::warning(
                "COBC-W001",
                "COPY ... REPLACING is a non-conforming source text manipulation feature",
            )
            .with_span(span),
        );
    }

    for directive in scanner::scan_replace_directives(source, fixed) {
        let is_source_manipulation = directive.is_off || !directive.replacings.is_empty();
        if !is_source_manipulation {
            continue;
        }
        let span = Span::new(
            directive.start as u32,
            (directive.start + "REPLACE".len()) as u32,
            FileId(0),
        );
        reporter.report(
            Diagnostic::warning(
                "COBC-W001",
                "REPLACE is a non-conforming source text manipulation feature",
            )
            .with_span(span),
        );
    }

    for (line_idx, line) in source.lines().enumerate() {
        if line.to_ascii_uppercase().contains("SAME SORT-MERGE AREA") {
            let start = source
                .lines()
                .take(line_idx)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            let rel = line
                .to_ascii_uppercase()
                .find("SAME SORT-MERGE AREA")
                .unwrap_or(0);
            let span = Span::new(
                (start + rel) as u32,
                (start + rel + "SAME SORT-MERGE AREA".len()) as u32,
                FileId(0),
            );
            reporter.report(
                Diagnostic::warning(
                    "COBC-W001",
                    "SAME SORT-MERGE AREA is a non-conforming sort/merge feature",
                )
                .with_span(span),
            );
        }
    }
}

fn reflow_fixed_format_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len());

    for line in source.split('\n') {
        let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
        let has_fixed_prefix = line_no_cr.len() >= 7
            && (line_no_cr.as_bytes()[..6].iter().all(u8::is_ascii_digit)
                || line_no_cr.as_bytes()[..6].iter().all(|b| *b == b' '));
        let (indicator, content) = if has_fixed_prefix {
            let indicator = line_no_cr.as_bytes()[6] as char;
            if let Some(indicator) = normalize_fixed_indicator(indicator) {
                (indicator, &line_no_cr[7..])
            } else {
                ('*', line_no_cr)
            }
        } else if line_no_cr.trim().is_empty() {
            (' ', "")
        } else {
            ('*', line_no_cr)
        };

        if indicator == '*' || indicator == '/' {
            out.push_str("      ");
            out.push(indicator);
            out.push_str(content);
            out.push('\n');
            continue;
        }

        let mut remaining = content.to_string();
        let mut first = true;
        while !remaining.is_empty() {
            let limit = if first { 65 } else { 64 };
            let final_chunk = remaining.len() <= limit;
            let mut take = if final_chunk {
                remaining.len()
            } else {
                choose_fixed_split(&remaining, limit).unwrap_or(limit)
            };
            let split_quote = if take < remaining.len() {
                quote_char_at_split(&remaining, take)
            } else {
                None
            };
            if split_quote.is_some() && take == limit {
                take -= 1;
            }

            let chunk = &remaining[..take];
            out.push_str("      ");
            out.push(if first { indicator } else { '-' });
            if final_chunk && !first {
                out.push_str(&close_continued_quote_run(chunk));
            } else {
                out.push_str(chunk);
            }
            if let Some(quote) = split_quote {
                out.push(quote);
            }
            out.push('\n');
            let mut rest = remaining[take..].to_string();
            if let Some(quote) = split_quote {
                rest.insert(0, quote);
            }
            remaining = rest;
            first = false;
        }
        if content.is_empty() {
            out.push_str("      ");
            out.push(indicator);
            out.push('\n');
        }
    }

    if !source.ends_with('\n') {
        out.pop();
    }
    out
}

fn close_continued_quote_run(chunk: &str) -> String {
    let bytes = chunk.as_bytes();
    let Some(&quote) = bytes.first() else {
        return chunk.to_string();
    };
    if quote != b'"' && quote != b'\'' {
        return chunk.to_string();
    }

    let run_len = bytes.iter().take_while(|&&b| b == quote).count();
    if run_len == 0 || run_len >= bytes.len() {
        return chunk.to_string();
    }
    if !bytes[run_len].is_ascii_whitespace() || (run_len - 1) % 2 == 1 {
        return chunk.to_string();
    }

    let mut out = String::with_capacity(chunk.len() + 1);
    out.push_str(&chunk[..run_len]);
    out.push(quote as char);
    out.push_str(&chunk[run_len..]);
    out
}

fn normalize_fixed_format_source(source: &str) -> String {
    let mut normalized = String::with_capacity(source.len());
    let mut current = String::new();

    for line in source.split('\n') {
        let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
        let has_fixed_prefix = line_no_cr.len() >= 7
            && (line_no_cr.as_bytes()[..6].iter().all(u8::is_ascii_digit)
                || line_no_cr.as_bytes()[..6].iter().all(|b| *b == b' '));
        let (indicator, content) = if has_fixed_prefix {
            let indicator = line_no_cr.as_bytes()[6] as char;
            if let Some(indicator) = normalize_fixed_indicator(indicator) {
                (indicator, &line_no_cr[7..])
            } else {
                ('*', line_no_cr)
            }
        } else if line_no_cr.trim().is_empty() {
            (' ', "")
        } else {
            ('*', line_no_cr)
        };

        if indicator == '*' || indicator == '/' {
            flush_normalized_line(&mut normalized, &mut current);
            normalized.push_str("*>");
            normalized.push_str(content);
            normalized.push('\n');
            continue;
        }

        if indicator == '-' {
            append_fixed_continuation(&mut current, content);
            continue;
        }

        flush_normalized_line(&mut normalized, &mut current);
        current.push_str(content);
    }

    flush_normalized_line(&mut normalized, &mut current);
    if !source.ends_with('\n') {
        normalized.pop();
    }
    normalized
}

fn flush_normalized_line(out: &mut String, current: &mut String) {
    if current.is_empty() {
        return;
    }
    out.push_str(current);
    out.push('\n');
    current.clear();
}

fn append_fixed_continuation(current: &mut String, continuation: &str) {
    let cont_bytes = continuation.as_bytes();
    let mut skip = 0;
    while skip < cont_bytes.len() && cont_bytes[skip] == b' ' {
        skip += 1;
    }

    let first_non_space = cont_bytes.get(skip).copied();
    let is_string_continuation = matches!(first_non_space, Some(b'"' | b'\''));
    if is_string_continuation {
        let quote = first_non_space.unwrap();
        skip += 1;

        let trimmed_len = current.trim_end().len();
        let trailing_spaces = current.len().saturating_sub(trimmed_len);

        let mut dropped_prev_quote = false;
        if trimmed_len > 0
            && current.as_bytes().get(trimmed_len - 1) == Some(&quote)
            && ends_inside_string_with_quote(&current[..trimmed_len - 1], quote)
        {
            current.truncate(trimmed_len);
            current.pop();
            dropped_prev_quote = true;
        }
        if !dropped_prev_quote {
            current.truncate(trimmed_len);
            if trailing_spaces == 1 {
                current.push(' ');
            }
        }

        let continued_quote_run = cont_bytes[skip..]
            .iter()
            .take_while(|&&b| b == quote)
            .count();
        if dropped_prev_quote
            && cont_bytes.get(skip).copied() == Some(quote)
            && continued_quote_run == 1
        {
            current.push(quote as char);
        }

        current.push_str(&continuation[skip..]);
        return;
    }

    let trimmed_len = current.trim_end().len();
    current.truncate(trimmed_len);

    let cont_trimmed = continuation.trim_start();
    let leading_spaces = continuation.len() - cont_trimmed.len();
    if leading_spaces == 1 && !current.is_empty() && !cont_trimmed.is_empty() {
        current.push(' ');
    }
    current.push_str(cont_trimmed);
}

fn choose_fixed_split(text: &str, limit: usize) -> Option<usize> {
    let mut in_string = false;
    let mut quote = b'\0';
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut last_space_outside = None;

    while i < bytes.len() && i < limit {
        let b = bytes[i];
        if in_string {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    i += 2;
                    continue;
                }
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
        } else if b == b' ' {
            last_space_outside = Some(i);
        }
        i += 1;
    }

    if !in_string {
        return last_space_outside.filter(|idx| *idx > 0).or(Some(limit));
    }

    let mut split = limit.min(text.len());
    while split > 0 {
        if ends_inside_string_with_quote(&text[..split], quote) {
            break;
        }
        split -= 1;
    }
    Some(split.max(1))
}

fn normalize_fixed_indicator(ch: char) -> Option<char> {
    match ch {
        'T' | 't' => Some(' '),
        'U' | 'u' => Some('*'),
        ' ' | '*' | '/' | '-' | 'D' | 'd' | '$' => Some(ch),
        _ => None,
    }
}

fn filter_inactive_fixed_debug_lines(source: &str) -> String {
    if fixed_source_has_debugging_mode(source) {
        return source.to_string();
    }

    let mut out = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let line_no_newline = line.trim_end_matches('\n').trim_end_matches('\r');
        let indicator = line_no_newline.as_bytes().get(6).copied().map(char::from);
        if matches!(indicator, Some('D' | 'd' | 'U' | 'u')) {
            continue;
        }
        out.push_str(line);
    }
    out
}

fn fixed_source_has_debugging_mode(source: &str) -> bool {
    let configuration_text = source
        .split('\n')
        .filter_map(|line| {
            let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
            let indicator = line_no_cr.as_bytes().get(6).copied().map(char::from);
            if matches!(indicator, Some('*' | '/')) {
                None
            } else {
                Some(line_no_cr.get(7..).unwrap_or("").to_ascii_uppercase())
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let Some(source_start) = configuration_text.find("SOURCE-COMPUTER") else {
        return false;
    };
    let source_tail = &configuration_text[source_start..];
    let source_end = [
        "OBJECT-COMPUTER",
        "SPECIAL-NAMES",
        "INPUT-OUTPUT",
        "DATA DIVISION",
        "PROCEDURE DIVISION",
    ]
    .iter()
    .filter_map(|marker| source_tail.find(marker))
    .filter(|idx| *idx > 0)
    .min()
    .unwrap_or(source_tail.len());
    source_tail[..source_end].contains("WITH DEBUGGING MODE")
}

fn ends_inside_string_with_quote(text: &str, quote: u8) -> bool {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut in_string = false;

    while pos < bytes.len() {
        if bytes[pos] != quote {
            pos += 1;
            continue;
        }

        if in_string {
            if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                pos += 2;
            } else {
                in_string = false;
                pos += 1;
            }
        } else {
            in_string = true;
            pos += 1;
        }
    }

    in_string
}

fn quote_char_at_split(text: &str, split: usize) -> Option<char> {
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut quote = b'\0';
    let mut i = 0;

    while i < bytes.len() && i < split {
        let b = bytes[i];
        if in_string {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    i += 2;
                    continue;
                }
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b;
        }
        i += 1;
    }

    if in_string {
        Some(quote as char)
    } else {
        None
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
        if is_known_nist_trailer_artifact(line_no_cr) {
            // Some extracted NIST fixed-format sources contain a duplicated
            // trailer line after several blank lines. It is not COBOL code
            // and must not reach the lexer/parser.
        } else if line_no_cr.len() > 72 {
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

fn is_known_nist_trailer_artifact(line: &str) -> bool {
    if !line.starts_with("                        ") {
        return false;
    }
    let tail = &line[24..];
    if tail.len() < 14 {
        return false;
    }

    let bytes = tail.as_bytes();
    bytes
        .get(0..2)
        .is_some_and(|s| s.iter().all(u8::is_ascii_uppercase))
        && bytes
            .get(2..6)
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
        && bytes.get(6) == Some(&b'.')
        && bytes.get(7) == Some(&b'2')
        && bytes
            .get(8..14)
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
        && tail[14..].contains("TOTAL NUMBER OF FLAGS EXPECTED")
}

/// Re-wraps fixed-format lines that exceed column 72 after REPLACING.
///
/// When a REPLACING substitution makes a word longer, the resulting line
/// can overflow column 72. This function splits such lines into
/// continuation lines, preserving the fixed-format layout.
fn rewrap_fixed_format_lines(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    for line in source.split('\n') {
        let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
        if line_no_cr.len() <= 72 {
            result.push_str(line_no_cr);
            result.push('\n');
            continue;
        }
        let mut remaining = line_no_cr;
        let mut first = true;
        while !remaining.is_empty() {
            let max_len = if first { 72 } else { 65 };
            if remaining.len() <= max_len {
                if first {
                    result.push_str(remaining);
                } else {
                    result.push_str("       ");
                    result.push_str(remaining);
                }
                result.push('\n');
                break;
            }

            let window = &remaining[..max_len];
            let min_split = if first { 7 } else { 0 };
            let split_at = window.rfind(' ').filter(|idx| *idx > min_split);

            if let Some(split_at) = split_at {
                if first {
                    result.push_str(&remaining[..split_at]);
                } else {
                    result.push_str("       ");
                    result.push_str(&remaining[..split_at]);
                }
                result.push('\n');
                remaining = remaining[split_at..].trim_start();
            } else if first {
                result.push_str(window);
                result.push('\n');
                remaining = remaining[max_len..].trim_start();
                if !remaining.is_empty() {
                    result.push_str("      -");
                    let take = remaining
                        .char_indices()
                        .nth(65)
                        .map(|(idx, _)| idx)
                        .unwrap_or(remaining.len());
                    result.push_str(&remaining[..take]);
                    result.push('\n');
                    remaining = remaining[take..].trim_start();
                }
                first = false;
                continue;
            } else {
                result.push_str("      -");
                result.push_str(window);
                result.push('\n');
                remaining = remaining[max_len..].trim_start();
            }

            first = false;
        }
    }
    if !source.ends_with('\n') && !source.ends_with("\r\n") && result.ends_with('\n') {
        result.pop();
    }
    result
}

/// Normalizes fixed-format copybook content before COPY expansion.
///
/// Copybooks are spliced into the including line at the COPY keyword position,
/// so their sequence area (columns 1-6) and indicator area (column 7) must be
/// removed. Identification area columns 73-80 are also stripped.
fn normalize_fixed_format_copybook(source: &str, first_line_inline: bool) -> String {
    let mut result = String::with_capacity(source.len());
    let mut lines: Vec<&str> = source.split('\n').collect();
    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }
    let last_idx = lines.len().saturating_sub(1);
    for (idx, line) in lines.iter().enumerate() {
        let line = *line;
        let line_no_cr = line.strip_suffix('\r').unwrap_or(line);
        let line_no_id = if line_no_cr.len() > 72 {
            &line_no_cr[..72]
        } else {
            line_no_cr
        };
        let indicator = if line_no_id.len() >= 7 {
            normalize_fixed_indicator(line_no_id.as_bytes()[6] as char).unwrap_or('*')
        } else {
            ' '
        };
        let content = if line_no_id.len() >= 7 {
            line_no_id[7..].trim_start_matches(' ')
        } else {
            ""
        };
        let inline_content = if first_line_inline && idx == 0 {
            content.trim_end_matches(' ')
        } else {
            content
        };
        if idx == 0 {
            if first_line_inline && indicator != '*' && indicator != '/' {
                if !inline_content.is_empty() {
                    result.push(' ');
                }
                result.push_str(inline_content);
            } else {
                // When the first line cannot be inlined (e.g., a comment
                // line), emit a newline to start a fresh fixed-format line.
                if first_line_inline {
                    result.push('\n');
                }
                result.push_str("      ");
                result.push(indicator);
                result.push_str(content);
            }
        } else {
            result.push_str("      ");
            result.push(indicator);
            result.push_str(content);
        }
        let should_preserve_inline_suffix = first_line_inline && idx == last_idx;
        if !should_preserve_inline_suffix {
            result.push('\n');
        }
    }
    if !source.ends_with('\n') && !source.ends_with("\r\n") {
        result.pop();
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
        let prefix = &source[last_end..stmt.start];
        let first_line_inline = if fixed {
            if let Some(line_start) = prefix.rfind('\n') {
                let inline = !prefix[line_start + 1..].trim_end_matches(' ').is_empty();
                result.push_str(&prefix[..line_start + 1]);
                result.push_str(prefix[line_start + 1..].trim_end_matches(' '));
                inline
            } else {
                let inline = !prefix.trim_end_matches(' ').is_empty();
                result.push_str(prefix.trim_end_matches(' '));
                inline
            }
        } else {
            result.push_str(prefix);
            false
        };

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
                            let content = if config.source_format == SourceFormat::Fixed {
                                strip_fixed_format_columns(&content)
                            } else {
                                content
                            };

                            // Apply REPLACING before fixed-format normalization so
                            // pseudo-text matching can still see continuation lines.
                            let content = if stmt.replacings.is_empty() {
                                content
                            } else {
                                let replaced =
                                    replacer::apply_replacing(&content, &stmt.replacings);
                                // Re-wrap lines that became too long after replacement.
                                if config.source_format == SourceFormat::Fixed {
                                    rewrap_fixed_format_lines(&replaced)
                                } else {
                                    replaced
                                }
                            };

                            // Normalize fixed-format copybooks before inlining.
                            // Keeping columns 1-7 would leak copybook sequence
                            // numbers into the caller's content area.
                            let content = if config.source_format == SourceFormat::Fixed {
                                normalize_fixed_format_copybook(&content, first_line_inline)
                            } else {
                                content
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

    fn assert_no_errors(result: &PreprocessedSource) {
        assert!(
            result.diagnostics.iter().all(|diag| !diag.is_error()),
            "{:?}",
            result.diagnostics
        );
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
        assert_no_errors(&result);
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
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_ccvs_t_lines_active_and_u_lines_inactive() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = "000100 IDENTIFICATION DIVISION.\n\
000200 PROGRAM-ID. TEST.\n\
000300 DATA DIVISION.\n\
000400 WORKING-STORAGE SECTION.\n\
000500T01 ACTIVE-ITEM PIC X.\n\
000600U01 INACTIVE-ITEM PIC X.\n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        assert!(
            result.source.contains("01 ACTIVE-ITEM PIC X."),
            "preprocessed source: {:?}",
            result.source
        );
        assert!(
            !result.source.contains("INACTIVE-ITEM"),
            "preprocessed source: {:?}",
            result.source
        );
        assert_no_errors(&result);
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
        assert_no_errors(&result);
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
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_replacing_keeps_long_identifier_across_continuations() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let copybook_content = concat!(
            "000100     MOVE   +00009 TO WRK-DS-05V00-O005-001  IN WRK-XN-00050-O005FKP0024.2\n",
            "000200-            -001 OF GRP-006 OF GRP-004 IN GRP-003 ( 2 ).         KP0024.2\n",
            "000300     ADD                                                          KP0024.2\n",
            "000400         +00001 TO                                                KP0024.2\n",
            "000500                   WRK-DS-09V00-901                               KP0024.2\n",
            "000600                                   SUBTRACT                       KP0024.2\n",
            "000700                                            1                     KP0024.2\n",
            "000800                                             FROM                 KP0024.2\n",
            "000900                  WRK-DS-05V00-O005-001 IN GRP-002 (1).           KP0024.2\n",
        );
        fs::write(dir.path().join("kp002.cpy"), copybook_content).unwrap();

        let source = concat!(
            "036100     COPY                                                    KP002\n",
            "036200             REPLACING == WRK-DS-09V00-901\n",
            "036300                          SUBTRACT 1 FROM\n",
            "036400                          WRK-DS-05V00-O005-001 IN GRP-002 (1)==\n",
            "036500             BY         WRK-DS-05V00-O005-001 IN WRK-XN-00050-O005\n",
            "036600-                  F-001 IN GRP-006 IN GRP-004 IN GRP-002 IN GRP-0\n",
            "036700-                      01 (1).\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };
        let result = preprocess(source, &source_path, &config);
        let normalized = result
            .source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            normalized.contains(
                "WRK-DS-05V00-O005-001 IN WRK-XN-00050-O005F-001 IN GRP-006 IN GRP-004 IN GRP-002 IN GRP-001 (1)."
            ),
            "expanded source: {:?}",
            result.source
        );
        assert_no_errors(&result);
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
        assert_no_errors(&result);
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
        assert_no_errors(&result);
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
        assert_no_errors(&result);
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
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_replace_handles_quote_heavy_pseudo_text_from_sm208a() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        let source = concat!(
            "036100 REPLACE   ==\"Z\"== BY                          ==\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036200-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036300-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036400-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036500-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036600-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
            "036700-    \"\"\"\"\"\"==.\n",
            "036800     MOVE \"Z\" TO WRK-XN-00322.\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);
        assert!(
            !result.source.contains("REPLACE"),
            "REPLACE directive should be consumed: {:?}",
            result.source
        );
        assert!(
            result.source.contains("MOVE"),
            "source after replacement should keep the following statement: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_replacing_handles_multiline_pseudo_text_from_sm206a() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();

        fs::write(
            dir.path().join("kp004.cpy"),
            concat!(
                "000100*    THIS COMMENT IS THE FIRST IMAGE IN KP004                     KP0044.2\n",
                "000200*    ADD  1 TO THE LIST.                                          KP0044.2\n",
                "000300 PST-INIT-004.                                                    KP0044.2\n",
                "000400     MOVE \"PSEUDO-TEXT/WORD\" TO FEATURE.                          KP0044.2\n",
                "000500     MOVE    ZERO TO WRK-DS-09V00-901.                            KP0044.2\n",
                "000600     MOVE    \"PST-TEST-004\" TO PAR-NAME.                          KP0044.2\n",
                "000700 PST-TEST-004.                                                    KP0044.2\n",
                "000800     ADD     5 TO WRK-DS-09V00-901.                               KP0044.2\n",
                "000900     THIS IS NOT REAL COBOL-74 SYNTAX HOWEVER                     KP0044.2\n",
                "001000             SHOVE +2 TO WRK-DS-09V00-902.                        KP0044.2\n",
                "001100     GO TO   PST-EXIT-004.                                        KP0044.2\n",
                "001200 PST-DELETE-004.                                                  KP0044.2\n",
                "001300     PERFORM DELETE.                                              KP0044.2\n",
                "001400 PST-EXIT-004.                                                    KP0044.2\n",
                "001500     EXIT.                                                        KP0044.2\n",
            ),
        )
        .unwrap();

        let source = concat!(
            "047100             COPY                                        KP004    \n",
            "047200                 REPLACING ==THIS IS NOT REAL COBOL-74 SYNTAX HOWE\n",
            "047300-                VER SHOVE==                                      \n",
            "047400                 BY MOVE                                           \n",
            "047500                    SPACE TO RE-MARK                              \n",
            "047600                    GO TO PAR-17                                  \n",
            "047700                    DE-LETE.                                      \n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);
        assert!(
            !result.source.contains("COPY"),
            "COPY statement should be expanded: {:?}",
            result.source
        );
        assert!(
            result.source.contains("MOVE"),
            "replacement text should be inserted into copybook content: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_expands_sm101a_section_copy() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("sm101a.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(
            dir.path().join("K1SEA.cpy"),
            concat!(
                "000100 SECT-COPY-1.                                                     K1SEA4.2\n",
                "000200     MOVE     95427 TO COPYSECT-1.                                K1SEA4.2\n",
                "000300 SECT-COPY-2.                                                     K1SEA4.2\n",
                "000400     MOVE     23121 TO COPYSECT-2.                                K1SEA4.2\n",
            ),
        )
        .unwrap();

        let source = concat!(
            "045500                                                       COPY K1SEA.SM1014.2\n",
            "045600D                                                      COPY K1SEA.SM1014.2\n",
            "045700     IF       COPYSECT-1 EQUAL TO 95427                           SM1014.2\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);

        assert!(
            !result.source.contains("COPY K1SEA"),
            "section COPY should be expanded from fixed-format source: {:?}",
            result.source
        );
        assert!(
            result.source.contains("SECT-COPY-1."),
            "expanded copybook section should be present: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_expands_sm101a_inline_data_item_copy() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("sm101a.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(
            dir.path().join("K1W02.cpy"),
            concat!(
                "000100     RCD-4    PIC 9(5) VALUE 02734.                               K1W024.2\n",
                "000200 77  RCD-5    PICTURE IS 99999 VALUE IS                           K1W024.2\n",
            ),
        )
        .unwrap();

        let source = "008900 77  COPY K1W02.                                                  \n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);

        assert!(
            !result.source.contains("COPY K1W02"),
            "inline data-item COPY should be expanded: {:?}",
            result.source
        );
        assert!(
            result.source.contains("RCD-4") && result.source.contains("RCD-5"),
            "expanded inline data-item copy should be present: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_expands_sm101a_inline_statement_copy() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("sm101a.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(
            dir.path().join("K1P01.cpy"),
            "000100          RCD-1                                                   K1P014.2\n",
        )
        .unwrap();

        let source = "055000     ADD     COPY K1P01. TO WRK-DS-05V00.                         \n";
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);

        assert!(
            !result.source.contains("COPY K1P01"),
            "inline statement COPY should be expanded: {:?}",
            result.source
        );
        assert!(
            result.source.contains("ADD") && result.source.contains("RCD-1"),
            "expanded inline statement copy should keep surrounding statement: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_fixed_copy_expands_sm101a_inline_statement_copy_in_full_context() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("sm101a.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(
            dir.path().join("K1SEA.cpy"),
            concat!(
                "000100 SECT-COPY-1.                                                     K1SEA4.2\n",
                "000200     MOVE     95427 TO COPYSECT-1.                                K1SEA4.2\n",
                "000300 SECT-COPY-2.                                                     K1SEA4.2\n",
                "000400     MOVE     23121 TO COPYSECT-2.                                K1SEA4.2\n",
                "000500 SECT-COPY-3.                                                     K1SEA4.2\n",
                "000600     MOVE     \"LIBCO\" TO COPYSECT-3.                              K1SEA4.2\n",
                "000700 SECT-COPY-4.                                                     K1SEA4.2\n",
                "000800     MOVE     \"PYTST\" TO COPYSECT-4.                              K1SEA4.2\n",
            ),
        )
        .unwrap();
        fs::write(
            dir.path().join("K1P01.cpy"),
            "000100          RCD-1                                                   K1P014.2\n",
        )
        .unwrap();

        let source = concat!(
            "010100 77  COPYSECT-1 PICTURE 9(5) VALUE 72459.                         \n",
            "010200 77  COPYSECT-2 PICTURE 9(5) VALUE 12132.                         \n",
            "010300 77  COPYSECT-3 PICTURE X(5) VALUE \"TSTLI\".                       \n",
            "010400 77  COPYSECT-4 PICTURE X(5) VALUE \"BCOPY\".                       \n",
            "045500                                                       COPY K1SEA.\n",
            "045600D                                                      COPY K1SEA.\n",
            "046100     IF       COPYSECT-1 EQUAL TO 95427                           \n",
            "046200             PERFORM PASS                                         \n",
            "054700*    ADD     COPY K1P01. TO WRK-DS-05V00.                         \n",
            "055000     ADD     COPY K1P01. TO WRK-DS-05V00.                         \n",
            "055200     IF       WRK-DS-05V00 EQUAL TO 97523                         \n",
            "056900     GO TO CLOSE-FILES.                                           \n",
            "000100 IDENTIFICATION DIVISION.                                         \n",
            "000200 PROGRAM-ID.                                                      \n",
            "000300     SM102A.                                                      \n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            source_format: SourceFormat::Fixed,
            ..Default::default()
        };

        let result = preprocess(source, &source_path, &config);

        assert!(
            !result
                .source
                .lines()
                .filter(|line| !line.trim_start().starts_with("*>"))
                .any(|line| line.contains("COPY K1P01")),
            "full-context inline statement COPY should be expanded in active code: {:?}",
            result.source
        );
        assert!(
            result.source.contains("ADD     RCD-1 TO WRK-DS-05V00.")
                || result.source.contains("ADD RCD-1 TO WRK-DS-05V00."),
            "expanded inline statement copy should keep the surrounding statement: {:?}",
            result.source
        );
        assert_no_errors(&result);
    }

    #[test]
    fn test_normalize_fixed_format_copybook_keeps_single_inline_line_open() {
        let source =
            "000100          RCD-1                                                   K1P014.2\n";
        let stripped = strip_fixed_format_columns(source);
        let normalized = normalize_fixed_format_copybook(&stripped, true);
        assert_eq!(normalized, " RCD-1");
    }

    #[test]
    fn test_preprocess_warns_for_copy_replacing_and_replace_off() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(dir.path().join("book.cpy"), "       DISPLAY \"X\".\n").unwrap();

        let source = concat!(
            "       IDENTIFICATION DIVISION.\n",
            "       PROGRAM-ID. TEST.\n",
            "       PROCEDURE DIVISION.\n",
            "           COPY book REPLACING ==X== BY ==Y==.\n",
            "           REPLACE OFF.\n",
            "           STOP RUN.\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            max_copy_depth: 8,
            source_format: SourceFormat::Free,
        };

        let result = preprocess(source, &source_path, &config);
        let warning_count = result
            .diagnostics
            .iter()
            .filter(|diag| diag.code == "COBC-W001")
            .count();

        assert_eq!(warning_count, 2, "{:?}", result.diagnostics);
        assert_no_errors(&result);
    }

    #[test]
    fn test_preprocess_warns_for_fixed_copy_replacing_and_replace_off() {
        let dir = setup_test_dir();
        let source_path = dir.path().join("test.cob");
        fs::write(&source_path, "").unwrap();
        fs::write(
            dir.path().join("ksm41.cpy"),
            "000100     DISPLAY \"COW\".\n",
        )
        .unwrap();

        let source = concat!(
            "000100 IDENTIFICATION DIVISION.\n",
            "000200 PROGRAM-ID. TEST.\n",
            "000300 PROCEDURE DIVISION.\n",
            "000400     COPY KSM41 REPLACING \"PIG\" BY \"HORSE\".\n",
            "000500     REPLACE OFF.\n",
            "000600     STOP RUN.\n",
        );
        let config = PreprocessorConfig {
            copy_paths: vec![dir.path().to_path_buf()],
            max_copy_depth: 8,
            source_format: SourceFormat::Fixed,
        };

        let result = preprocess(source, &source_path, &config);
        let warning_count = result
            .diagnostics
            .iter()
            .filter(|diag| diag.code == "COBC-W001")
            .count();

        assert_eq!(warning_count, 2, "{:?}", result.diagnostics);
        assert_no_errors(&result);
    }
}
