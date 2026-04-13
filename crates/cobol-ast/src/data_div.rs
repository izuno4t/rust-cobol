// COBOL AST - DATA DIVISION (sections and data items)

use cobol_common::Span;
use smol_str::SmolStr;

use crate::expr::{Literal, QualifiedName};
use crate::picture::PictureClause;

/// The DATA DIVISION describes all data used by the program.
///
/// Contains multiple sections, each serving a different purpose
/// (file buffers, working storage, parameters, screen definitions, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct DataDivision {
    /// FILE SECTION: file buffer record descriptions.
    pub file_section: Vec<FileDescription>,
    /// WORKING-STORAGE SECTION: persistent program data.
    pub working_storage: Vec<DataItem>,
    /// LOCAL-STORAGE SECTION: per-invocation data (COBOL 2002+).
    pub local_storage: Vec<DataItem>,
    /// LINKAGE SECTION: data received via CALL USING.
    pub linkage: Vec<DataItem>,
    /// SCREEN SECTION: screen layout definitions.
    pub screen: Vec<DataItem>,
    /// COMMUNICATION SECTION: message handling definitions.
    pub communication: Vec<CommunicationDescription>,
    /// REPORT SECTION: report writer definitions.
    pub report: Vec<DataItem>,
    pub span: Span,
}

/// A CD entry in the COMMUNICATION SECTION.
#[derive(Debug, Clone, PartialEq)]
pub struct CommunicationDescription {
    pub name: SmolStr,
    pub direction: CommunicationDirection,
    pub symbolic_queue: Option<SmolStr>,
    pub symbolic_sub_queue_1: Option<SmolStr>,
    pub symbolic_sub_queue_2: Option<SmolStr>,
    pub symbolic_sub_queue_3: Option<SmolStr>,
    pub message_date: Option<SmolStr>,
    pub message_time: Option<SmolStr>,
    pub symbolic_source: Option<SmolStr>,
    pub text_length: Option<SmolStr>,
    pub end_key: Option<SmolStr>,
    pub status_key: Option<SmolStr>,
    pub message_count: Option<SmolStr>,
    pub destination_count: Option<SmolStr>,
    pub destination_table_count: Option<u32>,
    pub destination_table_indexed_by: Vec<SmolStr>,
    pub error_key: Option<SmolStr>,
    pub destination: Option<SmolStr>,
    pub data_items: Vec<DataItem>,
    pub span: Span,
}

/// Direction/type of communication description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunicationDirection {
    Input,
    Output,
    InitialInput,
    InputOutput,
}

/// A file description (FD) or sort description (SD) entry.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDescription {
    /// Whether this is an FD or SD entry.
    pub fd_or_sd: FdType,
    /// The file name matching a SELECT in FILE-CONTROL.
    pub file_name: SmolStr,
    /// EXTERNAL attribute on the FD/SD entry.
    pub is_external: bool,
    /// BLOCK CONTAINS clause.
    pub block_contains: Option<BlockContains>,
    /// RECORD CONTAINS clause.
    pub record_contains: Option<RecordContains>,
    /// RECORD IS VARYING clause.
    pub record_varying: Option<RecordVarying>,
    /// LABEL RECORDS clause (deprecated in COBOL 2002).
    pub label_records: Option<LabelRecords>,
    /// DATA RECORDS clause (deprecated in COBOL 2002).
    pub data_records: Vec<SmolStr>,
    /// RECORDING MODE clause (implementation-specific).
    pub recording_mode: Option<SmolStr>,
    /// LINAGE clause.
    pub linage: Option<LinageClause>,
    /// The record description entries under this FD/SD.
    pub items: Vec<DataItem>,
    pub span: Span,
}

/// File description type: FD (file) or SD (sort).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdType {
    Fd,
    Sd,
}

/// BLOCK CONTAINS clause specifying physical block size.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockContains {
    pub min: Option<u32>,
    pub max: u32,
    pub unit: BlockUnit,
}

/// Unit for BLOCK CONTAINS clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockUnit {
    Records,
    Characters,
}

/// RECORD CONTAINS clause specifying logical record size.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordContains {
    pub min: Option<u32>,
    pub max: u32,
}

/// LABEL RECORDS clause values (deprecated in COBOL 2002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRecords {
    Standard,
    Omitted,
}

/// RECORD IS VARYING IN SIZE clause.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordVarying {
    pub min: Option<u32>,
    pub max: Option<u32>,
    /// DEPENDING ON data-name.
    pub depending_on: Option<SmolStr>,
}

/// LINAGE IS clause for logical page layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LinageClause {
    /// Number of lines in the page body.
    pub lines: LinageValue,
    /// WITH FOOTING AT line-number.
    pub footing: Option<LinageValue>,
    /// LINES AT TOP count.
    pub top: Option<LinageValue>,
    /// LINES AT BOTTOM count.
    pub bottom: Option<LinageValue>,
}

