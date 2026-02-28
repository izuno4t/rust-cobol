// COBOL HIR - High-level intermediate representation
//
// A desugared, simplified view of a COBOL program. The HIR strips away
// COBOL division/section/paragraph structure and expresses the program
// as a flat list of typed data items and executable statements.

use cobol_common::Span;
use smol_str::SmolStr;

/// A HIR program -- the desugared form of a COBOL compilation unit.
#[derive(Debug, Clone)]
pub struct HirProgram {
    pub name: SmolStr,
    pub data_items: Vec<HirDataItem>,
    pub paragraphs: Vec<HirParagraph>,
    pub body: Vec<HirStatement>,
    /// COBOL 2002+: Class definitions.
    pub classes: Vec<HirClass>,
    /// COBOL 2002+: User-defined function definitions.
    pub functions: Vec<HirFunction>,
    /// COBOL 2014+: Type definitions (TYPEDEF).
    pub typedefs: Vec<HirTypedef>,
    /// COBOL 2023+: Interface definitions (INTERFACE-ID).
    pub interfaces: Vec<HirInterface>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// COBOL 2002+: OOP constructs
// ---------------------------------------------------------------------------

/// A class definition lowered from CLASS-ID.
#[derive(Debug, Clone)]
pub struct HirClass {
    pub name: SmolStr,
    pub parent: Option<SmolStr>,
    pub factory_methods: Vec<HirMethod>,
    pub instance_methods: Vec<HirMethod>,
    pub factory_data: Vec<HirDataItem>,
    pub instance_data: Vec<HirDataItem>,
    pub span: Span,
}

/// A method definition within a class.
#[derive(Debug, Clone)]
pub struct HirMethod {
    pub name: SmolStr,
    pub params: Vec<HirParam>,
    pub returning: Option<SmolStr>,
    pub data_items: Vec<HirDataItem>,
    pub body: Vec<HirStatement>,
    pub span: Span,
}

/// A user-defined function definition (FUNCTION-ID).
#[derive(Debug, Clone)]
pub struct HirFunction {
    pub name: SmolStr,
    pub params: Vec<HirParam>,
    pub returning: HirType,
    pub data_items: Vec<HirDataItem>,
    pub body: Vec<HirStatement>,
    pub span: Span,
}

/// COBOL 2014+: A named type definition (TYPEDEF).
#[derive(Debug, Clone)]
pub struct HirTypedef {
    pub name: SmolStr,
    pub base_type: HirType,
    pub span: Span,
}

/// COBOL 2023+: An interface definition (INTERFACE-ID).
#[derive(Debug, Clone)]
pub struct HirInterface {
    pub name: SmolStr,
    pub methods: Vec<HirMethod>,
    pub span: Span,
}

/// A parameter declaration for methods and functions.
#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: SmolStr,
    pub mode: HirParamMode,
    pub data_type: HirType,
}

/// Parameter passing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirParamMode {
    ByReference,
    ByContent,
    ByValue,
}

/// A named paragraph from the PROCEDURE DIVISION, preserved for
/// PERFORM procedure-name support.
#[derive(Debug, Clone)]
pub struct HirParagraph {
    pub name: SmolStr,
    pub body: Vec<HirStatement>,
    pub span: Span,
}

/// A data item declaration extracted from the DATA DIVISION.
#[derive(Debug, Clone, PartialEq)]
pub struct HirDataItem {
    pub name: SmolStr,
    pub data_type: HirType,
    pub initial_value: Option<HirLiteral>,
    pub span: Span,
}

/// HIR-level type representation, simplified from PICTURE/USAGE.
#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    Alphanumeric {
        size: u32,
    },
    Numeric {
        size: u32,
        decimal_places: u32,
        is_signed: bool,
    },
    /// Group item containing subordinate data items.
    Group {
        members: Vec<HirDataItem>,
        size: u32,
    },
    /// Packed decimal (COMP-3 / PACKED-DECIMAL).
    Comp3 {
        size: u32,
        decimal_places: u32,
    },
    /// Binary integer (COMP / COMP-4 / COMP-5 / BINARY).
    Binary {
        size: u32,
    },
    Index,
    Pointer,
    /// COBOL 2002+: Boolean type (PIC 1 / USAGE BIT).
    Boolean,
    /// COBOL 2014+: IEEE 754 single precision (4 bytes).
    FloatShort,
    /// COBOL 2014+: IEEE 754 double precision (8 bytes).
    FloatLong,
    /// COBOL 2014+: IEEE 754 quad precision (16 bytes, approximated as f64).
    FloatExtended,
}

