// COBOL Compiler - Abstract syntax tree definitions
//
// This crate defines all AST node types for the COBOL compiler,
// covering COBOL-85 through COBOL 2023. These types are produced
// by the parser and consumed by semantic analysis and later phases.

pub mod data_div;
pub mod env_div;
pub mod expr;
pub mod ident_div;
pub mod picture;
pub mod proc_div;
pub mod program;
pub mod statement;

// Re-export top-level types for convenience.
pub use data_div::{DataDivision, DataItem, FileDescription, Usage};
pub use env_div::{
    AccessMode, ConfigurationSection, EnvironmentDivision, FileControlEntry, FileOrganization,
    InputOutputSection,
};
pub use expr::{
    ArithOp, ClassType, CompareOp, Condition, Expr, FigurativeConstant, Literal, QualifiedName,
    SignType, UnaryArithOp,
};
pub use ident_div::IdentificationDivision;
pub use picture::{PictureCategory, PictureClause};
pub use proc_div::{
    DeclarativeSection, Paragraph, ParamMode, ProcParam, ProcSection, ProcedureDivision, Sentence,
};
pub use program::CobolProgram;
pub use statement::Statement;
