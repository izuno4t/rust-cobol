// COBOL HIR - AST to HIR lowering
//
// Converts a parsed COBOL AST into the simplified HIR form:
// - Extracts data items from DATA DIVISION
// - Flattens PROCEDURE DIVISION into a list of HIR statements
// - Desugars EVALUATE into nested IF

use cobol_ast::{
    CobolProgram, DataDivision, DataItem, Expr, Literal, Statement,
    data_div::ValueClause,
    expr::{ArithOp, CompareOp, Condition, FigurativeConstant, UnaryArithOp},
    proc_div::{Paragraph, ProcedureDivision},
    statement::{
        AddStatement, ComputeStatement, DisplayStatement, EvaluateStatement, IfStatement,
        MoveStatement, PerformKind, PerformStatement, SubtractStatement, CallStatement,
    },
};
use cobol_common::Span;

use crate::hir::{
    HirBinOp, HirCompareOp, HirCondition, HirDataItem, HirExpr, HirLiteral, HirParagraph,
    HirPerformKind, HirProgram, HirStatement, HirType, HirUnaryOp,
};

/// Lowers a COBOL AST program into the HIR.
pub fn lower_to_hir(program: &CobolProgram) -> HirProgram {
    let name = program.identification.program_id.clone();

    let data_items = program
        .data
        .as_ref()
        .map(lower_data_division)
        .unwrap_or_default();

    let (body, paragraphs) = program
        .procedure
        .as_ref()
        .map(lower_procedure_division)
        .unwrap_or_default();

    HirProgram {
        name,
        data_items,
        paragraphs,
        body,
        span: program.span,
    }
}

// ---------------------------------------------------------------------------
// Data Division lowering
// ---------------------------------------------------------------------------

fn lower_data_division(data: &DataDivision) -> Vec<HirDataItem> {
    let mut items = Vec::new();
    for item in &data.working_storage {
        lower_data_item(item, &mut items);
    }
    for item in &data.local_storage {
        lower_data_item(item, &mut items);
    }
    for item in &data.linkage {
        lower_data_item(item, &mut items);
    }
    items
}

fn lower_data_item(item: &DataItem, out: &mut Vec<HirDataItem>) {
    // Skip FILLER and level 88 condition names
    if item.level == 88 {
        return;
    }

    if let Some(name) = &item.name {
        let data_type = determine_hir_type(item);
        let initial_value = item.value.as_ref().map(lower_value_clause);

        out.push(HirDataItem {
            name: name.clone(),
            data_type,
            initial_value,
            span: item.span,
        });
    }

    // Recursively lower child items (group items)
    for child in &item.children {
        lower_data_item(child, out);
    }
}

fn determine_hir_type(item: &DataItem) -> HirType {
    // Check USAGE first for special types
    if let Some(usage) = &item.usage {
        match usage {
            cobol_ast::Usage::Index => return HirType::Index,
            cobol_ast::Usage::Pointer | cobol_ast::Usage::FunctionPointer => {
                return HirType::Pointer
            }
            _ => {}
        }
    }

    // Derive type from PICTURE clause
    if let Some(pic) = &item.picture {
        match pic.category {
            cobol_ast::PictureCategory::Numeric | cobol_ast::PictureCategory::NumericEdited => {
                HirType::Numeric {
                    size: pic.size,
                    decimal_places: pic.decimal_positions,
                    is_signed: pic.is_signed,
                }
            }
            _ => HirType::Alphanumeric { size: pic.size },
        }
    } else if !item.children.is_empty() {
        // Group item: treated as alphanumeric with the sum of child sizes
        let total: u32 = item
            .children
            .iter()
            .map(|c| c.picture.as_ref().map(|p| p.size).unwrap_or(0))
            .sum();
        HirType::Alphanumeric {
            size: if total == 0 { 1 } else { total },
        }
    } else {
        // Default: single character alphanumeric
        HirType::Alphanumeric { size: 1 }
    }
}

fn lower_value_clause(value: &ValueClause) -> HirLiteral {
    lower_literal(&value.value)
}

