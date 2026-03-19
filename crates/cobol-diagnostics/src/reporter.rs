use crate::diagnostic::{Diagnostic, Severity};

/// Controls which warning levels are reported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WarningLevel {
    /// Show all diagnostics including hints and info.
    All,
    /// Show errors and warnings only (default).
    #[default]
    Default,
    /// Show errors only, suppress all warnings.
    None,
    /// Treat warnings as errors.
    Error,
}

/// Collects diagnostics emitted during compilation and provides
/// summary queries (error count, warning count, etc.).
#[derive(Debug, Default)]
pub struct DiagnosticReporter {
    diagnostics: Vec<Diagnostic>,
    warning_level: WarningLevel,
}

impl DiagnosticReporter {
    /// Creates an empty reporter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a reporter with a specific warning level.
    pub fn with_warning_level(warning_level: WarningLevel) -> Self {
        Self {
            diagnostics: Vec::new(),
            warning_level,
        }
    }

    /// Sets the warning level for this reporter.
    pub fn set_warning_level(&mut self, level: WarningLevel) {
        self.warning_level = level;
    }

    /// Returns the current warning level.
    pub fn warning_level(&self) -> WarningLevel {
        self.warning_level
    }

    /// Adds a diagnostic to the collection.
    ///
    /// If the warning level is `None`, warnings/info/hints are suppressed.
    /// If the warning level is `Error`, warnings are promoted to errors.
    pub fn report(&mut self, diagnostic: Diagnostic) {
        match self.warning_level {
            WarningLevel::None => {
                // Only collect errors, suppress everything else.
                if diagnostic.severity == Severity::Error {
                    self.diagnostics.push(diagnostic);
                }
            }
            WarningLevel::Error => {
                // Promote warnings to errors.
                if diagnostic.severity == Severity::Warning {
                    let promoted = Diagnostic {
                        severity: Severity::Error,
                        code: diagnostic.code,
                        message: diagnostic.message,
                        labels: diagnostic.labels,
                        notes: {
                            let mut notes = diagnostic.notes;
                            notes.push(
                                "this warning is promoted to an error due to -Werror".to_string(),
                            );
                            notes
                        },
                    };
                    self.diagnostics.push(promoted);
                } else {
                    self.diagnostics.push(diagnostic);
                }
            }
            WarningLevel::Default => {
                // Suppress hints and info.
                if diagnostic.severity != Severity::Hint && diagnostic.severity != Severity::Info {
                    self.diagnostics.push(diagnostic);
                }
            }
            WarningLevel::All => {
                self.diagnostics.push(diagnostic);
            }
        }
    }

    /// Returns `true` if any reported diagnostic has error severity.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
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
        let mut reporter = DiagnosticReporter::with_warning_level(WarningLevel::All);
        reporter.report(Diagnostic::error("E001", "error 1"));
        reporter.report(Diagnostic::error("E002", "error 2"));
        reporter.report(Diagnostic::warning("W001", "warning 1"));
        reporter.report(Diagnostic::info("I001", "info 1"));

        assert_eq!(reporter.error_count(), 2);
        assert_eq!(reporter.warning_count(), 1);
        assert_eq!(reporter.diagnostics().len(), 4);
    }

    #[test]
    fn test_warning_level_none() {
        let mut reporter = DiagnosticReporter::with_warning_level(WarningLevel::None);
        reporter.report(Diagnostic::error("E001", "error"));
        reporter.report(Diagnostic::warning("W001", "warning"));
        reporter.report(Diagnostic::info("I001", "info"));

        assert_eq!(reporter.error_count(), 1);
        assert_eq!(reporter.warning_count(), 0);
        assert_eq!(reporter.diagnostics().len(), 1);
    }

    #[test]
    fn test_warning_level_error() {
        let mut reporter = DiagnosticReporter::with_warning_level(WarningLevel::Error);
        reporter.report(Diagnostic::warning("W001", "a warning"));

        // Warning should be promoted to error.
        assert_eq!(reporter.error_count(), 1);
        assert_eq!(reporter.warning_count(), 0);
        assert!(reporter.has_errors());
    }

    #[test]
    fn test_warning_level_default_suppresses_info() {
        let mut reporter = DiagnosticReporter::new(); // Default level
        reporter.report(Diagnostic::error("E001", "error"));
        reporter.report(Diagnostic::warning("W001", "warning"));
        reporter.report(Diagnostic::info("I001", "info"));

        // Default level suppresses info/hint.
        assert_eq!(reporter.diagnostics().len(), 2);
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
