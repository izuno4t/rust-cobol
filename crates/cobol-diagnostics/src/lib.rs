// COBOL Compiler - Diagnostic reporting and error rendering

pub mod diagnostic;
pub mod reporter;

pub use diagnostic::{Diagnostic, Label, Severity};
pub use reporter::DiagnosticReporter;
