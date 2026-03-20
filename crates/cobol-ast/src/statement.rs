// COBOL AST - All COBOL statements

use cobol_common::Span;
use smol_str::SmolStr;

use crate::expr::{Condition, Expr, QualifiedName};
use crate::proc_div::ParamMode;

/// All COBOL statement types.
///
/// Covers COBOL-85 core statements through COBOL 2023 additions
/// (JSON/XML generation and parsing).
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    // --- Data movement and arithmetic ---
    Move(MoveStatement),
    Compute(Box<ComputeStatement>),
    Add(Box<AddStatement>),
    Subtract(Box<SubtractStatement>),
    Multiply(Box<MultiplyStatement>),
    Divide(Box<DivideStatement>),

    // --- Input/output ---
    Display(DisplayStatement),
    Accept(AcceptStatement),

    // --- Control flow ---
    If(Box<IfStatement>),
    Evaluate(Box<EvaluateStatement>),
    Perform(Box<PerformStatement>),
    GoTo(GoToStatement),
    Call(Box<CallStatement>),
    StopRun,
    Goback,
    Continue,
    ExitProgram,
    ExitParagraph,
    ExitSection,

    // --- File I/O ---
    Open(OpenStatement),
    Close(CloseStatement),
    Read(Box<ReadStatement>),
    Write(Box<WriteStatement>),
    Rewrite(Box<RewriteStatement>),
    Delete(Box<DeleteStatement>),
    Start(Box<StartStatement>),
    Return(Box<ReturnStatement>),

    // --- String handling ---
    String(Box<StringStatement>),
    Unstring(Box<UnstringStatement>),
    Inspect(Box<InspectStatement>),

    // --- Data initialization and manipulation ---
    Initialize(Box<InitializeStatement>),
    Set(Box<SetStatement>),

    // --- Table handling ---
    Search(Box<SearchStatement>),

    // --- Sort/merge ---
    Sort(Box<SortStatement>),
    Merge(Box<MergeStatement>),
    Release(ReleaseStatement),

    // --- Miscellaneous ---
    Cancel(CancelStatement),

    // --- COBOL 2002+ ---
    Raise(RaiseStatement),
    Resume(ResumeStatement),
    Invoke(Box<InvokeStatement>),
    Allocate(Box<AllocateStatement>),
    Free(FreeStatement),

    // --- COBOL 2014+ ---
    JsonGenerate(Box<JsonGenerateStatement>),
    JsonParse(Box<JsonParseStatement>),
    XmlGenerate(Box<XmlGenerateStatement>),
    XmlParse(Box<XmlParseStatement>),
    Validate(ValidateStatement),

    // --- Report writer ---
    Initiate(InitiateStatement),
    Generate(GenerateStatement),
    Terminate(TerminateStatement),
}

// ---------------------------------------------------------------------------
// Data movement and arithmetic statements
// ---------------------------------------------------------------------------

/// MOVE statement: copies data from one item to one or more targets.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveStatement {
    /// True for MOVE CORRESPONDING.
    pub corresponding: bool,
    pub from: Expr,
    /// Targets can be plain identifiers or reference-modified identifiers.
    pub to: Vec<Expr>,
    pub span: Span,
}

/// COMPUTE statement: evaluates an arithmetic expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeStatement {
    pub targets: Vec<RoundedTarget>,
    pub expr: Expr,
    pub on_size_error: Vec<Statement>,
    pub not_on_size_error: Vec<Statement>,
    pub span: Span,
}

/// A target with optional ROUNDED phrase.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundedTarget {
    pub target: QualifiedName,
    pub rounded: bool,
}

/// ADD statement.
#[derive(Debug, Clone, PartialEq)]
pub struct AddStatement {
    pub operands: Vec<Expr>,
    pub to: Vec<RoundedTarget>,
    pub giving: Vec<RoundedTarget>,
    /// True for ADD CORRESPONDING.
    pub corresponding: bool,
    pub on_size_error: Vec<Statement>,
    pub not_on_size_error: Vec<Statement>,
    pub span: Span,
}

