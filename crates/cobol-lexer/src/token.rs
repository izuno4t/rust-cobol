use cobol_common::Span;
use smol_str::SmolStr;

/// A single token produced by the COBOL lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: SmolStr,
    pub span: Span,
}

/// Every possible token kind the COBOL lexer can emit.
///
/// Covers reserved words from COBOL-85 through COBOL 2023,
/// literals, operators, punctuation, and special tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // ── Literals ──────────────────────────────────────────────
    IntegerLiteral,
    DecimalLiteral,
    StringLiteral,
    HexLiteral,
    BooleanLiteral,
    NationalLiteral,
    PictureString,

    // ── Identifiers ──────────────────────────────────────────
    Identifier,

    // ── Division keywords ────────────────────────────────────
    Identification,
    Environment,
    Data,
    Procedure,
    Division,
    Section,

    // ── IDENTIFICATION DIVISION ──────────────────────────────
    ProgramId,
    ClassId,
    MethodId,
    InterfaceId,
    FunctionId,
    FactoryKw,
    ObjectKw,

    // ── ENVIRONMENT DIVISION ─────────────────────────────────
    Configuration,
    SourceComputer,
    ObjectComputer,
    SpecialNames,
    Repository,
    InputOutput,
    FileControl,
    IoControl,
    Select,
    Assign,
    Organization,
    Sequential,
    Indexed,
    Relative,
    AccessMode,
    Dynamic,
    Random,
    RecordKey,
    AlternateRecordKey,
    FileStatus,

    // ── DATA DIVISION ────────────────────────────────────────
    File,
    WorkingStorage,
    LocalStorage,
    Linkage,
    Communication,
    Report,
    Screen,
    Fd,
    Sd,
    Pic,
    Value,
    Values,
    Redefines,
    Renames,
    Occurs,
    Times,
    Depending,
    Ascending,
    Descending,
    Key,
    Usage,
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
    SignKw,
    Leading,
    Trailing,
    Separate,
    Justified,
    Blank,
    When,
    Zero,
    Space,
    HighValue,
    LowValue,
    Quote,
    All,
    Filler,
    External,
    Global,
    GroupUsage,
    National,
    Typedef,

    // ── PROCEDURE DIVISION ───────────────────────────────────
    Move,
    To,
    Add,
    Subtract,
    Multiply,
    Divide,
    Compute,
    Giving,
    Remainder,
    Rounded,
    OnSizeError,
    NotOnSizeError,
    EndCompute,
    EndAdd,
    EndSubtract,
    EndMultiply,
    EndDivide,
    Corresponding,
    If,
    Then,
    Else,
    EndIf,
    Evaluate,
    Also,
    TrueKw,
    FalseKw,
    Other,
    EndEvaluate,
    Perform,
    Thru,
    Varying,
    Until,
    After,
    EndPerform,
    Go,
    GoTo,
    Call,
    Using,
    By,
    Reference,
    Content,
    Returning,
    EndCall,
    Cancel,
    Stop,
    Run,
    Accept,
    From,
    EndAccept,
    EndDisplay,
    Open,
    Close,
    Read,
    Write,
    Rewrite,
    Delete,
    Start,
    Return,
    EndRead,
    EndWrite,
    EndRewrite,
    EndDelete,
    EndStart,
    EndReturn,
    Input,
    Output,
    IoMode,
    Extend,
    Into,
    At,
    End,
    NotAtEnd,
    InvalidKey,
    NotInvalidKey,
    Eop,
    NotAtEop,
    With,
    Lock,
    NoLock,
    String,
    Unstring,
    Delimited,
    Delimiter,
    Count,
    Overflow,
    NotOnOverflow,
    OnException,
    NotOnException,
    EndString,
    EndUnstring,
    Inspect,
    Tallying,
    Replacing,
    Converting,
    Before,
    Initial,
    Merge,
    Search,
    EndSearch,
    Sort,
    EndSort,
    EndMerge,
    OnKw,
    SizeKw,
    ErrorKw,
    ForKw,
    FirstKw,
    ExceptionKw,
    Duplicates,
    Release,
    Set,
    Up,
    Down,
    Continue,
    Exit,
    Program,
    Method,
    Goback,
    Initialize,
    Initiate,
    Terminate,
    Generate,
    Raise,
    Resume,
    Allocate,
    Free,
    Validate,
    Invoke,
    New,
    Self_,
    Super,
    Null,

    // ── Intrinsic functions ──────────────────────────────────
    Function,

    // ── Conditional ──────────────────────────────────────────
    Not,
    And,
    Or,
    Greater,
    Less,
    Equal,
    Than,
    Numeric,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
    Positive,
    Negative,

    // ── File I/O ─────────────────────────────────────────────
    Line,
    Lines,
    Page,
    Advancing,
    Recording,
    Mode,
    Block,
    Contains,
    Records,
    Record,
    Characters,
    Label,
    Omitted,
    Linage,
    Footing,
    Top,
    Bottom,
    CodeSet,

    // ── COPY / REPLACE ───────────────────────────────────────
    Copy,
    Replace,
    Off,
    In,
    Of,

    // ── OOP (COBOL 2002+) ────────────────────────────────────
    Class,
    Inherits,
    Property,
    Get,

    // ── SCREEN SECTION ──────────────────────────────────────
    Column,
    Highlight,
    ReverseVideo,
    SourceField,

    // ── COBOL 2014 / 2023 ────────────────────────────────────
    Json,
    Xml,
    Parse,
    FloatShort,
    FloatLong,
    FloatExtended,

    // ── Operators and punctuation ────────────────────────────
    Period,
    Comma,
    Semicolon,
    LeftParen,
    RightParen,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    DoubleStar,
    Equals,
    GreaterThan,
    LessThan,
    GreaterEqual,
    LessEqual,
    NotEqual,
    DoubleColon,
    EqualGreater,

    // ── Special ──────────────────────────────────────────────
    LevelNumber,
    Eof,
    Newline,
    Error,
    CompilerDirective,
}

