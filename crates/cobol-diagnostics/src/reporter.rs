use crate::diagnostic::{Diagnostic, Severity};

/// Collects diagnostics emitted during compilation and provides
/// summary queries (error count, warning count, etc.).
#[derive(Debug, Default)]
pub struct DiagnosticReporter {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReporter {
    /// Creates an empty reporter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a diagnostic to the collection.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Returns `true` if any reported diagnostic has error severity.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    /// Returns the number of error-level diagnostics.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Returns the number of warning-level diagnostics.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// Returns a slice of all collected diagnostics.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Drains and returns all collected diagnostics, leaving the reporter empty.
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    #[test]
    fn test_reporter_empty() {
        let reporter = DiagnosticReporter::new();
        assert!(!reporter.has_errors());
        assert_eq!(reporter.error_count(), 0);
        assert_eq!(reporter.warning_count(), 0);
        assert!(reporter.diagnostics().is_empty());
    }

    #[test]
    fn test_reporter_has_errors() {
        let mut reporter = DiagnosticReporter::new();
        reporter.report(Diagnostic::warning("W001", "a warning"));
        assert!(!reporter.has_errors());

        reporter.report(Diagnostic::error("E001", "an error"));
        assert!(reporter.has_errors());
    }

    #[test]
    fn test_reporter_counts() {
        let mut reporter = DiagnosticReporter::new();
        reporter.report(Diagnostic::error("E001", "error 1"));
        reporter.report(Diagnostic::error("E002", "error 2"));
        reporter.report(Diagnostic::warning("W001", "warning 1"));
        reporter.report(Diagnostic::info("I001", "info 1"));

        assert_eq!(reporter.error_count(), 2);
        assert_eq!(reporter.warning_count(), 1);
        assert_eq!(reporter.diagnostics().len(), 4);
    }

    #[test]
    fn test_reporter_take() {
        let mut reporter = DiagnosticReporter::new();
        reporter.report(Diagnostic::error("E001", "error"));
        reporter.report(Diagnostic::warning("W001", "warning"));

        let taken = reporter.take_diagnostics();
        assert_eq!(taken.len(), 2);

        // Reporter should be empty after take
        assert!(reporter.diagnostics().is_empty());
        assert!(!reporter.has_errors());
        assert_eq!(reporter.error_count(), 0);
    }
}