/// SUBTRACT statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtractStatement {
    pub operands: Vec<Expr>,
    pub from: Vec<RoundedTarget>,
    /// Format 2: SUBTRACT ... FROM literal GIVING ...
    /// When present, the FROM clause is a literal/expr rather than a target.
    pub from_expr: Option<Expr>,
    pub giving: Vec<RoundedTarget>,
    /// True for SUBTRACT CORRESPONDING.
    pub corresponding: bool,
    pub on_size_error: Vec<Statement>,
    pub not_on_size_error: Vec<Statement>,
    pub span: Span,
}

/// MULTIPLY statement.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiplyStatement {
    pub operand: Expr,
    pub by: Vec<RoundedTarget>,
    /// Format 2: MULTIPLY ... BY literal GIVING ...
    /// When present, the BY clause is a literal/expr rather than a target.
    pub by_expr: Option<Expr>,
    pub giving: Vec<RoundedTarget>,
    pub on_size_error: Vec<Statement>,
    pub not_on_size_error: Vec<Statement>,
    pub span: Span,
}

/// DIVIDE statement.
#[derive(Debug, Clone, PartialEq)]
pub struct DivideStatement {
    pub operand: Expr,
    pub into: Vec<RoundedTarget>,
    /// Format 2: DIVIDE ... INTO literal GIVING ...
    /// When present, the INTO clause is a literal/expr rather than a target.
    pub into_expr: Option<Expr>,
    pub giving: Vec<RoundedTarget>,
    pub remainder: Option<QualifiedName>,
    pub on_size_error: Vec<Statement>,
    pub not_on_size_error: Vec<Statement>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Input/output statements
// ---------------------------------------------------------------------------

/// DISPLAY statement: writes data to a device.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayStatement {
    pub operands: Vec<Expr>,
    /// UPON device name.
    pub upon: Option<SmolStr>,
    pub with_no_advancing: bool,
    pub span: Span,
}

/// ACCEPT statement: reads data from a device or system information.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptStatement {
    pub target: QualifiedName,
    pub from: Option<AcceptSource>,
    pub span: Span,
}

/// Source for an ACCEPT statement.
#[derive(Debug, Clone, PartialEq)]
pub enum AcceptSource {
    Date,
    DateYyyymmdd,
    Day,
    DayOfWeek,
    Time,
    Console,
    Environment(SmolStr),
}

// ---------------------------------------------------------------------------
// Control flow statements
// ---------------------------------------------------------------------------

/// IF statement with optional ELSE.
#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub condition: Condition,
    pub then_body: Vec<Statement>,
    pub else_body: Vec<Statement>,
    pub span: Span,
}

/// EVALUATE statement (multi-branch switch).
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluateStatement {
    pub subjects: Vec<EvaluateSubject>,
    pub when_clauses: Vec<WhenClause>,
    pub when_other: Vec<Statement>,
    pub span: Span,
}

/// A subject expression in EVALUATE.
#[derive(Debug, Clone, PartialEq)]
pub enum EvaluateSubject {
    Expr(Expr),
    Condition(Condition),
    True,
    False,
}

/// A WHEN clause in EVALUATE.
#[derive(Debug, Clone, PartialEq)]
pub struct WhenClause {
    /// Outer Vec: ALSO-separated groups; inner Vec: OR-separated objects.
    pub objects: Vec<Vec<WhenObject>>,
    pub body: Vec<Statement>,
    pub span: Span,
}

/// A matching object in a WHEN clause.
#[derive(Debug, Clone, PartialEq)]
pub enum WhenObject {
    Any,
    True,
    False,
    Condition(Condition),
    Expr(Expr),
    Range { from: Expr, to: Expr },
    Not(Box<WhenObject>),
}

/// PERFORM statement (loop or procedure invocation).
#[derive(Debug, Clone, PartialEq)]
pub struct PerformStatement {
    pub kind: PerformKind,
    pub span: Span,
}

/// The kind of PERFORM: inline, out-of-line, or iterative.
#[derive(Debug, Clone, PartialEq)]
pub enum PerformKind {
    /// Inline PERFORM (PERFORM ... END-PERFORM).
    Simple { body: Vec<Statement> },
    /// Out-of-line PERFORM procedure-name [THRU procedure-name].
    ProcedureName {
        procedure: SmolStr,
        through: Option<SmolStr>,
    },
    /// PERFORM ... TIMES.
    Times { times: Expr, body: Vec<Statement> },
    /// PERFORM ... UNTIL.
    Until {
        test: PerformTest,
        condition: Condition,
        body: Vec<Statement>,
    },
    /// PERFORM ... VARYING.
    Varying {
        test: PerformTest,
        varying: Vec<VaryingClause>,
        body: Vec<Statement>,
    },
}