/// A LINAGE value can be an integer literal or a data-name reference.
#[derive(Debug, Clone, PartialEq)]
pub enum LinageValue {
    Integer(u32),
    DataName(SmolStr),
}

/// A data item (record description entry) in the DATA DIVISION.
///
/// Represents a single level-number entry with all its clauses.
/// Level numbers 01-49 define the record hierarchy, 66 is RENAMES,
/// 77 is standalone items, and 88 is condition names.
#[derive(Debug, Clone, PartialEq)]
pub struct DataItem {
    /// Level number: 01-49, 66, 77, or 88.
    pub level: u8,
    /// Data name, or None for FILLER.
    pub name: Option<SmolStr>,
    /// PICTURE clause (elementary items only).
    pub picture: Option<PictureClause>,
    /// USAGE clause.
    pub usage: Option<Usage>,
    /// VALUE clause.
    pub value: Option<ValueClause>,
    /// OCCURS clause (tables/arrays).
    pub occurs: Option<OccursClause>,
    /// REDEFINES clause: the name of the redefined item.
    pub redefines: Option<SmolStr>,
    /// RENAMES clause (level 66 only).
    pub renames: Option<RenamesClause>,
    /// SIGN clause.
    pub sign_clause: Option<SignClause>,
    /// JUSTIFIED RIGHT clause.
    pub justified: bool,
    /// BLANK WHEN ZERO clause.
    pub blank_when_zero: bool,
    /// EXTERNAL attribute.
    pub is_external: bool,
    /// GLOBAL attribute.
    pub is_global: bool,
    /// Condition values for level 88 items.
    pub condition_values: Vec<ConditionValue>,
    /// SCREEN SECTION: LINE NUMBER clause.
    pub line_clause: Option<u32>,
    /// SCREEN SECTION: COLUMN NUMBER clause.
    pub column_clause: Option<u32>,
    /// SCREEN SECTION: BLANK SCREEN flag.
    pub blank_screen: bool,
    /// SCREEN SECTION: BLANK LINE flag.
    pub blank_line: bool,
    /// SCREEN SECTION: HIGHLIGHT flag.
    pub highlight: bool,
    /// SCREEN SECTION: REVERSE-VIDEO flag.
    pub reverse_video: bool,
    /// SCREEN SECTION: SOURCE field (display value from this field).
    pub source_field: Option<QualifiedName>,
    /// SCREEN SECTION: USING field (input/output bound to this field).
    pub using_field: Option<QualifiedName>,
    /// Subordinate data items (group items contain children).
    pub children: Vec<DataItem>,
    pub span: Span,
}

/// USAGE clause values specifying internal representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Display,
    Computational,
    Comp,
    Comp1,
    Comp2,
    Comp3,
    Comp4,
    Comp5,
    Binary,
    PackedDecimal,
    Index,
    Pointer,
    FunctionPointer,
    National,
    FloatShort,
    FloatLong,
    FloatExtended,
}

/// OCCURS clause for defining tables (arrays).
#[derive(Debug, Clone, PartialEq)]
pub struct OccursClause {
    /// Minimum number of occurrences (for OCCURS DEPENDING ON).
    pub min: Option<u32>,
    /// Maximum (or fixed) number of occurrences.
    pub max: u32,
    /// DEPENDING ON data item for variable-length tables.
    pub depending_on: Option<QualifiedName>,
    /// ASCENDING KEY fields.
    pub ascending_keys: Vec<QualifiedName>,
    /// DESCENDING KEY fields.
    pub descending_keys: Vec<QualifiedName>,
    /// INDEXED BY index names.
    pub indexed_by: Vec<SmolStr>,
    pub span: Span,
}

/// RENAMES clause (level 66) specifying an alternative grouping.
#[derive(Debug, Clone, PartialEq)]
pub struct RenamesClause {
    /// The starting data item.
    pub from: QualifiedName,
    /// The ending data item (for THRU ranges).
    pub thru: Option<QualifiedName>,
    pub span: Span,
}

/// SIGN clause specifying sign representation.
#[derive(Debug, Clone, PartialEq)]
pub struct SignClause {
    pub position: SignPosition,
    pub separate: bool,
}

/// Sign position within the data item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignPosition {
    Leading,
    Trailing,
}

/// VALUE clause specifying an initial value.
#[derive(Debug, Clone, PartialEq)]
pub struct ValueClause {
    pub value: Literal,
    pub span: Span,
}

/// Condition value specification for level 88 items.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionValue {
    pub values: Vec<ConditionValueItem>,
    pub span: Span,
}

/// A single value or range in a level 88 VALUE clause.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionValueItem {
    /// A single literal value.
    Single(Literal),
    /// A THRU range of values.
    Range { from: Literal, to: Literal },
}
