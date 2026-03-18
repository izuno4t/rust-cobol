// COBOL AST - Arithmetic and conditional expressions

use cobol_common::Span;
use smol_str::SmolStr;

/// An arithmetic or data expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Identifier(QualifiedName),
    FunctionCall {
        name: SmolStr,
        args: Vec<Expr>,
        span: Span,
    },
    BinaryOp {
        op: ArithOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    UnaryOp {
        op: UnaryArithOp,
        operand: Box<Expr>,
        span: Span,
    },
    Paren {
        inner: Box<Expr>,
        span: Span,
    },
    /// Reference modification: `VAR(start:length)`.
    ///
    /// COBOL reference modification extracts a substring from an
    /// alphanumeric data item. Positions are 1-based.
    /// - `VAR(start:length)` -- both start and length specified
    /// - `VAR(start:)` -- start only; length defaults to remaining bytes
    ReferenceModification {
        variable: QualifiedName,
        start: Box<Expr>,
        length: Option<Box<Expr>>,
        span: Span,
    },
}

/// Binary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Power,
}

/// Unary arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryArithOp {
    Negate,
    Positive,
}

/// A conditional expression used in IF, EVALUATE, PERFORM UNTIL, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Comparison {
        left: Expr,
        op: CompareOp,
        right: Expr,
        span: Span,
    },
    ClassCondition {
        operand: Expr,
        class: ClassType,
        not: bool,
        span: Span,
    },
    SignCondition {
        operand: Expr,
        sign: SignType,
        not: bool,
        span: Span,
    },
    ConditionName(QualifiedName),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    Paren(Box<Condition>),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterEqual,
    LessEqual,
}

/// Class condition types for IS NUMERIC, IS ALPHABETIC, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassType {
    Numeric,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
    National,
}

/// Sign condition types for IS POSITIVE, IS NEGATIVE, IS ZERO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignType {
    Positive,
    Negative,
    Zero,
}

/// A qualified data name with optional OF/IN qualifiers and subscripts.
///
/// Example: `WS-FIELD OF WS-RECORD (1, WS-IDX)`
#[derive(Debug, Clone, PartialEq)]
pub struct QualifiedName {
    /// The base data name.
    pub name: SmolStr,
    /// OF/IN qualifiers, from innermost to outermost.
    pub qualifiers: Vec<SmolStr>,
    /// Subscript expressions (parenthesized indices).
    pub subscripts: Vec<Expr>,
    pub span: Span,
}

/// A literal value in COBOL source.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    Decimal(String),
    String(SmolStr),
    HexString(SmolStr),
    Boolean(SmolStr),
    National(SmolStr),
    FigurativeConstant(FigurativeConstant),
}

/// COBOL figurative constants (ZERO, SPACE, HIGH-VALUE, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FigurativeConstant {
    Zero,
    Space,
    HighValue,
    LowValue,
    Quote,
    All(SmolStr),
    Null,
}