/// When to test the UNTIL condition in a PERFORM loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformTest {
    /// WITH TEST BEFORE (default).
    Before,
    /// WITH TEST AFTER.
    After,
}

/// A VARYING phrase within a PERFORM VARYING statement.
#[derive(Debug, Clone, PartialEq)]
pub struct VaryingClause {
    pub identifier: QualifiedName,
    pub from: Expr,
    pub by: Expr,
    pub until: Condition,
}

/// GO TO statement.
#[derive(Debug, Clone, PartialEq)]
pub struct GoToStatement {
    pub targets: Vec<SmolStr>,
    /// DEPENDING ON identifier for GO TO ... DEPENDING ON.
    pub depending_on: Option<QualifiedName>,
    pub span: Span,
}

/// CALL statement: invokes an external program.
#[derive(Debug, Clone, PartialEq)]
pub struct CallStatement {
    /// Program name (literal or identifier).
    pub program: Expr,
    pub using: Vec<CallParam>,
    pub returning: Option<QualifiedName>,
    pub on_overflow: Vec<Statement>,
    pub on_exception: Vec<Statement>,
    pub not_on_exception: Vec<Statement>,
    pub span: Span,
}

/// A parameter in a CALL USING phrase.
#[derive(Debug, Clone, PartialEq)]
pub struct CallParam {
    pub mode: ParamMode,
    pub value: Expr,
}

// ---------------------------------------------------------------------------
// File I/O statements
// ---------------------------------------------------------------------------

/// OPEN statement: opens one or more files.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenStatement {
    pub entries: Vec<OpenEntry>,
    pub span: Span,
}

/// A single file open specification.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenEntry {
    pub mode: OpenMode,
    pub file_name: SmolStr,
}

/// File open modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Input,
    Output,
    IoMode,
    Extend,
}

/// CLOSE statement: closes one or more files.
#[derive(Debug, Clone, PartialEq)]
pub struct CloseStatement {
    pub files: Vec<CloseEntry>,
    pub span: Span,
}

/// A single file close specification.
#[derive(Debug, Clone, PartialEq)]
pub struct CloseEntry {
    pub file_name: SmolStr,
    pub close_option: Option<CloseOption>,
}

/// Close disposition options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOption {
    Reel,
    Unit,
    WithNoRewind,
    WithLock,
}

/// READ statement: reads a record from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadStatement {
    pub file_name: SmolStr,
    pub into: Option<QualifiedName>,
    pub key: Option<QualifiedName>,
    pub at_end: Vec<Statement>,
    pub not_at_end: Vec<Statement>,
    pub invalid_key: Vec<Statement>,
    pub not_invalid_key: Vec<Statement>,
    pub span: Span,
}

/// WRITE statement: writes a record to a file.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteStatement {
    pub record_name: QualifiedName,
    pub from: Option<Expr>,
    pub advancing: Option<WriteAdvancing>,
    pub invalid_key: Vec<Statement>,
    pub not_invalid_key: Vec<Statement>,
    pub at_eop: Vec<Statement>,
    pub not_at_eop: Vec<Statement>,
    pub span: Span,
}

/// WRITE ADVANCING specification.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteAdvancing {
    Lines(Expr),
    Page,
    MnemonicName(SmolStr),
}

/// REWRITE statement: updates a record in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct RewriteStatement {
    pub record_name: QualifiedName,
    pub from: Option<Expr>,
    pub invalid_key: Vec<Statement>,
    pub not_invalid_key: Vec<Statement>,
    pub span: Span,
}

/// DELETE statement: removes a record from an indexed or relative file.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    pub file_name: SmolStr,
    pub invalid_key: Vec<Statement>,
    pub not_invalid_key: Vec<Statement>,
    pub span: Span,
}

/// START statement: positions within an indexed or relative file.
#[derive(Debug, Clone, PartialEq)]
pub struct StartStatement {
    pub file_name: SmolStr,
    pub key_condition: Option<StartKeyCondition>,
    pub invalid_key: Vec<Statement>,
    pub not_invalid_key: Vec<Statement>,
    pub span: Span,
}

