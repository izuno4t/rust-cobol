// COBOL Parser - Parse error types

use cobol_common::Span;
use cobol_diagnostics::{Diagnostic, DiagnosticReporter};

/// Report a parse error with span information.
pub(crate) fn report_error(reporter: &mut DiagnosticReporter, span: Span, msg: &str) {
    let diag = Diagnostic::error("P001", msg).with_span(span);
    reporter.report(diag);
}

/// Report a parse error expecting a specific token.
pub(crate) fn report_expected(
    reporter: &mut DiagnosticReporter,
    span: Span,
    expected: &str,
    found: &str,
) {
    let msg = format!("expected {}, found {}", expected, found);
    let diag = Diagnostic::error("P002", msg).with_span(span);
    reporter.report(diag);
}
