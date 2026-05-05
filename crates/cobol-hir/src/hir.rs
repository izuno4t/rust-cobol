// COBOL HIR - High-level intermediate representation
//
// A desugared, simplified view of a COBOL program. The HIR strips away
// COBOL division/section/paragraph structure and expresses the program
// as a flat list of typed data items and executable statements.

use cobol_common::Span;
use smol_str::SmolStr;

/// A COBOL data name with its qualification path preserved.
///
/// `qualifiers` are stored from innermost to outermost to match the parser AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HirDataName {
    pub name: SmolStr,
    pub qualifiers: Vec<SmolStr>,
}

impl HirDataName {
    pub fn new(name: SmolStr, qualifiers: Vec<SmolStr>) -> Self {
        Self { name, qualifiers }
    }

    pub fn simple(name: impl Into<SmolStr>) -> Self {
        Self {
            name: name.into(),
            qualifiers: Vec::new(),
        }
    }

    pub fn is_qualified(&self) -> bool {
        !self.qualifiers.is_empty()
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }

    pub fn to_uppercase(&self) -> String {
        self.name.to_uppercase()
    }

    /// Returns the qualification path ordered from outermost to innermost.
    pub fn qualifiers_outer_to_inner(&self) -> impl DoubleEndedIterator<Item = &SmolStr> {
        self.qualifiers.iter().rev()
    }
}

impl From<SmolStr> for HirDataName {
    fn from(value: SmolStr) -> Self {
        Self::simple(value)
    }
}

impl From<&str> for HirDataName {
    fn from(value: &str) -> Self {
        Self::simple(value)
    }
}

