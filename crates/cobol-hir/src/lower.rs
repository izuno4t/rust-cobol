// COBOL HIR - AST to HIR lowering
//
// Converts a parsed COBOL AST into the simplified HIR form:
// - Extracts data items from DATA DIVISION
// - Flattens PROCEDURE DIVISION into a list of HIR statements
// - Desugars EVALUATE into nested IF

use std::collections::HashMap;

use cobol_ast::{
    data_div::ValueClause,
    expr::{ArithOp, ClassType, CompareOp, Condition, FigurativeConstant, SignType, UnaryArithOp},
    proc_div::{Paragraph, ProcedureDivision, UseStatement},
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
    HirBeforeAfter, HirBinOp, HirCallParam, HirClassType, HirCompareOp, HirCondition, HirDataItem,
    HirDeclarative, HirExpr, HirFileInfo, HirInspectKind, HirInspectReplacing, HirInspectTallying,
    HirLiteral, HirMoveTarget, HirOpenEntry, HirOpenMode, HirParagraph, HirParamMode,
    HirPerformKind, HirProgram, HirReplacingKind, HirSortKey, HirSortOrder, HirStartRelation,
    HirStatement, HirStringSource, HirTallyingKind, HirType, HirUnaryOp, HirUnstringDelimiter,
};

/// A single or range value for an 88-level condition.
#[derive(Debug, Clone)]
enum ConditionValue {
    Single(HirLiteral),
    Range { from: HirLiteral, to: HirLiteral },
}

/// Information about an 88-level condition name: the parent variable name
/// and the literal values that make the condition true.
#[derive(Debug, Clone)]
struct ConditionNameInfo {
    parent_name: SmolStr,
    values: Vec<ConditionValue>,
}

/// Lowers a COBOL AST program into the HIR.
pub fn lower_to_hir(program: &CobolProgram) -> HirProgram {
    let name = program.identification.program_id.clone();

    // Collect 88-level condition name mappings before lowering data items.
    let condition_names = program
        .data
        .as_ref()
        .map(collect_condition_names)
        .unwrap_or_default();

    let data_items = program
        .data
        .as_ref()
        .map(lower_data_division)
        .unwrap_or_default();

    let (body, paragraphs) = program
        .procedure
        .as_ref()
        .map(|proc| lower_procedure_division(proc, &condition_names))
        .unwrap_or_default();

    // Extract FILE STATUS variable mappings from ENVIRONMENT DIVISION.
    let file_status_vars = extract_file_status_vars(program);

    // Lower DECLARATIVES sections (USE AFTER EXCEPTION handlers).
    let declaratives = lower_declaratives(program, &condition_names);

    HirProgram {
        name,
        data_items,
        paragraphs,
        body,
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        file_status_vars,
        declaratives,
        span: program.span,
    }
}

/// Collect 88-level condition name information from the DATA DIVISION.
/// Maps each 88-level name to its parent variable name and the values
/// that make the condition true.
fn collect_condition_names(data: &DataDivision) -> HashMap<SmolStr, ConditionNameInfo> {
    let mut map = HashMap::new();
    for fd in &data.file_section {
        for item in &fd.items {
            collect_condition_names_from_item(item, &mut map);
        }
    }
    for item in &data.working_storage {
        collect_condition_names_from_item(item, &mut map);
    }
    for item in &data.local_storage {
        collect_condition_names_from_item(item, &mut map);
    }
    for item in &data.linkage {
        collect_condition_names_from_item(item, &mut map);
    }
    map
}

fn collect_condition_names_from_item(
    item: &DataItem,
    map: &mut HashMap<SmolStr, ConditionNameInfo>,
) {
    // Check children for 88-level items that belong to this parent
    if let Some(parent_name) = &item.name {
        for child in &item.children {
            if child.level == 88 {
                if let Some(cond_name) = &child.name {
                    let mut values = Vec::new();
                    for cv in &child.condition_values {
                        for val_item in &cv.values {
                            match val_item {
                                cobol_ast::data_div::ConditionValueItem::Single(lit) => {
                                    values.push(ConditionValue::Single(lower_literal(lit)));
                                }
                                cobol_ast::data_div::ConditionValueItem::Range { from, to } => {
                                    values.push(ConditionValue::Range {
                                        from: lower_literal(from),
                                        to: lower_literal(to),
                                    });
                                }
                            }
                        }
                    }
                    map.insert(
                        cond_name.clone(),
                        ConditionNameInfo {
                            parent_name: parent_name.clone(),
                            values,
                        },
                    );
                }
            }
        }
    }

    // Recurse into non-88 children
    for child in &item.children {
        if child.level != 88 {
            collect_condition_names_from_item(child, map);
        }
    }
}

