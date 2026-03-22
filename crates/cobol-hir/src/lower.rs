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
    HirAcceptSource, HirBeforeAfter, HirBinOp, HirCallParam, HirClassType, HirCompareOp,
    HirCondition, HirDataItem, HirDeclarative, HirExpr, HirFileInfo, HirInspectKind,
    HirInspectReplacing, HirInspectTallying, HirLiteral, HirMoveTarget, HirOpenEntry, HirOpenMode,
    HirParagraph, HirParam, HirParamMode, HirPerformKind, HirProgram, HirReplacingKind,
    HirScreenInfo, HirSearchWhen, HirSortKey, HirSortOrder, HirStartRelation, HirStatement,
    HirStringSource, HirTallyingKind, HirType, HirUnaryOp, HirUnstringDelimiter,
    HirVaryingAfter,
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

    let mut data_items = program
        .data
        .as_ref()
        .map(lower_data_division)
        .unwrap_or_default();

    // When any FD has a LINAGE clause, inject the implicit LINAGE-COUNTER
    // special register as a top-level numeric data item so codegen declares it.
    if let Some(data) = &program.data {
        let has_linage = data.file_section.iter().any(|fd| fd.linage.is_some());
        if has_linage {
            data_items.push(HirDataItem {
                name: SmolStr::new("LINAGE-COUNTER"),
                data_type: HirType::Numeric {
                    size: 6,
                    decimal_places: 0,
                    is_signed: false,
                },
                initial_value: None,
                redefines: None,
                renames: None,
                occurs: None,
                indexed_by: Vec::new(),
                screen_info: None,
                span: program.span,
            });
        }
    }

    // Inject SPECIAL-NAMES switch condition names as boolean data items
    // so codegen declares them as C variables.
    if let Some(ref env) = program.environment {
        if let Some(ref config) = env.configuration {
            for entry in &config.special_names {
                for cond_name in entry.on_condition.iter().chain(entry.off_condition.iter()) {
                    data_items.push(HirDataItem {
                        name: cond_name.clone(),
                        data_type: HirType::Numeric {
                            size: 1,
                            decimal_places: 0,
                            is_signed: false,
                        },
                        initial_value: Some(HirLiteral::Integer(0)),
                        redefines: None,
                        renames: None,
                        occurs: None,
                        indexed_by: Vec::new(),
                        screen_info: None,
                        span: entry.span,
                    });
                }
            }
        }
    }

    let (body, mut paragraphs) = program
        .procedure
        .as_ref()
        .map(|proc| lower_procedure_division(proc, &condition_names))
        .unwrap_or_default();

    // Extract FILE STATUS variable mappings from ENVIRONMENT DIVISION.
    let file_status_vars = extract_file_status_vars(program);

    // Extract file organization mappings.
    let file_organizations = extract_file_organizations(program);

    // Extract file assignment (ASSIGN TO) mappings.
    let file_assignments = extract_file_assignments(program);

    // Lower DECLARATIVES sections (USE AFTER EXCEPTION handlers).
    // Also collect individual paragraphs defined inside declarative sections
    // so they get proper forward declarations and function definitions in codegen.
    let (declaratives, decl_paragraphs) = lower_declaratives(program, &condition_names);
    // Only add declarative paragraphs that don't already exist in the main
    // paragraph list (some COBOL programs reuse paragraph names across
    // DECLARATIVES and normal procedure sections).
    {
        let existing: std::collections::HashSet<SmolStr> =
            paragraphs.iter().map(|p| p.name.clone()).collect();
        for dp in decl_paragraphs {
            if !existing.contains(&dp.name) {
                paragraphs.push(dp);
            }
        }
    }

    // Extract FD/SD file-name → first record name mapping.
    let file_records = extract_file_records(program);
    // Extract FD record aliases: additional record names → first record name.
    let fd_record_aliases = extract_fd_record_aliases(program);

    // Extract USING parameters from PROCEDURE DIVISION.
    let using_params = program
        .procedure
        .as_ref()
        .map(|proc| {
            proc.using_params
                .iter()
                .map(|p| {
                    let mode = match p.mode {
                        cobol_ast::proc_div::ParamMode::ByReference => HirParamMode::ByReference,
                        cobol_ast::proc_div::ParamMode::ByContent => HirParamMode::ByContent,
                        cobol_ast::proc_div::ParamMode::ByValue => HirParamMode::ByValue,
                    };
                    HirParam {
                        name: p.name.clone(),
                        mode,
                        data_type: HirType::Numeric {
                            size: 18,
                            decimal_places: 0,
                            is_signed: true,
                        },
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let mut hir = HirProgram {
        name,
        data_items,
        paragraphs,
        body,
        using_params,
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        file_organizations,
        file_assignments,
        file_status_vars,
        declaratives,
        file_records,
        fd_record_aliases,
        nested_programs: program.nested_programs.iter().map(lower_to_hir).collect(),
        span: program.span,
    };

    // Post-process: update Open statements with correct file organization and assign_to
    let org_map = hir.file_organizations.clone();
    let assign_map = hir.file_assignments.clone();
    patch_open_entries(&mut hir.body, &org_map, &assign_map);
    for para in &mut hir.paragraphs {
        patch_open_entries(&mut para.body, &org_map, &assign_map);
    }
    for decl in &mut hir.declaratives {
        patch_open_entries(&mut decl.body, &org_map, &assign_map);
    }

    // Post-process: resolve Write/Rewrite record_name → file_name
    let rec_to_file = extract_record_to_file_map(program);
    patch_write_file_names(&mut hir.body, &rec_to_file);
    for para in &mut hir.paragraphs {
        patch_write_file_names(&mut para.body, &rec_to_file);
    }
    for decl in &mut hir.declaratives {
        patch_write_file_names(&mut decl.body, &rec_to_file);
    }

    // Post-process: fix subscript dimensions.
    // The parser cannot distinguish `TABLE(IDX + 1)` (one subscript with
    // arithmetic) from `TABLE(+10 +10)` (two subscripts merged into one
    // expression).  We resolve this ambiguity using OCCURS dimensionality
    // from data definitions.
    let occurs_dims = build_occurs_dimension_map(&hir.data_items);
    fix_subscript_dimensions(&mut hir.body, &occurs_dims);
    for para in &mut hir.paragraphs {
        fix_subscript_dimensions(&mut para.body, &occurs_dims);
    }
    for decl in &mut hir.declaratives {
        fix_subscript_dimensions(&mut decl.body, &occurs_dims);
    }

    hir
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
    for item in &data.screen {
        collect_condition_names_from_item(item, &mut map);
    }
    for item in &data.communication {
        collect_condition_names_from_item(item, &mut map);
    }
    for item in &data.report {
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
    for item in &data.screen {
        lower_screen_data_item(item, &mut items);
    }
    for item in &data.communication {
        lower_data_item(item, &mut items);
    }
    for item in &data.report {
        lower_data_item(item, &mut items);
    }
    // Implicit Report Writer special registers
    if !data.report.is_empty() {
        items.push(HirDataItem {
            name: SmolStr::new("LINE-COUNTER"),
            data_type: HirType::Numeric {
                size: 6,
                decimal_places: 0,
                is_signed: false,
            },
            initial_value: Some(HirLiteral::Integer(0)),
            occurs: None,
            indexed_by: Vec::new(),
            redefines: None,
            renames: None,
            screen_info: None,
            span: Span::dummy(),
        });
        items.push(HirDataItem {
            name: SmolStr::new("PAGE-COUNTER"),
            data_type: HirType::Numeric {
                size: 6,
                decimal_places: 0,
                is_signed: false,
            },
            initial_value: Some(HirLiteral::Integer(0)),
            occurs: None,
            indexed_by: Vec::new(),
            redefines: None,
            renames: None,
            screen_info: None,
            span: Span::dummy(),
        });
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
        let occurs = item.occurs.as_ref().map(|o| o.max);

        let renames = item
            .renames
            .as_ref()
            .map(|r| (r.from.name.clone(), r.thru.as_ref().map(|t| t.name.clone())));

        let indexed_by = item
            .occurs
            .as_ref()
            .map(|o| o.indexed_by.clone())
            .unwrap_or_default();

        out.push(HirDataItem {
            name: name.clone(),
            data_type,
            initial_value,
            occurs,
            indexed_by: indexed_by.clone(),
            redefines: item.redefines.clone(),
            renames,
            screen_info: None,
            span: item.span,
        });
    }

    // Emit INDEXED BY names as Index-typed data items.
    if let Some(ref occurs) = item.occurs {
        for idx_name in &occurs.indexed_by {
            out.push(HirDataItem {
                name: idx_name.clone(),
                data_type: HirType::Index,
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                span: occurs.span,
            });
        }
    }

    // Recursively lower child items (group items)
    for child in &item.children {
        lower_data_item(child, out);
    }
}

/// Lower a screen section data item, attaching HirScreenInfo.
fn lower_screen_data_item(item: &DataItem, out: &mut Vec<HirDataItem>) {
    if item.level == 88 {
        return;
    }

    if let Some(name) = &item.name {
        let data_type = determine_hir_type(item);
        let initial_value = item.value.as_ref().map(lower_value_clause);
        let occurs = item.occurs.as_ref().map(|o| o.max);

        let renames = item
            .renames
            .as_ref()
            .map(|r| (r.from.name.clone(), r.thru.as_ref().map(|t| t.name.clone())));

        let has_screen_attrs = item.line_clause.is_some()
            || item.column_clause.is_some()
            || item.blank_screen
            || item.blank_line
            || item.highlight
            || item.reverse_video
            || item.source_field.is_some()
            || item.using_field.is_some();

        let screen_info = if has_screen_attrs || !item.children.is_empty() {
            // Extract VALUE as string for screen display purposes.
            let value_str = item.value.as_ref().and_then(|v| match &v.value {
                Literal::String(s) => Some(SmolStr::from(s.as_str())),
                _ => None,
            });
            let pic_str = item.picture.as_ref().map(|p| p.raw_string.clone());

            Some(HirScreenInfo {
                line: item.line_clause,
                column: item.column_clause,
                blank_screen: item.blank_screen,
                blank_line: item.blank_line,
                highlight: item.highlight,
                reverse_video: item.reverse_video,
                source: item.source_field.as_ref().map(|q| q.name.clone()),
                using_field: item.using_field.as_ref().map(|q| q.name.clone()),
                value: value_str,
                picture: pic_str,
            })
        } else {
            None
        };

        out.push(HirDataItem {
            name: name.clone(),
            data_type,
            initial_value,
            occurs,
            indexed_by: Vec::new(),
            redefines: item.redefines.clone(),
            renames,
            screen_info,
            span: item.span,
        });
    }

    for child in &item.children {
        lower_screen_data_item(child, out);
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
            cobol_ast::PictureCategory::National | cobol_ast::PictureCategory::NationalEdited => {
                HirType::National { size: pic.size }
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
            let member_name = child
                .name
                .clone()
                .unwrap_or_else(|| SmolStr::from("FILLER"));
            let data_type = determine_hir_type(child);
            let initial_value = child.value.as_ref().map(lower_value_clause);
            let occurs = child.occurs.as_ref().map(|o| o.max);
            let renames = child
                .renames
                .as_ref()
                .map(|r| (r.from.name.clone(), r.thru.as_ref().map(|t| t.name.clone())));
            let indexed_by_child = child
                .occurs
                .as_ref()
                .map(|o| o.indexed_by.clone())
                .unwrap_or_default();
            members.push(HirDataItem {
                name: member_name,
                data_type,
                initial_value,
                occurs,
                indexed_by: indexed_by_child,
                redefines: child.redefines.clone(),
                renames,
                screen_info: None,
                span: child.span,
            });
        }
        let total: u32 = members
            .iter()
            .filter(|m| m.redefines.is_none()) // REDEFINES overlay same storage
            .map(|m| {
                let element_size = match &m.data_type {
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
                    HirType::National { size } => *size * 2, // national chars are 2 bytes
                };
                // OCCURS multiplies the element size
                let count = m.occurs.unwrap_or(1);
                element_size * count
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
        Literal::FigurativeConstant(FigurativeConstant::All(s)) => HirLiteral::AllChar(s.clone()),
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
        if para.name.is_empty() {
            // Unnamed paragraphs: inline their statements into the body.
            body.extend(stmts);
        } else {
            // Named paragraphs are always registered, even if empty,
            // because they may be targets of GO TO or PERFORM THRU.
            paragraphs.push(HirParagraph {
                name: para.name.clone(),
                body: stmts,
                span: para.span,
            });
            body.push(HirStatement::Label {
                name: para.name.clone(),
            });
            body.push(HirStatement::Perform {
                kind: HirPerformKind::ProcedureName {
                    name: para.name.clone(),
                    through: None,
                },
                span: para.span,
            });
        }
    }

    // Lower sections and their paragraphs.
    // Track paragraph names to detect cross-section duplicates that would
    // cause C-level label/function redefinition errors.
    let mut seen_para_names: std::collections::HashSet<SmolStr> =
        paragraphs.iter().map(|p| p.name.clone()).collect();
    for section in &proc.sections {
        // Collect all statements in this section for the section-level paragraph
        let mut section_stmts = Vec::new();
        // Add a label for the section itself (for GO TO section-name)
        body.push(HirStatement::Label {
            name: section.name.clone(),
        });
        for para in &section.paragraphs {
            let stmts = lower_paragraph(para, condition_names);
            // If this paragraph name already exists in another section,
            // qualify it with the section name to avoid C-level collisions.
            let effective_name = if seen_para_names.contains(&para.name) {
                let qualified: SmolStr = format!("{}--{}", section.name, para.name).into();
                qualified
            } else {
                seen_para_names.insert(para.name.clone());
                para.name.clone()
            };
            body.push(HirStatement::Label {
                name: effective_name.clone(),
            });
            body.push(HirStatement::Perform {
                kind: HirPerformKind::ProcedureName {
                    name: effective_name.clone(),
                    through: None,
                },
                span: para.span,
            });
            section_stmts.extend(stmts.clone());
            paragraphs.push(HirParagraph {
                name: effective_name,
                body: stmts,
                span: para.span,
            });
        }
        // Register section name as a callable paragraph (for PERFORM section-name)
        paragraphs.push(HirParagraph {
            name: section.name.clone(),
            body: section_stmts,
            span: section.span,
        });
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
        Statement::Set(set) => Some(lower_set(set, condition_names)),
        Statement::String(string_stmt) => Some(lower_string_stmt(string_stmt, condition_names)),
        Statement::Unstring(unstring_stmt) => {
            Some(lower_unstring_stmt(unstring_stmt, condition_names))
        }
        Statement::Search(search) => Some(lower_search(search, condition_names)),
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
        Statement::ExitParagraph | Statement::ExitSection => Some(HirStatement::ExitParagraph {
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
        Statement::Validate(v) => Some(HirStatement::Validate {
            target: v.target.name.clone(),
            span: v.span,
        }),
        // --- Report writer statements ---
        Statement::Initiate(init) => Some(HirStatement::Initiate {
            report_names: init.report_names.clone(),
            span: init.span,
        }),
        Statement::Generate(gen) => Some(HirStatement::Generate {
            report_name: gen.report_name.clone(),
            span: gen.span,
        }),
        Statement::Terminate(term) => Some(HirStatement::Terminate {
            report_names: term.report_names.clone(),
            span: term.span,
        }),
        // Obsolete statements — lower to no-op or simple equivalents
        Statement::StopLiteral(expr) => {
            let hir_expr = lower_expr(expr);
            Some(HirStatement::Display {
                operands: vec![hir_expr],
                no_advancing: false,
                span: Span::new(0, 0, cobol_common::FileId(0)),
            })
        }
        Statement::Alter(_) => {
            // ALTER changes a GO TO target at runtime; not supported in HIR.
            // Emit nothing; the codegen cannot implement this obsolete feature.
            None
        }
        Statement::NextSentence => {
            // NEXT SENTENCE is an obsolete COBOL-85 construct.
            // Lower to CONTINUE (no-op) as an approximation.
            Some(HirStatement::Continue {
                span: Span::dummy(),
            })
        }
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
            let var_name = qualified_name_str(qname);
            if qname.subscripts.is_empty() {
                HirMoveTarget::Variable(var_name)
            } else {
                HirMoveTarget::Subscript {
                    variable: var_name,
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
            variable: qualified_name_str(variable),
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
        .map(|t| lower_qualified_name_to_expr(&t.target))
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
    let to = add
        .to
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let giving = add
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let on_size_error = lower_statements(&add.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&add.not_on_size_error, condition_names);
    HirStatement::Add {
        operands,
        to,
        giving,
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
    let from: Vec<HirExpr> = if let Some(ref from_e) = sub.from_expr {
        // Format 2: SUBTRACT ... FROM literal GIVING ...
        vec![lower_expr(from_e)]
    } else {
        sub.from
            .iter()
            .map(|t| lower_qualified_name_to_expr(&t.target))
            .collect()
    };
    let giving = sub
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let on_size_error = lower_statements(&sub.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&sub.not_on_size_error, condition_names);
    HirStatement::Subtract {
        operands,
        from,
        giving,
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
                let after_clauses: Vec<_> = varying[1..]
                    .iter()
                    .map(|c| HirVaryingAfter {
                        var: c.identifier.name.clone(),
                        from: lower_expr(&c.from),
                        by: lower_expr(&c.by),
                        until: lower_condition(&c.until, condition_names),
                    })
                    .collect();
                HirPerformKind::Varying {
                    var,
                    from,
                    by,
                    until,
                    after_clauses,
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
    let by: Vec<HirExpr> = if let Some(ref by_e) = mul.by_expr {
        // Format 2: MULTIPLY ... BY literal GIVING ...
        vec![lower_expr(by_e)]
    } else {
        mul.by
            .iter()
            .map(|t| lower_qualified_name_to_expr(&t.target))
            .collect()
    };
    let giving = mul
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let on_size_error = lower_statements(&mul.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&mul.not_on_size_error, condition_names);
    HirStatement::Multiply {
        operand,
        by,
        giving,
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
    let into: Vec<HirExpr> = if let Some(ref into_e) = div.into_expr {
        // Format 2: DIVIDE ... INTO literal GIVING ...
        vec![lower_expr(into_e)]
    } else {
        div.into
            .iter()
            .map(|t| lower_qualified_name_to_expr(&t.target))
            .collect()
    };
    let giving = div
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let remainder = div.remainder.as_ref().map(lower_qualified_name_to_expr);
    let on_size_error = lower_statements(&div.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&div.not_on_size_error, condition_names);
    HirStatement::Divide {
        operand,
        into,
        giving,
        remainder,
        on_size_error,
        not_on_size_error,
        span: div.span,
    }
}

fn lower_accept(accept: &AcceptStatement) -> HirStatement {
    use cobol_ast::statement::AcceptSource as AstSource;
    let source = match &accept.from {
        Some(AstSource::Date) => HirAcceptSource::Date,
        Some(AstSource::DateYyyymmdd) => HirAcceptSource::DateYyyymmdd,
        Some(AstSource::Day) => HirAcceptSource::Day,
        Some(AstSource::DayOfWeek) => HirAcceptSource::DayOfWeek,
        Some(AstSource::Time) => HirAcceptSource::Time,
        Some(AstSource::Environment(name)) => HirAcceptSource::Environment(name.clone()),
        Some(AstSource::Console) | None => HirAcceptSource::Console,
    };
    HirStatement::Accept {
        target: accept.target.name.clone(),
        source,
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
                assign_to: SmolStr::default(), // will be updated post-lowering
                organization: 1,               // will be updated post-lowering
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
    let into = read.into.as_ref().map(|q| {
        let subs: Vec<_> = q.subscripts.iter().map(lower_expr).collect();
        (q.name.clone(), subs)
    });
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
    let not_invalid_key = lower_statements(&write.not_invalid_key, condition_names);
    HirStatement::Write {
        record_name: write.record_name.name.clone(),
        file_name: SmolStr::default(), // resolved post-lowering
        from,
        invalid_key,
        not_invalid_key,
        span: write.span,
    }
}

fn lower_rewrite(rewrite: &cobol_ast::statement::RewriteStatement) -> HirStatement {
    let from = rewrite.from.as_ref().map(lower_expr);
    HirStatement::Rewrite {
        record_name: rewrite.record_name.name.clone(),
        file_name: SmolStr::default(), // resolved post-lowering
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

fn lower_set(
    set: &SetStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    use cobol_ast::statement::SetKind;
    match &set.kind {
        SetKind::To { targets, value } => {
            let target_exprs = targets
                .iter()
                .map(|q| lower_qualified_name_to_expr(q))
                .collect();
            let hir_value = lower_expr(value);
            HirStatement::Set {
                targets: target_exprs,
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
            let target_exprs: Vec<_> = targets.iter().map(lower_qualified_name_to_expr).collect();
            let hir_value = lower_expr(value);
            match direction {
                cobol_ast::statement::SetDirection::Up => HirStatement::Add {
                    operands: vec![hir_value],
                    to: target_exprs,
                    giving: Vec::new(),
                    on_size_error: Vec::new(),
                    not_on_size_error: Vec::new(),
                    span: set.span,
                },
                cobol_ast::statement::SetDirection::Down => HirStatement::Subtract {
                    operands: vec![hir_value],
                    from: target_exprs,
                    giving: Vec::new(),
                    on_size_error: Vec::new(),
                    not_on_size_error: Vec::new(),
                    span: set.span,
                },
            }
        }
        SetKind::ConditionTrue {
            conditions,
            value: _,
        } => {
            // SET condition-name TO TRUE:
            // For each condition name, find its parent data item and the
            // first VALUE, then emit MOVE value TO parent.
            let mut targets: Vec<HirMoveTarget> = Vec::new();
            let mut hir_value = HirExpr::Literal(HirLiteral::Integer(1));
            for cond_qn in conditions {
                let cond_name = &cond_qn.name;
                if let Some(info) = condition_names.get(cond_name) {
                    targets.push(HirMoveTarget::Variable(info.parent_name.clone()));
                    // Use the first value of the condition-name
                    if let Some(first_cv) = info.values.first() {
                        hir_value = match first_cv {
                            ConditionValue::Single(lit) => HirExpr::Literal(lit.clone()),
                            ConditionValue::Range { from, .. } => HirExpr::Literal(from.clone()),
                        };
                    }
                } else {
                    targets.push(HirMoveTarget::Variable(cond_name.clone()));
                }
            }
            HirStatement::Move {
                from: hir_value,
                to: targets,
                span: set.span,
            }
        }
        SetKind::Address { target, source } => HirStatement::SetAddress {
            target: target.name.clone(),
            source: source.name.clone(),
            span: set.span,
        },
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

fn lower_search(
    search: &cobol_ast::statement::SearchStatement,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> HirStatement {
    let at_end = lower_statements(&search.at_end, condition_names);
    let when_clauses = search
        .when_clauses
        .iter()
        .map(|w| HirSearchWhen {
            condition: lower_condition(&w.condition, condition_names),
            body: lower_statements(&w.body, condition_names),
        })
        .collect();
    HirStatement::Search {
        table_name: search.table_name.name.clone(),
        all: search.all,
        varying: search.varying.as_ref().map(|v| v.name.clone()),
        at_end,
        when_clauses,
        span: search.span,
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
    let (using, input_procedure) = match &sort.input {
        cobol_ast::statement::SortInput::Using(files) => (files.clone(), None),
        cobol_ast::statement::SortInput::InputProcedure { procedure, through } => {
            (Vec::new(), Some((procedure.clone(), through.clone())))
        }
    };
    let (giving, output_procedure) = match &sort.output {
        cobol_ast::statement::SortOutput::Giving(files) => (files.clone(), None),
        cobol_ast::statement::SortOutput::OutputProcedure { procedure, through } => {
            (Vec::new(), Some((procedure.clone(), through.clone())))
        }
    };
    HirStatement::Sort {
        file_name: sort.file_name.clone(),
        keys,
        using,
        giving,
        input_procedure,
        output_procedure,
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
        counter: lower_qualified_name_to_expr(&t.counter),
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
    let into = ret.into.as_ref().map(|q| {
        let subs: Vec<_> = q.subscripts.iter().map(lower_expr).collect();
        (q.name.clone(), subs)
    });
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
    let (giving, output_procedure) = match &merge.output {
        cobol_ast::statement::SortOutput::Giving(files) => (files.clone(), None),
        cobol_ast::statement::SortOutput::OutputProcedure { procedure, through } => {
            (Vec::new(), Some((procedure.clone(), through.clone())))
        }
    };
    HirStatement::Merge {
        file_name: merge.file_name.clone(),
        keys,
        using,
        giving,
        output_procedure,
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
    let (target, char_count) = match &alloc.target {
        AllocateTarget::DataName(qname) => (qname.name.clone(), None),
        AllocateTarget::Characters(expr) => (SmolStr::new("_ALLOC_CHARS"), Some(lower_expr(expr))),
    };
    let returning = alloc.returning.as_ref().map(|q| q.name.clone());
    HirStatement::Allocate {
        target,
        returning,
        char_count,
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

/// Produce a (possibly qualified) name string from a QualifiedName.
/// If qualifiers exist, the outermost qualifier is used as a prefix:
///   `FIELD OF GROUP` becomes `GROUP::FIELD`.
fn qualified_name_str(qname: &cobol_ast::expr::QualifiedName) -> SmolStr {
    if qname.qualifiers.is_empty() {
        qname.name.clone()
    } else {
        let group = qname.qualifiers.last().unwrap();
        SmolStr::new(format!("{}::{}", group, qname.name))
    }
}

/// Lower a `QualifiedName` (used as an arithmetic target) to a `HirExpr`.
/// Handles subscripts so that `TABLE(IDX)` becomes `HirExpr::Subscript`.
fn lower_qualified_name_to_expr(qname: &cobol_ast::expr::QualifiedName) -> HirExpr {
    let var_name = qualified_name_str(qname);
    if qname.subscripts.is_empty() {
        HirExpr::Variable(var_name)
    } else {
        HirExpr::Subscript {
            variable: var_name,
            subscripts: qname.subscripts.iter().map(lower_expr).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Expression and condition lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &Expr) -> HirExpr {
    match expr {
        Expr::Literal(lit) => HirExpr::Literal(lower_literal(lit)),
        Expr::Identifier(qname) => {
            // When qualifiers exist (e.g., FIELD-A OF WS-DST), compose a
            // disambiguated name using the outermost qualifier as prefix.
            let var_name = if qname.qualifiers.is_empty() {
                qname.name.clone()
            } else {
                // Use outermost qualifier (last element) as prefix
                let group = qname.qualifiers.last().unwrap();
                SmolStr::new(format!("{}::{}", group, qname.name))
            };
            if qname.subscripts.is_empty() {
                HirExpr::Variable(var_name)
            } else {
                HirExpr::Subscript {
                    variable: var_name,
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
            variable: qualified_name_str(variable),
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

fn extract_file_assignments(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .map(|fc| (fc.file_name.clone(), fc.assign_to.clone()))
        .collect()
}

/// Extract FD/SD file name → first record name mapping from the DATA DIVISION's
/// FILE SECTION.  Each `FileDescription` contributes a mapping from its file name
/// to the name of its first 01-level record entry.
fn extract_file_records(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let Some(data) = &program.data else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    for fd in &data.file_section {
        // Find the first named 01-level record item under this FD/SD.
        if let Some(first_item) = fd.items.iter().find_map(|item| item.name.clone()) {
            map.insert(fd.file_name.clone(), first_item);
        }
    }
    map
}

/// Build a map from additional FD record names to the first record name.
/// In COBOL, multiple 01-level items under the same FD share the same record buffer.
fn extract_fd_record_aliases(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let Some(data) = &program.data else {
        return HashMap::new();
    };
    let mut aliases = HashMap::new();
    for fd in &data.file_section {
        let named_items: Vec<SmolStr> = fd
            .items
            .iter()
            .filter_map(|item| item.name.clone())
            .collect();
        if named_items.len() > 1 {
            let first = &named_items[0];
            for other in &named_items[1..] {
                aliases.insert(other.clone(), first.clone());
            }
        }
    }
    aliases
}

fn patch_open_entries(
    stmts: &mut [HirStatement],
    org_map: &HashMap<SmolStr, u32>,
    assign_map: &HashMap<SmolStr, SmolStr>,
) {
    for stmt in stmts.iter_mut() {
        if let HirStatement::Open { entries, .. } = stmt {
            for entry in entries.iter_mut() {
                if let Some(&org) = org_map.get(&entry.file_name) {
                    entry.organization = org;
                }
                if let Some(path) = assign_map.get(&entry.file_name) {
                    entry.assign_to = path.clone();
                }
            }
        }
    }
}

/// Build a mapping from record names (level-01 items under FD) to file names.
fn extract_record_to_file_map(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let mut map = HashMap::new();
    if let Some(data) = &program.data {
        for fd in &data.file_section {
            for item in &fd.items {
                if let Some(ref name) = item.name {
                    map.insert(name.clone(), fd.file_name.clone());
                }
            }
        }
    }
    map
}

fn patch_write_file_names(stmts: &mut [HirStatement], rec_map: &HashMap<SmolStr, SmolStr>) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStatement::Write {
                record_name,
                file_name,
                ..
            } => {
                if let Some(fn_name) = rec_map.get(record_name.as_str()) {
                    *file_name = fn_name.clone();
                }
            }
            HirStatement::Rewrite {
                record_name,
                file_name,
                ..
            } => {
                if let Some(fn_name) = rec_map.get(record_name.as_str()) {
                    *file_name = fn_name.clone();
                }
            }
            HirStatement::If {
                then_body,
                else_body,
                ..
            } => {
                patch_write_file_names(then_body, rec_map);
                patch_write_file_names(else_body, rec_map);
            }
            HirStatement::Perform { kind, .. } => {
                let body = match kind {
                    HirPerformKind::Inline { body } => body.as_mut_slice(),
                    HirPerformKind::Times { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Until { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Varying { body, .. } => body.as_mut_slice(),
                    HirPerformKind::ProcedureName { .. } => &mut [],
                };
                patch_write_file_names(body, rec_map);
            }
            _ => {}
        }
    }
}

fn extract_file_organizations(program: &CobolProgram) -> HashMap<SmolStr, u32> {
    use cobol_ast::FileOrganization;
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .map(|fc| {
            let org = match fc.organization {
                Some(FileOrganization::Sequential) => 0,
                Some(FileOrganization::LineSequential) | None => 1,
                Some(FileOrganization::Indexed) => 2,
                Some(FileOrganization::Relative) => 3,
            };
            (fc.file_name.clone(), org)
        })
        .collect()
}

/// Lower DECLARATIVES sections from the PROCEDURE DIVISION.
/// Only USE AFTER EXCEPTION sections are lowered; other USE types are ignored.
///
/// Returns `(declaratives, extra_paragraphs)` where `extra_paragraphs` are the
/// individual paragraphs defined inside each declarative section.  These must be
/// appended to the program's `paragraphs` list so that PERFORM references from
/// the declarative body (e.g. `PERFORM DECL-PASS`) can be resolved at C level.
fn lower_declaratives(
    program: &CobolProgram,
    condition_names: &HashMap<SmolStr, ConditionNameInfo>,
) -> (Vec<HirDeclarative>, Vec<HirParagraph>) {
    let Some(proc) = &program.procedure else {
        return (Vec::new(), Vec::new());
    };
    let mut decls = Vec::new();
    let mut extra_paras = Vec::new();
    let mut seen_para_names = std::collections::HashSet::new();
    for decl in &proc.declaratives {
        if let UseStatement::AfterException { file_names } = &decl.use_statement {
            let body: Vec<HirStatement> = decl
                .paragraphs
                .iter()
                .flat_map(|para| lower_paragraph(para, condition_names))
                .collect();
            decls.push(HirDeclarative {
                name: decl.name.clone(),
                file_names: file_names.clone(),
                body,
            });
            // Also register each named paragraph inside the declarative section
            // so that codegen emits forward declarations and function definitions.
            // Skip duplicate paragraph names across declarative sections to avoid
            // C-level redefinition errors (e.g. INPUT-PROCESS in IX218A).
            for para in &decl.paragraphs {
                if !para.name.is_empty() && !seen_para_names.contains(&para.name) {
                    seen_para_names.insert(para.name.clone());
                    let stmts = lower_paragraph(para, condition_names);
                    extra_paras.push(HirParagraph {
                        name: para.name.clone(),
                        body: stmts,
                        span: para.span,
                    });
                }
            }
        }
    }
    (decls, extra_paras)
}

// ---------------------------------------------------------------------------
// Post-processing: fix subscript dimensions
// ---------------------------------------------------------------------------

/// Build a map from (uppercased, trimmed) variable name to the number of
/// OCCURS dimensions that must be subscripted when accessing it.
///
/// For example, given:
///   01 TABLE-1.  02 GRP OCCURS 3.  03 ITEM OCCURS 4.
/// The map contains: { "GRP" => 1, "ITEM" => 2 }.
fn build_occurs_dimension_map(data_items: &[HirDataItem]) -> HashMap<SmolStr, usize> {
    let mut map = HashMap::new();
    for item in data_items {
        let depth = if item.occurs.is_some() { 1 } else { 0 };
        if let HirType::Group { members, .. } = &item.data_type {
            collect_occurs_dimensions(&mut map, members, depth);
        }
        if depth > 0 {
            // Keep the maximum depth (flat data_items may re-process items
            // that already have a higher depth from their group ancestors).
            let entry = map.entry(item.name.clone()).or_insert(0);
            if depth > *entry {
                *entry = depth;
            }
        }
    }
    map
}

fn collect_occurs_dimensions(
    map: &mut HashMap<SmolStr, usize>,
    members: &[HirDataItem],
    ancestor_depth: usize,
) {
    for member in members {
        let depth = ancestor_depth + if member.occurs.is_some() { 1 } else { 0 };
        if depth > 0 {
            let entry = map.entry(member.name.clone()).or_insert(0);
            if depth > *entry {
                *entry = depth;
            }
        }
        if let HirType::Group { members: sub, .. } = &member.data_type {
            collect_occurs_dimensions(map, sub, depth);
        }
    }
}

/// Split a chain of `BinaryOp(Add/Sub)` into `target_count` subscript parts.
///
/// Works by peeling off the rightmost operand from a top-level Add/Sub one
/// level at a time.  For example, with target_count=2:
///   `(IN1 + 4) + 2`  →  `[IN1 + 4, 2]`
/// With target_count=3:
///   `((+8) + (+1)) + (+3)`  →  `[+8, +1, +3]`
///
/// This preserves legitimate intra-subscript arithmetic while splitting
/// inter-subscript boundaries merged by the parser.
fn split_subscript_expr(expr: &HirExpr, target_count: usize) -> Vec<HirExpr> {
    let mut parts = vec![expr.clone()];
    while parts.len() < target_count {
        // Find the first part (from the left) that is a splittable BinaryOp
        let split_idx = parts.iter().position(|p| {
            matches!(
                p,
                HirExpr::BinaryOp {
                    op: HirBinOp::Add | HirBinOp::Sub,
                    ..
                }
            )
        });
        let Some(idx) = split_idx else {
            break; // No more splittable expressions
        };
        let removed = parts.remove(idx);
        if let HirExpr::BinaryOp { op, left, right } = removed {
            parts.insert(idx, *left);
            let right_expr = if op == HirBinOp::Sub {
                HirExpr::UnaryOp {
                    op: HirUnaryOp::Neg,
                    operand: right,
                }
            } else {
                *right
            };
            parts.insert(idx + 1, right_expr);
        }
    }
    parts
}

/// Walk all statements and fix `HirExpr::Subscript` / `HirMoveTarget::Subscript`
/// nodes whose subscript count doesn't match the expected OCCURS dimensionality.
fn fix_subscript_dimensions(stmts: &mut [HirStatement], occurs_dims: &HashMap<SmolStr, usize>) {
    for stmt in stmts.iter_mut() {
        fix_subscripts_in_statement(stmt, occurs_dims);
    }
}

fn fix_subscripts_in_expr(expr: &mut HirExpr, occurs_dims: &HashMap<SmolStr, usize>) {
    match expr {
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            // First, recursively fix inner subscript expressions
            for sub in subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            // Check if we need to split subscripts
            let var_upper = SmolStr::new(variable.to_uppercase());
            let expected = occurs_dims
                .get(&var_upper)
                .or_else(|| occurs_dims.get(variable.as_str()))
                .copied()
                .unwrap_or(0);
            if expected > subscripts.len() {
                // Split BinaryOp(Add/Sub) expressions to reach the
                // expected dimension count.  Each subscript that is a
                // BinaryOp is split one level at a time, distributing
                // the missing dimensions across all splittable subscripts.
                let missing = expected - subscripts.len();
                let mut new_subs = Vec::new();
                let mut remaining = missing;
                for sub in subscripts.iter() {
                    if remaining > 0 {
                        let need = remaining + 1; // this sub should yield need parts
                        let parts = split_subscript_expr(sub, need);
                        let actually_added = parts.len().saturating_sub(1);
                        remaining = remaining.saturating_sub(actually_added);
                        new_subs.extend(parts);
                    } else {
                        new_subs.push(sub.clone());
                    }
                }
                if new_subs.len() == expected {
                    *subscripts = new_subs;
                }
            }
        }
        HirExpr::BinaryOp { left, right, .. } => {
            fix_subscripts_in_expr(left, occurs_dims);
            fix_subscripts_in_expr(right, occurs_dims);
        }
        HirExpr::UnaryOp { operand, .. } => {
            fix_subscripts_in_expr(operand, occurs_dims);
        }
        _ => {}
    }
}

fn fix_subscripts_in_move_target(
    target: &mut HirMoveTarget,
    occurs_dims: &HashMap<SmolStr, usize>,
) {
    if let HirMoveTarget::Subscript {
        variable,
        subscripts,
    } = target
    {
        for sub in subscripts.iter_mut() {
            fix_subscripts_in_expr(sub, occurs_dims);
        }
        let var_upper = SmolStr::new(variable.to_uppercase());
        let expected = occurs_dims
            .get(&var_upper)
            .or_else(|| occurs_dims.get(variable.as_str()))
            .copied()
            .unwrap_or(0);
        if expected > subscripts.len() {
            let missing = expected - subscripts.len();
            let mut new_subs = Vec::new();
            let mut remaining = missing;
            for sub in subscripts.iter() {
                if remaining > 0 {
                    let need = remaining + 1;
                    let parts = split_subscript_expr(sub, need);
                    let actually_added = parts.len().saturating_sub(1);
                    remaining = remaining.saturating_sub(actually_added);
                    new_subs.extend(parts);
                } else {
                    new_subs.push(sub.clone());
                }
            }
            if new_subs.len() == expected {
                *subscripts = new_subs;
            }
        }
    }
}

fn fix_subscripts_in_condition(cond: &mut HirCondition, occurs_dims: &HashMap<SmolStr, usize>) {
    match cond {
        HirCondition::Compare { left, right, .. } => {
            fix_subscripts_in_expr(left, occurs_dims);
            fix_subscripts_in_expr(right, occurs_dims);
        }
        HirCondition::ClassCondition { operand, .. } => {
            fix_subscripts_in_expr(operand, occurs_dims);
        }
        HirCondition::And(a, b) | HirCondition::Or(a, b) => {
            fix_subscripts_in_condition(a, occurs_dims);
            fix_subscripts_in_condition(b, occurs_dims);
        }
        HirCondition::Not(inner) => fix_subscripts_in_condition(inner, occurs_dims),
    }
}

fn fix_subscripts_in_perform_kind(
    kind: &mut HirPerformKind,
    occurs_dims: &HashMap<SmolStr, usize>,
) {
    match kind {
        HirPerformKind::Inline { body }
        | HirPerformKind::Times { body, .. }
        | HirPerformKind::Until { body, .. } => {
            fix_subscript_dimensions(body, occurs_dims);
        }
        HirPerformKind::Varying {
            from,
            by,
            until,
            body,
            ..
        } => {
            fix_subscripts_in_expr(from, occurs_dims);
            fix_subscripts_in_expr(by, occurs_dims);
            fix_subscripts_in_condition(until, occurs_dims);
            fix_subscript_dimensions(body, occurs_dims);
        }
        HirPerformKind::ProcedureName { .. } => {}
    }
}

fn fix_subscripts_in_statement(stmt: &mut HirStatement, occurs_dims: &HashMap<SmolStr, usize>) {
    match stmt {
        HirStatement::Move { from, to, .. } => {
            fix_subscripts_in_expr(from, occurs_dims);
            for t in to.iter_mut() {
                fix_subscripts_in_move_target(t, occurs_dims);
            }
        }
        HirStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            fix_subscripts_in_condition(condition, occurs_dims);
            fix_subscript_dimensions(then_body, occurs_dims);
            fix_subscript_dimensions(else_body, occurs_dims);
        }
        HirStatement::Compute { targets, expr, .. } => {
            fix_subscripts_in_expr(expr, occurs_dims);
            for t in targets.iter_mut() {
                fix_subscripts_in_expr(t, occurs_dims);
            }
        }
        HirStatement::Add {
            operands,
            to,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            for op in operands.iter_mut() {
                fix_subscripts_in_expr(op, occurs_dims);
            }
            for t in to.iter_mut() {
                fix_subscripts_in_expr(t, occurs_dims);
            }
            for g in giving.iter_mut() {
                fix_subscripts_in_expr(g, occurs_dims);
            }
            fix_subscript_dimensions(on_size_error, occurs_dims);
            fix_subscript_dimensions(not_on_size_error, occurs_dims);
        }
        HirStatement::Subtract {
            operands,
            from,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            for op in operands.iter_mut() {
                fix_subscripts_in_expr(op, occurs_dims);
            }
            for f in from.iter_mut() {
                fix_subscripts_in_expr(f, occurs_dims);
            }
            for g in giving.iter_mut() {
                fix_subscripts_in_expr(g, occurs_dims);
            }
            fix_subscript_dimensions(on_size_error, occurs_dims);
            fix_subscript_dimensions(not_on_size_error, occurs_dims);
        }
        HirStatement::Multiply {
            operand,
            by,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            fix_subscripts_in_expr(operand, occurs_dims);
            for b in by.iter_mut() {
                fix_subscripts_in_expr(b, occurs_dims);
            }
            for g in giving.iter_mut() {
                fix_subscripts_in_expr(g, occurs_dims);
            }
            fix_subscript_dimensions(on_size_error, occurs_dims);
            fix_subscript_dimensions(not_on_size_error, occurs_dims);
        }
        HirStatement::Divide {
            operand,
            into,
            giving,
            remainder,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            fix_subscripts_in_expr(operand, occurs_dims);
            for i in into.iter_mut() {
                fix_subscripts_in_expr(i, occurs_dims);
            }
            for g in giving.iter_mut() {
                fix_subscripts_in_expr(g, occurs_dims);
            }
            if let Some(r) = remainder {
                fix_subscripts_in_expr(r, occurs_dims);
            }
            fix_subscript_dimensions(on_size_error, occurs_dims);
            fix_subscript_dimensions(not_on_size_error, occurs_dims);
        }
        HirStatement::Display { operands, .. } => {
            for v in operands.iter_mut() {
                fix_subscripts_in_expr(v, occurs_dims);
            }
        }
        HirStatement::Perform { kind, .. } => {
            fix_subscripts_in_perform_kind(kind, occurs_dims);
        }
        HirStatement::Search {
            at_end,
            when_clauses,
            ..
        } => {
            fix_subscript_dimensions(at_end, occurs_dims);
            for wc in when_clauses.iter_mut() {
                fix_subscripts_in_condition(&mut wc.condition, occurs_dims);
                fix_subscript_dimensions(&mut wc.body, occurs_dims);
            }
        }
        // Statements without subscript expressions or already handled
        _ => {}
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
