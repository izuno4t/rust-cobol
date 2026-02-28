// COBOL Compiler - Diagnostic reporting and error rendering

pub mod diagnostic;
pub mod render;
pub mod reporter;

pub use diagnostic::{Diagnostic, Label, Severity};
pub use render::{render_diagnostic, render_diagnostics_to_stderr};
pub use reporter::DiagnosticReporter;
