// COBOL Compiler - Diagnostic rendering using ariadne
//
// Converts internal Diagnostic structs to ariadne Reports for
// source-annotated, colored error output.

use std::io::Write;

use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::diagnostic::{Diagnostic, Severity};

/// Renders a single diagnostic to the given writer using ariadne.
///
/// `file_name` is the source file name displayed in the report header.
/// `source` is the full source text of the file (needed for line/column display).
pub fn render_diagnostic<W: Write>(
    writer: &mut W,
    diag: &Diagnostic,
    file_name: &str,
    source: &str,
) {
    let kind = match diag.severity {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Info | Severity::Hint => ReportKind::Advice,
    };

    let label_color = match diag.severity {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Info => Color::Cyan,
        Severity::Hint => Color::Blue,
    };

    // Determine the primary offset for the report header.
    let primary_offset = diag
        .labels
        .first()
        .map(|l| l.span.start as usize)
        .unwrap_or(0);

    let mut builder = Report::build(kind, file_name, primary_offset)
        .with_code(&diag.code)
        .with_message(&diag.message);

    for label in &diag.labels {
        let start = label.span.start as usize;
        let end = label.span.end as usize;
        // Ensure the span is within bounds and non-empty for ariadne.
        let end = end.max(start + 1).min(source.len());
        let start = start.min(source.len().saturating_sub(1));

        let mut ariadne_label = Label::new((file_name, start..end)).with_color(label_color);
        if !label.message.is_empty() {
            ariadne_label = ariadne_label.with_message(&label.message);
        }
        builder = builder.with_label(ariadne_label);
    }

    for note in &diag.notes {
        builder = builder.with_note(note);
    }

    let report = builder.finish();
    let _ = report.write((file_name, Source::from(source)), writer);
}

/// Renders all diagnostics in the slice to stderr.
pub fn render_diagnostics_to_stderr(diagnostics: &[Diagnostic], file_name: &str, source: &str) {
    let mut stderr = std::io::stderr().lock();
    for diag in diagnostics {
        render_diagnostic(&mut stderr, diag, file_name, source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use cobol_common::span::{FileId, Span};

    #[test]
    fn test_render_error_diagnostic() {
        let source = "IDENTIFICATION DIVISION.\nPROGRAM-ID. HELLO.\nPROCEDURE DIVISION.\n    DISPLAY \"Hello\".\n    STOP RUN.\n";
        let diag = Diagnostic::error("E001", "unexpected token")
            .with_label(Span::new(44, 63, FileId(0)), "expected statement here");

        let mut buf = Vec::new();
        render_diagnostic(&mut buf, &diag, "test.cob", source);
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("E001"));
        assert!(output.contains("unexpected token"));
        assert!(output.contains("expected statement here"));
    }

    #[test]
    fn test_render_warning_diagnostic() {
        let source = "IDENTIFICATION DIVISION.\nPROGRAM-ID. HELLO.\n";
        let diag = Diagnostic::warning("W001", "unused variable")
            .with_label(Span::new(25, 43, FileId(0)), "declared here");

        let mut buf = Vec::new();
        render_diagnostic(&mut buf, &diag, "hello.cob", source);
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("W001"));
        assert!(output.contains("unused variable"));
    }

    #[test]
    fn test_render_diagnostic_with_note() {
        let source = "IDENTIFICATION DIVISION.\n";
        let diag = Diagnostic::error("E002", "type mismatch")
            .with_label(Span::new(0, 14, FileId(0)), "here")
            .with_note("expected NUMERIC, found ALPHANUMERIC");

        let mut buf = Vec::new();
        render_diagnostic(&mut buf, &diag, "test.cob", source);
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("type mismatch"));
        assert!(output.contains("expected NUMERIC, found ALPHANUMERIC"));
    }

    #[test]
    fn test_render_diagnostic_no_labels() {
        let source = "HELLO\n";
        let diag = Diagnostic::info("I001", "compilation started");

        let mut buf = Vec::new();
        render_diagnostic(&mut buf, &diag, "test.cob", source);
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("I001"));
        assert!(output.contains("compilation started"));
    }

    #[test]
    fn test_render_multiple_labels() {
        let source =
            "IDENTIFICATION DIVISION.\nPROGRAM-ID. HELLO.\nDATA DIVISION.\nWORKING-STORAGE SECTION.\n01 WS-A PIC X.\n01 WS-A PIC 9.\n";
        let diag = Diagnostic::error("E003", "duplicate identifier")
            .with_label(Span::new(78, 93, FileId(0)), "first definition")
            .with_label(Span::new(94, 108, FileId(0)), "duplicate here");

        let mut buf = Vec::new();
        render_diagnostic(&mut buf, &diag, "test.cob", source);
        let output = String::from_utf8(buf).unwrap();

        assert!(output.contains("duplicate identifier"));
        assert!(output.contains("first definition"));
        assert!(output.contains("duplicate here"));
    }
}
