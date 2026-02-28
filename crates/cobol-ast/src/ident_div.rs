// COBOL AST - IDENTIFICATION DIVISION

use cobol_common::Span;
use smol_str::SmolStr;

/// The IDENTIFICATION DIVISION contains program metadata.
///
/// Required in every COBOL program. The PROGRAM-ID paragraph is mandatory;
/// all other paragraphs are optional and deprecated in modern COBOL standards.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentificationDivision {
    /// The program name from the PROGRAM-ID paragraph.
    pub program_id: SmolStr,
    /// Whether the INITIAL attribute is specified (COBOL-85+).
    pub is_initial: bool,
    /// Whether the RECURSIVE attribute is specified (COBOL 2002+).
    pub is_recursive: bool,
    /// Whether the COMMON attribute is specified (nested programs).
    pub is_common: bool,
    /// AUTHOR paragraph (deprecated in COBOL 2002).
    pub author: Option<SmolStr>,
    /// INSTALLATION paragraph (deprecated in COBOL 2002).
    pub installation: Option<SmolStr>,
    /// DATE-WRITTEN paragraph (deprecated in COBOL 2002).
    pub date_written: Option<SmolStr>,
    /// DATE-COMPILED paragraph (deprecated in COBOL 2002).
    pub date_compiled: Option<SmolStr>,
    /// SECURITY paragraph (deprecated in COBOL 2002).
    pub security: Option<SmolStr>,
    pub span: Span,
}
