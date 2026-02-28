// COBOL Compiler - High-level intermediate representation
//
// This crate defines the HIR, a desugared and simplified view of a COBOL
// program. The HIR sits between the AST and code generation, stripping
// away division/section/paragraph structure to produce a flat list of
// typed data items and executable statements.

pub mod hir;
pub mod lower;

pub use hir::*;
pub use lower::lower_to_hir;

// COBOL 2002+ types are re-exported via `hir::*`:
//   HirClass, HirMethod, HirFunction, HirParam, HirParamMode
// COBOL 2014+ types:
//   HirTypedef
// COBOL 2023+ types:
//   HirInterface