fn lower_literal(lit: &Literal) -> HirLiteral {
    match lit {
        Literal::Integer(n) => HirLiteral::Integer(*n),
        Literal::Decimal(d) => HirLiteral::Decimal(d.clone()),
        Literal::String(s) => HirLiteral::String(s.clone()),
        Literal::FigurativeConstant(FigurativeConstant::Zero) => HirLiteral::Zero,
        Literal::FigurativeConstant(FigurativeConstant::Space) => HirLiteral::Space,
        Literal::FigurativeConstant(_) => HirLiteral::Zero, // simplification for now
        Literal::HexString(s) => HirLiteral::String(s.clone()),
        Literal::Boolean(s) => HirLiteral::String(s.clone()),
        Literal::National(s) => HirLiteral::String(s.clone()),
    }
}

// ---------------------------------------------------------------------------
// Procedure Division lowering
// ---------------------------------------------------------------------------

fn lower_procedure_division(
    proc: &ProcedureDivision,
) -> (Vec<HirStatement>, Vec<HirParagraph>) {
    let mut body = Vec::new();
    let mut paragraphs = Vec::new();

    // Lower top-level paragraphs
    for para in &proc.paragraphs {
        let stmts = lower_paragraph(para);
        if !stmts.is_empty() {
            // If the paragraph has a generated or empty name, inline its statements
            // into the body. Otherwise, keep it as a named paragraph.
            if para.name.is_empty() {
                body.extend(stmts);
            } else {
                // Add statements to body (for sequential execution) and
                // also record as a named paragraph (for PERFORM references).
                body.extend(stmts.clone());
                paragraphs.push(HirParagraph {
                    name: para.name.clone(),
                    body: stmts,
                    span: para.span,
                });
            }
        }
    }

    // Lower sections and their paragraphs
    for section in &proc.sections {
        for para in &section.paragraphs {
            let stmts = lower_paragraph(para);
            if !stmts.is_empty() {
                body.extend(stmts.clone());
                paragraphs.push(HirParagraph {
                    name: para.name.clone(),
                    body: stmts,
                    span: para.span,
                });
            }
        }
    }

    (body, paragraphs)
}

fn lower_paragraph(para: &Paragraph) -> Vec<HirStatement> {
    let mut stmts = Vec::new();
    for sentence in &para.sentences {
        for stmt in &sentence.statements {
            if let Some(hir_stmt) = lower_statement(stmt) {
                stmts.push(hir_stmt);
            }
        }
    }
    stmts
}