// ---------------------------------------------------------------------------
// Data Division lowering
// ---------------------------------------------------------------------------

fn lower_data_division(data: &DataDivision) -> Vec<HirDataItem> {
    let mut items = Vec::new();
    for fd in &data.file_section {
        for item in &fd.items {
            lower_data_item(item, &mut items);
        }
    }
    for item in &data.working_storage {
        lower_data_item(item, &mut items);
    }
    for item in &data.local_storage {
        lower_data_item(item, &mut items);
    }
    for item in &data.linkage {
        lower_data_item(item, &mut items);
    }
    // TODO: Add screen, communication, and report section processing
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
        let occurs = item.occurs.as_ref().map(|o| o.max);

        out.push(HirDataItem {
            name: name.clone(),
            data_type,
            initial_value,
            occurs,
            redefines: item.redefines.clone(),
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
                let occurs = child.occurs.as_ref().map(|o| o.max);
                members.push(HirDataItem {
                    name: name.clone(),
                    data_type,
                    initial_value,
                    occurs,
                    redefines: child.redefines.clone(),
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
        Literal::FigurativeConstant(FigurativeConstant::HighValue) => HirLiteral::HighValue,
        Literal::FigurativeConstant(FigurativeConstant::LowValue) => HirLiteral::LowValue,
        Literal::FigurativeConstant(FigurativeConstant::Quote) => HirLiteral::Quote,
        Literal::FigurativeConstant(FigurativeConstant::Null) => HirLiteral::Null,
        Literal::FigurativeConstant(FigurativeConstant::All) => HirLiteral::Zero, // ALL requires context
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
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> (Vec<HirStatement>, Vec<HirParagraph>) {
    let mut body = Vec::new();
    let mut paragraphs = Vec::new();

    // Lower top-level paragraphs
    for para in &proc.paragraphs {
        let stmts = lower_paragraph(para, condition_names);
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
            let stmts = lower_paragraph(para, condition_names);
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

fn lower_paragraph(
    para: &Paragraph,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> Vec<HirStatement> {
    let mut stmts = Vec::new();
    for sentence in &para.sentences {
        for stmt in &sentence.statements {
            if let Some(hir_stmt) = lower_statement(stmt, condition_names) {
                stmts.push(hir_stmt);
            }
        }
    }
    stmts
}

/// Lower a list of AST statements into a list of HIR statements,
/// filtering out any that cannot be lowered.
fn lower_statements(
    stmts: &[Statement],
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> Vec<HirStatement> {
    stmts
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect()
}

fn lower_statement(
    stmt: &Statement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> Option<HirStatement> {
    match stmt {
        Statement::Display(display) => Some(lower_display(display)),
        Statement::Accept(accept) => Some(lower_accept(accept)),
        Statement::Move(mv) => Some(lower_move(mv)),
        Statement::Compute(compute) => lower_compute(compute, condition_names),
        Statement::Add(add) => Some(lower_add(add, condition_names)),
        Statement::Subtract(sub) => Some(lower_subtract(sub, condition_names)),
        Statement::Multiply(mul) => Some(lower_multiply(mul, condition_names)),
        Statement::Divide(div) => Some(lower_divide(div, condition_names)),
        Statement::If(if_stmt) => Some(lower_if(if_stmt, condition_names)),
        Statement::Evaluate(eval) => Some(lower_evaluate(eval, condition_names)),
        Statement::Perform(perform) => Some(lower_perform(perform, condition_names)),
        Statement::Call(call) => Some(lower_call(call, condition_names)),
        Statement::GoTo(goto) => Some(lower_goto(goto)),
        Statement::Open(open) => Some(lower_open(open)),
        Statement::Close(close) => Some(lower_close(close)),
        Statement::Read(read) => Some(lower_read(read, condition_names)),
        Statement::Write(write) => Some(lower_write(write, condition_names)),
        Statement::Rewrite(rewrite) => Some(lower_rewrite(rewrite)),
        Statement::Delete(delete) => Some(lower_delete(delete)),
        Statement::Initialize(init) => Some(lower_initialize(init)),
        Statement::Set(set) => Some(lower_set(set)),
        Statement::String(string_stmt) => Some(lower_string_stmt(string_stmt, condition_names)),
        Statement::Unstring(unstring_stmt) => {
            Some(lower_unstring_stmt(unstring_stmt, condition_names))
        }
        Statement::Sort(sort) => Some(lower_sort(sort)),
        Statement::Inspect(inspect) => Some(lower_inspect(inspect)),
        // --- File I/O: additional statements ---
        Statement::Start(start) => Some(lower_start(start, condition_names)),
        Statement::Return(ret) => Some(lower_return(ret, condition_names)),
        // --- Sort/Merge: additional statements ---
        Statement::Merge(merge) => Some(lower_merge(merge)),
        Statement::Release(release) => Some(lower_release(release)),
        // --- Miscellaneous ---
        Statement::Cancel(cancel) => Some(lower_cancel(cancel)),
        Statement::StopRun => Some(HirStatement::StopRun {
            span: Span::dummy(),
        }),
        Statement::Goback => Some(HirStatement::Goback {
            span: Span::dummy(),
        }),
        Statement::Continue => Some(HirStatement::Continue {
            span: Span::dummy(),
        }),
        Statement::ExitProgram => Some(HirStatement::ExitProgram {
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
    if mv.corresponding {
        // MOVE CORRESPONDING: source and target are group names.
        let from_name = match &mv.from {
            Expr::Identifier(qname) => qname.name.clone(),
            _ => SmolStr::from("FILLER"),
        };
        let to_name = mv
            .to
            .first()
            .map(|e| match e {
                Expr::Identifier(qname) => qname.name.clone(),
                _ => SmolStr::from("FILLER"),
            })
            .unwrap_or_else(|| SmolStr::from("FILLER"));
        return HirStatement::MoveCorresponding {
            from: from_name,
            to: to_name,
            span: mv.span,
        };
    }
    let from = lower_expr(&mv.from);
    let to = mv.to.iter().map(lower_move_target).collect();
    HirStatement::Move {
        from,
        to,
        span: mv.span,
    }
}

fn lower_move_target(expr: &Expr) -> HirMoveTarget {
    match expr {
        Expr::Identifier(qname) => {
            if qname.subscripts.is_empty() {
                HirMoveTarget::Variable(qname.name.clone())
            } else {
                HirMoveTarget::Subscript {
                    variable: qname.name.clone(),
                    subscripts: qname.subscripts.iter().map(lower_expr).collect(),
                }
            }
        }
        Expr::ReferenceModification {
            variable,
            start,
            length,
            ..
        } => HirMoveTarget::ReferenceModification {
            variable: variable.name.clone(),
            start: lower_expr(start),
            length: length.as_ref().map(|l| lower_expr(l)),
        },
        _ => {
            // Fallback: should not happen for well-formed MOVE targets
            HirMoveTarget::Variable(SmolStr::from("FILLER"))
        }
    }
}

fn lower_compute(
    compute: &ComputeStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> Option<HirStatement> {
    if compute.targets.is_empty() {
        return None;
    }
    let expr = lower_expr(&compute.expr);
    let targets = compute
        .targets
        .iter()
        .map(|t| t.target.name.clone())
        .collect();
    let on_size_error = lower_statements(&compute.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&compute.not_on_size_error, condition_names);
    Some(HirStatement::Compute {
        targets,
        expr,
        on_size_error,
        not_on_size_error,
        span: compute.span,
    })
}

fn lower_add(
    add: &AddStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    if add.corresponding {
        // ADD CORRESPONDING: source is first operand (group), target is first TO (group).
        let from_name = match &add.operands[0] {
            Expr::Identifier(qname) => qname.name.clone(),
            _ => SmolStr::from("FILLER"),
        };
        let to_name = add
            .to
            .first()
            .map(|t| t.target.name.clone())
            .unwrap_or_else(|| SmolStr::from("FILLER"));
        let on_size_error = lower_statements(&add.on_size_error, condition_names);
        let not_on_size_error = lower_statements(&add.not_on_size_error, condition_names);
        return HirStatement::AddCorresponding {
            from: from_name,
            to: to_name,
            on_size_error,
            not_on_size_error,
            span: add.span,
        };
    }
    let operands = add.operands.iter().map(lower_expr).collect();
    let to = add.to.iter().map(|t| t.target.name.clone()).collect();
    let on_size_error = lower_statements(&add.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&add.not_on_size_error, condition_names);
    HirStatement::Add {
        operands,
        to,
        on_size_error,
        not_on_size_error,
        span: add.span,
    }
}

fn lower_subtract(
    sub: &SubtractStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    if sub.corresponding {
        // SUBTRACT CORRESPONDING: source is first operand (group), target is first FROM (group).
        let from_name = match &sub.operands[0] {
            Expr::Identifier(qname) => qname.name.clone(),
            _ => SmolStr::from("FILLER"),
        };
        let to_name = sub
            .from
            .first()
            .map(|t| t.target.name.clone())
            .unwrap_or_else(|| SmolStr::from("FILLER"));
        let on_size_error = lower_statements(&sub.on_size_error, condition_names);
        let not_on_size_error = lower_statements(&sub.not_on_size_error, condition_names);
        return HirStatement::SubtractCorresponding {
            from: from_name,
            to: to_name,
            on_size_error,
            not_on_size_error,
            span: sub.span,
        };
    }
    let operands = sub.operands.iter().map(lower_expr).collect();
    let from = sub.from.iter().map(|t| t.target.name.clone()).collect();
    let on_size_error = lower_statements(&sub.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&sub.not_on_size_error, condition_names);
    HirStatement::Subtract {
        operands,
        from,
        on_size_error,
        not_on_size_error,
        span: sub.span,
    }
}

fn lower_if(
    if_stmt: &IfStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let condition = lower_condition(&if_stmt.condition, condition_names);
    let then_body: Vec<_> = if_stmt
        .then_body
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    let else_body: Vec<_> = if_stmt
        .else_body
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    HirStatement::If {
        condition,
        then_body,
        else_body,
        span: if_stmt.span,
    }
}

/// Desugar EVALUATE into nested IF statements.
fn lower_evaluate(
    eval: &EvaluateStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    // Build nested IF chain from the WHEN clauses
    let mut else_body: Vec<HirStatement> = eval
        .when_other
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();

    // Process WHEN clauses in reverse to build the else chain
    for when in eval.when_clauses.iter().rev() {
        let then_body: Vec<HirStatement> = when
            .body
            .iter()
            .filter_map(|s| lower_statement(s, condition_names))
            .collect();

        // Build condition from the WHEN objects and subjects
        let condition = build_evaluate_condition(&eval.subjects, &when.objects, condition_names);

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
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
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
                    conditions.push(lower_condition(c, condition_names));
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
                        condition_names,
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

fn lower_perform(
    perform: &PerformStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let kind = match &perform.kind {
        PerformKind::Simple { body } => {
            let hir_body: Vec<_> = body
                .iter()
                .filter_map(|s| lower_statement(s, condition_names))
                .collect();
            HirPerformKind::Inline { body: hir_body }
        }
        PerformKind::ProcedureName { procedure, through } => HirPerformKind::ProcedureName {
            name: procedure.clone(),
            through: through.clone(),
        },
        PerformKind::Times { times, body } => {
            let count = lower_expr(times);
            let hir_body: Vec<_> = body
                .iter()
                .filter_map(|s| lower_statement(s, condition_names))
                .collect();
            HirPerformKind::Times {
                count,
                body: hir_body,
            }
        }
        PerformKind::Until {
            condition, body, ..
        } => {
            let hir_cond = lower_condition(condition, condition_names);
            let hir_body: Vec<_> = body
                .iter()
                .filter_map(|s| lower_statement(s, condition_names))
                .collect();
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
                let until = lower_condition(&clause.until, condition_names);
                let hir_body: Vec<_> = body
                    .iter()
                    .filter_map(|s| lower_statement(s, condition_names))
                    .collect();
                HirPerformKind::Varying {
                    var,
                    from,
                    by,
                    until,
                    body: hir_body,
                }
            } else {
                let hir_body: Vec<_> = body
                    .iter()
                    .filter_map(|s| lower_statement(s, condition_names))
                    .collect();
                HirPerformKind::Inline { body: hir_body }
            }
        }
    };
    HirStatement::Perform {
        kind,
        span: perform.span,
    }
}

fn lower_call(
    call: &CallStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    use cobol_ast::proc_div::ParamMode;
    let program = lower_expr(&call.program);
    let params = call
        .using
        .iter()
        .map(|p| {
            let mode = match p.mode {
                ParamMode::ByReference => HirParamMode::ByReference,
                ParamMode::ByContent => HirParamMode::ByContent,
                ParamMode::ByValue => HirParamMode::ByValue,
            };
            HirCallParam {
                expr: lower_expr(&p.value),
                mode,
            }
        })
        .collect();
    let on_exception = lower_statements(&call.on_exception, condition_names);
    let not_on_exception = lower_statements(&call.not_on_exception, condition_names);
    HirStatement::Call {
        program,
        params,
        on_exception,
        not_on_exception,
        span: call.span,
    }
}

fn lower_multiply(
    mul: &MultiplyStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let operand = lower_expr(&mul.operand);
    let by = mul.by.iter().map(|t| t.target.name.clone()).collect();
    let on_size_error = lower_statements(&mul.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&mul.not_on_size_error, condition_names);
    HirStatement::Multiply {
        operand,
        by,
        on_size_error,
        not_on_size_error,
        span: mul.span,
    }
}

fn lower_divide(
    div: &DivideStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let operand = lower_expr(&div.operand);
    let into = div.into.iter().map(|t| t.target.name.clone()).collect();
    let remainder = div.remainder.as_ref().map(|r| r.name.clone());
    let on_size_error = lower_statements(&div.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&div.not_on_size_error, condition_names);
    HirStatement::Divide {
        operand,
        into,
        remainder,
        on_size_error,
        not_on_size_error,
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

fn lower_read(
    read: &cobol_ast::statement::ReadStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let into = read.into.as_ref().map(|q| q.name.clone());
    let at_end: Vec<_> = read
        .at_end
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    let not_at_end: Vec<_> = read
        .not_at_end
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    HirStatement::Read {
        file_name: read.file_name.clone(),
        into,
        at_end,
        not_at_end,
        span: read.span,
    }
}

fn lower_write(
    write: &cobol_ast::statement::WriteStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let from = write.from.as_ref().map(lower_expr);
    let invalid_key = lower_statements(&write.invalid_key, condition_names);
    HirStatement::Write {
        record_name: write.record_name.name.clone(),
        from,
        invalid_key,
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
                    on_size_error: Vec::new(),
                    not_on_size_error: Vec::new(),
                    span: set.span,
                },
                cobol_ast::statement::SetDirection::Down => HirStatement::Subtract {
                    operands: vec![hir_value],
                    from: target_names,
                    on_size_error: Vec::new(),
                    not_on_size_error: Vec::new(),
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

fn lower_string_stmt(
    string_stmt: &cobol_ast::statement::StringStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    use cobol_ast::statement::StringDelimiter;
    let sources = string_stmt
        .sources
        .iter()
        .flat_map(|s| {
            let delimiter = match &s.delimited_by {
                StringDelimiter::Size => None,
                StringDelimiter::Value(expr) => Some(lower_expr(expr)),
            };
            s.items.iter().map(move |item| HirStringSource {
                value: lower_expr(item),
                delimiter: delimiter.clone(),
            })
        })
        .collect();
    let on_overflow = lower_statements(&string_stmt.on_overflow, condition_names);
    HirStatement::StringStmt {
        into: string_stmt.into.name.clone(),
        sources,
        on_overflow,
        span: string_stmt.span,
    }
}

fn lower_unstring_stmt(
    unstring_stmt: &cobol_ast::statement::UnstringStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let delimiters = unstring_stmt
        .delimiters
        .iter()
        .map(|d| HirUnstringDelimiter {
            all: d.all,
            value: lower_expr(&d.value),
        })
        .collect();
    let into = unstring_stmt
        .into
        .iter()
        .map(|t| t.target.name.clone())
        .collect();
    let on_overflow = lower_statements(&unstring_stmt.on_overflow, condition_names);
    HirStatement::UnstringStmt {
        source: unstring_stmt.source.name.clone(),
        delimiters,
        into,
        on_overflow,
        span: unstring_stmt.span,
    }
}

fn lower_sort(sort: &cobol_ast::statement::SortStatement) -> HirStatement {
    let keys = sort
        .keys
        .iter()
        .map(|k| {
            let order = match k.order {
                cobol_ast::statement::SortOrder::Ascending => HirSortOrder::Ascending,
                cobol_ast::statement::SortOrder::Descending => HirSortOrder::Descending,
            };
            let fields = k.fields.iter().map(|f| f.name.clone()).collect();
            HirSortKey { order, fields }
        })
        .collect();
    let using = match &sort.input {
        cobol_ast::statement::SortInput::Using(files) => files.clone(),
        cobol_ast::statement::SortInput::InputProcedure { .. } => Vec::new(),
    };
    let giving = match &sort.output {
        cobol_ast::statement::SortOutput::Giving(files) => files.clone(),
        cobol_ast::statement::SortOutput::OutputProcedure { .. } => Vec::new(),
    };
    HirStatement::Sort {
        file_name: sort.file_name.clone(),
        keys,
        using,
        giving,
        span: sort.span,
    }
}

fn lower_inspect(inspect: &cobol_ast::statement::InspectStatement) -> HirStatement {
    let kind = match &inspect.kind {
        cobol_ast::statement::InspectKind::Tallying { tallying } => HirInspectKind::Tallying {
            tallying: tallying.iter().map(lower_inspect_tallying).collect(),
        },
        cobol_ast::statement::InspectKind::Replacing { replacing } => HirInspectKind::Replacing {
            replacing: replacing.iter().map(lower_inspect_replacing).collect(),
        },
        cobol_ast::statement::InspectKind::TallyingReplacing {
            tallying,
            replacing,
        } => HirInspectKind::TallyingReplacing {
            tallying: tallying.iter().map(lower_inspect_tallying).collect(),
            replacing: replacing.iter().map(lower_inspect_replacing).collect(),
        },
        cobol_ast::statement::InspectKind::Converting { from, to, .. } => {
            HirInspectKind::Converting {
                from: lower_expr(from),
                to: lower_expr(to),
            }
        }
    };
    HirStatement::Inspect {
        target: inspect.target.name.clone(),
        kind,
        span: inspect.span,
    }
}

fn lower_inspect_tallying(t: &cobol_ast::statement::InspectTallying) -> HirInspectTallying {
    let kind = match &t.kind {
        cobol_ast::statement::TallyingKind::Characters => HirTallyingKind::Characters,
        cobol_ast::statement::TallyingKind::All(e) => HirTallyingKind::All(lower_expr(e)),
        cobol_ast::statement::TallyingKind::Leading(e) => HirTallyingKind::Leading(lower_expr(e)),
    };
    HirInspectTallying {
        counter: t.counter.name.clone(),
        kind,
        before_after: t.before_after.iter().map(lower_before_after).collect(),
    }
}

fn lower_inspect_replacing(r: &cobol_ast::statement::InspectReplacing) -> HirInspectReplacing {
    let kind = match &r.kind {
        cobol_ast::statement::ReplacingKind::Characters(e) => {
            HirReplacingKind::Characters(lower_expr(e))
        }
        cobol_ast::statement::ReplacingKind::All { from, to } => HirReplacingKind::All {
            from: lower_expr(from),
            to: lower_expr(to),
        },
        cobol_ast::statement::ReplacingKind::Leading { from, to } => HirReplacingKind::Leading {
            from: lower_expr(from),
            to: lower_expr(to),
        },
        cobol_ast::statement::ReplacingKind::First { from, to } => HirReplacingKind::First {
            from: lower_expr(from),
            to: lower_expr(to),
        },
    };
    HirInspectReplacing {
        kind,
        before_after: r.before_after.iter().map(lower_before_after).collect(),
    }
}

fn lower_before_after(ba: &cobol_ast::statement::BeforeAfter) -> HirBeforeAfter {
    HirBeforeAfter {
        is_before: ba.kind == cobol_ast::statement::BeforeAfterKind::Before,
        value: lower_expr(&ba.value),
    }
}

// ---------------------------------------------------------------------------
// File I/O: additional statement lowering
// ---------------------------------------------------------------------------

fn lower_start(
    start: &cobol_ast::statement::StartStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let key = start.key_condition.as_ref().map(|kc| kc.key.name.clone());
    let op = start
        .key_condition
        .as_ref()
        .map(|kc| match kc.op {
            cobol_ast::statement::StartRelation::Equal => HirStartRelation::Equal,
            cobol_ast::statement::StartRelation::GreaterThan => HirStartRelation::GreaterThan,
            cobol_ast::statement::StartRelation::GreaterEqual => HirStartRelation::GreaterEqual,
            cobol_ast::statement::StartRelation::NotLessThan => HirStartRelation::NotLessThan,
        })
        .unwrap_or(HirStartRelation::Equal);
    let invalid_key = lower_statements(&start.invalid_key, condition_names);
    let not_invalid_key = lower_statements(&start.not_invalid_key, condition_names);
    HirStatement::Start {
        file_name: start.file_name.clone(),
        key,
        op,
        invalid_key,
        not_invalid_key,
        span: start.span,
    }
}

fn lower_return(
    ret: &cobol_ast::statement::ReturnStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let into = ret.into.as_ref().map(|q| q.name.clone());
    let at_end: Vec<_> = ret
        .at_end
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    let not_at_end: Vec<_> = ret
        .not_at_end
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();
    HirStatement::Return {
        file_name: ret.file_name.clone(),
        into,
        at_end,
        not_at_end,
        span: ret.span,
    }
}

fn lower_cancel(cancel: &cobol_ast::statement::CancelStatement) -> HirStatement {
    let programs = cancel.programs.iter().map(lower_expr).collect();
    HirStatement::Cancel {
        programs,
        span: cancel.span,
    }
}

fn lower_merge(merge: &cobol_ast::statement::MergeStatement) -> HirStatement {
    let keys = merge
        .keys
        .iter()
        .map(|k| {
            let order = match k.order {
                cobol_ast::statement::SortOrder::Ascending => HirSortOrder::Ascending,
                cobol_ast::statement::SortOrder::Descending => HirSortOrder::Descending,
            };
            let fields = k.fields.iter().map(|f| f.name.clone()).collect();
            HirSortKey { order, fields }
        })
        .collect();
    let using = merge.using.clone();
    let giving = match &merge.output {
        cobol_ast::statement::SortOutput::Giving(files) => files.clone(),
        cobol_ast::statement::SortOutput::OutputProcedure { .. } => Vec::new(),
    };
    HirStatement::Merge {
        file_name: merge.file_name.clone(),
        keys,
        using,
        giving,
        span: merge.span,
    }
}

fn lower_release(release: &cobol_ast::statement::ReleaseStatement) -> HirStatement {
    let from = release.from.as_ref().map(lower_expr);
    HirStatement::Release {
        record_name: release.record_name.name.clone(),
        from,
        span: release.span,
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
        Expr::Identifier(qname) => {
            if qname.subscripts.is_empty() {
                HirExpr::Variable(qname.name.clone())
            } else {
                HirExpr::Subscript {
                    variable: qname.name.clone(),
                    subscripts: qname.subscripts.iter().map(lower_expr).collect(),
                }
            }
        }
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
        Expr::FunctionCall { name, args, .. } => {
            // TODO: Implement proper function call support with type resolution.
            // For now, emit as a function call in HIR preserving name and args.
            HirExpr::FunctionCall {
                name: name.clone(),
                args: args.iter().map(lower_expr).collect(),
            }
        }
        Expr::ReferenceModification {
            variable,
            start,
            length,
            ..
        } => HirExpr::ReferenceModification {
            variable: variable.name.clone(),
            start: Box::new(lower_expr(start)),
            length: length.as_ref().map(|l| Box::new(lower_expr(l))),
        },
    }
}

fn lower_condition(
    cond: &Condition,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirCondition {
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
            Box::new(lower_condition(a, condition_names)),
            Box::new(lower_condition(b, condition_names)),
        ),
        Condition::Or(a, b) => HirCondition::Or(
            Box::new(lower_condition(a, condition_names)),
            Box::new(lower_condition(b, condition_names)),
        ),
        Condition::Not(inner) => {
            HirCondition::Not(Box::new(lower_condition(inner, condition_names)))
        }
        Condition::Paren(inner) => lower_condition(inner, condition_names),
        Condition::SignCondition {
            operand, sign, not, ..
        } => {
            let hir_op = match (sign, *not) {
                (SignType::Positive, false) => HirCompareOp::Gt,
                (SignType::Positive, true) => HirCompareOp::Le,
                (SignType::Negative, false) => HirCompareOp::Lt,
                (SignType::Negative, true) => HirCompareOp::Ge,
                (SignType::Zero, false) => HirCompareOp::Eq,
                (SignType::Zero, true) => HirCompareOp::Ne,
            };
            HirCondition::Compare {
                left: lower_expr(operand),
                op: hir_op,
                right: HirExpr::Literal(HirLiteral::Integer(0)),
            }
        }
        Condition::ClassCondition {
            operand,
            class,
            not,
            ..
        } => {
            let hir_class = match class {
                ClassType::Numeric => HirClassType::Numeric,
                ClassType::Alphabetic => HirClassType::Alphabetic,
                ClassType::AlphabeticLower => HirClassType::AlphabeticLower,
                ClassType::AlphabeticUpper => HirClassType::AlphabeticUpper,
                ClassType::National => HirClassType::Alphabetic, // fallback for NATIONAL
            };
            let cond = HirCondition::ClassCondition {
                operand: lower_expr(operand),
                class: hir_class,
            };
            if *not {
                HirCondition::Not(Box::new(cond))
            } else {
                cond
            }
        }
        Condition::ConditionName(qname) => {
            // 88-level condition name: look up the parent variable and values
            if let Some(info) = condition_names.get(&qname.name) {
                let conditions: Vec<HirCondition> = info
                    .values
                    .iter()
                    .map(|v| match v {
                        ConditionValue::Single(lit) => HirCondition::Compare {
                            left: HirExpr::Variable(info.parent_name.clone()),
                            op: HirCompareOp::Eq,
                            right: HirExpr::Literal(lit.clone()),
                        },
                        ConditionValue::Range { from, to } => HirCondition::And(
                            Box::new(HirCondition::Compare {
                                left: HirExpr::Variable(info.parent_name.clone()),
                                op: HirCompareOp::Ge,
                                right: HirExpr::Literal(from.clone()),
                            }),
                            Box::new(HirCondition::Compare {
                                left: HirExpr::Variable(info.parent_name.clone()),
                                op: HirCompareOp::Le,
                                right: HirExpr::Literal(to.clone()),
                            }),
                        ),
                    })
                    .collect();
                if conditions.is_empty() {
                    // No values found, fall back to variable == 1
                    HirCondition::Compare {
                        left: HirExpr::Variable(qname.name.clone()),
                        op: HirCompareOp::Eq,
                        right: HirExpr::Literal(HirLiteral::Integer(1)),
                    }
                } else {
                    conditions
                        .into_iter()
                        .reduce(|acc, c| HirCondition::Or(Box::new(acc), Box::new(c)))
                        .unwrap()
                }
            } else {
                // Not a known 88-level, fall back to variable == 1
                HirCondition::Compare {
                    left: HirExpr::Variable(qname.name.clone()),
                    op: HirCompareOp::Eq,
                    right: HirExpr::Literal(HirLiteral::Integer(1)),
                }
            }
        }
    }
}

/// Extract FILE STATUS variable mappings from the ENVIRONMENT DIVISION's
/// FILE-CONTROL paragraph.  Each `FileControlEntry` that has a `file_status`
/// clause contributes a mapping of file_name → status variable name.
fn extract_file_status_vars(program: &CobolProgram) -> Vec<HirFileInfo> {
    let Some(env) = &program.environment else {
        return Vec::new();
    };
    let Some(io) = &env.input_output else {
        return Vec::new();
    };
    io.file_controls
        .iter()
        .filter_map(|fc| {
            fc.file_status.as_ref().map(|qname| HirFileInfo {
                file_name: fc.file_name.clone(),
                status_var: qname.name.clone(),
            })
        })
        .collect()
}

/// Lower DECLARATIVES sections from the PROCEDURE DIVISION.
/// Only USE AFTER EXCEPTION sections are lowered; other USE types are ignored.
fn lower_declaratives(
    program: &CobolProgram,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> Vec<HirDeclarative> {
    let Some(proc) = &program.procedure else {
        return Vec::new();
    };
    proc.declaratives
        .iter()
        .filter_map(|decl| {
            if let UseStatement::AfterException { file_names } = &decl.use_statement {
                let body: Vec<HirStatement> = decl
                    .paragraphs
                    .iter()
                    .flat_map(|para| lower_paragraph(para, condition_names))
                    .collect();
                Some(HirDeclarative {
                    name: decl.name.clone(),
                    file_names: file_names.clone(),
                    body,
                })
            } else {
                None
            }
        })
        .collect()
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

    #[test]
    fn test_lower_reference_modification_in_display() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(20).
PROCEDURE DIVISION.
    DISPLAY WS-NAME(1:5).
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(!hir.body.is_empty());
        match &hir.body[0] {
            crate::hir::HirStatement::Display { operands, .. } => {
                assert_eq!(operands.len(), 1);
                match &operands[0] {
                    crate::hir::HirExpr::ReferenceModification {
                        variable,
                        start,
                        length,
                    } => {
                        assert_eq!(variable.as_str(), "WS-NAME");
                        assert!(matches!(
                            **start,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(1))
                        ));
                        assert!(length.is_some());
                        let len = length.as_ref().unwrap();
                        assert!(matches!(
                            **len,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(5))
                        ));
                    }
                    other => panic!("expected ReferenceModification, got {:?}", other),
                }
            }
            other => panic!("expected Display, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_reference_modification_in_move_target() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-MOVE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(20).
PROCEDURE DIVISION.
    MOVE \"ABC\" TO WS-NAME(3:3).
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(!hir.body.is_empty());
        match &hir.body[0] {
            crate::hir::HirStatement::Move { to, .. } => {
                assert_eq!(to.len(), 1);
                match &to[0] {
                    crate::hir::HirMoveTarget::ReferenceModification {
                        variable,
                        start,
                        length,
                    } => {
                        assert_eq!(variable.as_str(), "WS-NAME");
                        assert!(matches!(
                            start,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(3))
                        ));
                        assert!(length.is_some());
                        let len = length.as_ref().unwrap();
                        assert!(matches!(
                            len,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(3))
                        ));
                    }
                    other => panic!("expected ReferenceModification target, got {:?}", other),
                }
            }
            other => panic!("expected Move, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_reference_modification_start_only() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-START.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(20).
PROCEDURE DIVISION.
    DISPLAY WS-NAME(5:).
    STOP RUN.
";
        let hir = parse_and_lower(src);
        match &hir.body[0] {
            crate::hir::HirStatement::Display { operands, .. } => match &operands[0] {
                crate::hir::HirExpr::ReferenceModification {
                    variable,
                    start,
                    length,
                } => {
                    assert_eq!(variable.as_str(), "WS-NAME");
                    assert!(matches!(
                        **start,
                        crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(5))
                    ));
                    assert!(
                        length.is_none(),
                        "length should be None for start-only ref mod"
                    );
                }
                other => panic!("expected ReferenceModification, got {:?}", other),
            },
            other => panic!("expected Display, got {:?}", other),
        }
    }
}
