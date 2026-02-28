use cobol_common::Span;

/// Severity level of a diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A labeled span within a diagnostic, pointing to a specific source location.
#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

/// A compiler diagnostic with severity, code, message, labeled spans, and notes.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Creates an error-level diagnostic.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates a warning-level diagnostic.
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.into(),
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates an info-level diagnostic.
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code: code.into(),
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Adds a label at the given span with an empty message.
    pub fn with_span(mut self, span: Span) -> Self {
        self.labels.push(Label {
            span,
            message: String::new(),
        });
        self
    }

    /// Adds a label at the given span with a message.
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a note to this diagnostic.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Returns `true` if this diagnostic has error severity.
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::span::FileId;

    #[test]
    fn test_error_creation() {
        let diag = Diagnostic::error("E001", "unexpected token");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, "E001");
        assert_eq!(diag.message, "unexpected token");
        assert!(diag.labels.is_empty());
        assert!(diag.notes.is_empty());
    }

    #[test]
    fn test_warning_creation() {
        let diag = Diagnostic::warning("W001", "unused variable");
        assert_eq!(diag.severity, Severity::Warning);
        assert_eq!(diag.code, "W001");
        assert_eq!(diag.message, "unused variable");
    }

    #[test]
    fn test_diagnostic_with_note() {
        let diag = Diagnostic::error("E002", "type mismatch")
            .with_note("expected NUMERIC, found ALPHANUMERIC");
        assert_eq!(diag.notes.len(), 1);
        assert_eq!(diag.notes[0], "expected NUMERIC, found ALPHANUMERIC");
    }

    #[test]
    fn test_diagnostic_with_label() {
        let span = Span::new(10, 20, FileId(0));
        let diag = Diagnostic::error("E003", "undeclared identifier")
            .with_label(span, "not found in DATA DIVISION");
        assert_eq!(diag.labels.len(), 1);
        assert_eq!(diag.labels[0].span, span);
        assert_eq!(diag.labels[0].message, "not found in DATA DIVISION");
    }

    #[test]
    fn test_is_error() {
        let error = Diagnostic::error("E001", "bad");
        assert!(error.is_error());

        let warning = Diagnostic::warning("W001", "meh");
        assert!(!warning.is_error());

        let info = Diagnostic::info("I001", "fyi");
        assert!(!info.is_error());
    }

    #[test]
    fn test_with_span() {
        let span = Span::new(0, 5, FileId(1));
        let diag = Diagnostic::error("E004", "syntax error").with_span(span);
        assert_eq!(diag.labels.len(), 1);
        assert_eq!(diag.labels[0].span, span);
        assert!(diag.labels[0].message.is_empty());
    }

    #[test]
    fn test_builder_chaining() {
        let span1 = Span::new(0, 5, FileId(0));
        let span2 = Span::new(10, 15, FileId(0));
        let diag = Diagnostic::error("E005", "conflicting definitions")
            .with_label(span1, "first definition here")
            .with_label(span2, "second definition here")
            .with_note("remove one of the definitions");

        assert_eq!(diag.labels.len(), 2);
        assert_eq!(diag.notes.len(), 1);
    }
}
