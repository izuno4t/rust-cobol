// COBOL AST - Top-level program node

use cobol_common::Span;

use crate::data_div::DataDivision;
use crate::env_div::EnvironmentDivision;
use crate::ident_div::IdentificationDivision;
use crate::proc_div::ProcedureDivision;

/// The root AST node for a COBOL compilation unit.
///
/// A COBOL program always has an IDENTIFICATION DIVISION, and may optionally
/// contain ENVIRONMENT, DATA, and PROCEDURE divisions. Programs may also
/// contain nested programs (COBOL-85+).
#[derive(Debug, Clone, PartialEq)]
pub struct CobolProgram {
    pub identification: IdentificationDivision,
    pub environment: Option<EnvironmentDivision>,
    pub data: Option<DataDivision>,
    pub procedure: Option<ProcedureDivision>,
    /// Nested programs contained within this program.
    pub nested_programs: Vec<CobolProgram>,
    pub span: Span,
}