/// Key condition for the START statement.
#[derive(Debug, Clone, PartialEq)]
pub struct StartKeyCondition {
    pub key: QualifiedName,
    pub op: StartRelation,
}

/// Relational operator for START key comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartRelation {
    Equal,
    GreaterThan,
    GreaterEqual,
    NotLessThan,
}

/// RETURN statement: retrieves records from a sort/merge file.
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStatement {
    pub file_name: SmolStr,
    pub into: Option<QualifiedName>,
    pub at_end: Vec<Statement>,
    pub not_at_end: Vec<Statement>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// String handling statements
// ---------------------------------------------------------------------------

/// STRING statement: concatenates data items.
#[derive(Debug, Clone, PartialEq)]
pub struct StringStatement {
    pub sources: Vec<StringSource>,
    pub into: QualifiedName,
    pub pointer: Option<QualifiedName>,
    pub on_overflow: Vec<Statement>,
    pub not_on_overflow: Vec<Statement>,
    pub span: Span,
}

/// A source phrase in the STRING statement.
#[derive(Debug, Clone, PartialEq)]
pub struct StringSource {
    pub items: Vec<Expr>,
    pub delimited_by: StringDelimiter,
}

/// Delimiter specification for STRING.
#[derive(Debug, Clone, PartialEq)]
pub enum StringDelimiter {
    Size,
    Value(Expr),
}

/// UNSTRING statement: splits a data item into multiple targets.
#[derive(Debug, Clone, PartialEq)]
pub struct UnstringStatement {
    pub source: QualifiedName,
    pub delimiters: Vec<UnstringDelimiter>,
    pub into: Vec<UnstringTarget>,
    pub pointer: Option<QualifiedName>,
    pub tallying: Option<QualifiedName>,
    pub on_overflow: Vec<Statement>,
    pub not_on_overflow: Vec<Statement>,
    pub span: Span,
}

/// A delimiter specification for UNSTRING.
#[derive(Debug, Clone, PartialEq)]
pub struct UnstringDelimiter {
    pub all: bool,
    pub value: Expr,
}

/// A target specification for UNSTRING INTO.
#[derive(Debug, Clone, PartialEq)]
pub struct UnstringTarget {
    pub target: QualifiedName,
    pub delimiter_in: Option<QualifiedName>,
    pub count_in: Option<QualifiedName>,
}

/// INSPECT statement: examines and optionally replaces characters.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectStatement {
    pub target: QualifiedName,
    pub kind: InspectKind,
    pub span: Span,
}

/// The kind of INSPECT operation.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectKind {
    Tallying {
        tallying: Vec<InspectTallying>,
    },
    Replacing {
        replacing: Vec<InspectReplacing>,
    },
    TallyingReplacing {
        tallying: Vec<InspectTallying>,
        replacing: Vec<InspectReplacing>,
    },
    Converting {
        from: Box<Expr>,
        to: Box<Expr>,
        before_after: Vec<BeforeAfter>,
    },
}

/// A tallying phrase in INSPECT.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectTallying {
    pub counter: QualifiedName,
    pub kind: TallyingKind,
    pub before_after: Vec<BeforeAfter>,
}

/// Kind of tallying in INSPECT.
#[derive(Debug, Clone, PartialEq)]
pub enum TallyingKind {
    Characters,
    All(Expr),
    Leading(Expr),
}

/// A replacing phrase in INSPECT.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectReplacing {
    pub kind: ReplacingKind,
    pub before_after: Vec<BeforeAfter>,
}

/// Kind of replacing in INSPECT.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplacingKind {
    Characters(Expr),
    All { from: Expr, to: Expr },
    Leading { from: Expr, to: Expr },
    First { from: Expr, to: Expr },
}

/// BEFORE/AFTER INITIAL phrase for INSPECT.
#[derive(Debug, Clone, PartialEq)]
pub struct BeforeAfter {
    pub kind: BeforeAfterKind,
    pub value: Expr,
}

/// Whether a clause is BEFORE or AFTER.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeforeAfterKind {
    Before,
    After,
}

// ---------------------------------------------------------------------------
// Data initialization and manipulation statements
// ---------------------------------------------------------------------------

/// INITIALIZE statement: sets data items to default values.
#[derive(Debug, Clone, PartialEq)]
pub struct InitializeStatement {
    pub targets: Vec<QualifiedName>,
    pub replacing: Vec<InitializeReplacing>,
    pub with_filler: bool,
    pub span: Span,
}