impl TokenKind {
    /// Look up a COBOL keyword by name, returning the corresponding `TokenKind`.
    ///
    /// Matching is case-insensitive. Hyphenated keywords such as `PROGRAM-ID`
    /// and synonyms such as `THRU`/`THROUGH` are handled.
    ///
    /// Returns `None` when `word` is not a recognised keyword.
    pub fn from_keyword(word: &str) -> Option<TokenKind> {
        // COBOL is case-insensitive; normalise to uppercase for lookup.
        let upper = word.to_uppercase();
        match upper.as_str() {
            // ── Division keywords ────────────────────────────
            "IDENTIFICATION" | "ID" => Some(TokenKind::Identification),
            "ENVIRONMENT" => Some(TokenKind::Environment),
            "DATA" => Some(TokenKind::Data),
            "PROCEDURE" => Some(TokenKind::Procedure),
            "DIVISION" => Some(TokenKind::Division),
            "SECTION" => Some(TokenKind::Section),

            // ── IDENTIFICATION DIVISION ──────────────────────
            "PROGRAM-ID" => Some(TokenKind::ProgramId),
            "CLASS-ID" => Some(TokenKind::ClassId),
            "METHOD-ID" => Some(TokenKind::MethodId),
            "INTERFACE-ID" => Some(TokenKind::InterfaceId),
            "FUNCTION-ID" => Some(TokenKind::FunctionId),
            "FACTORY" => Some(TokenKind::FactoryKw),
            "OBJECT" => Some(TokenKind::ObjectKw),

            // ── ENVIRONMENT DIVISION ─────────────────────────
            "CONFIGURATION" => Some(TokenKind::Configuration),
            "SOURCE-COMPUTER" => Some(TokenKind::SourceComputer),
            "OBJECT-COMPUTER" => Some(TokenKind::ObjectComputer),
            "SPECIAL-NAMES" => Some(TokenKind::SpecialNames),
            "REPOSITORY" => Some(TokenKind::Repository),
            "INPUT-OUTPUT" => Some(TokenKind::InputOutput),
            "FILE-CONTROL" => Some(TokenKind::FileControl),
            "I-O-CONTROL" => Some(TokenKind::IoControl),
            "SELECT" => Some(TokenKind::Select),
            "ASSIGN" => Some(TokenKind::Assign),
            "ORGANIZATION" => Some(TokenKind::Organization),
            "SEQUENTIAL" => Some(TokenKind::Sequential),
            "INDEXED" => Some(TokenKind::Indexed),
            "RELATIVE" => Some(TokenKind::Relative),
            "ACCESS" => Some(TokenKind::AccessMode),
            "DYNAMIC" => Some(TokenKind::Dynamic),
            "RANDOM" => Some(TokenKind::Random),
            "RECORD-KEY" => Some(TokenKind::RecordKey),
            "ALTERNATE" => Some(TokenKind::AlternateRecordKey),
            "FILE-STATUS" => Some(TokenKind::FileStatus),

            // ── DATA DIVISION ────────────────────────────────
            "FILE" => Some(TokenKind::File),
            "WORKING-STORAGE" => Some(TokenKind::WorkingStorage),
            "LOCAL-STORAGE" => Some(TokenKind::LocalStorage),
            "LINKAGE" => Some(TokenKind::Linkage),
            "COMMUNICATION" => Some(TokenKind::Communication),
            "REPORT" | "REPORTING" => Some(TokenKind::Report),
            "SCREEN" => Some(TokenKind::Screen),
            "FD" => Some(TokenKind::Fd),
            "SD" => Some(TokenKind::Sd),
            "PIC" | "PICTURE" => Some(TokenKind::Pic),
            "VALUE" => Some(TokenKind::Value),
            "VALUES" => Some(TokenKind::Values),
            "REDEFINES" => Some(TokenKind::Redefines),
            "RENAMES" => Some(TokenKind::Renames),
            "OCCURS" => Some(TokenKind::Occurs),
            "TIMES" => Some(TokenKind::Times),
            "DEPENDING" => Some(TokenKind::Depending),
            "ASCENDING" => Some(TokenKind::Ascending),
            "DESCENDING" => Some(TokenKind::Descending),
            "KEY" => Some(TokenKind::Key),
            "USAGE" => Some(TokenKind::Usage),
            "DISPLAY" => Some(TokenKind::Display),
            "COMPUTATIONAL" => Some(TokenKind::Computational),
            "COMP" => Some(TokenKind::Comp),
            "COMP-1" => Some(TokenKind::Comp1),
            "COMP-2" => Some(TokenKind::Comp2),
            "COMP-3" => Some(TokenKind::Comp3),
            "COMP-4" => Some(TokenKind::Comp4),
            "COMP-5" => Some(TokenKind::Comp5),
            "BINARY" => Some(TokenKind::Binary),
            "PACKED-DECIMAL" => Some(TokenKind::PackedDecimal),
            "INDEX" => Some(TokenKind::Index),
            "POINTER" => Some(TokenKind::Pointer),
            "FUNCTION-POINTER" => Some(TokenKind::FunctionPointer),
            "SIGN" => Some(TokenKind::SignKw),
            "LEADING" => Some(TokenKind::Leading),
            "TRAILING" => Some(TokenKind::Trailing),
            "SEPARATE" => Some(TokenKind::Separate),
            "JUSTIFIED" | "JUST" => Some(TokenKind::Justified),
            "BLANK" => Some(TokenKind::Blank),
            "WHEN" => Some(TokenKind::When),
            "ZERO" | "ZEROS" | "ZEROES" => Some(TokenKind::Zero),
            "SPACE" | "SPACES" => Some(TokenKind::Space),
            "HIGH-VALUE" | "HIGH-VALUES" => Some(TokenKind::HighValue),
            "LOW-VALUE" | "LOW-VALUES" => Some(TokenKind::LowValue),
            "QUOTE" | "QUOTES" => Some(TokenKind::Quote),
            "ALL" => Some(TokenKind::All),
            "FILLER" => Some(TokenKind::Filler),
            "EXTERNAL" => Some(TokenKind::External),
            "GLOBAL" => Some(TokenKind::Global),
            "GROUP-USAGE" => Some(TokenKind::GroupUsage),
            "NATIONAL" => Some(TokenKind::National),
            "TYPEDEF" => Some(TokenKind::Typedef),

            // ── PROCEDURE DIVISION ───────────────────────────
            "MOVE" => Some(TokenKind::Move),
            "TO" => Some(TokenKind::To),
            "ADD" => Some(TokenKind::Add),
            "SUBTRACT" => Some(TokenKind::Subtract),
            "MULTIPLY" => Some(TokenKind::Multiply),
            "DIVIDE" => Some(TokenKind::Divide),
            "COMPUTE" => Some(TokenKind::Compute),
            "GIVING" => Some(TokenKind::Giving),
            "REMAINDER" => Some(TokenKind::Remainder),
            "ROUNDED" => Some(TokenKind::Rounded),
            "CORRESPONDING" | "CORR" => Some(TokenKind::Corresponding),
            "IF" => Some(TokenKind::If),
            "THEN" => Some(TokenKind::Then),
            "ELSE" => Some(TokenKind::Else),
            "END-IF" => Some(TokenKind::EndIf),
            "EVALUATE" => Some(TokenKind::Evaluate),
            "ALSO" => Some(TokenKind::Also),
            "TRUE" => Some(TokenKind::TrueKw),
            "FALSE" => Some(TokenKind::FalseKw),
            "OTHER" => Some(TokenKind::Other),
            "END-EVALUATE" => Some(TokenKind::EndEvaluate),
            "PERFORM" => Some(TokenKind::Perform),
            "THRU" | "THROUGH" => Some(TokenKind::Thru),
            "VARYING" => Some(TokenKind::Varying),
            "UNTIL" => Some(TokenKind::Until),
            "AFTER" => Some(TokenKind::After),
            "END-PERFORM" => Some(TokenKind::EndPerform),
            "GO" => Some(TokenKind::Go),
            "GO-TO" | "GOTO" => Some(TokenKind::GoTo),
            "CALL" => Some(TokenKind::Call),
            "USING" => Some(TokenKind::Using),
            "BY" => Some(TokenKind::By),
            "REFERENCE" => Some(TokenKind::Reference),
            "CONTENT" => Some(TokenKind::Content),
            "RETURNING" => Some(TokenKind::Returning),
            "END-CALL" => Some(TokenKind::EndCall),
            "CANCEL" => Some(TokenKind::Cancel),
            "STOP" => Some(TokenKind::Stop),
            "RUN" => Some(TokenKind::Run),
            "ACCEPT" => Some(TokenKind::Accept),
            "FROM" => Some(TokenKind::From),
            "END-ACCEPT" => Some(TokenKind::EndAccept),
            "END-DISPLAY" => Some(TokenKind::EndDisplay),
            "OPEN" => Some(TokenKind::Open),
            "CLOSE" => Some(TokenKind::Close),
            "READ" => Some(TokenKind::Read),
            "WRITE" => Some(TokenKind::Write),
            "REWRITE" => Some(TokenKind::Rewrite),
            "DELETE" => Some(TokenKind::Delete),
            "START" => Some(TokenKind::Start),
            "RETURN" => Some(TokenKind::Return),
            "END-READ" => Some(TokenKind::EndRead),
            "END-WRITE" => Some(TokenKind::EndWrite),
            "END-REWRITE" => Some(TokenKind::EndRewrite),
            "END-DELETE" => Some(TokenKind::EndDelete),
            "END-START" => Some(TokenKind::EndStart),
            "END-RETURN" => Some(TokenKind::EndReturn),
            "INPUT" => Some(TokenKind::Input),
            "OUTPUT" => Some(TokenKind::Output),
            "I-O" => Some(TokenKind::IoMode),
            "EXTEND" => Some(TokenKind::Extend),
            "INTO" => Some(TokenKind::Into),
            "AT" => Some(TokenKind::At),
            "END" => Some(TokenKind::End),
            "INVALID" => Some(TokenKind::InvalidKey),
            "SIZE" => Some(TokenKind::SizeKw),
            "ERROR" => Some(TokenKind::ErrorKw),
            "FOR" => Some(TokenKind::ForKw),
            "FIRST" => Some(TokenKind::FirstKw),
            "EXCEPTION" => Some(TokenKind::ExceptionKw),
            "DUPLICATES" => Some(TokenKind::Duplicates),
            "EOP" | "END-OF-PAGE" => Some(TokenKind::Eop),
            "END-COMPUTE" => Some(TokenKind::EndCompute),
            "END-ADD" => Some(TokenKind::EndAdd),
            "END-SUBTRACT" => Some(TokenKind::EndSubtract),
            "END-MULTIPLY" => Some(TokenKind::EndMultiply),
            "END-DIVIDE" => Some(TokenKind::EndDivide),
            "END-SEARCH" => Some(TokenKind::EndSearch),
            "END-SORT" => Some(TokenKind::EndSort),
            "END-MERGE" => Some(TokenKind::EndMerge),
            "WITH" => Some(TokenKind::With),
            "LOCK" => Some(TokenKind::Lock),
            "STRING" => Some(TokenKind::String),
            "UNSTRING" => Some(TokenKind::Unstring),
            "DELIMITED" => Some(TokenKind::Delimited),
            "DELIMITER" => Some(TokenKind::Delimiter),
            "COUNT" => Some(TokenKind::Count),
            "OVERFLOW" => Some(TokenKind::Overflow),
            "END-STRING" => Some(TokenKind::EndString),
            "END-UNSTRING" => Some(TokenKind::EndUnstring),
            "INSPECT" => Some(TokenKind::Inspect),
            "TALLYING" => Some(TokenKind::Tallying),
            "REPLACING" => Some(TokenKind::Replacing),
            "CONVERTING" => Some(TokenKind::Converting),
            "BEFORE" => Some(TokenKind::Before),
            "INITIAL" => Some(TokenKind::Initial),
            "MERGE" => Some(TokenKind::Merge),
            "SEARCH" => Some(TokenKind::Search),
            "SORT" => Some(TokenKind::Sort),
            "ON" => Some(TokenKind::OnKw),
            "RELEASE" => Some(TokenKind::Release),
            "SET" => Some(TokenKind::Set),
            "UP" => Some(TokenKind::Up),
            "DOWN" => Some(TokenKind::Down),
            "CONTINUE" => Some(TokenKind::Continue),
            "EXIT" => Some(TokenKind::Exit),
            "PROGRAM" => Some(TokenKind::Program),
            "METHOD" => Some(TokenKind::Method),
            "GOBACK" => Some(TokenKind::Goback),
            "INITIALIZE" => Some(TokenKind::Initialize),
            "INITIATE" => Some(TokenKind::Initiate),
            "TERMINATE" => Some(TokenKind::Terminate),
            "GENERATE" => Some(TokenKind::Generate),
            "RAISE" => Some(TokenKind::Raise),
            "RESUME" => Some(TokenKind::Resume),
            "ALLOCATE" => Some(TokenKind::Allocate),
            "FREE" => Some(TokenKind::Free),
            "VALIDATE" => Some(TokenKind::Validate),
            "INVOKE" => Some(TokenKind::Invoke),
            "NEW" => Some(TokenKind::New),
            "SELF" => Some(TokenKind::Self_),
            "SUPER" => Some(TokenKind::Super),
            "NULL" | "NULLS" => Some(TokenKind::Null),

            // ── Intrinsic functions ──────────────────────────
            "FUNCTION" => Some(TokenKind::Function),

            // ── Conditional ──────────────────────────────────
            "NOT" => Some(TokenKind::Not),
            "AND" => Some(TokenKind::And),
            "OR" => Some(TokenKind::Or),
            "GREATER" => Some(TokenKind::Greater),
            "LESS" => Some(TokenKind::Less),
            "EQUAL" => Some(TokenKind::Equal),
            "THAN" => Some(TokenKind::Than),
            "NUMERIC" => Some(TokenKind::Numeric),
            "ALPHABETIC" => Some(TokenKind::Alphabetic),
            "ALPHABETIC-LOWER" => Some(TokenKind::AlphabeticLower),
            "ALPHABETIC-UPPER" => Some(TokenKind::AlphabeticUpper),
            "POSITIVE" => Some(TokenKind::Positive),
            "NEGATIVE" => Some(TokenKind::Negative),

            // ── File I/O ─────────────────────────────────────
            "LINE" => Some(TokenKind::Line),
            "LINES" => Some(TokenKind::Lines),
            "PAGE" => Some(TokenKind::Page),
            "ADVANCING" => Some(TokenKind::Advancing),
            "RECORDING" => Some(TokenKind::Recording),
            "MODE" => Some(TokenKind::Mode),
            "BLOCK" => Some(TokenKind::Block),
            "CONTAINS" => Some(TokenKind::Contains),
            "RECORDS" => Some(TokenKind::Records),
            "RECORD" => Some(TokenKind::Record),
            "CHARACTERS" => Some(TokenKind::Characters),
            "LABEL" => Some(TokenKind::Label),
            "OMITTED" => Some(TokenKind::Omitted),
            "LINAGE" => Some(TokenKind::Linage),
            "FOOTING" => Some(TokenKind::Footing),
            "TOP" => Some(TokenKind::Top),
            "BOTTOM" => Some(TokenKind::Bottom),
            "CODE-SET" => Some(TokenKind::CodeSet),

            // ── COPY / REPLACE ───────────────────────────────
            "COPY" => Some(TokenKind::Copy),
            "REPLACE" => Some(TokenKind::Replace),
            "OFF" => Some(TokenKind::Off),
            "IN" => Some(TokenKind::In),
            "OF" => Some(TokenKind::Of),

            // ── OOP (COBOL 2002+) ────────────────────────────
            "CLASS" => Some(TokenKind::Class),
            "INHERITS" => Some(TokenKind::Inherits),
            "PROPERTY" => Some(TokenKind::Property),
            "GET" => Some(TokenKind::Get),

            // ── SCREEN SECTION ─────────────────────────────────
            "COLUMN" | "COL" => Some(TokenKind::Column),
            "HIGHLIGHT" => Some(TokenKind::Highlight),
            "REVERSE-VIDEO" => Some(TokenKind::ReverseVideo),
            "SOURCE" => Some(TokenKind::SourceField),

            // ── COBOL 2014 / 2023 ────────────────────────────
            "JSON" => Some(TokenKind::Json),
            "XML" => Some(TokenKind::Xml),
            "PARSE" => Some(TokenKind::Parse),
            "FLOAT-SHORT" => Some(TokenKind::FloatShort),
            "FLOAT-LONG" => Some(TokenKind::FloatLong),
            "FLOAT-EXTENDED" => Some(TokenKind::FloatExtended),

            _ => None,
        }
    }

