// COBOL HIR - AST to HIR lowering
//
// Converts a parsed COBOL AST into the simplified HIR form:
// - Extracts data items from DATA DIVISION
// - Flattens PROCEDURE DIVISION into a list of HIR statements
// - Desugars EVALUATE into nested IF

use cobol_ast::{
    data_div::ValueClause,
    expr::{ArithOp, CompareOp, Condition, FigurativeConstant, UnaryArithOp},
    proc_div::{Paragraph, ProcedureDivision},
    statement::{
        AcceptStatement, AddStatement, CallStatement, ComputeStatement, DisplayStatement,
        DivideStatement, EvaluateStatement, GoToStatement, IfStatement, InitializeStatement,
        MoveStatement, MultiplyStatement, PerformKind, PerformStatement, SetStatement,
        SubtractStatement,
    },
    CobolProgram, DataDivision, DataItem, Expr, Literal, Statement, Usage,
};
use cobol_common::Span;
use smol_str::SmolStr;

use crate::hir::{
    HirBinOp, HirCompareOp, HirCondition, HirDataItem, HirExpr, HirLiteral, HirOpenEntry,
    HirOpenMode, HirParagraph, HirPerformKind, HirProgram, HirStatement, HirType, HirUnaryOp,
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
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
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
            Usage::Index => return HirType::Index,
            Usage::Pointer | Usage::FunctionPointer => return HirType::Pointer,
            Usage::Comp3 | Usage::PackedDecimal => {
                if let Some(pic) = &item.picture {
                    return HirType::Comp3 {
                        size: pic.size,
                        decimal_places: pic.decimal_positions,
                    };
                }
                return HirType::Comp3 {
                    size: 1,
                    decimal_places: 0,
                };
            }
            Usage::Comp | Usage::Comp4 | Usage::Comp5 | Usage::Binary | Usage::Computational => {
                if let Some(pic) = &item.picture {
                    return HirType::Binary { size: pic.size };
                }
                return HirType::Binary { size: 4 };
            }
            Usage::FloatShort | Usage::Comp1 => return HirType::FloatShort,
            Usage::FloatLong | Usage::Comp2 => return HirType::FloatLong,
            Usage::FloatExtended => return HirType::FloatExtended,
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
        // Group item: build members list and compute total size
        let mut members = Vec::new();
        for child in &item.children {
            if child.level == 88 {
                continue;
            }
            if let Some(name) = &child.name {
                let data_type = determine_hir_type(child);
                let initial_value = child.value.as_ref().map(lower_value_clause);
                members.push(HirDataItem {
                    name: name.clone(),
                    data_type,
                    initial_value,
                    span: child.span,
                });
            }
        }
        let total: u32 = members
            .iter()
            .map(|m| match &m.data_type {
                HirType::Alphanumeric { size } => *size,
                HirType::Numeric { size, .. } => *size,
                HirType::Group { size, .. } => *size,
                HirType::Comp3 { size, .. } => (*size + 2) / 2, // packed decimal byte size
                HirType::Binary { size } => {
                    if *size <= 4 {
                        2
                    } else if *size <= 9 {
                        4
                    } else {
                        8
                    }
                }
                HirType::Index => 4,
                HirType::Pointer => 8,
                HirType::Boolean => 1,
                HirType::FloatShort => 4,
                HirType::FloatLong => 8,
                HirType::FloatExtended => 16,
            })
            .sum();
        HirType::Group {
            members,
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

fn lower_procedure_division(proc: &ProcedureDivision) -> (Vec<HirStatement>, Vec<HirParagraph>) {
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
        Statement::Accept(accept) => Some(lower_accept(accept)),
        Statement::Move(mv) => Some(lower_move(mv)),
        Statement::Compute(compute) => lower_compute(compute),
        Statement::Add(add) => Some(lower_add(add)),
        Statement::Subtract(sub) => Some(lower_subtract(sub)),
        Statement::Multiply(mul) => Some(lower_multiply(mul)),
        Statement::Divide(div) => Some(lower_divide(div)),
        Statement::If(if_stmt) => Some(lower_if(if_stmt)),
        Statement::Evaluate(eval) => Some(lower_evaluate(eval)),
        Statement::Perform(perform) => Some(lower_perform(perform)),
        Statement::Call(call) => Some(lower_call(call)),
        Statement::GoTo(goto) => Some(lower_goto(goto)),
        Statement::Open(open) => Some(lower_open(open)),
        Statement::Close(close) => Some(lower_close(close)),
        Statement::Read(read) => Some(lower_read(read)),
        Statement::Write(write) => Some(lower_write(write)),
        Statement::Rewrite(rewrite) => Some(lower_rewrite(rewrite)),
        Statement::Delete(delete) => Some(lower_delete(delete)),
        Statement::Initialize(init) => Some(lower_initialize(init)),
        Statement::Set(set) => Some(lower_set(set)),
        Statement::String(string_stmt) => Some(lower_string_stmt(string_stmt)),
        Statement::Unstring(unstring_stmt) => Some(lower_unstring_stmt(unstring_stmt)),
        Statement::Sort(sort) => Some(lower_sort(sort)),
        Statement::StopRun => Some(HirStatement::StopRun {
            span: Span::dummy(),
        }),
        Statement::Goback => Some(HirStatement::Goback {
            span: Span::dummy(),
        }),
        Statement::Continue => Some(HirStatement::Continue {
            span: Span::dummy(),
        }),
        Statement::ExitProgram => Some(HirStatement::StopRun {
            span: Span::dummy(),
        }),
        Statement::ExitParagraph | Statement::ExitSection => Some(HirStatement::Continue {
            span: Span::dummy(),
        }),
        // --- COBOL 2002+ statements ---
        Statement::Raise(raise) => Some(lower_raise(raise)),
        Statement::Resume(resume) => Some(lower_resume(resume)),
        Statement::Invoke(invoke) => Some(lower_invoke(invoke)),
        Statement::Allocate(alloc) => Some(lower_allocate(alloc)),
        Statement::Free(free) => Some(lower_free(free)),
        // --- COBOL 2014+ statements ---
        Statement::JsonGenerate(jg) => Some(lower_json_generate(jg)),
        Statement::JsonParse(jp) => Some(lower_json_parse(jp)),
        Statement::XmlGenerate(xg) => Some(lower_xml_generate(xg)),
        Statement::XmlParse(xp) => Some(lower_xml_parse(xp)),
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
    let mut else_body: Vec<HirStatement> =
        eval.when_other.iter().filter_map(lower_statement).collect();

    // Process WHEN clauses in reverse to build the else chain
    for when in eval.when_clauses.iter().rev() {
        let then_body: Vec<HirStatement> = when.body.iter().filter_map(lower_statement).collect();

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
                    let inner_cond = build_evaluate_condition(subjects, &[vec![*inner.clone()]]);
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
        PerformKind::ProcedureName { procedure, .. } => HirPerformKind::ProcedureName {
            name: procedure.clone(),
        },
        PerformKind::Times { times, body } => {
            let count = lower_expr(times);
            let hir_body: Vec<_> = body.iter().filter_map(lower_statement).collect();
            HirPerformKind::Times {
                count,
                body: hir_body,
            }
        }
        PerformKind::Until {
            condition, body, ..
        } => {
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

fn lower_multiply(mul: &MultiplyStatement) -> HirStatement {
    let operand = lower_expr(&mul.operand);
    let by = mul.by.iter().map(|t| t.target.name.clone()).collect();
    HirStatement::Multiply {
        operand,
        by,
        span: mul.span,
    }
}

fn lower_divide(div: &DivideStatement) -> HirStatement {
    let operand = lower_expr(&div.operand);
    let into = div.into.iter().map(|t| t.target.name.clone()).collect();
    let remainder = div.remainder.as_ref().map(|r| r.name.clone());
    HirStatement::Divide {
        operand,
        into,
        remainder,
        span: div.span,
    }
}

fn lower_accept(accept: &AcceptStatement) -> HirStatement {
    HirStatement::Accept {
        target: accept.target.name.clone(),
        span: accept.span,
    }
}

fn lower_goto(goto: &GoToStatement) -> HirStatement {
    let targets = goto.targets.clone();
    let depending_on = goto.depending_on.as_ref().map(|q| q.name.clone());
    HirStatement::GoTo {
        targets,
        depending_on,
        span: goto.span,
    }
}

fn lower_open(open: &cobol_ast::statement::OpenStatement) -> HirStatement {
    let entries = open
        .entries
        .iter()
        .map(|e| {
            let mode = match e.mode {
                cobol_ast::statement::OpenMode::Input => HirOpenMode::Input,
                cobol_ast::statement::OpenMode::Output => HirOpenMode::Output,
                cobol_ast::statement::OpenMode::IoMode => HirOpenMode::IoMode,
                cobol_ast::statement::OpenMode::Extend => HirOpenMode::Extend,
            };
            HirOpenEntry {
                mode,
                file_name: e.file_name.clone(),
            }
        })
        .collect();
    HirStatement::Open {
        entries,
        span: open.span,
    }
}

fn lower_close(close: &cobol_ast::statement::CloseStatement) -> HirStatement {
    let files = close.files.iter().map(|e| e.file_name.clone()).collect();
    HirStatement::Close {
        files,
        span: close.span,
    }
}

fn lower_read(read: &cobol_ast::statement::ReadStatement) -> HirStatement {
    let into = read.into.as_ref().map(|q| q.name.clone());
    let at_end: Vec<_> = read.at_end.iter().filter_map(lower_statement).collect();
    let not_at_end: Vec<_> = read.not_at_end.iter().filter_map(lower_statement).collect();
    HirStatement::Read {
        file_name: read.file_name.clone(),
        into,
        at_end,
        not_at_end,
        span: read.span,
    }
}

fn lower_write(write: &cobol_ast::statement::WriteStatement) -> HirStatement {
    let from = write.from.as_ref().map(lower_expr);
    HirStatement::Write {
        record_name: write.record_name.name.clone(),
        from,
        span: write.span,
    }
}

fn lower_rewrite(rewrite: &cobol_ast::statement::RewriteStatement) -> HirStatement {
    let from = rewrite.from.as_ref().map(lower_expr);
    HirStatement::Rewrite {
        record_name: rewrite.record_name.name.clone(),
        from,
        span: rewrite.span,
    }
}

fn lower_delete(delete: &cobol_ast::statement::DeleteStatement) -> HirStatement {
    HirStatement::Delete {
        file_name: delete.file_name.clone(),
        span: delete.span,
    }
}

fn lower_initialize(init: &InitializeStatement) -> HirStatement {
    let targets = init.targets.iter().map(|q| q.name.clone()).collect();
    HirStatement::Initialize {
        targets,
        span: init.span,
    }
}

fn lower_set(set: &SetStatement) -> HirStatement {
    use cobol_ast::statement::SetKind;
    match &set.kind {
        SetKind::To { targets, value } => {
            let target_names = targets.iter().map(|q| q.name.clone()).collect();
            let hir_value = lower_expr(value);
            HirStatement::Set {
                targets: target_names,
                value: hir_value,
                span: set.span,
            }
        }
        SetKind::UpDown {
            targets,
            direction,
            value,
        } => {
            // Desugar SET UP/DOWN BY to an arithmetic operation
            let target_names: Vec<_> = targets.iter().map(|q| q.name.clone()).collect();
            let hir_value = lower_expr(value);
            match direction {
                cobol_ast::statement::SetDirection::Up => HirStatement::Add {
                    operands: vec![hir_value],
                    to: target_names,
                    span: set.span,
                },
                cobol_ast::statement::SetDirection::Down => HirStatement::Subtract {
                    operands: vec![hir_value],
                    from: target_names,
                    span: set.span,
                },
            }
        }
        SetKind::ConditionTrue { conditions, value } => {
            // SET condition-name TO TRUE/FALSE
            let target_names = conditions.iter().map(|q| q.name.clone()).collect();
            let hir_value = if *value {
                HirExpr::Literal(HirLiteral::Integer(1))
            } else {
                HirExpr::Literal(HirLiteral::Integer(0))
            };
            HirStatement::Set {
                targets: target_names,
                value: hir_value,
                span: set.span,
            }
        }
        SetKind::Address { target, source } => {
            // SET pointer TO ADDRESS OF source -- simplified
            HirStatement::Set {
                targets: vec![target.name.clone()],
                value: HirExpr::Variable(source.name.clone()),
                span: set.span,
            }
        }
    }
}

fn lower_string_stmt(string_stmt: &cobol_ast::statement::StringStatement) -> HirStatement {
    let sources = string_stmt
        .sources
        .iter()
        .flat_map(|s| s.items.iter().map(lower_expr))
        .collect();
    HirStatement::StringStmt {
        into: string_stmt.into.name.clone(),
        sources,
        span: string_stmt.span,
    }
}

fn lower_unstring_stmt(unstring_stmt: &cobol_ast::statement::UnstringStatement) -> HirStatement {
    let into = unstring_stmt
        .into
        .iter()
        .map(|t| t.target.name.clone())
        .collect();
    HirStatement::UnstringStmt {
        source: unstring_stmt.source.name.clone(),
        into,
        span: unstring_stmt.span,
    }
}

fn lower_sort(sort: &cobol_ast::statement::SortStatement) -> HirStatement {
    HirStatement::Sort {
        file_name: sort.file_name.clone(),
        span: sort.span,
    }
}

// ---------------------------------------------------------------------------
// COBOL 2002+ statement lowering
// ---------------------------------------------------------------------------

fn lower_raise(raise: &cobol_ast::statement::RaiseStatement) -> HirStatement {
    use cobol_ast::statement::RaiseTarget;
    let exception = match &raise.exception {
        RaiseTarget::Exception(name) => name.clone(),
        RaiseTarget::Identifier(qname) => qname.name.clone(),
    };
    HirStatement::Raise {
        exception,
        span: raise.span,
    }
}

fn lower_resume(resume: &cobol_ast::statement::ResumeStatement) -> HirStatement {
    HirStatement::Resume {
        target: resume.target.clone(),
        span: resume.span,
    }
}

fn lower_invoke(invoke: &cobol_ast::statement::InvokeStatement) -> HirStatement {
    let object = lower_expr(&invoke.object);
    let method = match &invoke.method {
        Expr::Literal(Literal::String(s)) => s.clone(),
        Expr::Identifier(qname) => qname.name.clone(),
        _ => SmolStr::new("UNKNOWN"),
    };
    let params = invoke.using.iter().map(|p| lower_expr(&p.value)).collect();
    let returning = invoke.returning.as_ref().map(|q| q.name.clone());
    HirStatement::Invoke {
        object,
        method,
        params,
        returning,
        span: invoke.span,
    }
}

fn lower_allocate(alloc: &cobol_ast::statement::AllocateStatement) -> HirStatement {
    use cobol_ast::statement::AllocateTarget;
    let target = match &alloc.target {
        AllocateTarget::DataName(qname) => qname.name.clone(),
        AllocateTarget::Characters(_) => SmolStr::new("_ALLOC_CHARS"),
    };
    let returning = alloc.returning.as_ref().map(|q| q.name.clone());
    HirStatement::Allocate {
        target,
        returning,
        span: alloc.span,
    }
}

fn lower_free(free: &cobol_ast::statement::FreeStatement) -> HirStatement {
    let targets = free.targets.iter().map(|q| q.name.clone()).collect();
    HirStatement::Free {
        targets,
        span: free.span,
    }
}

// ---------------------------------------------------------------------------
// COBOL 2014+ statement lowering
// ---------------------------------------------------------------------------

fn lower_json_generate(jg: &cobol_ast::statement::JsonGenerateStatement) -> HirStatement {
    HirStatement::JsonGenerate {
        source: jg.source.name.clone(),
        target: jg.target.name.clone(),
        span: jg.span,
    }
}

fn lower_json_parse(jp: &cobol_ast::statement::JsonParseStatement) -> HirStatement {
    HirStatement::JsonParse {
        source: jp.source.name.clone(),
        target: jp.target.name.clone(),
        span: jp.span,
    }
}

fn lower_xml_generate(xg: &cobol_ast::statement::XmlGenerateStatement) -> HirStatement {
    HirStatement::XmlGenerate {
        source: xg.source.name.clone(),
        target: xg.target.name.clone(),
        span: xg.span,
    }
}

fn lower_xml_parse(xp: &cobol_ast::statement::XmlParseStatement) -> HirStatement {
    HirStatement::XmlParse {
        source: xp.source.name.clone(),
        processing_procedure: xp.processing_procedure.clone(),
        span: xp.span,
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
        Condition::And(a, b) => {
            HirCondition::And(Box::new(lower_condition(a)), Box::new(lower_condition(b)))
        }
        Condition::Or(a, b) => {
            HirCondition::Or(Box::new(lower_condition(a)), Box::new(lower_condition(b)))
        }
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

    // -----------------------------------------------------------------------
    // COBOL 2002+ tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_lower_raise_exception() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RAISE.
PROCEDURE DIVISION.
    RAISE EXCEPTION \"EC-SIZE-OVERFLOW\".
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(
            hir.body
                .iter()
                .any(|s| matches!(s, HirStatement::Raise { .. })),
            "Expected RAISE statement in HIR body"
        );
    }

    #[test]
    fn test_lower_resume() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RESUME.
PROCEDURE DIVISION.
    RESUME.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(
            hir.body
                .iter()
                .any(|s| matches!(s, HirStatement::Resume { .. })),
            "Expected RESUME statement in HIR body"
        );
    }

    #[test]
    fn test_lower_invoke() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INVOKE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  MY-OBJ USAGE POINTER.
01  MY-RESULT PIC 9(5).
PROCEDURE DIVISION.
    INVOKE MY-OBJ \"DO-SOMETHING\" RETURNING MY-RESULT.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(
            hir.body
                .iter()
                .any(|s| matches!(s, HirStatement::Invoke { .. })),
            "Expected INVOKE statement in HIR body"
        );
    }

    #[test]
    fn test_lower_allocate_and_free() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ALLOC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  MY-PTR USAGE POINTER.
PROCEDURE DIVISION.
    ALLOCATE MY-PTR.
    FREE MY-PTR.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(
            hir.body
                .iter()
                .any(|s| matches!(s, HirStatement::Allocate { .. })),
            "Expected ALLOCATE statement in HIR body"
        );
        assert!(
            hir.body
                .iter()
                .any(|s| matches!(s, HirStatement::Free { .. })),
            "Expected FREE statement in HIR body"
        );
    }

    #[test]
    fn test_lower_local_storage() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-LOCAL.
DATA DIVISION.
LOCAL-STORAGE SECTION.
01  LS-COUNTER PIC 9(5) VALUE 0.
WORKING-STORAGE SECTION.
01  WS-COUNTER PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    ADD 1 TO LS-COUNTER.
    ADD 1 TO WS-COUNTER.
    DISPLAY LS-COUNTER.
    DISPLAY WS-COUNTER.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(hir.data_items.len() >= 2, "Expected at least 2 data items");
        // Both LOCAL-STORAGE and WORKING-STORAGE items should be present
        assert!(hir
            .data_items
            .iter()
            .any(|d| d.name.as_str() == "LS-COUNTER"));
        assert!(hir
            .data_items
            .iter()
            .any(|d| d.name.as_str() == "WS-COUNTER"));
    }

    #[test]
    fn test_lower_boolean_literal() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-BOOL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLAG PIC 9(1) VALUE 0.
PROCEDURE DIVISION.
    MOVE 1 TO WS-FLAG.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(hir.data_items.iter().any(|d| d.name.as_str() == "WS-FLAG"));
    }

    #[test]
    fn test_hir_program_has_classes_and_functions() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-OOP.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        // Classes and functions should be empty for a normal program
        assert!(hir.classes.is_empty());
        assert!(hir.functions.is_empty());
    }

    // -----------------------------------------------------------------------
    // COBOL 2014+ tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_lower_float_short_data_item() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let float_item = hir
            .data_items
            .iter()
            .find(|d| d.name.as_str() == "WS-FLOAT");
        assert!(float_item.is_some(), "Expected WS-FLOAT data item");
        assert_eq!(float_item.unwrap().data_type, HirType::FloatShort);
    }

    #[test]
    fn test_lower_float_long_data_item() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-L.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-L USAGE FLOAT-LONG.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let float_item = hir
            .data_items
            .iter()
            .find(|d| d.name.as_str() == "WS-FLOAT-L");
        assert!(float_item.is_some(), "Expected WS-FLOAT-L data item");
        assert_eq!(float_item.unwrap().data_type, HirType::FloatLong);
    }

    #[test]
    fn test_lower_float_extended_data_item() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-E USAGE FLOAT-EXTENDED.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let float_item = hir
            .data_items
            .iter()
            .find(|d| d.name.as_str() == "WS-FLOAT-E");
        assert!(float_item.is_some(), "Expected WS-FLOAT-E data item");
        assert_eq!(float_item.unwrap().data_type, HirType::FloatExtended);
    }

    #[test]
    fn test_lower_comp1_as_float_short() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-COMP1.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-COMP1 USAGE COMP-1.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let item = hir
            .data_items
            .iter()
            .find(|d| d.name.as_str() == "WS-COMP1");
        assert!(item.is_some(), "Expected WS-COMP1 data item");
        assert_eq!(item.unwrap().data_type, HirType::FloatShort);
    }

    #[test]
    fn test_lower_comp2_as_float_long() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-COMP2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-COMP2 USAGE COMP-2.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let item = hir
            .data_items
            .iter()
            .find(|d| d.name.as_str() == "WS-COMP2");
        assert!(item.is_some(), "Expected WS-COMP2 data item");
        assert_eq!(item.unwrap().data_type, HirType::FloatLong);
    }

    #[test]
    fn test_hir_program_has_typedefs_and_interfaces() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-2014.
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        // Typedefs and interfaces should be empty for a normal program
        assert!(hir.typedefs.is_empty());
        assert!(hir.interfaces.is_empty());
    }
}
