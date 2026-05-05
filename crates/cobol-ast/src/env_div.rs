// COBOL AST - ENVIRONMENT DIVISION

use cobol_common::Span;
use smol_str::SmolStr;

use crate::expr::QualifiedName;

/// The ENVIRONMENT DIVISION describes the computing environment.
///
/// Contains configuration information and file control entries
/// that connect logical file names to physical devices.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentDivision {
    pub configuration: Option<ConfigurationSection>,
    pub input_output: Option<InputOutputSection>,
    pub span: Span,
}

/// The CONFIGURATION SECTION specifies computer-dependent information.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationSection {
    /// SOURCE-COMPUTER paragraph.
    pub source_computer: Option<SmolStr>,
    /// OBJECT-COMPUTER paragraph.
    pub object_computer: Option<SmolStr>,
    /// SPECIAL-NAMES paragraph entries.
    pub special_names: Vec<SpecialNameEntry>,
    /// SPECIAL-NAMES CLASS clauses.
    pub special_classes: Vec<SpecialClassEntry>,
    /// SPECIAL-NAMES ALPHABET clauses.
    pub special_alphabets: Vec<SpecialAlphabetEntry>,
    /// SPECIAL-NAMES DECIMAL-POINT IS COMMA.
    pub decimal_point_is_comma: bool,
    /// REPOSITORY paragraph entries (COBOL 2002+).
    pub repository: Vec<RepositoryEntry>,
    pub span: Span,
}

/// An entry in the SPECIAL-NAMES paragraph that maps an implementor-name
/// to a user-defined mnemonic name, and optionally defines ON/OFF STATUS
/// condition names for external switches.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialNameEntry {
    pub system_name: SmolStr,
    pub user_name: Option<SmolStr>,
    /// Condition name for ON STATUS (switch is on).
    pub on_condition: Option<SmolStr>,
    /// Condition name for OFF STATUS (switch is off).
    pub off_condition: Option<SmolStr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialClassEntry {
    pub name: SmolStr,
    pub ranges: Vec<SpecialClassRange>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpecialClassRange {
    Single(SmolStr),
    Range { from: SmolStr, to: SmolStr },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialAlphabetEntry {
    pub name: SmolStr,
    /// Each inner vector contains characters with the same collating rank.
    pub ranks: Vec<Vec<SmolStr>>,
    pub span: Span,
}

/// An entry in the REPOSITORY paragraph (COBOL 2002+).
///
/// Identifies classes, interfaces, functions, or programs available
/// to the compilation unit.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryEntry {
    pub kind: RepositoryEntryKind,
    pub name: SmolStr,
    pub external_name: Option<SmolStr>,
    pub span: Span,
}

/// The kind of entity referenced by a REPOSITORY paragraph entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryEntryKind {
    Class,
    Interface,
    Function,
    Program,
}

/// The INPUT-OUTPUT SECTION describes file connections.
#[derive(Debug, Clone, PartialEq)]
pub struct InputOutputSection {
    /// FILE-CONTROL paragraph entries.
    pub file_controls: Vec<FileControlEntry>,
    /// I-O-CONTROL SAME RECORD AREA groups.
    pub same_record_areas: Vec<Vec<SmolStr>>,
    pub span: Span,
}

/// A SELECT...ASSIGN entry in the FILE-CONTROL paragraph.
///
/// Connects a logical file name used in the program to a physical
/// file or device, and specifies access characteristics.
#[derive(Debug, Clone, PartialEq)]
pub struct FileControlEntry {
    /// The logical file name (SELECT file-name).
    pub file_name: SmolStr,
    /// SELECT OPTIONAL clause.
    pub optional: bool,
    /// The device or file path (ASSIGN TO).
    pub assign_to: SmolStr,
    /// ORGANIZATION clause.
    pub organization: Option<FileOrganization>,
    /// ACCESS MODE clause.
    pub access_mode: Option<AccessMode>,
    /// RECORD KEY clause.
    pub record_key: Option<QualifiedName>,
    /// RELATIVE KEY clause.
    pub relative_key: Option<QualifiedName>,
    /// ALTERNATE RECORD KEY clauses.
    pub alternate_keys: Vec<AlternateKey>,
    /// FILE STATUS clause.
    pub file_status: Option<QualifiedName>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlternateKey {
    pub name: QualifiedName,
    pub duplicates: bool,
}

/// File organization types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOrganization {
    Sequential,
    Indexed,
    Relative,
    LineSequential,
}

/// File access modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Sequential,
    Random,
    Dynamic,
}