    /// Returns `true` if this token kind represents a COBOL keyword
    /// (as opposed to a literal, operator, punctuation, or special token).
    pub fn is_keyword(&self) -> bool {
        !matches!(
            self,
            // Literals
            TokenKind::IntegerLiteral
                | TokenKind::DecimalLiteral
                | TokenKind::StringLiteral
                | TokenKind::HexLiteral
                | TokenKind::BooleanLiteral
                | TokenKind::NationalLiteral
                | TokenKind::PictureString
                // Identifiers
                | TokenKind::Identifier
                // Operators and punctuation
                | TokenKind::Period
                | TokenKind::Comma
                | TokenKind::Semicolon
                | TokenKind::LeftParen
                | TokenKind::RightParen
                | TokenKind::Colon
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::DoubleStar
                | TokenKind::Equals
                | TokenKind::GreaterThan
                | TokenKind::LessThan
                | TokenKind::GreaterEqual
                | TokenKind::LessEqual
                | TokenKind::NotEqual
                | TokenKind::DoubleColon
                | TokenKind::EqualGreater
                // Special
                | TokenKind::LevelNumber
                | TokenKind::Eof
                | TokenKind::Newline
                | TokenKind::Error
                | TokenKind::CompilerDirective
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::FileId;

    #[test]
    fn test_token_creation() {
        let token = Token {
            kind: TokenKind::Identifier,
            text: "WS-NAME".into(),
            span: Span::new(0, 7, FileId(0)),
        };
        assert_eq!(token.kind, TokenKind::Identifier);
        assert_eq!(token.text.as_str(), "WS-NAME");
    }

    #[test]
    fn test_keyword_lookup() {
        assert_eq!(
            TokenKind::from_keyword("IDENTIFICATION"),
            Some(TokenKind::Identification)
        );
        assert_eq!(
            TokenKind::from_keyword("DIVISION"),
            Some(TokenKind::Division)
        );
        assert_eq!(TokenKind::from_keyword("NOTAKEYWORD"), None);
    }

    #[test]
    fn test_keyword_case_insensitive() {
        assert_eq!(
            TokenKind::from_keyword("identification"),
            Some(TokenKind::Identification)
        );
        assert_eq!(
            TokenKind::from_keyword("Division"),
            Some(TokenKind::Division)
        );
    }

    #[test]
    fn test_keyword_synonyms() {
        assert_eq!(TokenKind::from_keyword("THRU"), Some(TokenKind::Thru));
        assert_eq!(TokenKind::from_keyword("THROUGH"), Some(TokenKind::Thru));
        assert_eq!(TokenKind::from_keyword("ZERO"), Some(TokenKind::Zero));
        assert_eq!(TokenKind::from_keyword("ZEROS"), Some(TokenKind::Zero));
        assert_eq!(TokenKind::from_keyword("ZEROES"), Some(TokenKind::Zero));
    }

    #[test]
    fn test_hyphenated_keywords() {
        assert_eq!(
            TokenKind::from_keyword("PROGRAM-ID"),
            Some(TokenKind::ProgramId)
        );
        assert_eq!(TokenKind::from_keyword("END-IF"), Some(TokenKind::EndIf));
        assert_eq!(
            TokenKind::from_keyword("WORKING-STORAGE"),
            Some(TokenKind::WorkingStorage)
        );
        assert_eq!(
            TokenKind::from_keyword("END-PERFORM"),
            Some(TokenKind::EndPerform)
        );
    }

    #[test]
    fn test_is_keyword() {
        assert!(TokenKind::Identification.is_keyword());
        assert!(TokenKind::Move.is_keyword());
        assert!(!TokenKind::Identifier.is_keyword());
        assert!(!TokenKind::IntegerLiteral.is_keyword());
        assert!(!TokenKind::Period.is_keyword());
        assert!(!TokenKind::Eof.is_keyword());
    }

    #[test]
    fn test_additional_synonyms() {
        // PICTURE / PIC
        assert_eq!(TokenKind::from_keyword("PICTURE"), Some(TokenKind::Pic));
        assert_eq!(TokenKind::from_keyword("PIC"), Some(TokenKind::Pic));

        // SPACE / SPACES
        assert_eq!(TokenKind::from_keyword("SPACE"), Some(TokenKind::Space));
        assert_eq!(TokenKind::from_keyword("SPACES"), Some(TokenKind::Space));

        // CORRESPONDING / CORR
        assert_eq!(
            TokenKind::from_keyword("CORRESPONDING"),
            Some(TokenKind::Corresponding)
        );
        assert_eq!(
            TokenKind::from_keyword("CORR"),
            Some(TokenKind::Corresponding)
        );

        // JUSTIFIED / JUST
        assert_eq!(
            TokenKind::from_keyword("JUSTIFIED"),
            Some(TokenKind::Justified)
        );
        assert_eq!(TokenKind::from_keyword("JUST"), Some(TokenKind::Justified));

        // NULL / NULLS
        assert_eq!(TokenKind::from_keyword("NULL"), Some(TokenKind::Null));
        assert_eq!(TokenKind::from_keyword("NULLS"), Some(TokenKind::Null));

        // HIGH-VALUE / HIGH-VALUES, LOW-VALUE / LOW-VALUES
        assert_eq!(
            TokenKind::from_keyword("HIGH-VALUE"),
            Some(TokenKind::HighValue)
        );
        assert_eq!(
            TokenKind::from_keyword("HIGH-VALUES"),
            Some(TokenKind::HighValue)
        );
        assert_eq!(
            TokenKind::from_keyword("LOW-VALUE"),
            Some(TokenKind::LowValue)
        );
        assert_eq!(
            TokenKind::from_keyword("LOW-VALUES"),
            Some(TokenKind::LowValue)
        );

        // QUOTE / QUOTES
        assert_eq!(TokenKind::from_keyword("QUOTE"), Some(TokenKind::Quote));
        assert_eq!(TokenKind::from_keyword("QUOTES"), Some(TokenKind::Quote));
    }

    #[test]
    fn test_data_division_keywords() {
        assert_eq!(TokenKind::from_keyword("FD"), Some(TokenKind::Fd));
        assert_eq!(TokenKind::from_keyword("SD"), Some(TokenKind::Sd));
        assert_eq!(
            TokenKind::from_keyword("REDEFINES"),
            Some(TokenKind::Redefines)
        );
        assert_eq!(TokenKind::from_keyword("OCCURS"), Some(TokenKind::Occurs));
        assert_eq!(TokenKind::from_keyword("COMP"), Some(TokenKind::Comp));
        assert_eq!(TokenKind::from_keyword("COMP-1"), Some(TokenKind::Comp1));
        assert_eq!(TokenKind::from_keyword("COMP-3"), Some(TokenKind::Comp3));
        assert_eq!(TokenKind::from_keyword("BINARY"), Some(TokenKind::Binary));
        assert_eq!(
            TokenKind::from_keyword("PACKED-DECIMAL"),
            Some(TokenKind::PackedDecimal)
        );
    }

    #[test]
    fn test_procedure_division_keywords() {
        assert_eq!(TokenKind::from_keyword("MOVE"), Some(TokenKind::Move));
        assert_eq!(TokenKind::from_keyword("PERFORM"), Some(TokenKind::Perform));
        assert_eq!(TokenKind::from_keyword("CALL"), Some(TokenKind::Call));
        assert_eq!(TokenKind::from_keyword("GOBACK"), Some(TokenKind::Goback));
        assert_eq!(TokenKind::from_keyword("STOP"), Some(TokenKind::Stop));
        assert_eq!(
            TokenKind::from_keyword("EVALUATE"),
            Some(TokenKind::Evaluate)
        );
        assert_eq!(TokenKind::from_keyword("INSPECT"), Some(TokenKind::Inspect));
    }

    #[test]
    fn test_end_keywords() {
        assert_eq!(TokenKind::from_keyword("END-IF"), Some(TokenKind::EndIf));
        assert_eq!(
            TokenKind::from_keyword("END-EVALUATE"),
            Some(TokenKind::EndEvaluate)
        );
        assert_eq!(
            TokenKind::from_keyword("END-PERFORM"),
            Some(TokenKind::EndPerform)
        );
        assert_eq!(
            TokenKind::from_keyword("END-CALL"),
            Some(TokenKind::EndCall)
        );
        assert_eq!(
            TokenKind::from_keyword("END-READ"),
            Some(TokenKind::EndRead)
        );
        assert_eq!(
            TokenKind::from_keyword("END-WRITE"),
            Some(TokenKind::EndWrite)
        );
        assert_eq!(
            TokenKind::from_keyword("END-STRING"),
            Some(TokenKind::EndString)
        );
        assert_eq!(
            TokenKind::from_keyword("END-UNSTRING"),
            Some(TokenKind::EndUnstring)
        );
        assert_eq!(
            TokenKind::from_keyword("END-ACCEPT"),
            Some(TokenKind::EndAccept)
        );
        assert_eq!(
            TokenKind::from_keyword("END-DISPLAY"),
            Some(TokenKind::EndDisplay)
        );
        assert_eq!(
            TokenKind::from_keyword("END-REWRITE"),
            Some(TokenKind::EndRewrite)
        );
        assert_eq!(
            TokenKind::from_keyword("END-DELETE"),
            Some(TokenKind::EndDelete)
        );
        assert_eq!(
            TokenKind::from_keyword("END-START"),
            Some(TokenKind::EndStart)
        );
        assert_eq!(
            TokenKind::from_keyword("END-RETURN"),
            Some(TokenKind::EndReturn)
        );
    }

    #[test]
    fn test_cobol_2014_2023_keywords() {
        assert_eq!(TokenKind::from_keyword("JSON"), Some(TokenKind::Json));
        assert_eq!(TokenKind::from_keyword("XML"), Some(TokenKind::Xml));
        assert_eq!(TokenKind::from_keyword("PARSE"), Some(TokenKind::Parse));
        assert_eq!(
            TokenKind::from_keyword("FLOAT-SHORT"),
            Some(TokenKind::FloatShort)
        );
        assert_eq!(
            TokenKind::from_keyword("FLOAT-LONG"),
            Some(TokenKind::FloatLong)
        );
        assert_eq!(
            TokenKind::from_keyword("FLOAT-EXTENDED"),
            Some(TokenKind::FloatExtended)
        );
    }

    #[test]
    fn test_oop_keywords() {
        assert_eq!(TokenKind::from_keyword("CLASS"), Some(TokenKind::Class));
        assert_eq!(
            TokenKind::from_keyword("INHERITS"),
            Some(TokenKind::Inherits)
        );
        assert_eq!(
            TokenKind::from_keyword("PROPERTY"),
            Some(TokenKind::Property)
        );
        assert_eq!(TokenKind::from_keyword("INVOKE"), Some(TokenKind::Invoke));
        assert_eq!(TokenKind::from_keyword("SELF"), Some(TokenKind::Self_));
        assert_eq!(TokenKind::from_keyword("SUPER"), Some(TokenKind::Super));
    }

    #[test]
    fn test_conditional_keywords() {
        assert_eq!(TokenKind::from_keyword("NOT"), Some(TokenKind::Not));
        assert_eq!(TokenKind::from_keyword("AND"), Some(TokenKind::And));
        assert_eq!(TokenKind::from_keyword("OR"), Some(TokenKind::Or));
        assert_eq!(TokenKind::from_keyword("GREATER"), Some(TokenKind::Greater));
        assert_eq!(TokenKind::from_keyword("LESS"), Some(TokenKind::Less));
        assert_eq!(TokenKind::from_keyword("EQUAL"), Some(TokenKind::Equal));
        assert_eq!(TokenKind::from_keyword("NUMERIC"), Some(TokenKind::Numeric));
        assert_eq!(
            TokenKind::from_keyword("ALPHABETIC"),
            Some(TokenKind::Alphabetic)
        );
        assert_eq!(
            TokenKind::from_keyword("ALPHABETIC-LOWER"),
            Some(TokenKind::AlphabeticLower)
        );
        assert_eq!(
            TokenKind::from_keyword("ALPHABETIC-UPPER"),
            Some(TokenKind::AlphabeticUpper)
        );
    }

    #[test]
    fn test_is_keyword_comprehensive() {
        // All keywords should return true
        let keywords = [
            TokenKind::Identification,
            TokenKind::Environment,
            TokenKind::Data,
            TokenKind::Procedure,
            TokenKind::Division,
            TokenKind::Section,
            TokenKind::ProgramId,
            TokenKind::Move,
            TokenKind::If,
            TokenKind::Perform,
            TokenKind::EndIf,
            TokenKind::Zero,
            TokenKind::Space,
            TokenKind::Copy,
            TokenKind::Replace,
            TokenKind::Class,
            TokenKind::Json,
            TokenKind::Function,
            TokenKind::Not,
            TokenKind::And,
            TokenKind::Line,
            TokenKind::Page,
        ];
        for kw in &keywords {
            assert!(kw.is_keyword(), "{:?} should be a keyword", kw);
        }

        // Non-keywords should return false
        let non_keywords = [
            TokenKind::IntegerLiteral,
            TokenKind::DecimalLiteral,
            TokenKind::StringLiteral,
            TokenKind::HexLiteral,
            TokenKind::BooleanLiteral,
            TokenKind::NationalLiteral,
            TokenKind::PictureString,
            TokenKind::Identifier,
            TokenKind::Period,
            TokenKind::Comma,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Equals,
            TokenKind::LevelNumber,
            TokenKind::Eof,
            TokenKind::Newline,
            TokenKind::Error,
            TokenKind::CompilerDirective,
        ];
        for nk in &non_keywords {
            assert!(!nk.is_keyword(), "{:?} should not be a keyword", nk);
        }
    }
}