/// A literal value in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirLiteral {
    Integer(i64),
    Decimal(String),
    String(SmolStr),
    Zero,
    Space,
}

/// A MOVE target, which may be a plain variable or a reference-modified variable.
#[derive(Debug, Clone)]
pub enum HirMoveTarget {
    /// A simple variable name (e.g., `WS-NAME`).
    Variable(SmolStr),
    /// A reference-modified variable (e.g., `WS-NAME(3:5)`).
    ReferenceModification {
        variable: SmolStr,
        start: HirExpr,
        length: Option<HirExpr>,
    },
}

/// An executable statement in the HIR.
#[derive(Debug, Clone)]
pub enum HirStatement {
    Display {
        operands: Vec<HirExpr>,
        no_advancing: bool,
        span: Span,
    },
    Move {
        from: HirExpr,
        to: Vec<HirMoveTarget>,
        span: Span,
    },
    Compute {
        targets: Vec<SmolStr>,
        expr: HirExpr,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Add {
        operands: Vec<HirExpr>,
        to: Vec<SmolStr>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Subtract {
        operands: Vec<HirExpr>,
        from: Vec<SmolStr>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Multiply {
        operand: HirExpr,
        by: Vec<SmolStr>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Divide {
        operand: HirExpr,
        into: Vec<SmolStr>,
        remainder: Option<SmolStr>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    If {
        condition: HirCondition,
        then_body: Vec<HirStatement>,
        else_body: Vec<HirStatement>,
        span: Span,
    },
    Perform {
        kind: HirPerformKind,
        span: Span,
    },
    Call {
        program: HirExpr,
        params: Vec<HirExpr>,
        on_exception: Vec<HirStatement>,
        not_on_exception: Vec<HirStatement>,
        span: Span,
    },
    StopRun {
        span: Span,
    },
    /// EXIT PROGRAM statement (returns from called program, unlike STOP RUN which terminates).
    ExitProgram {
        span: Span,
    },
    Goback {
        span: Span,
    },
    Continue {
        span: Span,
    },
    /// OPEN statement.
    Open {
        entries: Vec<HirOpenEntry>,
        span: Span,
    },
    /// CLOSE statement.
    Close {
        files: Vec<SmolStr>,
        span: Span,
    },
    /// READ statement.
    Read {
        file_name: SmolStr,
        into: Option<SmolStr>,
        at_end: Vec<HirStatement>,
        not_at_end: Vec<HirStatement>,
        span: Span,
    },
    /// WRITE statement.
    Write {
        record_name: SmolStr,
        from: Option<HirExpr>,
        invalid_key: Vec<HirStatement>,
        span: Span,
    },
    /// REWRITE statement.
    Rewrite {
        record_name: SmolStr,
        from: Option<HirExpr>,
        span: Span,
    },
    /// DELETE statement.
    Delete {
        file_name: SmolStr,
        span: Span,
    },
    /// GO TO statement.
    GoTo {
        targets: Vec<SmolStr>,
        depending_on: Option<SmolStr>,
        span: Span,
    },
    /// INITIALIZE statement.
    Initialize {
        targets: Vec<SmolStr>,
        span: Span,
    },
    /// SET statement (simplified to assignment).
    Set {
        targets: Vec<SmolStr>,
        value: HirExpr,
        span: Span,
    },
    /// STRING statement.
    StringStmt {
        into: SmolStr,
        sources: Vec<HirExpr>,
        on_overflow: Vec<HirStatement>,
        span: Span,
    },
    /// UNSTRING statement.
    UnstringStmt {
        source: SmolStr,
        delimiters: Vec<HirUnstringDelimiter>,
        into: Vec<SmolStr>,
        on_overflow: Vec<HirStatement>,
        span: Span,
    },
    /// ACCEPT statement.
    Accept {
        target: SmolStr,
        span: Span,
    },
    /// SORT statement.
    Sort {
        file_name: SmolStr,
        keys: Vec<HirSortKey>,
        using: Vec<SmolStr>,
        giving: Vec<SmolStr>,
        span: Span,
    },
    /// INSPECT statement.
    Inspect {
        target: SmolStr,
        kind: HirInspectKind,
        span: Span,
    },
    // --- COBOL 2002+ statements ---
    /// INVOKE statement: method invocation on an object.
    Invoke {
        object: HirExpr,
        method: SmolStr,
        params: Vec<HirExpr>,
        returning: Option<SmolStr>,
        span: Span,
    },
    /// RAISE statement: raises an exception.
    Raise {
        exception: SmolStr,
        span: Span,
    },
    /// RESUME statement: resumes execution after exception handling.
    Resume {
        target: Option<SmolStr>,
        span: Span,
    },
    /// ALLOCATE statement: dynamic memory allocation.
    Allocate {
        target: SmolStr,
        returning: Option<SmolStr>,
        span: Span,
    },
    /// FREE statement: releases dynamically allocated memory.
    Free {
        targets: Vec<SmolStr>,
        span: Span,
    },
    // --- COBOL 2014+ statements ---
    /// VALIDATE statement: validates a data item against its constraints.
    Validate {
        target: SmolStr,
        span: Span,
    },
    /// JSON GENERATE statement: serialize a COBOL group item to JSON.
    JsonGenerate {
        source: SmolStr,
        target: SmolStr,
        span: Span,
    },
    /// JSON PARSE statement: deserialize JSON into COBOL data items.
    JsonParse {
        source: SmolStr,
        target: SmolStr,
        span: Span,
    },
    /// XML GENERATE statement: serialize a COBOL group item to XML.
    XmlGenerate {
        source: SmolStr,
        target: SmolStr,
        span: Span,
    },
    /// XML PARSE statement: parse XML using a processing procedure.
    XmlParse {
        source: SmolStr,
        processing_procedure: SmolStr,
        span: Span,
    },
    // --- File I/O: additional statements ---
    /// START statement: positions within an indexed or relative file.
    Start {
        file_name: SmolStr,
        key: Option<SmolStr>,
        op: HirStartRelation,
        invalid_key: Vec<HirStatement>,
        not_invalid_key: Vec<HirStatement>,
        span: Span,
    },
    /// RETURN statement: retrieves records from a sort/merge file.
    Return {
        file_name: SmolStr,
        into: Option<SmolStr>,
        at_end: Vec<HirStatement>,
        not_at_end: Vec<HirStatement>,
        span: Span,
    },
    /// CANCEL statement: releases resources for a called program.
    Cancel {
        programs: Vec<HirExpr>,
        span: Span,
    },
    /// MERGE statement: merges sorted files.
    Merge {
        file_name: SmolStr,
        keys: Vec<HirSortKey>,
        using: Vec<SmolStr>,
        giving: Vec<SmolStr>,
        span: Span,
    },
    /// RELEASE statement: sends a record to the sort file.
    Release {
        record_name: SmolStr,
        from: Option<HirExpr>,
        span: Span,
    },
}

/// An expression in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirExpr {
    Literal(HirLiteral),
    Variable(SmolStr),
    BinaryOp {
        op: HirBinOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    UnaryOp {
        op: HirUnaryOp,
        operand: Box<HirExpr>,
    },
    /// A function call expression (e.g., FUNCTION LENGTH, FUNCTION UPPER-CASE).
    FunctionCall {
        name: SmolStr,
        args: Vec<HirExpr>,
    },
    /// Reference modification: `VAR(start:length)`.
    ///
    /// Extracts a substring from an alphanumeric variable.
    /// `start` is 1-based. `length` is optional (defaults to remaining bytes).
    ReferenceModification {
        variable: SmolStr,
        start: Box<HirExpr>,
        length: Option<Box<HirExpr>>,
    },
}

/// Binary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Unary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
}

/// A conditional expression in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirCondition {
    Compare {
        left: HirExpr,
        op: HirCompareOp,
        right: HirExpr,
    },
    And(Box<HirCondition>, Box<HirCondition>),
    Or(Box<HirCondition>, Box<HirCondition>),
    Not(Box<HirCondition>),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirCompareOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

/// The kind of PERFORM construct.
#[derive(Debug, Clone)]
pub enum HirPerformKind {
    /// Inline block of statements.
    Inline { body: Vec<HirStatement> },
    /// PERFORM ... TIMES.
    Times {
        count: HirExpr,
        body: Vec<HirStatement>,
    },
    /// PERFORM ... UNTIL.
    Until {
        condition: HirCondition,
        body: Vec<HirStatement>,
    },
    /// PERFORM ... VARYING.
    Varying {
        var: SmolStr,
        from: HirExpr,
        by: HirExpr,
        until: HirCondition,
        body: Vec<HirStatement>,
    },
    /// PERFORM procedure-name [THRU procedure-name].
    ProcedureName {
        name: SmolStr,
        through: Option<SmolStr>,
    },
}

/// A file open entry.
#[derive(Debug, Clone)]
pub struct HirOpenEntry {
    pub mode: HirOpenMode,
    pub file_name: SmolStr,
}

/// File open modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirOpenMode {
    Input,
    Output,
    IoMode,
    Extend,
}

/// A sort key specification.
#[derive(Debug, Clone)]
pub struct HirSortKey {
    pub order: HirSortOrder,
    pub fields: Vec<SmolStr>,
}

/// Sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirSortOrder {
    Ascending,
    Descending,
}

/// A delimiter specification for UNSTRING.
#[derive(Debug, Clone)]
pub struct HirUnstringDelimiter {
    pub all: bool,
    pub value: HirExpr,
}

/// Relational operator for START key comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirStartRelation {
    Equal,
    GreaterThan,
    GreaterEqual,
    NotLessThan,
}

/// The kind of INSPECT operation (simplified).
#[derive(Debug, Clone)]
pub enum HirInspectKind {
    /// INSPECT TALLYING -- count occurrences.
    Tallying,
    /// INSPECT REPLACING -- replace characters.
    Replacing,
    /// INSPECT TALLYING AND REPLACING.
    TallyingReplacing,
    /// INSPECT CONVERTING.
    Converting,
}

impl std::fmt::Display for HirProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "HIR Program: {}", self.name)?;
        if !self.data_items.is_empty() {
            writeln!(f, "  Data Items:")?;
            for item in &self.data_items {
                writeln!(
                    f,
                    "    {} {:?} = {:?}",
                    item.name, item.data_type, item.initial_value
                )?;
            }
        }
        if !self.body.is_empty() {
            writeln!(f, "  Body:")?;
            for stmt in &self.body {
                write_stmt(f, stmt, 4)?;
            }
        }
        if !self.paragraphs.is_empty() {
            writeln!(f, "  Paragraphs:")?;
            for para in &self.paragraphs {
                writeln!(f, "    {}:", para.name)?;
                for stmt in &para.body {
                    write_stmt(f, stmt, 6)?;
                }
            }
        }
        if !self.classes.is_empty() {
            writeln!(f, "  Classes:")?;
            for class in &self.classes {
                writeln!(f, "    CLASS {}", class.name)?;
                if let Some(ref parent) = class.parent {
                    writeln!(f, "      INHERITS {parent}")?;
                }
            }
        }
        if !self.functions.is_empty() {
            writeln!(f, "  Functions:")?;
            for func in &self.functions {
                writeln!(f, "    FUNCTION {}", func.name)?;
            }
        }
        if !self.typedefs.is_empty() {
            writeln!(f, "  Typedefs:")?;
            for td in &self.typedefs {
                writeln!(f, "    TYPEDEF {} {:?}", td.name, td.base_type)?;
            }
        }
        if !self.interfaces.is_empty() {
            writeln!(f, "  Interfaces:")?;
            for iface in &self.interfaces {
                writeln!(f, "    INTERFACE {}", iface.name)?;
            }
        }
        Ok(())
    }
}

