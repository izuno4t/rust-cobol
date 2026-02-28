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
    pub span: Span,
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
#[derive(Debug, Clone)]
pub struct HirDataItem {
    pub name: SmolStr,
    pub data_type: HirType,
    pub initial_value: Option<HirLiteral>,
    pub span: Span,
}

/// HIR-level type representation, simplified from PICTURE/USAGE.
#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    Alphanumeric { size: u32 },
    Numeric { size: u32, decimal_places: u32, is_signed: bool },
    Index,
    Pointer,
}

/// A literal value in the HIR.
#[derive(Debug, Clone)]
pub enum HirLiteral {
    Integer(i64),
    Decimal(String),
    String(SmolStr),
    Zero,
    Space,
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
        to: Vec<SmolStr>,
        span: Span,
    },
    Compute {
        target: SmolStr,
        expr: HirExpr,
        span: Span,
    },
    Add {
        operands: Vec<HirExpr>,
        to: Vec<SmolStr>,
        span: Span,
    },
    Subtract {
        operands: Vec<HirExpr>,
        from: Vec<SmolStr>,
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
        span: Span,
    },
    StopRun {
        span: Span,
    },
    Goback {
        span: Span,
    },
    Continue {
        span: Span,
    },
}

/// An expression in the HIR.
#[derive(Debug, Clone)]
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
}

/// Binary arithmetic operators.
#[derive(Debug, Clone, Copy)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Unary arithmetic operators.
#[derive(Debug, Clone, Copy)]
pub enum HirUnaryOp {
    Neg,
}

/// A conditional expression in the HIR.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Copy)]
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
    /// PERFORM procedure-name.
    ProcedureName { name: SmolStr },
}

impl std::fmt::Display for HirProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "HIR Program: {}", self.name)?;
        if !self.data_items.is_empty() {
            writeln!(f, "  Data Items:")?;
            for item in &self.data_items {
                writeln!(f, "    {} {:?} = {:?}", item.name, item.data_type, item.initial_value)?;
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
        HirStatement::Display { operands, no_advancing, .. } => {
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
            writeln!(f, "{pad}MOVE {} TO {}", format_expr(from), to.join(", "))
        }
        HirStatement::Compute { target, expr, .. } => {
            writeln!(f, "{pad}COMPUTE {} = {}", target, format_expr(expr))
        }
        HirStatement::Add { operands, to, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            writeln!(f, "{pad}ADD {} TO {}", ops.join(" "), to.join(", "))
        }
        HirStatement::Subtract { operands, from, .. } => {
            let ops: Vec<_> = operands.iter().map(format_expr).collect();
            writeln!(f, "{pad}SUBTRACT {} FROM {}", ops.join(" "), from.join(", "))
        }
        HirStatement::If { then_body, else_body, .. } => {
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
        HirStatement::Goback { .. } => writeln!(f, "{pad}GOBACK"),
        HirStatement::Continue { .. } => writeln!(f, "{pad}CONTINUE"),
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
    }
}
