// COBOL AST - PROCEDURE DIVISION structure

use cobol_common::Span;
use smol_str::SmolStr;

use crate::statement::Statement;

/// The PROCEDURE DIVISION contains the program's executable logic.
///
/// May optionally specify parameters (USING) and a return value (RETURNING).
/// Code is organized into optional declaratives, sections, and paragraphs.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureDivision {
    /// Parameters received via CALL USING.
    pub using_params: Vec<ProcParam>,
    /// RETURNING data item name.
    pub returning: Option<SmolStr>,
    /// DECLARATIVES section (USE statements for exception handling).
    pub declaratives: Vec<DeclarativeSection>,
    /// Named sections containing paragraphs.
    pub sections: Vec<ProcSection>,
    /// Top-level paragraphs not contained in any section.
    pub paragraphs: Vec<Paragraph>,
    pub span: Span,
}

/// A parameter declaration in PROCEDURE DIVISION USING.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcParam {
    /// Parameter passing mode.
    pub mode: ParamMode,
    /// Parameter data name.
    pub name: SmolStr,
    pub span: Span,
}

/// Parameter passing modes for CALL and PROCEDURE DIVISION USING.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamMode {
    /// BY REFERENCE (default): callee can modify the caller's data.
    ByReference,
    /// BY CONTENT: callee receives a copy.
    ByContent,
    /// BY VALUE: callee receives the value (COBOL 2002+).
    ByValue,
}

/// A DECLARATIVES section containing USE statements and associated paragraphs.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclarativeSection {
    pub name: SmolStr,
    pub use_statement: UseStatement,
    pub paragraphs: Vec<Paragraph>,
    pub span: Span,
}

/// USE statement variants for declarative sections.
#[derive(Debug, Clone, PartialEq)]
pub enum UseStatement {
    /// USE AFTER EXCEPTION/ERROR ON file-names.
    AfterException {
        file_names: Vec<SmolStr>,
        is_global: bool,
    },
    /// USE BEFORE REPORTING report-group.
    BeforeReporting { report_group: SmolStr },
    /// USE FOR DEBUGGING ON debug-items.
    ForDebugging { debug_items: Vec<SmolStr> },
}

/// A named section in the PROCEDURE DIVISION containing paragraphs.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcSection {
    pub name: SmolStr,
    pub paragraphs: Vec<Paragraph>,
    pub span: Span,
}

/// A paragraph: a named sequence of sentences.
#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub name: SmolStr,
    pub sentences: Vec<Sentence>,
    pub span: Span,
}

/// A sentence: a sequence of statements terminated by a period.
#[derive(Debug, Clone, PartialEq)]
pub struct Sentence {
    pub statements: Vec<Statement>,
    pub span: Span,
}