impl AsRef<str> for HirDataName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<str> for HirDataName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for HirDataName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirItemId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirParagraphId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirLabelId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct HirRefMod {
    pub start: Box<HirExpr>,
    pub length: Option<Box<HirExpr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirInitializeReplacing {
    pub category: HirInitializeCategory,
    pub value: HirExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirInitializeCategory {
    Alphabetic,
    Alphanumeric,
    Numeric,
    AlphanumericEdited,
    NumericEdited,
    National,
    NationalEdited,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HirDataRef {
    pub item_id: HirItemId,
    pub name: HirDataName,
    pub subscripts: Vec<HirExpr>,
    pub refmod: Option<HirRefMod>,
}

/// A HIR program -- the desugared form of a COBOL compilation unit.
#[derive(Debug, Clone)]
pub struct HirProgram {
    pub name: SmolStr,
    pub data_items: Vec<HirDataItem>,
    pub communication_descriptions: Vec<HirCommunicationDescription>,
    pub paragraphs: Vec<HirParagraph>,
    pub body: Vec<HirStatement>,
    /// PROCEDURE DIVISION USING parameters (non-empty means sub-program).
    pub using_params: Vec<HirParam>,
    /// COBOL 2002+: Class definitions.
    pub classes: Vec<HirClass>,
    /// COBOL 2002+: User-defined function definitions.
    pub functions: Vec<HirFunction>,
    /// COBOL 2014+: Type definitions (TYPEDEF).
    pub typedefs: Vec<HirTypedef>,
    /// COBOL 2023+: Interface definitions (INTERFACE-ID).
    pub interfaces: Vec<HirInterface>,
    /// File organization mapping: file_name → org value for runtime.
    pub file_organizations: std::collections::HashMap<SmolStr, u32>,
    /// File assignment mapping: file_name → ASSIGN TO path/name.
    pub file_assignments: std::collections::HashMap<SmolStr, SmolStr>,
    /// SELECT OPTIONAL mapping: files declared with OPTIONAL.
    pub file_optionals: std::collections::HashSet<SmolStr>,
    /// Relative key mapping: file_name → RELATIVE KEY variable name.
    pub file_relative_keys: std::collections::HashMap<SmolStr, SmolStr>,
    /// FILE STATUS variable mapping: file_name → status variable name.
    pub file_status_vars: Vec<HirFileInfo>,
    /// DECLARATIVES sections: USE AFTER EXCEPTION handlers for file I/O.
    pub declaratives: Vec<HirDeclarative>,
    /// FD/SD file name → first record name mapping.
    /// Used by codegen to determine the record buffer for READ without INTO.
    pub file_records: std::collections::HashMap<SmolStr, SmolStr>,
    /// Maps each additional FD record name to the first record name.
    /// Multiple 01-level items under the same FD share the same record buffer.
    pub fd_record_aliases: std::collections::HashMap<SmolStr, SmolStr>,
    /// FD file names with RECORD IS VARYING.
    pub variable_record_files: std::collections::HashSet<SmolStr>,
    /// FD file name → DEPENDING ON data item for RECORD IS VARYING.
    pub variable_record_depending: std::collections::HashMap<SmolStr, SmolStr>,
    /// FD file name → lower/upper record-size bounds for RECORD IS VARYING.
    pub variable_record_bounds: std::collections::HashMap<SmolStr, (u32, u32)>,
    /// I-O-CONTROL SAME RECORD AREA file groups.
    pub same_record_areas: Vec<Vec<SmolStr>>,
    /// SPECIAL-NAMES DECIMAL-POINT IS COMMA.
    pub decimal_point_is_comma: bool,
    /// SPECIAL-NAMES CLASS clauses.
    pub special_class_conditions: std::collections::HashMap<SmolStr, Vec<HirClassRange>>,
    /// PROGRAM COLLATING SEQUENCE ranks. Each inner vector has equal rank.
    pub program_collating_sequence: Option<Vec<Vec<SmolStr>>>,
    /// Nested programs (COBOL 85 inter-program communication).
    pub nested_programs: Vec<HirProgram>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirClassRange {
    pub from: SmolStr,
    pub to: SmolStr,
}

#[derive(Debug, Clone)]
pub struct HirCommunicationDescription {
    pub name: SmolStr,
    pub record_name: Option<SmolStr>,
    pub symbolic_queue: Option<SmolStr>,
    pub symbolic_sub_queue_1: Option<SmolStr>,
    pub symbolic_sub_queue_2: Option<SmolStr>,
    pub symbolic_sub_queue_3: Option<SmolStr>,
    pub status_key: Option<SmolStr>,
    pub message_count: Option<SmolStr>,
    pub text_length: Option<SmolStr>,
    pub end_key: Option<SmolStr>,
    pub error_key: Option<SmolStr>,
    pub symbolic_source: Option<SmolStr>,
    pub destination_count: Option<SmolStr>,
    pub destination: Option<SmolStr>,
    pub destination_table_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirDeclarativeUse {
    AfterException,
    ForDebugging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirCloseOption {
    Reel,
    Unit,
    WithNoRewind,
    WithLock,
}

/// A lowered DECLARATIVES section.
#[derive(Debug, Clone)]
pub struct HirDeclarative {
    pub name: SmolStr,
    pub use_kind: HirDeclarativeUse,
    pub is_global: bool,
    /// File names this declarative applies to.
    pub file_names: Vec<SmolStr>,
    /// Debug items this declarative applies to.
    pub debug_items: Vec<SmolStr>,
    /// Statements in the declarative body.
    pub body: Vec<HirStatement>,
}

/// Maps a COBOL file name to its FILE STATUS variable.
#[derive(Debug, Clone)]
pub struct HirFileInfo {
    pub file_name: SmolStr,
    pub status_var: SmolStr,
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

/// A CALL parameter with its passing mode.
#[derive(Debug, Clone)]
pub struct HirCallParam {
    pub expr: HirExpr,
    pub mode: HirParamMode,
}

/// A named paragraph from the PROCEDURE DIVISION, preserved for
/// PERFORM procedure-name support.
#[derive(Debug, Clone)]
pub struct HirParagraph {
    pub id: HirParagraphId,
    pub name: SmolStr,
    pub kind: HirParagraphKind,
    pub section_id: Option<HirParagraphId>,
    pub segment_number: Option<u32>,
    pub body: Vec<HirStatement>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirParagraphKind {
    Paragraph,
    Section,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirTransferTarget {
    Paragraph { id: HirParagraphId, name: SmolStr },
    Label { id: HirLabelId, name: SmolStr },
}

impl HirTransferTarget {
    pub fn name(&self) -> &str {
        match self {
            HirTransferTarget::Paragraph { name, .. } | HirTransferTarget::Label { name, .. } => {
                name.as_str()
            }
        }
    }

    pub fn paragraph_id(&self) -> Option<HirParagraphId> {
        match self {
            HirTransferTarget::Paragraph { id, .. } => Some(*id),
            HirTransferTarget::Label { .. } => None,
        }
    }
}

/// Screen section item attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct HirScreenInfo {
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub blank_screen: bool,
    pub blank_line: bool,
    pub highlight: bool,
    pub reverse_video: bool,
    pub source: Option<SmolStr>,
    pub using_field: Option<SmolStr>,
    pub value: Option<SmolStr>,
    pub picture: Option<SmolStr>,
}

/// A data item declaration extracted from the DATA DIVISION.
#[derive(Debug, Clone, PartialEq)]
pub struct HirDataItem {
    pub name: SmolStr,
    pub data_type: HirType,
    pub picture: Option<SmolStr>,
    pub is_numeric_edited: bool,
    pub sign: Option<HirSignClause>,
    pub blank_when_zero: bool,
    pub scale_adjustment: i32,
    pub is_external: bool,
    pub initial_value: Option<HirLiteral>,
    /// OCCURS clause: number of repetitions (None = scalar).
    pub occurs: Option<u32>,
    /// OCCURS DEPENDING ON data item, when present.
    pub occurs_depending_on: Option<SmolStr>,
    /// INDEXED BY names from OCCURS clause.
    pub indexed_by: Vec<SmolStr>,
    /// REDEFINES clause: the name of the redefined item.
    pub redefines: Option<SmolStr>,
    /// RENAMES clause (level 66): (from_name, optional thru_name).
    pub renames: Option<(SmolStr, Option<SmolStr>)>,
    /// SCREEN SECTION attributes (None for non-screen items).
    pub screen_info: Option<HirScreenInfo>,
    /// JUSTIFIED RIGHT clause.
    pub justified: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HirSignClause {
    pub position: HirSignPosition,
    pub separate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirSignPosition {
    Leading,
    Trailing,
}

impl HirDataItem {
    /// Creates a synthetic data item with neutral COBOL metadata defaults.
    ///
    /// Lowering real source data items should still fill source-derived
    /// metadata explicitly. This constructor is for implicit registers and
    /// codegen tests, where the HIR/data layout contract should not be copied
    /// field-by-field at every call site.
    pub fn new(name: impl Into<SmolStr>, data_type: HirType, span: Span) -> Self {
        Self {
            name: name.into(),
            data_type,
            picture: None,
            is_numeric_edited: false,
            sign: None,
            blank_when_zero: false,
            scale_adjustment: 0,
            is_external: false,
            initial_value: None,
            occurs: None,
            occurs_depending_on: None,
            indexed_by: Vec::new(),
            redefines: None,
            renames: None,
            screen_info: None,
            justified: false,
            span,
        }
    }

    pub fn with_initial_value(mut self, initial_value: HirLiteral) -> Self {
        self.initial_value = Some(initial_value);
        self
    }
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
    /// COBOL 2002+: National (PIC N) – UTF-16 character data.
    National {
        size: u32,
    },
}

/// A literal value in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirLiteral {
    Integer(i64),
    Decimal(String),
    String(SmolStr),
    Zero,
    Space,
    /// ALL "X": fill target with repeated character/string.
    AllChar(SmolStr),
    /// HIGH-VALUE / HIGH-VALUES: all bits 1 (0xFF per byte).
    HighValue,
    /// LOW-VALUE / LOW-VALUES: all bits 0 (0x00 per byte).
    LowValue,
    /// QUOTE / QUOTES: double-quote character per byte.
    Quote,
    /// NULL / NULLS: null pointer.
    Null,
}

/// A MOVE target, which may be a plain variable or a reference-modified variable.
#[derive(Debug, Clone)]
pub enum HirMoveTarget {
    DataRef(HirDataRef),
    /// A simple variable name (e.g., `WS-NAME`).
    Variable(HirDataName),
    /// A reference-modified variable (e.g., `WS-NAME(3:5)`).
    ReferenceModification {
        variable: HirDataName,
        start: HirExpr,
        length: Option<HirExpr>,
    },
    /// A subscripted table element (e.g., `WS-TABLE(3)`).
    Subscript {
        variable: HirDataName,
        subscripts: Vec<HirExpr>,
    },
}

/// Source for ACCEPT statement.
#[derive(Debug, Clone, PartialEq)]
pub enum HirAcceptSource {
    Console,
    Date,
    DateYyyymmdd,
    Day,
    DayOfWeek,
    Time,
    Environment(SmolStr),
    MessageCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirCommunicationMode {
    Input,
    Output,
    InputOutput,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HirSendOption {
    Emi,
    Egi,
    Esi,
    Identifier(HirExpr),
}

#[derive(Debug, Clone)]
pub enum HirWriteAdvancing {
    Lines(HirExpr),
    Page,
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
    SetConditionTrue {
        assignments: Vec<(HirMoveTarget, HirExpr)>,
        span: Span,
    },
    SetSwitchStatus {
        assignments: Vec<(SmolStr, bool)>,
        span: Span,
    },
    /// MOVE CORRESPONDING: move matching fields from source group to target group.
    MoveCorresponding {
        from: HirDataName,
        from_subscripts: Vec<HirExpr>,
        to: HirDataName,
        to_subscripts: Vec<HirExpr>,
        span: Span,
    },
    /// ADD CORRESPONDING: add matching numeric fields from source to target group.
    AddCorresponding {
        from: HirDataName,
        to: HirDataName,
        rounded: bool,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    /// SUBTRACT CORRESPONDING: subtract matching numeric fields from source to target.
    SubtractCorresponding {
        from: HirDataName,
        to: HirDataName,
        rounded: bool,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Compute {
        targets: Vec<HirExpr>,
        target_rounded: Vec<bool>,
        expr: HirExpr,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Add {
        operands: Vec<HirExpr>,
        to: Vec<HirExpr>,
        to_rounded: Vec<bool>,
        giving: Vec<HirExpr>,
        giving_rounded: Vec<bool>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Subtract {
        operands: Vec<HirExpr>,
        from: Vec<HirExpr>,
        from_rounded: Vec<bool>,
        giving: Vec<HirExpr>,
        giving_rounded: Vec<bool>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Multiply {
        operand: HirExpr,
        by: Vec<HirExpr>,
        by_rounded: Vec<bool>,
        giving: Vec<HirExpr>,
        giving_rounded: Vec<bool>,
        on_size_error: Vec<HirStatement>,
        not_on_size_error: Vec<HirStatement>,
        span: Span,
    },
    Divide {
        operand: HirExpr,
        into: Vec<HirExpr>,
        into_rounded: Vec<bool>,
        giving: Vec<HirExpr>,
        giving_rounded: Vec<bool>,
        remainder: Option<HirExpr>,
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
        kind: Box<HirPerformKind>,
        span: Span,
    },
    Call {
        program: HirExpr,
        params: Vec<HirCallParam>,
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
    ExitParagraph {
        span: Span,
    },
    Goback {
        span: Span,
    },
    Alter {
        pairs: Vec<(HirTransferTarget, HirTransferTarget)>,
        span: Span,
    },
    Continue {
        span: Span,
    },
    /// A label marker for a paragraph in the body (used for GO TO targets).
    Label {
        target: HirTransferTarget,
    },
    /// OPEN statement.
    Open {
        entries: Vec<HirOpenEntry>,
        span: Span,
    },
    /// CLOSE statement.
    Close {
        files: Vec<SmolStr>,
        close_options: Vec<Option<HirCloseOption>>,
        span: Span,
    },
    /// READ statement.
    Read {
        file_name: SmolStr,
        is_next: bool,
        /// INTO target: (variable_name, subscripts).
        into: Option<(SmolStr, Vec<HirExpr>)>,
        /// Optional key for random/dynamic READ.
        key: Option<SmolStr>,
        at_end: Vec<HirStatement>,
        not_at_end: Vec<HirStatement>,
        invalid_key: Vec<HirStatement>,
        not_invalid_key: Vec<HirStatement>,
        span: Span,
    },
    /// WRITE statement.
    Write {
        record_name: SmolStr,
        /// The file name this record belongs to (for FILE_ID resolution).
        file_name: SmolStr,
        from: Option<HirExpr>,
        advancing: Option<HirWriteAdvancing>,
        invalid_key: Vec<HirStatement>,
        not_invalid_key: Vec<HirStatement>,
        at_eop: Vec<HirStatement>,
        not_at_eop: Vec<HirStatement>,
        span: Span,
    },
    /// REWRITE statement.
    Rewrite {
        record_name: SmolStr,
        /// The file name this record belongs to (for FILE_ID resolution).
        file_name: SmolStr,
        from: Option<HirExpr>,
        invalid_key: Vec<HirStatement>,
        not_invalid_key: Vec<HirStatement>,
        span: Span,
    },
    /// DELETE statement.
    Delete {
        file_name: SmolStr,
        invalid_key: Vec<HirStatement>,
        not_invalid_key: Vec<HirStatement>,
        span: Span,
    },
    /// GO TO statement.
    GoTo {
        targets: Vec<HirTransferTarget>,
        depending_on: Option<HirExpr>,
        span: Span,
    },
    /// INITIALIZE statement.
    Initialize {
        targets: Vec<SmolStr>,
        replacing: Vec<HirInitializeReplacing>,
        span: Span,
    },
    /// SET statement (simplified to assignment).
    Set {
        targets: Vec<HirExpr>,
        value: HirExpr,
        span: Span,
    },
    /// SET ADDRESS OF target TO source — pointer assignment.
    SetAddress {
        target: SmolStr,
        source: SmolStr,
        span: Span,
    },
    /// STRING statement.
    StringStmt {
        into: SmolStr,
        sources: Vec<HirStringSource>,
        pointer: Option<SmolStr>,
        on_overflow: Vec<HirStatement>,
        not_on_overflow: Vec<HirStatement>,
        span: Span,
    },
    /// UNSTRING statement.
    UnstringStmt {
        source: SmolStr,
        delimiters: Vec<HirUnstringDelimiter>,
        into: Vec<HirUnstringTarget>,
        pointer: Option<SmolStr>,
        tallying: Option<SmolStr>,
        on_overflow: Vec<HirStatement>,
        not_on_overflow: Vec<HirStatement>,
        span: Span,
    },
    /// ACCEPT statement.
    Accept {
        target: HirExpr,
        source: HirAcceptSource,
        span: Span,
    },
    Enable {
        mode: HirCommunicationMode,
        terminal: bool,
        target: SmolStr,
        key: HirExpr,
        span: Span,
    },
    Disable {
        mode: HirCommunicationMode,
        terminal: bool,
        target: SmolStr,
        key: HirExpr,
        span: Span,
    },
    Send {
        target: SmolStr,
        from: Option<HirExpr>,
        with: Option<HirSendOption>,
        replacing_line: bool,
        span: Span,
    },
    Receive {
        target: SmolStr,
        mode: HirReceiveMode,
        into: SmolStr,
        no_data: Vec<HirStatement>,
        span: Span,
    },
    Purge {
        target: SmolStr,
        span: Span,
    },
    /// SORT statement.
    Sort {
        file_name: SmolStr,
        keys: Vec<HirSortKey>,
        using: Vec<SmolStr>,
        giving: Vec<SmolStr>,
        /// INPUT PROCEDURE name and optional THRU.
        input_procedure: Option<(SmolStr, Option<SmolStr>)>,
        /// OUTPUT PROCEDURE name and optional THRU.
        output_procedure: Option<(SmolStr, Option<SmolStr>)>,
        span: Span,
    },
    /// INSPECT statement.
    Inspect {
        target: HirExpr,
        kind: HirInspectKind,
        span: Span,
    },
    // --- Table handling ---
    /// SEARCH statement: serial or binary table search.
    Search {
        table_name: SmolStr,
        /// True for SEARCH ALL (binary search).
        all: bool,
        /// VARYING clause: the index to vary.
        varying: Option<SmolStr>,
        /// AT END statements.
        at_end: Vec<HirStatement>,
        /// WHEN clauses: (condition, body).
        when_clauses: Vec<HirSearchWhen>,
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
        /// For ALLOCATE n CHARACTERS, the character count expression.
        char_count: Option<HirExpr>,
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
        /// INTO target: (variable_name, subscripts).
        into: Option<(SmolStr, Vec<HirExpr>)>,
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
        /// OUTPUT PROCEDURE name and optional THRU.
        output_procedure: Option<(SmolStr, Option<SmolStr>)>,
        span: Span,
    },
    /// RELEASE statement: sends a record to the sort file.
    Release {
        record_name: SmolStr,
        from: Option<HirExpr>,
        span: Span,
    },
    // --- Report writer statements ---
    /// INITIATE statement: initializes report processing.
    Initiate {
        report_names: Vec<SmolStr>,
        span: Span,
    },
    /// GENERATE statement: produces report detail or summary lines.
    Generate {
        report_name: SmolStr,
        span: Span,
    },
    /// TERMINATE statement: terminates report processing.
    Terminate {
        report_names: Vec<SmolStr>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirReceiveMode {
    Message,
    Segment,
}

/// An expression in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirExpr {
    Literal(HirLiteral),
    DataRef(HirDataRef),
    Variable(HirDataName),
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
        variable: HirDataName,
        start: Box<HirExpr>,
        length: Option<Box<HirExpr>>,
    },
    /// Subscripted table access: `TABLE(idx1, idx2, ...)`.
    ///
    /// Subscripts are 1-based COBOL indices.
    Subscript {
        variable: HirDataName,
        subscripts: Vec<HirExpr>,
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

/// Class condition types for IS NUMERIC, IS ALPHABETIC, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirClassType {
    Numeric,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
    Custom(SmolStr),
}

/// A conditional expression in the HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirCondition {
    Compare {
        left: HirExpr,
        op: HirCompareOp,
        right: HirExpr,
    },
    /// Class condition: IS NUMERIC, IS ALPHABETIC, etc.
    ClassCondition {
        operand: HirExpr,
        class: HirClassType,
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
#[allow(clippy::large_enum_variant)]
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
        test: HirPerformTest,
        condition: HirCondition,
        body: Vec<HirStatement>,
    },
    /// PERFORM ... VARYING.
    Varying {
        test: HirPerformTest,
        var: SmolStr,
        var_expr: HirExpr,
        from: HirExpr,
        by: HirExpr,
        until: HirCondition,
        after_clauses: Vec<HirVaryingAfter>,
        body: Vec<HirStatement>,
    },
    /// PERFORM procedure-name [THRU procedure-name].
    ProcedureName {
        target: HirTransferTarget,
        through: Option<HirTransferTarget>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirPerformTest {
    Before,
    After,
}

/// An AFTER clause in PERFORM VARYING ... AFTER.
#[derive(Debug, Clone)]
pub struct HirVaryingAfter {
    pub var: SmolStr,
    pub var_expr: HirExpr,
    pub from: HirExpr,
    pub by: HirExpr,
    pub until: HirCondition,
}

/// A file open entry.
#[derive(Debug, Clone)]
pub struct HirOpenEntry {
    pub mode: HirOpenMode,
    pub file_name: SmolStr,
    /// ASSIGN TO path (physical file name or device).
    pub assign_to: SmolStr,
    /// SELECT OPTIONAL clause.
    pub optional: bool,
    /// File organization: 0=Sequential, 1=LineSequential, 2=Relative, 3=Indexed.
    pub organization: u32,
    /// Access mode: 0=Sequential, 1=Random, 2=Dynamic.
    pub access_mode: u32,
    /// Primary record key variable for indexed files.
    pub record_key: Option<SmolStr>,
    /// Alternate record key variables for indexed files.
    pub alternate_keys: Vec<HirAlternateKey>,
    /// Relative key variable for relative files.
    pub relative_key: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirAlternateKey {
    pub name: SmolStr,
    pub duplicates: bool,
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

/// A source operand for STRING, optionally with a DELIMITED BY clause.
#[derive(Debug, Clone)]
pub struct HirStringSource {
    pub value: HirExpr,
    /// `None` means DELIMITED BY SIZE (the whole field).
    pub delimiter: Option<HirExpr>,
}

/// A delimiter specification for UNSTRING.
#[derive(Debug, Clone)]
pub struct HirUnstringDelimiter {
    pub all: bool,
    pub value: HirExpr,
}

#[derive(Debug, Clone)]
pub struct HirUnstringTarget {
    pub target: SmolStr,
    pub delimiter_in: Option<SmolStr>,
    pub count_in: Option<SmolStr>,
}

/// A WHEN clause in a SEARCH statement.
#[derive(Debug, Clone)]
pub struct HirSearchWhen {
    pub condition: HirCondition,
    pub body: Vec<HirStatement>,
}

/// Relational operator for START key comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirStartRelation {
    Equal,
    GreaterThan,
    GreaterEqual,
    NotLessThan,
}

/// The kind of INSPECT operation.
#[derive(Debug, Clone)]
pub enum HirInspectKind {
    /// INSPECT TALLYING -- count occurrences.
    Tallying { tallying: Vec<HirInspectTallying> },
    /// INSPECT REPLACING -- replace characters.
    Replacing { replacing: Vec<HirInspectReplacing> },
    /// INSPECT TALLYING AND REPLACING.
    TallyingReplacing {
        tallying: Vec<HirInspectTallying>,
        replacing: Vec<HirInspectReplacing>,
    },
    /// INSPECT CONVERTING.
    Converting {
        from: HirExpr,
        to: HirExpr,
        before_after: Vec<HirBeforeAfter>,
    },
}

/// A tallying phrase in INSPECT TALLYING.
#[derive(Debug, Clone)]
pub struct HirInspectTallying {
    pub counter: HirExpr,
    pub kind: HirTallyingKind,
    pub before_after: Vec<HirBeforeAfter>,
}

/// Kind of tallying in INSPECT.
#[derive(Debug, Clone)]
pub enum HirTallyingKind {
    Characters,
    All(HirExpr),
    Leading(HirExpr),
    Trailing(HirExpr),
}

/// A replacing phrase in INSPECT REPLACING.
#[derive(Debug, Clone)]
pub struct HirInspectReplacing {
    pub kind: HirReplacingKind,
    pub before_after: Vec<HirBeforeAfter>,
}

/// Kind of replacing in INSPECT.
#[derive(Debug, Clone)]
pub enum HirReplacingKind {
    Characters(HirExpr),
    All { from: HirExpr, to: HirExpr },
    Leading { from: HirExpr, to: HirExpr },
    First { from: HirExpr, to: HirExpr },
}

/// BEFORE/AFTER INITIAL phrase for INSPECT.
#[derive(Debug, Clone)]
pub struct HirBeforeAfter {
    pub is_before: bool,
    pub value: HirExpr,
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
        if !self.nested_programs.is_empty() {
            writeln!(f, "  Nested Programs:")?;
            for nested in &self.nested_programs {
                writeln!(f, "    {}", nested.name)?;
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
        HirStatement::Enable {
            mode,
            terminal,
            target,
            key,
            ..
        } => {
            let mode = match mode {
                HirCommunicationMode::Input => "INPUT",
                HirCommunicationMode::Output => "OUTPUT",
                HirCommunicationMode::InputOutput => "I-O",
            };
            let terminal = if *terminal { " TERMINAL" } else { "" };
            writeln!(
                f,
                "{pad}ENABLE {mode}{terminal} {target} WITH KEY {}",
                format_expr(key)
            )
        }
        HirStatement::Disable {
            mode,
            terminal,
            target,
            key,
            ..
        } => {
            let mode = match mode {
                HirCommunicationMode::Input => "INPUT",
                HirCommunicationMode::Output => "OUTPUT",
                HirCommunicationMode::InputOutput => "I-O",
            };
            let terminal = if *terminal { " TERMINAL" } else { "" };
            writeln!(
                f,
                "{pad}DISABLE {mode}{terminal} {target} WITH KEY {}",
                format_expr(key)
            )
        }
        HirStatement::Send {
            target,
            from,
            with,
            replacing_line,
            ..
        } => {
            write!(f, "{pad}SEND {target}")?;
            if let Some(from) = from {
                write!(f, " FROM {}", format_expr(from))?;
            }
            if let Some(with) = with {
                let with = match with {
                    HirSendOption::Emi => "EMI",
                    HirSendOption::Egi => "EGI",
                    HirSendOption::Esi => "ESI",
                    HirSendOption::Identifier(expr) => {
                        return writeln!(
                            f,
                            "{pad}SEND {target}{} WITH {}{}",
                            from.as_ref()
                                .map(|from| format!(" FROM {}", format_expr(from)))
                                .unwrap_or_default(),
                            format_expr(expr),
                            if *replacing_line {
                                " REPLACING LINE"
                            } else {
                                ""
                            }
                        );
                    }
                };
                write!(f, " WITH {with}")?;
            }
            if *replacing_line {
                write!(f, " REPLACING LINE")?;
            }
            writeln!(f)
        }
        HirStatement::Receive {
            target,
            mode,
            into,
            no_data,
            ..
        } => {
            let mode = match mode {
                HirReceiveMode::Message => "MESSAGE",
                HirReceiveMode::Segment => "SEGMENT",
            };
            write!(f, "{pad}RECEIVE {target} {mode} INTO {into}")?;
            if let Some(first) = no_data.first() {
                write!(f, " NO DATA ")?;
                write_stmt(f, first, indent)?;
                return Ok(());
            }
            writeln!(f)
        }
        HirStatement::Purge { target, .. } => {
            writeln!(f, "{pad}PURGE {target}")
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
        HirStatement::SetConditionTrue { assignments, .. } => {
            let rendered: Vec<_> = assignments
                .iter()
                .map(|(target, value)| {
                    format!("{} TO {}", format_move_target(target), format_expr(value))
                })
                .collect();
            writeln!(f, "{pad}SET CONDITION {}", rendered.join(", "))
        }
        HirStatement::SetSwitchStatus { assignments, .. } => {
            let rendered: Vec<_> = assignments
                .iter()
                .map(|(target, value)| format!("{target} TO {}", if *value { "ON" } else { "OFF" }))
                .collect();
            writeln!(f, "{pad}SET {}", rendered.join(" "))
        }
        HirStatement::MoveCorresponding { from, to, .. } => {
            writeln!(
                f,
                "{pad}MOVE CORRESPONDING {} TO {}",
                format_data_name(from),
                format_data_name(to)
            )
        }
        HirStatement::AddCorresponding { from, to, .. } => {
            writeln!(
                f,
                "{pad}ADD CORRESPONDING {} TO {}",
                format_data_name(from),
                format_data_name(to)
            )
        }
        HirStatement::SubtractCorresponding { from, to, .. } => {
            writeln!(
                f,
                "{pad}SUBTRACT CORRESPONDING {} FROM {}",
                format_data_name(from),
                format_data_name(to)
            )
        }
        HirStatement::Compute { targets, expr, .. } => {
            let tgt_strs: Vec<_> = targets.iter().map(format_expr).collect();
            writeln!(
                f,
                "{pad}COMPUTE {} = {}",
                tgt_strs.join(", "),
                format_expr(expr)
            )
        }
        HirStatement::Add { operands, to, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            let tos: Vec<_> = to.iter().map(format_expr).collect();
            writeln!(f, "{pad}ADD {} TO {}", ops.join(" "), tos.join(", "))
        }
        HirStatement::Subtract { operands, from, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            let froms: Vec<_> = from.iter().map(format_expr).collect();
            writeln!(
                f,
                "{pad}SUBTRACT {} FROM {}",
                ops.join(" "),
                froms.join(", ")
            )
        }
        HirStatement::Multiply { operand, by, .. } => {
            let bys: Vec<_> = by.iter().map(format_expr).collect();
            writeln!(
                f,
                "{pad}MULTIPLY {} BY {}",
                format_expr(operand),
                bys.join(", ")
            )
        }
        HirStatement::Divide { operand, into, .. } => {
            let intos: Vec<_> = into.iter().map(format_expr).collect();
            writeln!(
                f,
                "{pad}DIVIDE {} INTO {}",
                format_expr(operand),
                intos.join(", ")
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
            writeln!(
                f,
                "{pad}PERFORM {:?}",
                std::mem::discriminant(kind.as_ref())
            )
        }
        HirStatement::Call { program, .. } => {
            writeln!(f, "{pad}CALL {}", format_expr(program))
        }
        HirStatement::StopRun { .. } => writeln!(f, "{pad}STOP RUN"),
        HirStatement::ExitProgram { .. } => writeln!(f, "{pad}EXIT PROGRAM"),
        HirStatement::ExitParagraph { .. } => writeln!(f, "{pad}EXIT PARAGRAPH"),
        HirStatement::Goback { .. } => writeln!(f, "{pad}GOBACK"),
        HirStatement::Alter { pairs, .. } => {
            let pairs = pairs
                .iter()
                .map(|(from, to)| format!("{} TO {}", from.name(), to.name()))
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(f, "{pad}ALTER {pairs}")
        }
        HirStatement::Continue { .. } => writeln!(f, "{pad}CONTINUE"),
        HirStatement::Label { target } => writeln!(f, "{pad}{}.", target.name()),
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
                    .map(|target| target.name())
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
                    .map(format_expr)
                    .collect::<Vec<_>>()
                    .join(", "),
                format_expr(value)
            )
        }
        HirStatement::SetAddress { target, source, .. } => {
            writeln!(f, "{pad}SET ADDRESS OF {target} TO {source}")
        }
        HirStatement::StringStmt { into, .. } => writeln!(f, "{pad}STRING INTO {into}"),
        HirStatement::UnstringStmt { source, .. } => writeln!(f, "{pad}UNSTRING {source}"),
        HirStatement::Accept { target, .. } => writeln!(f, "{pad}ACCEPT {target:?}"),
        HirStatement::Sort { file_name, .. } => writeln!(f, "{pad}SORT {file_name}"),
        HirStatement::Inspect { .. } => writeln!(f, "{pad}INSPECT"),
        HirStatement::Search {
            table_name, all, ..
        } => {
            if *all {
                writeln!(f, "{pad}SEARCH ALL {table_name}")
            } else {
                writeln!(f, "{pad}SEARCH {table_name}")
            }
        }
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
        HirStatement::Initiate { report_names, .. } => {
            writeln!(f, "{pad}INITIATE {}", report_names.join(", "))
        }
        HirStatement::Generate { report_name, .. } => {
            writeln!(f, "{pad}GENERATE {report_name}")
        }
        HirStatement::Terminate { report_names, .. } => {
            writeln!(f, "{pad}TERMINATE {}", report_names.join(", "))
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
            HirLiteral::HighValue => "HIGH-VALUE".to_string(),
            HirLiteral::LowValue => "LOW-VALUE".to_string(),
            HirLiteral::Quote => "QUOTE".to_string(),
            HirLiteral::Null => "NULL".to_string(),
            HirLiteral::AllChar(s) => format!("ALL \"{}\"", s),
        },
        HirExpr::DataRef(data_ref) => format_data_ref(data_ref),
        HirExpr::Variable(name) => format_data_name(name),
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
                format!(
                    "{}({}:{})",
                    format_data_name(variable),
                    format_expr(start),
                    format_expr(len)
                )
            } else {
                format!("{}({}:)", format_data_name(variable), format_expr(start))
            }
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            let subs: Vec<_> = subscripts.iter().map(format_expr).collect();
            format!("{}({})", format_data_name(variable), subs.join(", "))
        }
    }
}

fn format_move_target(target: &HirMoveTarget) -> String {
    match target {
        HirMoveTarget::DataRef(data_ref) => format_data_ref(data_ref),
        HirMoveTarget::Variable(name) => format_data_name(name),
        HirMoveTarget::ReferenceModification {
            variable,
            start,
            length,
        } => {
            if let Some(len) = length {
                format!(
                    "{}({}:{})",
                    format_data_name(variable),
                    format_expr(start),
                    format_expr(len)
                )
            } else {
                format!("{}({}:)", format_data_name(variable), format_expr(start))
            }
        }
        HirMoveTarget::Subscript {
            variable,
            subscripts,
        } => {
            let subs: Vec<_> = subscripts.iter().map(format_expr).collect();
            format!("{}({})", format_data_name(variable), subs.join(", "))
        }
    }
}

fn format_data_name(name: &HirDataName) -> String {
    if name.qualifiers.is_empty() {
        name.name.to_string()
    } else {
        let mut parts = Vec::with_capacity(name.qualifiers.len() + 1);
        parts.push(name.name.to_string());
        parts.extend(name.qualifiers.iter().map(ToString::to_string));
        parts.join(" OF ")
    }
}

fn format_data_ref(data_ref: &HirDataRef) -> String {
    let mut rendered = format_data_name(&data_ref.name);
    if !data_ref.subscripts.is_empty() {
        let subs: Vec<_> = data_ref.subscripts.iter().map(format_expr).collect();
        rendered.push('(');
        rendered.push_str(&subs.join(", "));
        rendered.push(')');
    }
    if let Some(refmod) = &data_ref.refmod {
        let start = format_expr(&refmod.start);
        if let Some(length) = &refmod.length {
            rendered.push('(');
            rendered.push_str(&format!("{start}:{}", format_expr(length)));
            rendered.push(')');
        } else {
            rendered.push('(');
            rendered.push_str(&format!("{start}:"));
            rendered.push(')');
        }
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_data_item_uses_neutral_metadata_defaults() {
        let item = HirDataItem::new("WS-FIELD", HirType::Alphanumeric { size: 4 }, Span::dummy());

        assert_eq!(item.name.as_str(), "WS-FIELD");
        assert_eq!(item.picture, None);
        assert!(!item.is_numeric_edited);
        assert!(!item.blank_when_zero);
        assert_eq!(item.scale_adjustment, 0);
        assert!(!item.is_external);
        assert_eq!(item.initial_value, None);
        assert_eq!(item.occurs, None);
        assert!(item.indexed_by.is_empty());
        assert_eq!(item.redefines, None);
        assert_eq!(item.renames, None);
        assert_eq!(item.screen_info, None);
        assert!(!item.justified);
    }

    #[test]
    fn synthetic_data_item_can_set_initial_value() {
        let item = HirDataItem::new(
            "SWITCH-STATE",
            HirType::Numeric {
                size: 1,
                decimal_places: 0,
                is_signed: false,
            },
            Span::dummy(),
        )
        .with_initial_value(HirLiteral::Integer(0));

        assert_eq!(item.initial_value, Some(HirLiteral::Integer(0)));
    }
}