fn lower_statement(stmt: &Statement) -> Option<HirStatement> {
    match stmt {
        Statement::Display(display) => Some(lower_display(display)),
        Statement::Move(mv) => Some(lower_move(mv)),
        Statement::Compute(compute) => lower_compute(compute),
        Statement::Add(add) => Some(lower_add(add)),
        Statement::Subtract(sub) => Some(lower_subtract(sub)),
        Statement::If(if_stmt) => Some(lower_if(if_stmt)),
        Statement::Evaluate(eval) => Some(lower_evaluate(eval)),
        Statement::Perform(perform) => Some(lower_perform(perform)),
        Statement::Call(call) => Some(lower_call(call)),
        Statement::StopRun => Some(HirStatement::StopRun { span: Span::dummy() }),
        Statement::Goback => Some(HirStatement::Goback { span: Span::dummy() }),
        Statement::Continue => Some(HirStatement::Continue { span: Span::dummy() }),
        Statement::ExitProgram => Some(HirStatement::StopRun { span: Span::dummy() }),
        // Statements not yet lowered are silently skipped
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Statement lowering
// ---------------------------------------------------------------------------

fn lower_display(display: &DisplayStatement) -> HirStatement {
    let operands = display.operands.iter().map(lower_expr).collect();
    HirStatement::Display {
        operands,
        no_advancing: display.with_no_advancing,
        span: display.span,
    }
}

fn lower_move(mv: &MoveStatement) -> HirStatement {
    let from = lower_expr(&mv.from);
    let to = mv.to.iter().map(|q| q.name.clone()).collect();
    HirStatement::Move {
        from,
        to,
        span: mv.span,
    }
}

fn lower_compute(compute: &ComputeStatement) -> Option<HirStatement> {
    let target = compute.targets.first()?;
    let expr = lower_expr(&compute.expr);
    Some(HirStatement::Compute {
        target: target.target.name.clone(),
        expr,
        span: compute.span,
    })
}

fn lower_add(add: &AddStatement) -> HirStatement {
    let operands = add.operands.iter().map(lower_expr).collect();
    let to = add.to.iter().map(|t| t.target.name.clone()).collect();
    HirStatement::Add {
        operands,
        to,
        span: add.span,
    }
}

fn lower_subtract(sub: &SubtractStatement) -> HirStatement {
    let operands = sub.operands.iter().map(lower_expr).collect();
    let from = sub.from.iter().map(|t| t.target.name.clone()).collect();
    HirStatement::Subtract {
        operands,
        from,
        span: sub.span,
    }
}

fn lower_if(if_stmt: &IfStatement) -> HirStatement {
    let condition = lower_condition(&if_stmt.condition);
    let then_body: Vec<_> = if_stmt
        .then_body
        .iter()
        .filter_map(lower_statement)
        .collect();
    let else_body: Vec<_> = if_stmt
        .else_body
        .iter()
        .filter_map(lower_statement)
        .collect();
    HirStatement::If {
        condition,
        then_body,
        else_body,
        span: if_stmt.span,
    }
}

/// Desugar EVALUATE into nested IF statements.
fn lower_evaluate(eval: &EvaluateStatement) -> HirStatement {
    // Build nested IF chain from the WHEN clauses
    let mut else_body: Vec<HirStatement> = eval
        .when_other
        .iter()
        .filter_map(lower_statement)
        .collect();

    // Process WHEN clauses in reverse to build the else chain
    for when in eval.when_clauses.iter().rev() {
        let then_body: Vec<HirStatement> =
            when.body.iter().filter_map(lower_statement).collect();

        // Build condition from the WHEN objects and subjects
        let condition = build_evaluate_condition(&eval.subjects, &when.objects);

        let if_stmt = HirStatement::If {
            condition,
            then_body,
            else_body,
            span: when.span,
        };

        else_body = vec![if_stmt];
    }

    // The result is the outermost IF (or the first element of else_body)
    if else_body.len() == 1 {
        else_body.remove(0)
    } else {
        // Wrap in an inline PERFORM if multiple statements
        HirStatement::Perform {
            kind: HirPerformKind::Inline { body: else_body },
            span: eval.span,
        }
    }
}

fn build_evaluate_condition(
    subjects: &[cobol_ast::statement::EvaluateSubject],
    object_groups: &[Vec<cobol_ast::statement::WhenObject>],
) -> HirCondition {
    use cobol_ast::statement::{EvaluateSubject, WhenObject};

    // For each subject/object pair, build a condition and AND them together
    let mut conditions: Vec<HirCondition> = Vec::new();

    for (i, objects) in object_groups.iter().enumerate() {
        let subject_expr = if i < subjects.len() {
            match &subjects[i] {
                EvaluateSubject::Expr(e) => Some(lower_expr(e)),
                EvaluateSubject::True => None,
                EvaluateSubject::False => None,
                EvaluateSubject::Condition(_) => None,
            }
        } else {
            None
        };

        for obj in objects {
            match obj {
                WhenObject::Any => {
                    // ANY matches everything -- skip adding condition
                }
                WhenObject::Expr(e) => {
                    if let Some(ref subj) = subject_expr {
                        conditions.push(HirCondition::Compare {
                            left: subj.clone(),
                            op: HirCompareOp::Eq,
                            right: lower_expr(e),
                        });
                    }
                }
                WhenObject::Condition(c) => {
                    conditions.push(lower_condition(c));
                }
                WhenObject::True => {
                    // TRUE matches when the subject evaluates to true
                }
                WhenObject::False => {
                    // FALSE matches when the subject evaluates to false
                }
                WhenObject::Range { from, to } => {
                    if let Some(ref subj) = subject_expr {
                        let ge = HirCondition::Compare {
                            left: subj.clone(),
                            op: HirCompareOp::Ge,
                            right: lower_expr(from),
                        };
                        let le = HirCondition::Compare {
                            left: subj.clone(),
                            op: HirCompareOp::Le,
                            right: lower_expr(to),
                        };
                        conditions.push(HirCondition::And(Box::new(ge), Box::new(le)));
                    }
                }
                WhenObject::Not(inner) => {
                    // Recursively handle NOT
                    let inner_cond = build_evaluate_condition(
                        subjects,
                        &[vec![*inner.clone()]],
                    );
                    conditions.push(HirCondition::Not(Box::new(inner_cond)));
                }
            }
        }
    }

    // AND all conditions together; if none, use a tautology
    if conditions.is_empty() {
        HirCondition::Compare {
            left: HirExpr::Literal(HirLiteral::Integer(1)),
            op: HirCompareOp::Eq,
            right: HirExpr::Literal(HirLiteral::Integer(1)),
        }
    } else {
        conditions
            .into_iter()
            .reduce(|acc, c| HirCondition::And(Box::new(acc), Box::new(c)))
            .unwrap()
    }
}

fn lower_perform(perform: &PerformStatement) -> HirStatement {
    let kind = match &perform.kind {
        PerformKind::Simple { body } => {
            let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
            HirPerformKind::Inline { body: hir_body }
        }
        PerformKind::ProcedureName { procedure, .. } => {
            HirPerformKind::ProcedureName {
                name: procedure.clone(),
            }
        }
        PerformKind::Times { times, body } => {
            let count = lower_expr(times);
            let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
            HirPerformKind::Times {
                count,
                body: hir_body,
            }
        }
        PerformKind::Until { condition, body, .. } => {
            let hir_cond = lower_condition(condition);
            let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
            HirPerformKind::Until {
                condition: hir_cond,
                body: hir_body,
            }
        }
        PerformKind::Varying { varying, body, .. } => {
            if let Some(clause) = varying.first() {
                let var = clause.identifier.name.clone();
                let from = lower_expr(&clause.from);
                let by = lower_expr(&clause.by);
                let until = lower_condition(&clause.until);
                let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
                HirPerformKind::Varying {
                    var,
                    from,
                    by,
                    until,
                    body: hir_body,
                }
            } else {
                let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
                HirPerformKind::Inline { body: hir_body }
            }
        }
    };
    HirStatement::Perform {
        kind,
        span: perform.span,
    }
}

fn lower_call(call: &CallStatement) -> HirStatement {
    let program = lower_expr(&call.program);
    let params = call.using.iter().map(|p| lower_expr(&p.value)).collect();
    HirStatement::Call {
        program,
        params,
        span: call.span,
    }
}

// ---------------------------------------------------------------------------
// Expression and condition lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &Expr) -> HirExpr {
    match expr {
        Expr::Literal(lit) => HirExpr::Literal(lower_literal(lit)),
        Expr::Identifier(qname) => HirExpr::Variable(qname.name.clone()),
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let hir_op = match op {
                ArithOp::Add => HirBinOp::Add,
                ArithOp::Subtract => HirBinOp::Sub,
                ArithOp::Multiply => HirBinOp::Mul,
                ArithOp::Divide => HirBinOp::Div,
                ArithOp::Power => HirBinOp::Pow,
            };
            HirExpr::BinaryOp {
                op: hir_op,
                left: Box::new(lower_expr(left)),
                right: Box::new(lower_expr(right)),
            }
        }
        Expr::UnaryOp { op, operand, .. } => match op {
            UnaryArithOp::Negate => HirExpr::UnaryOp {
                op: HirUnaryOp::Neg,
                operand: Box::new(lower_expr(operand)),
            },
            UnaryArithOp::Positive => lower_expr(operand),
        },
        Expr::Paren { inner, .. } => lower_expr(inner),
        Expr::FunctionCall { name, args: _, .. } => {
            // Function calls are not fully supported yet; use the name as a
            // variable reference for now.
            HirExpr::Variable(name.clone())
        }
    }
}

