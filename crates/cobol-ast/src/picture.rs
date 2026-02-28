// COBOL AST - PICTURE clause representation

use cobol_common::Span;
use smol_str::SmolStr;

/// Represents a parsed PICTURE clause.
///
/// The raw string preserves the original source text (e.g. "S9(7)V99"),
/// while the derived fields provide structured information about the
/// picture's category and size.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureClause {
    /// Original PIC string, e.g. "S9(7)V99".
    pub raw_string: SmolStr,
    /// Semantic category derived from the picture symbols.
    pub category: PictureCategory,
    /// Total storage size in character positions.
    pub size: u32,
    /// Number of positions after the decimal point (V).
    pub decimal_positions: u32,
    /// Whether the picture includes a sign (S).
    pub is_signed: bool,
    /// Whether the picture contains editing symbols.
    pub is_edited: bool,
    pub span: Span,
}

/// The semantic category of a PICTURE clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureCategory {
    Alphabetic,
    Alphanumeric,
    AlphanumericEdited,
    Numeric,
    NumericEdited,
    National,
    NationalEdited,
    Boolean,
}