/// A REPLACING phrase in INITIALIZE.
#[derive(Debug, Clone, PartialEq)]
pub struct InitializeReplacing {
    pub category: InitializeCategory,
    pub value: Expr,
}

/// Data categories for INITIALIZE REPLACING.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitializeCategory {
    Alphabetic,
    Alphanumeric,
    Numeric,
    AlphanumericEdited,
    NumericEdited,
    National,
    NationalEdited,
}

/// SET statement: assigns values to special-purpose items.
#[derive(Debug, Clone, PartialEq)]
pub struct SetStatement {
    pub kind: SetKind,
    pub span: Span,
}

/// The kind of SET operation.
#[derive(Debug, Clone, PartialEq)]
pub enum SetKind {
    /// SET identifier TO value.
    To {
        targets: Vec<QualifiedName>,
        value: Expr,
    },
    /// SET identifier UP/DOWN BY value.
    UpDown {
        targets: Vec<QualifiedName>,
        direction: SetDirection,
        value: Expr,
    },
    /// SET condition-name TO TRUE/FALSE.
    ConditionTrue {
        conditions: Vec<QualifiedName>,
        value: bool,
    },
    /// SET pointer TO ADDRESS OF identifier.
    Address {
        target: QualifiedName,
        source: QualifiedName,
    },
}

/// Direction for SET UP/DOWN BY.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetDirection {
    Up,
    Down,
}

// ---------------------------------------------------------------------------
// Table handling statements
// ---------------------------------------------------------------------------

/// SEARCH statement: serial or binary table lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchStatement {
    /// The table name to search.
    pub table_name: QualifiedName,
    /// True for SEARCH ALL (binary search).
    pub all: bool,
    /// VARYING clause: the index to vary during serial search.
    pub varying: Option<QualifiedName>,
    /// Statements executed when no WHEN condition is satisfied.
    pub at_end: Vec<Statement>,
    /// One or more WHEN clauses.
    pub when_clauses: Vec<SearchWhenClause>,
    pub span: Span,
}

/// A WHEN clause within a SEARCH statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchWhenClause {
    pub condition: Condition,
    pub body: Vec<Statement>,
}

// ---------------------------------------------------------------------------
// Sort/merge statements
// ---------------------------------------------------------------------------

/// SORT statement: sorts records.
#[derive(Debug, Clone, PartialEq)]
pub struct SortStatement {
    pub file_name: SmolStr,
    pub keys: Vec<SortKey>,
    pub duplicates: bool,
    pub input: SortInput,
    pub output: SortOutput,
    pub span: Span,
}

/// A sort key specification.
#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub order: SortOrder,
    pub fields: Vec<QualifiedName>,
}

/// Sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Ascending,
    Descending,
}

/// Input source for SORT.
#[derive(Debug, Clone, PartialEq)]
pub enum SortInput {
    Using(Vec<SmolStr>),
    InputProcedure {
        procedure: SmolStr,
        through: Option<SmolStr>,
    },
}

/// Output destination for SORT.
#[derive(Debug, Clone, PartialEq)]
pub enum SortOutput {
    Giving(Vec<SmolStr>),
    OutputProcedure {
        procedure: SmolStr,
        through: Option<SmolStr>,
    },
}

/// MERGE statement: merges sorted files.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeStatement {
    pub file_name: SmolStr,
    pub keys: Vec<SortKey>,
    pub using: Vec<SmolStr>,
    pub output: SortOutput,
    pub span: Span,
}

/// RELEASE statement: sends a record to the sort file.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseStatement {
    pub record_name: QualifiedName,
    pub from: Option<Expr>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Miscellaneous statements
// ---------------------------------------------------------------------------

