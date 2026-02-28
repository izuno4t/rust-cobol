// COBOL Compiler - Common types, traits, and utilities shared across all crates

pub mod cobol_standard;
pub mod source;
pub mod source_format;
pub mod span;

pub use cobol_standard::CobolStandard;
pub use source::SourceFile;
pub use source_format::SourceFormat;
pub use span::{FileId, Span};