fn write_stmt(
    f: &mut std::fmt::Formatter<'_>,
    stmt: &HirStatement,
    indent: usize,
) -> std::fmt::Result {
    let pad = " ".repeat(indent);
    match stmt {
        HirStatement::Display {
            operands,
            no_advancing,
            ..
        } => {
            write!(f, "{pad}DISPLAY")?;
            for op in operands {
                write!(f, " {}", format_expr(op))?;
            }
            if *no_advancing {
                write!(f, " WITH NO ADVANCING")?;
            }
            writeln!(f)
        }
        HirStatement::Move { from, to, .. } => {
            let targets: Vec<_> = to.iter().map(format_move_target).collect();
            writeln!(
                f,
                "{pad}MOVE {} TO {}",
                format_expr(from),
                targets.join(", ")
            )
        }
        HirStatement::Compute { targets, expr, .. } => {
            writeln!(
                f,
                "{pad}COMPUTE {} = {}",
                targets.join(", "),
                format_expr(expr)
            )
        }
        HirStatement::Add { operands, to, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            writeln!(f, "{pad}ADD {} TO {}", ops.join(" "), to.join(", "))
        }
        HirStatement::Subtract { operands, from, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            writeln!(
                f,
                "{pad}SUBTRACT {} FROM {}",
                ops.join(" "),
                from.join(", ")
            )
        }
        HirStatement::Multiply { operand, by, .. } => {
            writeln!(
                f,
                "{pad}MULTIPLY {} BY {}",
                format_expr(operand),
                by.join(", ")
            )
        }
        HirStatement::Divide { operand, into, .. } => {
            writeln!(
                f,
                "{pad}DIVIDE {} INTO {}",
                format_expr(operand),
                into.join(", ")
            )
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            writeln!(f, "{pad}IF ...")?;
            for s in then_body {
                write_stmt(f, s, indent + 2)?;
            }
            if !else_body.is_empty() {
                writeln!(f, "{pad}ELSE")?;
                for s in else_body {
                    write_stmt(f, s, indent + 2)?;
                }
            }
            writeln!(f, "{pad}END-IF")
        }
        HirStatement::Perform { kind, .. } => {
            writeln!(f, "{pad}PERFORM {:?}", std::mem::discriminant(kind))
        }
        HirStatement::Call { program, .. } => {
            writeln!(f, "{pad}CALL {}", format_expr(program))
        }
        HirStatement::StopRun { .. } => writeln!(f, "{pad}STOP RUN"),
        HirStatement::ExitProgram { .. } => writeln!(f, "{pad}EXIT PROGRAM"),
        HirStatement::Goback { .. } => writeln!(f, "{pad}GOBACK"),
        HirStatement::Continue { .. } => writeln!(f, "{pad}CONTINUE"),
        HirStatement::Open { entries, .. } => {
            let names: Vec<_> = entries.iter().map(|e| e.file_name.to_string()).collect();
            writeln!(f, "{pad}OPEN {}", names.join(", "))
        }
        HirStatement::Close { files, .. } => {
            writeln!(
                f,
                "{pad}CLOSE {}",
                files
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        HirStatement::Read { file_name, .. } => writeln!(f, "{pad}READ {file_name}"),
        HirStatement::Write { record_name, .. } => writeln!(f, "{pad}WRITE {record_name}"),
        HirStatement::Rewrite { record_name, .. } => writeln!(f, "{pad}REWRITE {record_name}"),
        HirStatement::Delete { file_name, .. } => writeln!(f, "{pad}DELETE {file_name}"),
        HirStatement::GoTo { targets, .. } => {
            writeln!(
                f,
                "{pad}GO TO {}",
                targets
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        HirStatement::Initialize { targets, .. } => {
            writeln!(
                f,
                "{pad}INITIALIZE {}",
                targets
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        HirStatement::Set { targets, value, .. } => {
            writeln!(
                f,
                "{pad}SET {} TO {}",
                targets
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                format_expr(value)
            )
        }
        HirStatement::StringStmt { into, .. } => writeln!(f, "{pad}STRING INTO {into}"),
        HirStatement::UnstringStmt { source, .. } => writeln!(f, "{pad}UNSTRING {source}"),
        HirStatement::Accept { target, .. } => writeln!(f, "{pad}ACCEPT {target}"),
        HirStatement::Sort { file_name, .. } => writeln!(f, "{pad}SORT {file_name}"),
        HirStatement::Inspect { target, .. } => writeln!(f, "{pad}INSPECT {target}"),
        HirStatement::Invoke { object, method, .. } => {
            writeln!(f, "{pad}INVOKE {} \"{}\"", format_expr(object), method)
        }
        HirStatement::Raise { exception, .. } => writeln!(f, "{pad}RAISE {exception}"),
        HirStatement::Resume { target, .. } => {
            if let Some(t) = target {
                writeln!(f, "{pad}RESUME {t}")
            } else {
                writeln!(f, "{pad}RESUME")
            }
        }
        HirStatement::Allocate { target, .. } => writeln!(f, "{pad}ALLOCATE {target}"),
        HirStatement::Free { targets, .. } => {
            writeln!(
                f,
                "{pad}FREE {}",
                targets
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        HirStatement::Validate { target, .. } => writeln!(f, "{pad}VALIDATE {target}"),
        HirStatement::JsonGenerate { source, target, .. } => {
            writeln!(f, "{pad}JSON GENERATE {target} FROM {source}")
        }
        HirStatement::JsonParse { source, target, .. } => {
            writeln!(f, "{pad}JSON PARSE {source} INTO {target}")
        }
        HirStatement::XmlGenerate { source, target, .. } => {
            writeln!(f, "{pad}XML GENERATE {target} FROM {source}")
        }
        HirStatement::XmlParse {
            source,
            processing_procedure,
            ..
        } => {
            writeln!(
                f,
                "{pad}XML PARSE {source} PROCESSING PROCEDURE {processing_procedure}"
            )
        }
        HirStatement::Start { file_name, .. } => writeln!(f, "{pad}START {file_name}"),
        HirStatement::Return { file_name, .. } => writeln!(f, "{pad}RETURN {file_name}"),
        HirStatement::Cancel { programs, .. } => {
            let names: Vec<_> = programs.iter().map(format_expr).collect();
            writeln!(f, "{pad}CANCEL {}", names.join(", "))
        }
        HirStatement::Merge { file_name, .. } => writeln!(f, "{pad}MERGE {file_name}"),
        HirStatement::Release { record_name, .. } => {
            writeln!(f, "{pad}RELEASE {record_name}")
        }
    }
}

fn format_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Literal(lit) => match lit {
            HirLiteral::Integer(n) => n.to_string(),
            HirLiteral::Decimal(d) => d.clone(),
            HirLiteral::String(s) => format!("\"{}\"", s),
            HirLiteral::Zero => "ZERO".to_string(),
            HirLiteral::Space => "SPACE".to_string(),
        },
        HirExpr::Variable(name) => name.to_string(),
        HirExpr::BinaryOp { op, left, right } => {
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => "**",
            };
            format!("({} {} {})", format_expr(left), op_str, format_expr(right))
        }
        HirExpr::UnaryOp { op, operand } => {
            let op_str = match op {
                HirUnaryOp::Neg => "-",
            };
            format!("({}{})", op_str, format_expr(operand))
        }
        HirExpr::FunctionCall { name, args } => {
            let arg_strs: Vec<_> = args.iter().map(format_expr).collect();
            format!("FUNCTION {}({})", name, arg_strs.join(", "))
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } => {
            if let Some(len) = length {
                format!("{}({}:{})", variable, format_expr(start), format_expr(len))
            } else {
                format!("{}({}:)", variable, format_expr(start))
            }
        }
    }
}

fn format_move_target(target: &HirMoveTarget) -> String {
    match target {
        HirMoveTarget::Variable(name) => name.to_string(),
        HirMoveTarget::ReferenceModification {
            variable,
            start,
            length,
        } => {
            if let Some(len) = length {
                format!("{}({}:{})", variable, format_expr(start), format_expr(len))
            } else {
                format!("{}({}:)", variable, format_expr(start))
            }
        }
    }
}