/// CANCEL statement: releases resources for a called program.
#[derive(Debug, Clone, PartialEq)]
pub struct CancelStatement {
    pub programs: Vec<Expr>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// COBOL 2002+ statements
// ---------------------------------------------------------------------------

/// RAISE statement: raises an exception (COBOL 2002+).
#[derive(Debug, Clone, PartialEq)]
pub struct RaiseStatement {
    pub exception: RaiseTarget,
    pub span: Span,
}

/// Target of a RAISE statement.
#[derive(Debug, Clone, PartialEq)]
pub enum RaiseTarget {
    /// RAISE EXCEPTION exception-name.
    Exception(SmolStr),
    /// RAISE identifier (exception object).
    Identifier(QualifiedName),
}

/// RESUME statement: resumes execution after an exception (COBOL 2002+).
#[derive(Debug, Clone, PartialEq)]
pub struct ResumeStatement {
    pub target: Option<SmolStr>,
    pub span: Span,
}

/// INVOKE statement: invokes a method on an object (COBOL 2002+).
#[derive(Debug, Clone, PartialEq)]
pub struct InvokeStatement {
    pub object: Expr,
    pub method: Expr,
    pub using: Vec<CallParam>,
    pub returning: Option<QualifiedName>,
    pub span: Span,
}

/// ALLOCATE statement: dynamically allocates memory (COBOL 2002+).
#[derive(Debug, Clone, PartialEq)]
pub struct AllocateStatement {
    pub target: AllocateTarget,
    pub returning: Option<QualifiedName>,
    pub initialized: bool,
    pub span: Span,
}

/// Target for ALLOCATE statement.
#[derive(Debug, Clone, PartialEq)]
pub enum AllocateTarget {
    /// ALLOCATE data-name.
    DataName(QualifiedName),
    /// ALLOCATE n CHARACTERS.
    Characters(Expr),
}

/// FREE statement: releases allocated memory (COBOL 2002+).
#[derive(Debug, Clone, PartialEq)]
pub struct FreeStatement {
    pub targets: Vec<QualifiedName>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// COBOL 2014+ statements
// ---------------------------------------------------------------------------

/// JSON GENERATE statement (COBOL 2014+).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonGenerateStatement {
    pub target: QualifiedName,
    pub source: QualifiedName,
    pub count: Option<QualifiedName>,
    pub name_mapping: Vec<JsonNameMapping>,
    pub suppress: Vec<QualifiedName>,
    pub on_exception: Vec<Statement>,
    pub not_on_exception: Vec<Statement>,
    pub span: Span,
}

/// A name mapping for JSON GENERATE NAME OF.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonNameMapping {
    pub data_name: QualifiedName,
    pub json_name: SmolStr,
}

/// JSON PARSE statement (COBOL 2014+).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonParseStatement {
    pub source: QualifiedName,
    pub target: QualifiedName,
    pub name_mapping: Vec<JsonNameMapping>,
    pub on_exception: Vec<Statement>,
    pub not_on_exception: Vec<Statement>,
    pub span: Span,
}

/// XML GENERATE statement (COBOL 2014+).
#[derive(Debug, Clone, PartialEq)]
pub struct XmlGenerateStatement {
    pub target: QualifiedName,
    pub source: QualifiedName,
    pub count: Option<QualifiedName>,
    pub encoding: Option<Expr>,
    pub xml_declaration: bool,
    pub attributes: bool,
    pub namespace: Option<Expr>,
    pub namespace_prefix: Option<Expr>,
    pub name_mapping: Vec<XmlNameMapping>,
    pub suppress: Vec<QualifiedName>,
    pub on_exception: Vec<Statement>,
    pub not_on_exception: Vec<Statement>,
    pub span: Span,
}

/// A name mapping for XML GENERATE NAME OF.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlNameMapping {
    pub data_name: QualifiedName,
    pub xml_name: SmolStr,
}

/// XML PARSE statement (COBOL 2014+).
#[derive(Debug, Clone, PartialEq)]
pub struct XmlParseStatement {
    pub source: QualifiedName,
    pub processing_procedure: SmolStr,
    pub through: Option<SmolStr>,
    pub on_exception: Vec<Statement>,
    pub not_on_exception: Vec<Statement>,
    pub span: Span,
}

/// VALIDATE statement (COBOL 2014+).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidateStatement {
    pub target: QualifiedName,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Report writer statements
// ---------------------------------------------------------------------------

/// INITIATE statement: initializes one or more reports.
#[derive(Debug, Clone, PartialEq)]
pub struct InitiateStatement {
    pub report_names: Vec<SmolStr>,
    pub span: Span,
}

/// GENERATE statement: produces a report detail or summary line.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerateStatement {
    pub report_name: SmolStr,
    pub span: Span,
}

/// TERMINATE statement: terminates processing of one or more reports.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminateStatement {
    pub report_names: Vec<SmolStr>,
    pub span: Span,
}