fn lower_condition(cond: &Condition) -> HirCondition {
    match cond {
        Condition::Comparison {
            left, op, right, ..
        } => {
            let hir_op = match op {
                CompareOp::Equal => HirCompareOp::Eq,
                CompareOp::NotEqual => HirCompareOp::Ne,
                CompareOp::GreaterThan => HirCompareOp::Gt,
                CompareOp::LessThan => HirCompareOp::Lt,
                CompareOp::GreaterEqual => HirCompareOp::Ge,
                CompareOp::LessEqual => HirCompareOp::Le,
            };
            HirCondition::Compare {
                left: lower_expr(left),
                op: hir_op,
                right: lower_expr(right),
            }
        }
        Condition::And(a, b) => HirCondition::And(
            Box::new(lower_condition(a)),
            Box::new(lower_condition(b)),
        ),
        Condition::Or(a, b) => HirCondition::Or(
            Box::new(lower_condition(a)),
            Box::new(lower_condition(b)),
        ),
        Condition::Not(inner) => HirCondition::Not(Box::new(lower_condition(inner))),
        Condition::Paren(inner) => lower_condition(inner),
        // Class and sign conditions are simplified to a comparison for now
        Condition::ClassCondition { .. } | Condition::SignCondition { .. } => {
            // Placeholder: always true
            HirCondition::Compare {
                left: HirExpr::Literal(HirLiteral::Integer(1)),
                op: HirCompareOp::Eq,
                right: HirExpr::Literal(HirLiteral::Integer(1)),
            }
        }
        Condition::ConditionName(qname) => {
            // Condition name: reference the variable
            HirCondition::Compare {
                left: HirExpr::Variable(qname.name.clone()),
                op: HirCompareOp::Eq,
                right: HirExpr::Literal(HirLiteral::Integer(1)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::{FileId, SourceFormat};
    use cobol_lexer::Lexer;
    use cobol_parser::Parser;

    fn parse_and_lower(source: &str) -> HirProgram {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        let program = parser.parse_program().unwrap();
        lower_to_hir(&program)
    }

    #[test]
    fn test_lower_hello_world() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert_eq!(hir.name.as_str(), "HELLO-WORLD");
        assert!(!hir.body.is_empty());

        // First statement should be DISPLAY
        assert!(matches!(hir.body[0], HirStatement::Display { .. }));
        // Second statement should be STOP RUN
        assert!(matches!(hir.body[1], HirStatement::StopRun { .. }));
    }

    #[test]
    fn test_lower_display_operands() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DISPLAY.
PROCEDURE DIVISION.
    DISPLAY \"Hello\" \"World\".
    STOP RUN.
";
        let hir = parse_and_lower(src);
        if let HirStatement::Display { operands, .. } = &hir.body[0] {
            assert_eq!(operands.len(), 2);
        } else {
            panic!("Expected DISPLAY statement");
        }
    }

    #[test]
    fn test_lower_data_items() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DATA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
01  WS-COUNT PIC 9(5).
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert_eq!(hir.data_items.len(), 2);
        assert_eq!(hir.data_items[0].name.as_str(), "WS-NAME");
        assert_eq!(
            hir.data_items[0].data_type,
            HirType::Alphanumeric { size: 20 }
        );
        assert_eq!(hir.data_items[1].name.as_str(), "WS-COUNT");
        assert!(matches!(
            hir.data_items[1].data_type,
            HirType::Numeric { size: 5, .. }
        ));
    }

    #[test]
    fn test_lower_move() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-MOVE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC X(10).
01  WS-B PIC X(10).
PROCEDURE DIVISION.
    MOVE \"HELLO\" TO WS-A WS-B.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        if let HirStatement::Move { to, .. } = &hir.body[0] {
            assert_eq!(to.len(), 2);
        } else {
            panic!("Expected MOVE statement");
        }
    }

    #[test]
    fn test_lower_if() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-IF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3).
PROCEDURE DIVISION.
    IF WS-A > 100
        DISPLAY \"BIG\"
    ELSE
        DISPLAY \"SMALL\"
    END-IF.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        if let HirStatement::If {
            then_body,
            else_body,
            ..
        } = &hir.body[0]
        {
            assert_eq!(then_body.len(), 1);
            assert_eq!(else_body.len(), 1);
        } else {
            panic!("Expected IF statement");
        }
    }

    #[test]
    fn test_lower_program_display_format() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO.
PROCEDURE DIVISION.
    DISPLAY \"Hello\".
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let output = format!("{}", hir);
        assert!(output.contains("HELLO"));
        assert!(output.contains("DISPLAY"));
        assert!(output.contains("STOP RUN"));
    }
}
