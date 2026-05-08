// COBOL HIR - AST to HIR lowering
//
// Converts a parsed COBOL AST into the simplified HIR form:
// - Extracts data items from DATA DIVISION
// - Flattens PROCEDURE DIVISION into a list of HIR statements
// - Desugars EVALUATE into nested IF

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use cobol_ast::{
    data_div::{SignClause, SignPosition, ValueClause},
    expr::{
        ArithOp, ClassType, CompareOp, Condition, FigurativeConstant, QualifiedName, SignType,
        UnaryArithOp,
    },
    proc_div::{Paragraph, ProcedureDivision, UseStatement},
    statement::{
        AcceptStatement, AddStatement, CallStatement, CommunicationMode, ComputeStatement,
        DisableStatement, DisplayStatement, DivideStatement, EnableStatement, EvaluateStatement,
        GoToStatement, IfStatement, InitializeStatement, MoveStatement, MultiplyStatement,
        PerformKind, PerformStatement, PerformTest, PurgeStatement, ReceiveStatement, SendOption,
        SendStatement, SetStatement, SubtractStatement,
    },
    CobolProgram, DataDivision, DataItem, Expr, FileDescription, Literal, Statement, Usage,
};
use cobol_common::Span;
use smol_str::SmolStr;

use crate::hir::{
    HirAcceptSource, HirAlternateKey, HirBeforeAfter, HirBinOp, HirCallParam, HirClassRange,
    HirClassType, HirCloseOption, HirCommunicationMode, HirCompareOp, HirCondition, HirDataItem,
    HirDataName, HirDataRef, HirDeclarative, HirDeclarativeUse, HirExpr, HirFileInfo,
    HirInitializeCategory, HirInitializeReplacing, HirInspectKind, HirInspectReplacing,
    HirInspectTallying, HirItemId, HirLiteral, HirMoveTarget, HirOpenEntry, HirOpenMode,
    HirParagraph, HirParagraphId, HirParagraphKind, HirParam, HirParamMode, HirPerformKind,
    HirPerformTest, HirProgram, HirReceiveMode, HirRefMod, HirReplacingKind, HirScreenInfo,
    HirSearchWhen, HirSendOption, HirSignClause, HirSignPosition, HirSortKey, HirSortOrder,
    HirStartRelation, HirStatement, HirStringSource, HirTallyingKind, HirTransferTarget, HirType,
    HirUnaryOp, HirUnstringDelimiter, HirUnstringTarget, HirValidationValue, HirVaryingAfter,
    HirWriteAdvancing,
};

#[derive(Debug, Clone)]
struct ResolvedDataItemEntry {
    item_id: HirItemId,
    name: HirDataName,
}

thread_local! {
    static ACTIVE_DATA_CATALOG: RefCell<Option<Vec<ResolvedDataItemEntry>>> = const { RefCell::new(None) };
    static ACTIVE_TRANSFER_TARGETS: RefCell<Option<HashMap<SmolStr, HirTransferTarget>>> = const { RefCell::new(None) };
}

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
    parent_name: HirDataName,
    values: Vec<ConditionValue>,
}

type ConditionNameMap = HashMap<SmolStr, Vec<ConditionNameInfo>>;

#[derive(Debug, Clone)]
struct ParagraphPlan {
    id: HirParagraphId,
    name: SmolStr,
    kind: HirParagraphKind,
    section_id: Option<HirParagraphId>,
    segment_number: Option<u32>,
    span: Span,
}

#[derive(Debug, Clone)]
struct SectionPlan {
    entry: ParagraphPlan,
    paragraphs: Vec<ParagraphPlan>,
}

type OpenMetadata = (
    u32,
    Option<SmolStr>,
    Vec<HirAlternateKey>,
    Option<SmolStr>,
    bool,
);
type OpenMetadataMap = HashMap<SmolStr, OpenMetadata>;
type ReadMetadata = (u32, u32, Option<SmolStr>, Option<SmolStr>);
type ReadMetadataMap = HashMap<SmolStr, ReadMetadata>;

fn key_data_name(q: &QualifiedName) -> SmolStr {
    q.qualifiers
        .first()
        .cloned()
        .unwrap_or_else(|| q.name.clone())
}

fn source_computer_has_debugging_mode(program: &CobolProgram) -> bool {
    program
        .environment
        .as_ref()
        .and_then(|env| env.configuration.as_ref())
        .and_then(|config| config.source_computer.as_ref())
        .is_some_and(|source| source.to_ascii_uppercase().contains("WITH DEBUGGING MODE"))
}

/// Lowers a COBOL AST program into the HIR.
pub fn lower_to_hir(program: &CobolProgram) -> HirProgram {
    let name = program.identification.program_id.clone();

    // Collect 88-level condition name mappings before lowering data items.
    let mut condition_names = program
        .data
        .as_ref()
        .map(collect_condition_names)
        .unwrap_or_default();
    collect_special_name_switch_conditions(program, &mut condition_names);

    let mut data_items = program
        .data
        .as_ref()
        .map(lower_data_division)
        .unwrap_or_default();

    // When any FD has a LINAGE clause, inject the implicit LINAGE-COUNTER
    // special register as a top-level numeric data item so codegen declares it.
    if let Some(data) = &program.data {
        if let Some(linage) = data.file_section.iter().find_map(|fd| fd.linage.as_ref()) {
            data_items.push(HirDataItem::new(
                "LINAGE-COUNTER",
                HirType::Numeric {
                    size: 6,
                    decimal_places: 0,
                    is_signed: false,
                },
                program.span,
            ));
            if let cobol_ast::data_div::LinageValue::Integer(lines) = linage.lines {
                data_items.push(HirDataItem::new(
                    format!("LINAGE-MARKER-LINES-{lines}"),
                    HirType::Alphanumeric { size: 1 },
                    program.span,
                ));
            } else if let cobol_ast::data_div::LinageValue::DataName(name) = &linage.lines {
                data_items.push(HirDataItem::new(
                    format!("LINAGE-MARKER-LINES-NAME-{name}"),
                    HirType::Alphanumeric { size: 1 },
                    program.span,
                ));
            }
        }
    }

    // Inject SPECIAL-NAMES switch condition names as boolean data items
    // so codegen declares them as C variables.
    if let Some(ref env) = program.environment {
        if let Some(ref config) = env.configuration {
            for entry in &config.special_names {
                if let Some(user_name) = &entry.user_name {
                    data_items.push(
                        HirDataItem::new(
                            user_name.clone(),
                            HirType::Numeric {
                                size: 1,
                                decimal_places: 0,
                                is_signed: false,
                            },
                            entry.span,
                        )
                        .with_initial_value(HirLiteral::Integer(
                            initial_special_switch_status(&entry.system_name),
                        )),
                    );
                }
                if let Some(cond_name) = &entry.on_condition {
                    data_items.push(
                        HirDataItem::new(
                            cond_name.clone(),
                            HirType::Numeric {
                                size: 1,
                                decimal_places: 0,
                                is_signed: false,
                            },
                            entry.span,
                        )
                        .with_initial_value(HirLiteral::Integer(0)),
                    );
                }
                if let Some(cond_name) = &entry.off_condition {
                    data_items.push(
                        HirDataItem::new(
                            cond_name.clone(),
                            HirType::Numeric {
                                size: 1,
                                decimal_places: 0,
                                is_signed: false,
                            },
                            entry.span,
                        )
                        .with_initial_value(HirLiteral::Integer(1)),
                    );
                }
            }
        }
    }

    let debugging_mode_enabled = source_computer_has_debugging_mode(program);

    if let Some(proc) = &program.procedure {
        let has_debugging_use = proc
            .declaratives
            .iter()
            .any(|decl| matches!(decl.use_statement, UseStatement::ForDebugging { .. }));
        if debugging_mode_enabled && has_debugging_use {
            for name in [
                "DEBUG-LINE",
                "DEBUG-NAME",
                "DEBUG-SUB-1",
                "DEBUG-SUB-2",
                "DEBUG-SUB-3",
            ] {
                data_items.push(HirDataItem::new(
                    name,
                    HirType::Alphanumeric { size: 80 },
                    program.span,
                ));
            }
            data_items.push(HirDataItem::new(
                "DEBUG-CONTENTS",
                HirType::Alphanumeric { size: 1024 },
                program.span,
            ));
        }
    }

    let data_catalog = build_resolved_data_catalog(&data_items);

    let (body, mut paragraphs, next_paragraph_id) =
        with_resolved_data_catalog(data_catalog.clone(), || {
            program
                .procedure
                .as_ref()
                .map(|proc| lower_procedure_division(proc, &condition_names))
                .unwrap_or_else(|| (Vec::new(), Vec::new(), 1))
        });

    // Extract FILE STATUS variable mappings from ENVIRONMENT DIVISION.
    let file_status_vars = extract_file_status_vars(program);

    // Extract file organization mappings.
    let file_organizations = extract_file_organizations(program);

    // Extract file access mode mappings.
    let file_access_modes = extract_file_access_modes(program);

    // Extract file assignment (ASSIGN TO) mappings.
    let file_assignments = extract_file_assignments(program);

    // Extract SELECT OPTIONAL mappings.
    let file_optionals = extract_file_optionals(program);

    // Extract relative key mappings.
    let file_relative_keys = extract_relative_keys(program);

    // Lower DECLARATIVES sections (USE AFTER EXCEPTION handlers).
    // Also collect individual paragraphs defined inside declarative sections
    // so they get proper forward declarations and function definitions in codegen.
    let (declaratives, decl_paragraphs) = with_resolved_data_catalog(data_catalog, || {
        lower_declaratives(
            program,
            &condition_names,
            next_paragraph_id,
            debugging_mode_enabled,
        )
    });
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
    let variable_record_files = extract_variable_record_files(program);
    let variable_record_depending = extract_variable_record_depending(program);
    let variable_record_bounds = extract_variable_record_bounds(program);
    let same_record_areas = extract_same_record_areas(program);

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
                        data_type: find_data_item_type_by_name(&data_items, &p.name).unwrap_or(
                            HirType::Numeric {
                                size: 18,
                                decimal_places: 0,
                                is_signed: true,
                            },
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let mut hir = HirProgram {
        name,
        data_items,
        communication_descriptions: lower_communication_descriptions(program.data.as_ref()),
        paragraphs,
        body,
        using_params,
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        file_organizations,
        file_access_modes,
        file_assignments,
        file_optionals,
        file_relative_keys,
        file_status_vars,
        declaratives,
        file_records,
        fd_record_aliases,
        variable_record_files,
        variable_record_depending,
        variable_record_bounds,
        same_record_areas,
        decimal_point_is_comma: program
            .environment
            .as_ref()
            .and_then(|env| env.configuration.as_ref())
            .is_some_and(|config| config.decimal_point_is_comma),
        special_class_conditions: collect_special_class_conditions(program),
        program_collating_sequence: collect_program_collating_sequence(program),
        nested_programs: program.nested_programs.iter().map(lower_to_hir).collect(),
        span: program.span,
    };

    // Post-process: update Open statements with correct file organization and assign_to
    let org_map = hir.file_organizations.clone();
    let assign_map = hir.file_assignments.clone();
    let open_meta_map = extract_open_metadata(program);
    patch_open_entries(&mut hir.body, &org_map, &assign_map, &open_meta_map);
    for para in &mut hir.paragraphs {
        patch_open_entries(&mut para.body, &org_map, &assign_map, &open_meta_map);
    }
    for decl in &mut hir.declaratives {
        patch_open_entries(&mut decl.body, &org_map, &assign_map, &open_meta_map);
    }

    let read_meta_map = extract_read_metadata(program);
    patch_read_keys(&mut hir.body, &read_meta_map);
    patch_start_keys(&mut hir.body, &read_meta_map);
    for para in &mut hir.paragraphs {
        patch_read_keys(&mut para.body, &read_meta_map);
        patch_start_keys(&mut para.body, &read_meta_map);
    }
    for decl in &mut hir.declaratives {
        patch_read_keys(&mut decl.body, &read_meta_map);
        patch_start_keys(&mut decl.body, &read_meta_map);
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

fn collect_special_name_switch_conditions(
    program: &CobolProgram,
    condition_names: &mut ConditionNameMap,
) {
    let Some(env) = &program.environment else {
        return;
    };
    let Some(config) = &env.configuration else {
        return;
    };

    for entry in &config.special_names {
        let Some(user_name) = &entry.user_name else {
            continue;
        };
        if let Some(cond_name) = &entry.on_condition {
            condition_names
                .entry(cond_name.clone())
                .or_default()
                .push(ConditionNameInfo {
                    parent_name: user_name.clone().into(),
                    values: vec![ConditionValue::Single(HirLiteral::Integer(1))],
                });
        }
        if let Some(cond_name) = &entry.off_condition {
            condition_names
                .entry(cond_name.clone())
                .or_default()
                .push(ConditionNameInfo {
                    parent_name: user_name.clone().into(),
                    values: vec![ConditionValue::Single(HirLiteral::Integer(0))],
                });
        }
    }
}

fn initial_special_switch_status(system_name: &str) -> i64 {
    let normalized = system_name.trim_matches('"').to_ascii_uppercase();
    if normalized.ends_with("O51") || normalized.ends_with("XXXXX051") {
        1
    } else {
        0
    }
}

fn collect_special_class_conditions(
    program: &CobolProgram,
) -> HashMap<SmolStr, Vec<HirClassRange>> {
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(config) = &env.configuration else {
        return HashMap::new();
    };

    config
        .special_classes
        .iter()
        .map(|class| {
            let ranges = class
                .ranges
                .iter()
                .map(|range| match range {
                    cobol_ast::env_div::SpecialClassRange::Single(value) => HirClassRange {
                        from: value.clone(),
                        to: value.clone(),
                    },
                    cobol_ast::env_div::SpecialClassRange::Range { from, to } => HirClassRange {
                        from: from.clone(),
                        to: to.clone(),
                    },
                })
                .collect();
            (class.name.clone(), ranges)
        })
        .collect()
}

fn collect_program_collating_sequence(program: &CobolProgram) -> Option<Vec<Vec<SmolStr>>> {
    let config = program
        .environment
        .as_ref()
        .and_then(|env| env.configuration.as_ref())?;
    let object_text = config.object_computer.as_ref()?.to_ascii_uppercase();
    let words = object_text.split_whitespace().collect::<Vec<_>>();
    let alphabet_name = words.windows(4).find_map(|window| {
        if window[0] == "COLLATING" && window[1] == "SEQUENCE" && window[2] == "IS" {
            Some(window[3])
        } else {
            None
        }
    })?;
    config
        .special_alphabets
        .iter()
        .find(|alphabet| alphabet.name.eq_ignore_ascii_case(alphabet_name))
        .map(|alphabet| alphabet.ranks.clone())
}

fn with_resolved_data_catalog<T>(catalog: Vec<ResolvedDataItemEntry>, f: impl FnOnce() -> T) -> T {
    ACTIVE_DATA_CATALOG.with(|slot| {
        let previous = slot.replace(Some(catalog));
        let result = f();
        slot.replace(previous);
        result
    })
}

fn with_transfer_targets<T>(
    targets: HashMap<SmolStr, HirTransferTarget>,
    f: impl FnOnce() -> T,
) -> T {
    ACTIVE_TRANSFER_TARGETS.with(|slot| {
        let previous = slot.replace(Some(targets));
        let result = f();
        slot.replace(previous);
        result
    })
}

fn resolve_transfer_target(name: &SmolStr) -> HirTransferTarget {
    ACTIVE_TRANSFER_TARGETS.with(|slot| {
        slot.borrow()
            .as_ref()
            .and_then(|targets| targets.get(name))
            .cloned()
            .unwrap_or_else(|| HirTransferTarget::Paragraph {
                id: HirParagraphId(u32::MAX),
                name: name.clone(),
            })
    })
}

fn build_resolved_data_catalog(data_items: &[HirDataItem]) -> Vec<ResolvedDataItemEntry> {
    fn collect(
        items: &[HirDataItem],
        ancestors_outer_to_inner: &[SmolStr],
        seen: &mut HashSet<(Span, SmolStr)>,
        next_id: &mut u32,
        out: &mut Vec<ResolvedDataItemEntry>,
    ) {
        for item in items {
            let duplicate_key = (item.span, item.name.clone());
            if !seen.insert(duplicate_key) {
                continue;
            }

            let qualifiers = ancestors_outer_to_inner.iter().rev().cloned().collect();
            out.push(ResolvedDataItemEntry {
                item_id: HirItemId(*next_id),
                name: HirDataName::new(item.name.clone(), qualifiers),
            });
            *next_id += 1;

            if let HirType::Group { members, .. } = &item.data_type {
                let mut child_ancestors = ancestors_outer_to_inner.to_vec();
                child_ancestors.push(item.name.clone());
                collect(members, &child_ancestors, seen, next_id, out);
            }
        }
    }

    let mut seen = HashSet::new();
    let mut next_id = 0;
    let mut out = Vec::new();
    collect(data_items, &[], &mut seen, &mut next_id, &mut out);
    out
}

fn find_data_item_type_by_name(items: &[HirDataItem], name: &SmolStr) -> Option<HirType> {
    for item in items {
        if item.name.eq_ignore_ascii_case(name.as_str()) {
            return Some(item.data_type.clone());
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_data_item_type_by_name(members, name) {
                return Some(found);
            }
        }
    }
    None
}

fn resolve_data_name(name: &HirDataName) -> Option<ResolvedDataItemEntry> {
    fn matches_qualifiers(candidate: &HirDataName, query: &HirDataName) -> bool {
        if query.qualifiers.is_empty() {
            return true;
        }

        let mut search_from = 0usize;
        for qualifier in &query.qualifiers {
            let Some(offset) = candidate.qualifiers[search_from..]
                .iter()
                .position(|ancestor| ancestor.eq(qualifier.as_str()))
            else {
                return false;
            };
            search_from += offset + 1;
        }
        true
    }

    ACTIVE_DATA_CATALOG.with(|slot| {
        let catalog = slot.borrow();
        let catalog = catalog.as_ref()?;
        let mut matches = catalog
            .iter()
            .filter(|entry| entry.name.name.eq(name.name.as_str()))
            .filter(|entry| matches_qualifiers(&entry.name, name))
            .cloned();
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    })
}

fn build_data_ref(
    name: HirDataName,
    subscripts: Vec<HirExpr>,
    refmod: Option<HirRefMod>,
) -> Option<HirDataRef> {
    let resolved = resolve_data_name(&name)?;
    Some(HirDataRef {
        item_id: resolved.item_id,
        name: resolved.name,
        subscripts,
        refmod,
    })
}

fn lower_communication_descriptions(
    data: Option<&DataDivision>,
) -> Vec<crate::hir::HirCommunicationDescription> {
    let Some(data) = data else {
        return Vec::new();
    };
    data.communication
        .iter()
        .map(|cd| {
            let mut names = Vec::new();
            for item in &cd.data_items {
                collect_data_item_names(item, &mut names);
            }
            crate::hir::HirCommunicationDescription {
                name: cd.name.clone(),
                record_name: cd.data_items.first().and_then(|item| item.name.clone()),
                symbolic_queue: cd
                    .symbolic_queue
                    .clone()
                    .or_else(|| infer_comm_item_name(&names, &["QUEUE_SET"])),
                symbolic_sub_queue_1: cd.symbolic_sub_queue_1.clone(),
                symbolic_sub_queue_2: cd.symbolic_sub_queue_2.clone(),
                symbolic_sub_queue_3: cd.symbolic_sub_queue_3.clone(),
                status_key: cd.status_key.clone().or_else(|| {
                    infer_comm_item_name(&names, &["IN_STATUS", "OUT_STATUS", "STATUS_KEY"])
                }),
                message_count: cd
                    .message_count
                    .clone()
                    .or_else(|| infer_comm_item_name(&names, &["MSG_COUNT", "MESSAGE_COUNT"])),
                text_length: cd.text_length.clone().or_else(|| {
                    infer_comm_item_name(&names, &["IN_LENGTH", "OUT_LENGTH", "TEXT_LENGTH"])
                }),
                end_key: cd
                    .end_key
                    .clone()
                    .or_else(|| infer_comm_item_name(&names, &["END_KEY"])),
                error_key: cd.error_key.clone(),
                symbolic_source: cd.symbolic_source.clone().or_else(|| {
                    infer_comm_item_name(&names, &["SYM_SOURCE", "SYMBOLIC_SOURCE", "WHERE_FROM"])
                }),
                destination_count: cd.destination_count.clone(),
                destination: cd.destination.clone(),
                destination_table_count: cd.destination_table_count,
            }
        })
        .collect()
}

fn collect_data_item_names(item: &DataItem, names: &mut Vec<SmolStr>) {
    if let Some(name) = &item.name {
        names.push(name.clone());
    }
    for child in &item.children {
        collect_data_item_names(child, names);
    }
}

fn infer_comm_item_name(names: &[SmolStr], candidates: &[&str]) -> Option<SmolStr> {
    names.iter().find_map(|name| {
        let normalized = name.replace('-', "_").to_ascii_uppercase();
        candidates
            .iter()
            .any(|candidate| {
                normalized == *candidate
                    || normalized
                        .strip_prefix(candidate)
                        .is_some_and(|suffix| suffix.starts_with('_'))
            })
            .then(|| name.clone())
    })
}

/// Collect 88-level condition name information from the DATA DIVISION.
/// Maps each 88-level name to its parent variable name and the values
/// that make the condition true.
fn collect_condition_names(data: &DataDivision) -> ConditionNameMap {
    let mut map = HashMap::new();
    for fd in &data.file_section {
        for item in &fd.items {
            collect_condition_names_from_item(item, &[], &mut map);
        }
    }
    for item in &data.working_storage {
        collect_condition_names_from_item(item, &[], &mut map);
    }
    for item in &data.local_storage {
        collect_condition_names_from_item(item, &[], &mut map);
    }
    for item in &data.linkage {
        collect_condition_names_from_item(item, &[], &mut map);
    }
    for item in &data.screen {
        collect_condition_names_from_item(item, &[], &mut map);
    }
    for cd in &data.communication {
        for item in &cd.data_items {
            collect_condition_names_from_item(item, &[], &mut map);
        }
    }
    for item in &data.report {
        collect_condition_names_from_item(item, &[], &mut map);
    }
    map
}

fn collect_condition_names_from_item(
    item: &DataItem,
    inherited_ancestors: &[SmolStr],
    map: &mut ConditionNameMap,
) {
    let current_parent_name = item.name.as_ref().map(|name| {
        HirDataName::new(
            name.clone(),
            inherited_ancestors.iter().rev().cloned().collect(),
        )
    });
    // Check children for 88-level items that belong to this parent
    if let Some(parent_name) = current_parent_name.clone() {
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
                    map.entry(cond_name.clone())
                        .or_default()
                        .push(ConditionNameInfo {
                            parent_name: parent_name.clone(),
                            values,
                        });
                }
            }
        }
    }

    // Recurse into non-88 children
    let mut child_ancestors = inherited_ancestors.to_vec();
    if let Some(name) = &item.name {
        child_ancestors.push(name.clone());
    }
    for child in &item.children {
        if child.level != 88 {
            collect_condition_names_from_item(child, &child_ancestors, map);
        }
    }
}

fn resolve_condition_name<'a>(
    name: &HirDataName,
    condition_names: &'a ConditionNameMap,
) -> Option<&'a ConditionNameInfo> {
    let candidates = condition_names.get(&name.name)?;
    if name.qualifiers.is_empty() {
        return candidates.last();
    }
    candidates
        .iter()
        .find(|info| {
            name.qualifiers.iter().all(|qualifier| {
                info.parent_name.name == *qualifier
                    || info
                        .parent_name
                        .qualifiers
                        .iter()
                        .any(|candidate| candidate == qualifier)
            })
        })
        .or_else(|| candidates.last())
}

// ---------------------------------------------------------------------------
// Data Division lowering
// ---------------------------------------------------------------------------

fn lower_data_division(data: &DataDivision) -> Vec<HirDataItem> {
    let mut items = Vec::new();
    for fd in &data.file_section {
        for item in &fd.items {
            lower_data_item(item, fd.is_external, &mut items);
        }
    }
    for item in &data.working_storage {
        lower_data_item(item, false, &mut items);
    }
    for item in &data.local_storage {
        lower_data_item(item, false, &mut items);
    }
    for item in &data.linkage {
        lower_data_item(item, false, &mut items);
    }
    for item in &data.screen {
        lower_screen_data_item(item, &mut items);
    }
    for cd in &data.communication {
        for item in &cd.data_items {
            lower_data_item(item, false, &mut items);
        }
    }
    for item in &data.report {
        lower_data_item(item, false, &mut items);
    }
    // Implicit Report Writer special registers
    if !data.report.is_empty() {
        let counter_type = HirType::Numeric {
            size: 6,
            decimal_places: 0,
            is_signed: false,
        };
        items.push(
            HirDataItem::new("LINE-COUNTER", counter_type.clone(), Span::dummy())
                .with_initial_value(HirLiteral::Integer(0)),
        );
        items.push(
            HirDataItem::new("PAGE-COUNTER", counter_type, Span::dummy())
                .with_initial_value(HirLiteral::Integer(0)),
        );
    }
    items
}

fn lower_data_item(item: &DataItem, inherited_external: bool, out: &mut Vec<HirDataItem>) {
    lower_data_item_with_usage(item, inherited_external, None, None, out);
}

fn lower_data_item_with_usage(
    item: &DataItem,
    inherited_external: bool,
    inherited_usage: Option<&Usage>,
    inherited_sign: Option<&SignClause>,
    out: &mut Vec<HirDataItem>,
) {
    // Skip FILLER and level 88 condition names
    if item.level == 88 {
        return;
    }

    let hir_name = item.name.clone().or_else(|| {
        if item.redefines.is_some() && !item.children.is_empty() {
            Some(SmolStr::from(format!(
                "FILLER-REDEFINES-{}",
                item.span.start
            )))
        } else {
            None
        }
    });

    if let Some(name) = hir_name {
        let data_type = determine_hir_type_with_usage(item, inherited_usage, inherited_sign);
        let effective_sign = item.sign_clause.as_ref().or(inherited_sign);
        let initial_value = item.value.as_ref().map(lower_value_clause);
        let validation_values = validation_values_for_item(item);
        let occurs = item.occurs.as_ref().map(|o| o.max);
        let occurs_depending_on = item
            .occurs
            .as_ref()
            .and_then(|o| o.depending_on.as_ref().map(|name| name.name.clone()));

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
            name,
            data_type,
            picture: item.picture.as_ref().map(|p| p.raw_string.clone()),
            is_numeric_edited: is_numeric_edited_item(item),
            sign: lower_sign_clause(effective_sign),
            blank_when_zero: item.blank_when_zero,
            scale_adjustment: picture_scale_adjustment(item),
            is_external: inherited_external || item.is_external,
            initial_value,
            validation_values,
            occurs,
            occurs_depending_on,
            indexed_by: indexed_by.clone(),
            redefines: item.redefines.clone(),
            renames,
            screen_info: None,
            justified: item.justified,
            span: item.span,
        });
    }

    // Emit INDEXED BY names as Index-typed data items.
    if let Some(ref occurs) = item.occurs {
        for idx_name in &occurs.indexed_by {
            out.push(HirDataItem::new(
                idx_name.clone(),
                HirType::Index,
                occurs.span,
            ));
        }
    }

    // Recursively lower child items (group items)
    let child_usage = item.usage.as_ref().or(inherited_usage);
    let child_sign = item.sign_clause.as_ref().or(inherited_sign);
    for child in &item.children {
        lower_data_item_with_usage(
            child,
            inherited_external || item.is_external,
            child_usage,
            child_sign,
            out,
        );
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
        let validation_values = validation_values_for_item(item);
        let occurs = item.occurs.as_ref().map(|o| o.max);
        let occurs_depending_on = item
            .occurs
            .as_ref()
            .and_then(|o| o.depending_on.as_ref().map(|name| name.name.clone()));

        let renames = item
            .renames
            .as_ref()
            .map(|r| (r.from.name.clone(), r.thru.as_ref().map(|t| t.name.clone())));

        let screen_info = screen_info_for_item(item, true);

        out.push(HirDataItem {
            name: name.clone(),
            data_type,
            picture: item.picture.as_ref().map(|p| p.raw_string.clone()),
            is_numeric_edited: is_numeric_edited_item(item),
            sign: lower_sign_clause(item.sign_clause.as_ref()),
            blank_when_zero: item.blank_when_zero,
            scale_adjustment: picture_scale_adjustment(item),
            is_external: false,
            initial_value,
            validation_values,
            occurs,
            occurs_depending_on,
            indexed_by: Vec::new(),
            redefines: item.redefines.clone(),
            renames,
            screen_info,
            justified: item.justified,
            span: item.span,
        });
    }

    for child in &item.children {
        lower_screen_data_item(child, out);
    }
}

fn screen_info_for_item(item: &DataItem, include_children: bool) -> Option<HirScreenInfo> {
    let has_screen_attrs = item.line_clause.is_some()
        || item.column_clause.is_some()
        || item.blank_screen
        || item.blank_line
        || item.highlight
        || item.reverse_video
        || item.source_field.is_some()
        || item.using_field.is_some();

    if !has_screen_attrs && (!include_children || item.children.is_empty()) {
        return None;
    }

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
}

fn validation_values_for_item(item: &DataItem) -> Vec<HirValidationValue> {
    let mut values = Vec::new();
    if let Some(value) = item.value.as_ref() {
        values.push(HirValidationValue::Single(lower_value_clause(value)));
    }
    for child in &item.children {
        if child.level != 88 {
            continue;
        }
        for condition_value in &child.condition_values {
            for value in &condition_value.values {
                match value {
                    cobol_ast::data_div::ConditionValueItem::Single(lit) => {
                        values.push(HirValidationValue::Single(lower_literal(lit)));
                    }
                    cobol_ast::data_div::ConditionValueItem::Range { from, to } => {
                        values.push(HirValidationValue::Range {
                            from: lower_literal(from),
                            to: lower_literal(to),
                        });
                    }
                }
            }
        }
    }
    values
}

fn determine_hir_type(item: &DataItem) -> HirType {
    determine_hir_type_with_usage(item, None, None)
}

fn determine_hir_type_with_usage(
    item: &DataItem,
    inherited_usage: Option<&Usage>,
    inherited_sign: Option<&SignClause>,
) -> HirType {
    if item.picture.is_none() && !item.children.is_empty() {
        // Group items stay groups even when USAGE is specified on the group.
        // The usage affects descendants semantically, but collapsing the
        // group into a scalar loses nested OCCURS structure needed by codegen.
        let mut members = Vec::new();
        let child_usage = item.usage.as_ref().or(inherited_usage);
        let child_sign = item.sign_clause.as_ref().or(inherited_sign);
        for child in &item.children {
            if child.level == 88 {
                continue;
            }
            let member_name = child
                .name
                .clone()
                .unwrap_or_else(|| SmolStr::from("FILLER"));
            let data_type = determine_hir_type_with_usage(child, child_usage, child_sign);
            let effective_sign = child.sign_clause.as_ref().or(child_sign);
            let initial_value = child.value.as_ref().map(lower_value_clause);
            let validation_values = validation_values_for_item(child);
            let occurs = child.occurs.as_ref().map(|o| o.max);
            let occurs_depending_on = child
                .occurs
                .as_ref()
                .and_then(|o| o.depending_on.as_ref().map(|name| name.name.clone()));
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
                picture: child.picture.as_ref().map(|p| p.raw_string.clone()),
                is_numeric_edited: is_numeric_edited_item(child),
                sign: lower_sign_clause(effective_sign),
                blank_when_zero: child.blank_when_zero,
                scale_adjustment: picture_scale_adjustment(child),
                is_external: child.is_external,
                initial_value,
                validation_values,
                occurs,
                occurs_depending_on,
                indexed_by: indexed_by_child,
                redefines: child.redefines.clone(),
                renames,
                screen_info: screen_info_for_item(child, false),
                justified: child.justified,
                span: child.span,
            });
        }
        let total: u32 = members
            .iter()
            .filter(|m| m.redefines.is_none() && m.renames.is_none())
            .map(|m| {
                let element_size = match &m.data_type {
                    HirType::Alphanumeric { size } => *size,
                    HirType::Numeric { size, .. } => {
                        if m.sign.is_some_and(|sign| sign.separate) {
                            *size + 1
                        } else {
                            *size
                        }
                    }
                    HirType::Group { size, .. } => *size,
                    HirType::Comp3 { size, .. } => (*size + 2) / 2,
                    HirType::Binary { .. } => 8,
                    HirType::Index => 8,
                    HirType::Pointer => 8,
                    HirType::Boolean => 1,
                    HirType::FloatShort => 4,
                    HirType::FloatLong => 8,
                    HirType::FloatExtended => 16,
                    HirType::National { size } => *size * 2,
                };
                element_size * m.occurs.unwrap_or(1)
            })
            .sum();
        return HirType::Group {
            members,
            size: if total == 0 { 1 } else { total },
        };
    }

    // Check USAGE first for special types
    if let Some(usage) = item.usage.as_ref().or(inherited_usage) {
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
                    let decimal_places = effective_decimal_places(item);
                    if decimal_places > 0 {
                        return HirType::Numeric {
                            size: pic.size,
                            decimal_places,
                            is_signed: pic.is_signed,
                        };
                    }
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
            cobol_ast::PictureCategory::NumericEdited => HirType::Alphanumeric { size: pic.size },
            cobol_ast::PictureCategory::Numeric => HirType::Numeric {
                size: pic.size,
                decimal_places: effective_decimal_places(item),
                is_signed: pic.is_signed,
            },
            cobol_ast::PictureCategory::National | cobol_ast::PictureCategory::NationalEdited => {
                HirType::National { size: pic.size }
            }
            _ => HirType::Alphanumeric { size: pic.size },
        }
    } else {
        // Default: single character alphanumeric
        HirType::Alphanumeric { size: 1 }
    }
}

fn lower_sign_clause(clause: Option<&SignClause>) -> Option<HirSignClause> {
    let clause = clause?;
    let position = match clause.position {
        SignPosition::Leading => HirSignPosition::Leading,
        SignPosition::Trailing => HirSignPosition::Trailing,
    };
    Some(HirSignClause {
        position,
        separate: clause.separate,
    })
}

fn is_numeric_edited_item(item: &DataItem) -> bool {
    item.picture
        .as_ref()
        .is_some_and(|pic| pic.category == cobol_ast::PictureCategory::NumericEdited)
}

fn effective_decimal_places(item: &DataItem) -> u32 {
    let Some(pic) = &item.picture else {
        return 0;
    };
    if pic.decimal_positions > 0 {
        return pic.decimal_positions;
    }
    let adjustment = picture_scale_adjustment(item);
    if adjustment < 0 {
        pic.size + (-adjustment) as u32
    } else {
        0
    }
}

fn picture_scale_adjustment(item: &DataItem) -> i32 {
    let Some(pic) = &item.picture else {
        return 0;
    };
    let raw = pic.raw_string.to_ascii_uppercase();
    let mut seen_digit = false;
    let mut adjustment = 0;
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'9' | b'Z' | b'*' => {
                seen_digit = true;
                parse_repeat_count_for_picture_scale(bytes, &mut i);
            }
            b'P' => {
                let count = parse_repeat_count_for_picture_scale(bytes, &mut i) as i32;
                if seen_digit {
                    adjustment += count;
                } else {
                    adjustment -= count;
                }
            }
            _ => {}
        }
        i += 1;
    }
    adjustment
}

fn parse_repeat_count_for_picture_scale(bytes: &[u8], i: &mut usize) -> u32 {
    if *i + 1 < bytes.len() && bytes[*i + 1] == b'(' {
        let start = *i + 2;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b')' {
            end += 1;
        }
        if end < bytes.len() {
            *i = end;
            let s = std::str::from_utf8(&bytes[start..end]).unwrap_or("1");
            return s.parse().unwrap_or(1);
        }
    }
    1
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
    condition_names: &ConditionNameMap,
) -> (Vec<HirStatement>, Vec<HirParagraph>, u32) {
    let mut next_paragraph_id = 1u32;
    let mut alloc_paragraph_id = || {
        let id = HirParagraphId(next_paragraph_id);
        next_paragraph_id += 1;
        id
    };
    let mut top_level_plans = Vec::with_capacity(proc.paragraphs.len());
    for para in &proc.paragraphs {
        if para.name.is_empty() {
            top_level_plans.push(None);
        } else {
            top_level_plans.push(Some(ParagraphPlan {
                id: alloc_paragraph_id(),
                name: para.name.clone(),
                kind: HirParagraphKind::Paragraph,
                section_id: None,
                segment_number: None,
                span: para.span,
            }));
        }
    }

    let mut section_plans = Vec::with_capacity(proc.sections.len());
    let mut seen_para_names: HashSet<SmolStr> = top_level_plans
        .iter()
        .flatten()
        .map(|plan| plan.name.clone())
        .collect();
    let mut seen_effective_names = seen_para_names.clone();
    for section in &proc.sections {
        let section_id = alloc_paragraph_id();
        let entry = ParagraphPlan {
            id: section_id,
            name: section.name.clone(),
            kind: HirParagraphKind::Section,
            section_id: None,
            segment_number: section.segment_number,
            span: section.span,
        };
        let mut paragraph_plans = Vec::with_capacity(section.paragraphs.len());
        for para in &section.paragraphs {
            let effective_name = if seen_para_names.contains(&para.name) {
                let base: SmolStr = format!("{}--{}", section.name, para.name).into();
                if seen_effective_names.insert(base.clone()) {
                    base
                } else {
                    let mut counter = 2usize;
                    loop {
                        let candidate: SmolStr =
                            format!("{}--{}--{}", section.name, para.name, counter).into();
                        if seen_effective_names.insert(candidate.clone()) {
                            break candidate;
                        }
                        counter += 1;
                    }
                }
            } else {
                seen_para_names.insert(para.name.clone());
                let base = para.name.clone();
                seen_effective_names.insert(base.clone());
                base
            };
            paragraph_plans.push(ParagraphPlan {
                id: alloc_paragraph_id(),
                name: effective_name,
                kind: HirParagraphKind::Paragraph,
                section_id: Some(section_id),
                segment_number: section.segment_number,
                span: para.span,
            });
        }
        section_plans.push(SectionPlan {
            entry,
            paragraphs: paragraph_plans,
        });
    }

    let mut transfer_targets = HashMap::new();
    for plan in top_level_plans.iter().flatten() {
        transfer_targets.insert(
            plan.name.clone(),
            HirTransferTarget::Paragraph {
                id: plan.id,
                name: plan.name.clone(),
            },
        );
    }
    for (section_src, section) in proc.sections.iter().zip(section_plans.iter()) {
        transfer_targets.insert(
            section.entry.name.clone(),
            HirTransferTarget::Paragraph {
                id: section.entry.id,
                name: section.entry.name.clone(),
            },
        );
        for plan in &section.paragraphs {
            transfer_targets.insert(
                plan.name.clone(),
                HirTransferTarget::Paragraph {
                    id: plan.id,
                    name: plan.name.clone(),
                },
            );
        }
        for (para_src, plan) in section_src.paragraphs.iter().zip(section.paragraphs.iter()) {
            let qualified_name: SmolStr =
                format!("{}--{}", section.entry.name, para_src.name).into();
            transfer_targets.insert(
                qualified_name,
                HirTransferTarget::Paragraph {
                    id: plan.id,
                    name: plan.name.clone(),
                },
            );
        }
    }

    let named_paragraph_ids: HashSet<HirParagraphId> = top_level_plans
        .iter()
        .flatten()
        .map(|plan| plan.id)
        .chain(section_plans.iter().map(|section| section.entry.id))
        .chain(
            section_plans
                .iter()
                .flat_map(|section| section.paragraphs.iter().map(|plan| plan.id)),
        )
        .collect();

    let mut body = Vec::new();
    let mut paragraphs = Vec::new();
    with_transfer_targets(transfer_targets, || {
        for (para, plan) in proc.paragraphs.iter().zip(top_level_plans.iter()) {
            let stmts = lower_paragraph(para, condition_names);
            if let Some(plan) = plan {
                let local_stmts = truncate_paragraph_body(&stmts, plan.id, &named_paragraph_ids);
                paragraphs.push(HirParagraph {
                    id: plan.id,
                    name: plan.name.clone(),
                    kind: plan.kind,
                    section_id: plan.section_id,
                    segment_number: plan.segment_number,
                    body: local_stmts.clone(),
                    span: plan.span,
                });
                body.push(HirStatement::Label {
                    target: HirTransferTarget::Paragraph {
                        id: plan.id,
                        name: plan.name.clone(),
                    },
                });
                body.extend(local_stmts);
            } else {
                body.extend(stmts);
            }
        }

        for (section, plan) in proc.sections.iter().zip(section_plans.iter()) {
            body.push(HirStatement::Label {
                target: HirTransferTarget::Paragraph {
                    id: plan.entry.id,
                    name: plan.entry.name.clone(),
                },
            });
            paragraphs.push(HirParagraph {
                id: plan.entry.id,
                name: plan.entry.name.clone(),
                kind: plan.entry.kind,
                section_id: None,
                segment_number: plan.entry.segment_number,
                body: Vec::new(),
                span: plan.entry.span,
            });
            for (para, para_plan) in section.paragraphs.iter().zip(plan.paragraphs.iter()) {
                let stmts = lower_paragraph(para, condition_names);
                let local_stmts =
                    truncate_paragraph_body(&stmts, para_plan.id, &named_paragraph_ids);
                body.push(HirStatement::Label {
                    target: HirTransferTarget::Paragraph {
                        id: para_plan.id,
                        name: para_plan.name.clone(),
                    },
                });
                body.extend(local_stmts.clone());
                paragraphs.push(HirParagraph {
                    id: para_plan.id,
                    name: para_plan.name.clone(),
                    kind: para_plan.kind,
                    section_id: para_plan.section_id,
                    segment_number: para_plan.segment_number,
                    body: local_stmts,
                    span: para_plan.span,
                });
            }
        }
    });

    (body, paragraphs, next_paragraph_id)
}

fn truncate_paragraph_body(
    stmts: &[HirStatement],
    current_id: HirParagraphId,
    named_paragraph_ids: &HashSet<HirParagraphId>,
) -> Vec<HirStatement> {
    let mut body = Vec::new();
    for stmt in stmts {
        if let HirStatement::Label { target } = stmt {
            if let Some(id) = target.paragraph_id() {
                if id != current_id && named_paragraph_ids.contains(&id) {
                    break;
                }
            }
        }
        body.push(stmt.clone());
    }
    body
}

fn lower_paragraph(para: &Paragraph, condition_names: &ConditionNameMap) -> Vec<HirStatement> {
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
fn lower_statements(stmts: &[Statement], condition_names: &ConditionNameMap) -> Vec<HirStatement> {
    stmts
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect()
}

fn lower_statement(stmt: &Statement, condition_names: &ConditionNameMap) -> Option<HirStatement> {
    match stmt {
        Statement::Display(display) => Some(lower_display(display)),
        Statement::Accept(accept) => Some(lower_accept(accept)),
        Statement::Enable(enable) => Some(lower_enable(enable)),
        Statement::Disable(disable) => Some(lower_disable(disable)),
        Statement::Send(send) => Some(lower_send(send)),
        Statement::Receive(receive) => Some(lower_receive(receive, condition_names)),
        Statement::Purge(purge) => Some(lower_purge(purge)),
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
        Statement::StopLiteral(expr) => Some(HirStatement::StopLiteral {
            operand: lower_expr(expr),
            span: Span::new(0, 0, cobol_common::FileId(0)),
        }),
        Statement::Alter(alter) => Some(HirStatement::Alter {
            pairs: alter
                .pairs
                .iter()
                .map(|(from, to)| (resolve_transfer_target(from), resolve_transfer_target(to)))
                .collect(),
            span: alter.span,
        }),
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
        let (from_name, from_subscripts) = match &mv.from {
            Expr::Identifier(qname) => (
                resolve_canonical_data_name(qname),
                qname.subscripts.iter().map(lower_expr).collect(),
            ),
            _ => (HirDataName::simple("FILLER"), Vec::new()),
        };
        let (to_name, to_subscripts) = mv
            .to
            .first()
            .map(|e| match e {
                Expr::Identifier(qname) => (
                    resolve_canonical_data_name(qname),
                    qname.subscripts.iter().map(lower_expr).collect(),
                ),
                _ => (HirDataName::simple("FILLER"), Vec::new()),
            })
            .unwrap_or_else(|| (HirDataName::simple("FILLER"), Vec::new()));
        return HirStatement::MoveCorresponding {
            from: from_name,
            from_subscripts,
            to: to_name,
            to_subscripts,
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
            let var_name = lower_data_name(qname);
            let subscripts: Vec<_> = qname.subscripts.iter().map(lower_expr).collect();
            build_data_ref(var_name.clone(), subscripts.clone(), None)
                .map(HirMoveTarget::DataRef)
                .unwrap_or_else(|| {
                    if qname.subscripts.is_empty() {
                        HirMoveTarget::Variable(var_name)
                    } else {
                        HirMoveTarget::Subscript {
                            variable: var_name,
                            subscripts,
                        }
                    }
                })
        }
        Expr::ReferenceModification {
            variable,
            start,
            length,
            ..
        } => {
            let subscripts: Vec<_> = variable.subscripts.iter().map(lower_expr).collect();
            let variable = lower_data_name(variable);
            let start = lower_expr(start);
            let length = length.as_ref().map(|expr| lower_expr(expr));
            let refmod = HirRefMod {
                start: Box::new(start.clone()),
                length: length.clone().map(Box::new),
            };
            build_data_ref(variable.clone(), subscripts, Some(refmod))
                .map(HirMoveTarget::DataRef)
                .unwrap_or(HirMoveTarget::ReferenceModification {
                    variable,
                    start,
                    length,
                })
        }
        _ => {
            // Fallback: should not happen for well-formed MOVE targets
            HirMoveTarget::Variable(HirDataName::simple("FILLER"))
        }
    }
}

fn lower_compute(
    compute: &ComputeStatement,
    condition_names: &ConditionNameMap,
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
    let target_rounded = compute.targets.iter().map(|t| t.rounded).collect();
    let on_size_error = lower_statements(&compute.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&compute.not_on_size_error, condition_names);
    Some(HirStatement::Compute {
        targets,
        target_rounded,
        expr,
        on_size_error,
        not_on_size_error,
        span: compute.span,
    })
}

fn lower_add(add: &AddStatement, condition_names: &ConditionNameMap) -> HirStatement {
    if add.corresponding {
        // ADD CORRESPONDING: source is first operand (group), target is first TO (group).
        let from_name = match &add.operands[0] {
            Expr::Identifier(qname) => resolve_canonical_data_name(qname),
            _ => HirDataName::simple("FILLER"),
        };
        let to_name = add
            .to
            .first()
            .map(|t| resolve_canonical_data_name(&t.target))
            .unwrap_or_else(|| HirDataName::simple("FILLER"));
        let rounded = add.to.first().is_some_and(|t| t.rounded);
        let on_size_error = lower_statements(&add.on_size_error, condition_names);
        let not_on_size_error = lower_statements(&add.not_on_size_error, condition_names);
        return HirStatement::AddCorresponding {
            from: from_name,
            to: to_name,
            rounded,
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
    let to_rounded = add.to.iter().map(|t| t.rounded).collect();
    let giving = add
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let giving_rounded = add.giving.iter().map(|t| t.rounded).collect();
    let on_size_error = lower_statements(&add.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&add.not_on_size_error, condition_names);
    HirStatement::Add {
        operands,
        to,
        to_rounded,
        giving,
        giving_rounded,
        on_size_error,
        not_on_size_error,
        span: add.span,
    }
}

fn lower_subtract(sub: &SubtractStatement, condition_names: &ConditionNameMap) -> HirStatement {
    if sub.corresponding {
        // SUBTRACT CORRESPONDING: source is first operand (group), target is first FROM (group).
        let from_name = match &sub.operands[0] {
            Expr::Identifier(qname) => resolve_canonical_data_name(qname),
            _ => HirDataName::simple("FILLER"),
        };
        let to_name = sub
            .from
            .first()
            .map(|t| resolve_canonical_data_name(&t.target))
            .unwrap_or_else(|| HirDataName::simple("FILLER"));
        let rounded = sub.from.first().is_some_and(|t| t.rounded);
        let on_size_error = lower_statements(&sub.on_size_error, condition_names);
        let not_on_size_error = lower_statements(&sub.not_on_size_error, condition_names);
        return HirStatement::SubtractCorresponding {
            from: from_name,
            to: to_name,
            rounded,
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
    let from_rounded = if sub.from_expr.is_some() {
        vec![false]
    } else {
        sub.from.iter().map(|t| t.rounded).collect()
    };
    let giving = sub
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let giving_rounded = sub.giving.iter().map(|t| t.rounded).collect();
    let on_size_error = lower_statements(&sub.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&sub.not_on_size_error, condition_names);
    HirStatement::Subtract {
        operands,
        from,
        from_rounded,
        giving,
        giving_rounded,
        on_size_error,
        not_on_size_error,
        span: sub.span,
    }
}

fn lower_if(if_stmt: &IfStatement, condition_names: &ConditionNameMap) -> HirStatement {
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
fn lower_evaluate(eval: &EvaluateStatement, condition_names: &ConditionNameMap) -> HirStatement {
    // Build nested IF chain from the WHEN clauses
    let mut else_body: Vec<HirStatement> = eval
        .when_other
        .iter()
        .filter_map(|s| lower_statement(s, condition_names))
        .collect();

    let mut effective_whens = Vec::new();
    let mut pending_objects = Vec::new();
    for when in &eval.when_clauses {
        pending_objects.push(when.objects.clone());
        if !when.body.is_empty() {
            effective_whens.push((pending_objects, &when.body, when.span));
            pending_objects = Vec::new();
        }
    }

    // Process WHEN clauses in reverse to build the else chain.
    for (object_alternatives, body, span) in effective_whens.into_iter().rev() {
        let then_body: Vec<HirStatement> = body
            .iter()
            .filter_map(|s| lower_statement(s, condition_names))
            .collect();

        let condition = object_alternatives
            .iter()
            .map(|objects| build_evaluate_condition(&eval.subjects, objects, condition_names))
            .reduce(|acc, condition| HirCondition::Or(Box::new(acc), Box::new(condition)))
            .unwrap_or(HirCondition::Compare {
                left: HirExpr::Literal(HirLiteral::Integer(1)),
                op: HirCompareOp::Eq,
                right: HirExpr::Literal(HirLiteral::Integer(1)),
            });

        let if_stmt = HirStatement::If {
            condition,
            then_body,
            else_body,
            span,
        };

        else_body = vec![if_stmt];
    }

    // The result is the outermost IF (or the first element of else_body)
    if else_body.len() == 1 {
        else_body.remove(0)
    } else {
        // Wrap in an inline PERFORM if multiple statements
        HirStatement::Perform {
            kind: Box::new(HirPerformKind::Inline { body: else_body }),
            span: eval.span,
        }
    }
}

fn build_evaluate_condition(
    subjects: &[cobol_ast::statement::EvaluateSubject],
    object_groups: &[Vec<cobol_ast::statement::WhenObject>],
    condition_names: &ConditionNameMap,
) -> HirCondition {
    use cobol_ast::statement::{EvaluateSubject, WhenObject};

    // For each subject/object pair, build a condition and AND them together
    let mut conditions: Vec<HirCondition> = Vec::new();

    for (i, objects) in object_groups.iter().enumerate() {
        let (subject_expr, subject_condition) = if i < subjects.len() {
            match &subjects[i] {
                EvaluateSubject::Expr(e) => (Some(lower_expr(e)), None),
                EvaluateSubject::True => (
                    None,
                    Some(HirCondition::Compare {
                        left: HirExpr::Literal(HirLiteral::Integer(1)),
                        op: HirCompareOp::Eq,
                        right: HirExpr::Literal(HirLiteral::Integer(1)),
                    }),
                ),
                EvaluateSubject::False => (
                    None,
                    Some(HirCondition::Compare {
                        left: HirExpr::Literal(HirLiteral::Integer(1)),
                        op: HirCompareOp::Eq,
                        right: HirExpr::Literal(HirLiteral::Integer(0)),
                    }),
                ),
                EvaluateSubject::Condition(c) => (None, Some(lower_condition(c, condition_names))),
            }
        } else {
            (None, None)
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
                    let object_condition = lower_condition(c, condition_names);
                    if let Some(subject_condition) = &subject_condition {
                        conditions.push(match subjects.get(i) {
                            Some(EvaluateSubject::False) => {
                                HirCondition::Not(Box::new(object_condition))
                            }
                            _ => HirCondition::And(
                                Box::new(subject_condition.clone()),
                                Box::new(object_condition),
                            ),
                        });
                    } else {
                        conditions.push(object_condition);
                    }
                }
                WhenObject::True => {
                    if let Some(subject_condition) = &subject_condition {
                        conditions.push(subject_condition.clone());
                    }
                }
                WhenObject::False => {
                    if let Some(subject_condition) = &subject_condition {
                        conditions.push(HirCondition::Not(Box::new(subject_condition.clone())));
                    }
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

fn lower_perform(perform: &PerformStatement, condition_names: &ConditionNameMap) -> HirStatement {
    let kind = match &perform.kind {
        PerformKind::Simple { body } => {
            let hir_body: Vec<_> = body
                .iter()
                .filter_map(|s| lower_statement(s, condition_names))
                .collect();
            HirPerformKind::Inline { body: hir_body }
        }
        PerformKind::ProcedureName { procedure, through } => HirPerformKind::ProcedureName {
            target: resolve_transfer_target(procedure),
            through: through.as_ref().map(resolve_transfer_target),
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
            test,
            condition,
            body,
        } => {
            let hir_cond = lower_condition(condition, condition_names);
            let hir_body: Vec<_> = body
                .iter()
                .filter_map(|s| lower_statement(s, condition_names))
                .collect();
            HirPerformKind::Until {
                test: lower_perform_test(*test),
                condition: hir_cond,
                body: hir_body,
            }
        }
        PerformKind::Varying {
            test,
            varying,
            body,
        } => {
            if let Some(clause) = varying.first() {
                let var = clause.identifier.name.clone();
                let var_expr = lower_qualified_name_to_expr(&clause.identifier);
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
                        var_expr: lower_qualified_name_to_expr(&c.identifier),
                        from: lower_expr(&c.from),
                        by: lower_expr(&c.by),
                        until: lower_condition(&c.until, condition_names),
                    })
                    .collect();
                HirPerformKind::Varying {
                    test: lower_perform_test(*test),
                    var,
                    var_expr,
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
        kind: Box::new(kind),
        span: perform.span,
    }
}

fn lower_perform_test(test: PerformTest) -> HirPerformTest {
    match test {
        PerformTest::Before => HirPerformTest::Before,
        PerformTest::After => HirPerformTest::After,
    }
}

fn lower_call(call: &CallStatement, condition_names: &ConditionNameMap) -> HirStatement {
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
    let mut on_exception = lower_statements(&call.on_exception, condition_names);
    if !call.on_overflow.is_empty() {
        on_exception.extend(lower_statements(&call.on_overflow, condition_names));
    }
    let not_on_exception = lower_statements(&call.not_on_exception, condition_names);
    HirStatement::Call {
        program,
        params,
        on_exception,
        not_on_exception,
        span: call.span,
    }
}

fn lower_multiply(mul: &MultiplyStatement, condition_names: &ConditionNameMap) -> HirStatement {
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
    let by_rounded: Vec<bool> = if mul.by_expr.is_some() {
        vec![false]
    } else {
        mul.by.iter().map(|t| t.rounded).collect()
    };
    let giving = mul
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let giving_rounded = mul.giving.iter().map(|t| t.rounded).collect();
    let on_size_error = lower_statements(&mul.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&mul.not_on_size_error, condition_names);
    HirStatement::Multiply {
        operand,
        by,
        by_rounded,
        giving,
        giving_rounded,
        on_size_error,
        not_on_size_error,
        span: mul.span,
    }
}

fn lower_divide(div: &DivideStatement, condition_names: &ConditionNameMap) -> HirStatement {
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
    let into_rounded = if div.into_expr.is_some() {
        vec![false]
    } else {
        div.into.iter().map(|t| t.rounded).collect()
    };
    let giving = div
        .giving
        .iter()
        .map(|t| lower_qualified_name_to_expr(&t.target))
        .collect();
    let giving_rounded = div.giving.iter().map(|t| t.rounded).collect();
    let remainder = div.remainder.as_ref().map(lower_qualified_name_to_expr);
    let on_size_error = lower_statements(&div.on_size_error, condition_names);
    let not_on_size_error = lower_statements(&div.not_on_size_error, condition_names);
    HirStatement::Divide {
        operand,
        into,
        into_rounded,
        giving,
        giving_rounded,
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
        Some(AstSource::MessageCount) => HirAcceptSource::MessageCount,
        Some(AstSource::Environment(name)) => HirAcceptSource::Environment(name.clone()),
        Some(AstSource::Console) | None => HirAcceptSource::Console,
    };
    HirStatement::Accept {
        target: lower_qualified_name_to_expr(&accept.target),
        source,
        span: accept.span,
    }
}

fn lower_communication_mode(mode: CommunicationMode) -> HirCommunicationMode {
    match mode {
        CommunicationMode::Input => HirCommunicationMode::Input,
        CommunicationMode::Output => HirCommunicationMode::Output,
        CommunicationMode::InputOutput => HirCommunicationMode::InputOutput,
    }
}

fn lower_send_option(option: &SendOption) -> HirSendOption {
    match option {
        SendOption::Emi => HirSendOption::Emi,
        SendOption::Egi => HirSendOption::Egi,
        SendOption::Esi => HirSendOption::Esi,
        SendOption::Identifier(expr) => HirSendOption::Identifier(lower_expr(expr)),
    }
}

fn lower_enable(enable: &EnableStatement) -> HirStatement {
    HirStatement::Enable {
        mode: lower_communication_mode(enable.mode),
        terminal: enable.terminal,
        target: enable.target.name.clone(),
        key: lower_expr(&enable.key),
        span: enable.span,
    }
}

fn lower_disable(disable: &DisableStatement) -> HirStatement {
    HirStatement::Disable {
        mode: lower_communication_mode(disable.mode),
        terminal: disable.terminal,
        target: disable.target.name.clone(),
        key: lower_expr(&disable.key),
        span: disable.span,
    }
}

fn lower_send(send: &SendStatement) -> HirStatement {
    HirStatement::Send {
        target: send.target.name.clone(),
        from: send.from.as_ref().map(lower_expr),
        with: send.with.as_ref().map(lower_send_option),
        replacing_line: send.replacing_line,
        span: send.span,
    }
}

fn lower_receive(receive: &ReceiveStatement, condition_names: &ConditionNameMap) -> HirStatement {
    HirStatement::Receive {
        target: receive.target.name.clone(),
        mode: match receive.mode {
            cobol_ast::statement::ReceiveMode::Message => HirReceiveMode::Message,
            cobol_ast::statement::ReceiveMode::Segment => HirReceiveMode::Segment,
        },
        into: receive.into.name.clone(),
        no_data: receive
            .no_data
            .iter()
            .filter_map(|stmt| lower_statement(stmt, condition_names))
            .collect(),
        span: receive.span,
    }
}

fn lower_purge(purge: &PurgeStatement) -> HirStatement {
    HirStatement::Purge {
        target: purge.target.name.clone(),
        span: purge.span,
    }
}

fn lower_goto(goto: &GoToStatement) -> HirStatement {
    let targets = goto.targets.iter().map(resolve_transfer_target).collect();
    let depending_on = goto.depending_on.as_ref().map(lower_qualified_name_to_expr);
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
                optional: false,               // will be updated post-lowering
                organization: 1,               // will be updated post-lowering
                access_mode: 0,                // will be updated post-lowering
                record_key: None,              // will be updated post-lowering
                alternate_keys: Vec::new(),    // will be updated post-lowering
                relative_key: None,            // will be updated post-lowering
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
    let close_options = close
        .files
        .iter()
        .map(|entry| {
            entry.close_option.map(|option| match option {
                cobol_ast::statement::CloseOption::Reel => HirCloseOption::Reel,
                cobol_ast::statement::CloseOption::Unit => HirCloseOption::Unit,
                cobol_ast::statement::CloseOption::WithNoRewind => HirCloseOption::WithNoRewind,
                cobol_ast::statement::CloseOption::WithLock => HirCloseOption::WithLock,
            })
        })
        .collect();
    HirStatement::Close {
        files,
        close_options,
        span: close.span,
    }
}

fn lower_read(
    read: &cobol_ast::statement::ReadStatement,
    condition_names: &ConditionNameMap,
) -> HirStatement {
    let into = read.into.as_ref().map(|q| {
        let subs: Vec<_> = q.subscripts.iter().map(lower_expr).collect();
        (q.name.clone(), subs)
    });
    let key = read.key.as_ref().map(key_data_name);
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
    let invalid_key = lower_statements(&read.invalid_key, condition_names);
    let not_invalid_key = lower_statements(&read.not_invalid_key, condition_names);
    HirStatement::Read {
        file_name: read.file_name.clone(),
        is_next: read.is_next,
        into,
        key,
        at_end,
        not_at_end,
        invalid_key,
        not_invalid_key,
        span: read.span,
    }
}

fn lower_write(
    write: &cobol_ast::statement::WriteStatement,
    condition_names: &ConditionNameMap,
) -> HirStatement {
    let from = write.from.as_ref().map(lower_expr);
    let advancing = write.advancing.as_ref().map(|advancing| match advancing {
        cobol_ast::statement::WriteAdvancing::Lines(expr) => {
            HirWriteAdvancing::Lines(lower_expr(expr))
        }
        cobol_ast::statement::WriteAdvancing::Page => HirWriteAdvancing::Page,
        cobol_ast::statement::WriteAdvancing::MnemonicName(name) => {
            HirWriteAdvancing::Lines(HirExpr::Variable(name.clone().into()))
        }
    });
    let invalid_key = lower_statements(&write.invalid_key, condition_names);
    let not_invalid_key = lower_statements(&write.not_invalid_key, condition_names);
    let at_eop = lower_statements(&write.at_eop, condition_names);
    let not_at_eop = lower_statements(&write.not_at_eop, condition_names);
    HirStatement::Write {
        record_name: write.record_name.name.clone(),
        file_name: SmolStr::default(), // resolved post-lowering
        from,
        advancing,
        invalid_key,
        not_invalid_key,
        at_eop,
        not_at_eop,
        span: write.span,
    }
}

fn lower_rewrite(rewrite: &cobol_ast::statement::RewriteStatement) -> HirStatement {
    let from = rewrite.from.as_ref().map(lower_expr);
    let condition_names = HashMap::new();
    let invalid_key = lower_statements(&rewrite.invalid_key, &condition_names);
    let not_invalid_key = lower_statements(&rewrite.not_invalid_key, &condition_names);
    HirStatement::Rewrite {
        record_name: rewrite.record_name.name.clone(),
        file_name: SmolStr::default(), // resolved post-lowering
        from,
        invalid_key,
        not_invalid_key,
        span: rewrite.span,
    }
}

fn lower_delete(delete: &cobol_ast::statement::DeleteStatement) -> HirStatement {
    let condition_names = HashMap::new();
    let invalid_key = lower_statements(&delete.invalid_key, &condition_names);
    let not_invalid_key = lower_statements(&delete.not_invalid_key, &condition_names);
    HirStatement::Delete {
        file_name: delete.file_name.clone(),
        invalid_key,
        not_invalid_key,
        span: delete.span,
    }
}

fn lower_initialize(init: &InitializeStatement) -> HirStatement {
    let targets = init.targets.iter().map(|q| q.name.clone()).collect();
    let replacing = init
        .replacing
        .iter()
        .map(|entry| HirInitializeReplacing {
            category: match entry.category {
                cobol_ast::statement::InitializeCategory::Alphabetic => {
                    HirInitializeCategory::Alphabetic
                }
                cobol_ast::statement::InitializeCategory::Alphanumeric => {
                    HirInitializeCategory::Alphanumeric
                }
                cobol_ast::statement::InitializeCategory::Numeric => HirInitializeCategory::Numeric,
                cobol_ast::statement::InitializeCategory::AlphanumericEdited => {
                    HirInitializeCategory::AlphanumericEdited
                }
                cobol_ast::statement::InitializeCategory::NumericEdited => {
                    HirInitializeCategory::NumericEdited
                }
                cobol_ast::statement::InitializeCategory::National => {
                    HirInitializeCategory::National
                }
                cobol_ast::statement::InitializeCategory::NationalEdited => {
                    HirInitializeCategory::NationalEdited
                }
            },
            value: lower_expr(&entry.value),
        })
        .collect();
    HirStatement::Initialize {
        targets,
        replacing,
        span: init.span,
    }
}

fn lower_set(set: &SetStatement, condition_names: &ConditionNameMap) -> HirStatement {
    use cobol_ast::statement::SetKind;
    match &set.kind {
        SetKind::To { targets, value } => {
            let target_exprs = targets.iter().map(lower_qualified_name_to_expr).collect();
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
                    to_rounded: Vec::new(),
                    giving: Vec::new(),
                    giving_rounded: Vec::new(),
                    on_size_error: Vec::new(),
                    not_on_size_error: Vec::new(),
                    span: set.span,
                },
                cobol_ast::statement::SetDirection::Down => HirStatement::Subtract {
                    operands: vec![hir_value],
                    from: target_exprs,
                    from_rounded: Vec::new(),
                    giving: Vec::new(),
                    giving_rounded: Vec::new(),
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
            let mut assignments: Vec<(HirMoveTarget, HirExpr)> = Vec::new();
            for cond_qn in conditions {
                let cond_name = lower_data_name(cond_qn);
                if let Some(info) = resolve_condition_name(&cond_name, condition_names) {
                    let target = build_data_ref(info.parent_name.clone(), Vec::new(), None)
                        .map(HirMoveTarget::DataRef)
                        .unwrap_or_else(|| HirMoveTarget::Variable(info.parent_name.clone()));
                    // Use the first value of the condition-name
                    let hir_value = if let Some(first_cv) = info.values.first() {
                        match first_cv {
                            ConditionValue::Single(lit) => HirExpr::Literal(lit.clone()),
                            ConditionValue::Range { from, .. } => HirExpr::Literal(from.clone()),
                        }
                    } else {
                        HirExpr::Literal(HirLiteral::Integer(1))
                    };
                    assignments.push((target, hir_value));
                } else {
                    let fallback_name: HirDataName = cond_name;
                    let target = build_data_ref(fallback_name.clone(), Vec::new(), None)
                        .map(HirMoveTarget::DataRef)
                        .unwrap_or(HirMoveTarget::Variable(fallback_name));
                    assignments.push((target, HirExpr::Literal(HirLiteral::Integer(1))));
                }
            }
            HirStatement::SetConditionTrue {
                assignments,
                span: set.span,
            }
        }
        SetKind::SwitchStatus { assignments } => HirStatement::SetSwitchStatus {
            assignments: assignments
                .iter()
                .map(|(target, value)| (target.name.clone(), *value))
                .collect(),
            span: set.span,
        },
        SetKind::Address { target, source } => HirStatement::SetAddress {
            target: target.name.clone(),
            source: source.name.clone(),
            span: set.span,
        },
    }
}

fn lower_string_stmt(
    string_stmt: &cobol_ast::statement::StringStatement,
    condition_names: &ConditionNameMap,
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
    let not_on_overflow = lower_statements(&string_stmt.not_on_overflow, condition_names);
    HirStatement::StringStmt {
        into: string_stmt.into.name.clone(),
        sources,
        pointer: string_stmt.pointer.as_ref().map(|name| name.name.clone()),
        on_overflow,
        not_on_overflow,
        span: string_stmt.span,
    }
}

fn lower_unstring_stmt(
    unstring_stmt: &cobol_ast::statement::UnstringStatement,
    condition_names: &ConditionNameMap,
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
        .map(|t| HirUnstringTarget {
            target: t.target.name.clone(),
            delimiter_in: t.delimiter_in.as_ref().map(|name| name.name.clone()),
            count_in: t.count_in.as_ref().map(|name| name.name.clone()),
        })
        .collect();
    let on_overflow = lower_statements(&unstring_stmt.on_overflow, condition_names);
    let not_on_overflow = lower_statements(&unstring_stmt.not_on_overflow, condition_names);
    HirStatement::UnstringStmt {
        source: unstring_stmt.source.name.clone(),
        delimiters,
        into,
        pointer: unstring_stmt.pointer.as_ref().map(|name| name.name.clone()),
        tallying: unstring_stmt
            .tallying
            .as_ref()
            .map(|name| name.name.clone()),
        on_overflow,
        not_on_overflow,
        span: unstring_stmt.span,
    }
}

fn lower_search(
    search: &cobol_ast::statement::SearchStatement,
    condition_names: &ConditionNameMap,
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
        cobol_ast::statement::InspectKind::Converting {
            from,
            to,
            before_after,
        } => HirInspectKind::Converting {
            from: lower_expr(from),
            to: lower_expr(to),
            before_after: before_after.iter().map(lower_before_after).collect(),
        },
    };
    HirStatement::Inspect {
        target: lower_qualified_name_to_expr(&inspect.target),
        kind,
        span: inspect.span,
    }
}

fn lower_inspect_tallying(t: &cobol_ast::statement::InspectTallying) -> HirInspectTallying {
    let kind = match &t.kind {
        cobol_ast::statement::TallyingKind::Characters => HirTallyingKind::Characters,
        cobol_ast::statement::TallyingKind::All(e) => HirTallyingKind::All(lower_expr(e)),
        cobol_ast::statement::TallyingKind::Leading(e) => HirTallyingKind::Leading(lower_expr(e)),
        cobol_ast::statement::TallyingKind::Trailing(e) => HirTallyingKind::Trailing(lower_expr(e)),
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
    condition_names: &ConditionNameMap,
) -> HirStatement {
    let key = start
        .key_condition
        .as_ref()
        .map(|kc| key_data_name(&kc.key));
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
    condition_names: &ConditionNameMap,
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

/// Produce a structured data name from a `QualifiedName`.
fn lower_data_name(qname: &cobol_ast::expr::QualifiedName) -> HirDataName {
    HirDataName::new(qname.name.clone(), qname.qualifiers.clone())
}

fn resolve_canonical_data_name(qname: &cobol_ast::expr::QualifiedName) -> HirDataName {
    let name = lower_data_name(qname);
    resolve_data_name(&name)
        .map(|resolved| resolved.name)
        .unwrap_or(name)
}

/// Lower a `QualifiedName` (used as an arithmetic target) to a `HirExpr`.
/// Handles subscripts so that `TABLE(IDX)` becomes `HirExpr::Subscript`.
fn lower_qualified_name_to_expr(qname: &cobol_ast::expr::QualifiedName) -> HirExpr {
    let var_name = lower_data_name(qname);
    let subscripts: Vec<_> = qname.subscripts.iter().map(lower_expr).collect();
    build_data_ref(var_name.clone(), subscripts.clone(), None)
        .map(HirExpr::DataRef)
        .unwrap_or_else(|| {
            if qname.subscripts.is_empty() {
                HirExpr::Variable(var_name)
            } else {
                HirExpr::Subscript {
                    variable: var_name,
                    subscripts,
                }
            }
        })
}

// ---------------------------------------------------------------------------
// Expression and condition lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &Expr) -> HirExpr {
    match expr {
        Expr::Literal(lit) => HirExpr::Literal(lower_literal(lit)),
        Expr::Identifier(qname) => lower_qualified_name_to_expr(qname),
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
        } => {
            let variable = lower_data_name(variable);
            let subscripts = match expr {
                Expr::ReferenceModification { variable, .. } => {
                    variable.subscripts.iter().map(lower_expr).collect()
                }
                _ => Vec::new(),
            };
            let start_expr = lower_expr(start);
            let length_expr = length.as_ref().map(|expr| lower_expr(expr));
            let refmod = HirRefMod {
                start: Box::new(start_expr.clone()),
                length: length_expr.clone().map(Box::new),
            };
            build_data_ref(variable.clone(), subscripts, Some(refmod))
                .map(HirExpr::DataRef)
                .unwrap_or(HirExpr::ReferenceModification {
                    variable,
                    start: Box::new(start_expr),
                    length: length_expr.map(Box::new),
                })
        }
    }
}

fn lower_condition(cond: &Condition, condition_names: &ConditionNameMap) -> HirCondition {
    match cond {
        Condition::Comparison {
            left, op, right, ..
        } => {
            // Check if right side is a condition name used in abbreviated context.
            // e.g. "IF CONT-E EQUAL TO ZEROS AND GREATERZERO" where GREATERZERO
            // is an 88-level condition name. The parser treats "AND GREATERZERO"
            // as an abbreviated comparison (inheriting EQUAL operator), but the
            // right operand is actually a condition name, not a data item.
            if let Expr::Identifier(ref qn) = right {
                if condition_names.contains_key(&qn.name) {
                    return lower_condition(&Condition::ConditionName(qn.clone()), condition_names);
                }
            }
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
                ClassType::Custom(name) => HirClassType::Custom(name.clone()),
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
            let condition_name = lower_data_name(qname);
            if let Some(info) = resolve_condition_name(&condition_name, condition_names) {
                let subscripts: Vec<_> = qname.subscripts.iter().map(lower_expr).collect();
                let parent_expr =
                    build_data_ref(info.parent_name.clone(), subscripts.clone(), None)
                        .map(HirExpr::DataRef)
                        .unwrap_or_else(|| {
                            if qname.subscripts.is_empty() {
                                HirExpr::Variable(info.parent_name.clone())
                            } else {
                                HirExpr::Subscript {
                                    variable: info.parent_name.clone(),
                                    subscripts,
                                }
                            }
                        });
                let conditions: Vec<HirCondition> = info
                    .values
                    .iter()
                    .map(|v| match v {
                        ConditionValue::Single(lit) => HirCondition::Compare {
                            left: parent_expr.clone(),
                            op: HirCompareOp::Eq,
                            right: HirExpr::Literal(lit.clone()),
                        },
                        ConditionValue::Range { from, to } => HirCondition::And(
                            Box::new(HirCondition::Compare {
                                left: parent_expr.clone(),
                                op: HirCompareOp::Ge,
                                right: HirExpr::Literal(from.clone()),
                            }),
                            Box::new(HirCondition::Compare {
                                left: parent_expr.clone(),
                                op: HirCompareOp::Le,
                                right: HirExpr::Literal(to.clone()),
                            }),
                        ),
                    })
                    .collect();
                if conditions.is_empty() {
                    // No values found, fall back to the parent storage item.
                    HirCondition::Compare {
                        left: parent_expr,
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
                let name: HirDataName = qname.name.clone().into();
                let left = build_data_ref(name.clone(), Vec::new(), None)
                    .map(HirExpr::DataRef)
                    .unwrap_or(HirExpr::Variable(name));
                HirCondition::Compare {
                    left,
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

fn extract_file_optionals(program: &CobolProgram) -> std::collections::HashSet<SmolStr> {
    let Some(env) = &program.environment else {
        return std::collections::HashSet::new();
    };
    let Some(io) = &env.input_output else {
        return std::collections::HashSet::new();
    };
    io.file_controls
        .iter()
        .filter(|fc| fc.optional)
        .map(|fc| fc.file_name.clone())
        .collect()
}

fn extract_open_metadata(program: &CobolProgram) -> OpenMetadataMap {
    use cobol_ast::AccessMode;
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .map(|fc| {
            let access_mode = match fc.access_mode {
                Some(AccessMode::Random) => 1,
                Some(AccessMode::Dynamic) => 2,
                Some(AccessMode::Sequential) | None => 0,
            };
            (
                fc.file_name.clone(),
                (
                    access_mode,
                    fc.record_key.as_ref().map(key_data_name),
                    fc.alternate_keys
                        .iter()
                        .map(|q| HirAlternateKey {
                            name: key_data_name(&q.name),
                            duplicates: q.duplicates,
                        })
                        .collect(),
                    fc.relative_key.as_ref().map(key_data_name),
                    fc.optional,
                ),
            )
        })
        .collect()
}

fn extract_relative_keys(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .filter_map(|fc| {
            fc.relative_key
                .as_ref()
                .map(|q| (fc.file_name.clone(), q.name.clone()))
        })
        .collect()
}

fn extract_read_metadata(program: &CobolProgram) -> ReadMetadataMap {
    use cobol_ast::{AccessMode, FileOrganization};

    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .map(|fc| {
            let organization = match fc.organization {
                Some(FileOrganization::Sequential) => 0,
                Some(FileOrganization::LineSequential) | None => 1,
                Some(FileOrganization::Relative) => 2,
                Some(FileOrganization::Indexed) => 3,
            };
            let access_mode = match fc.access_mode {
                Some(AccessMode::Random) => 1,
                Some(AccessMode::Dynamic) => 2,
                Some(AccessMode::Sequential) | None => 0,
            };
            (
                fc.file_name.clone(),
                (
                    organization,
                    access_mode,
                    fc.record_key.as_ref().map(key_data_name),
                    fc.relative_key.as_ref().map(key_data_name),
                ),
            )
        })
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

fn extract_same_record_areas(program: &CobolProgram) -> Vec<Vec<SmolStr>> {
    program
        .environment
        .as_ref()
        .and_then(|env| env.input_output.as_ref())
        .map(|io| io.same_record_areas.clone())
        .unwrap_or_default()
}

fn extract_variable_record_files(program: &CobolProgram) -> std::collections::HashSet<SmolStr> {
    let Some(data) = &program.data else {
        return std::collections::HashSet::new();
    };
    data.file_section
        .iter()
        .filter(|fd| {
            fd.record_varying.is_some()
                || fd
                    .record_contains
                    .as_ref()
                    .is_some_and(|record| record.min.is_some())
                || fd_has_multiple_record_sizes(fd)
        })
        .map(|fd| fd.file_name.clone())
        .collect()
}

fn fd_has_multiple_record_sizes(fd: &FileDescription) -> bool {
    let mut sizes = fd.items.iter().filter(|item| item.level == 1).map(|item| {
        hir_type_record_size(&determine_hir_type_with_usage(item, None, None))
            * item.occurs.as_ref().map_or(1, |occurs| occurs.max)
    });
    let Some(first_size) = sizes.next() else {
        return false;
    };
    sizes.any(|size| size != first_size)
}

fn hir_type_record_size(data_type: &HirType) -> u32 {
    match data_type {
        HirType::Alphanumeric { size } => *size,
        HirType::Numeric { size, .. } => *size,
        HirType::Group { size, .. } => *size,
        HirType::Comp3 { size, .. } => (*size + 2) / 2,
        HirType::Binary { .. } => 8,
        HirType::Index => 8,
        HirType::Pointer => 8,
        HirType::Boolean => 1,
        HirType::FloatShort => 4,
        HirType::FloatLong => 8,
        HirType::FloatExtended => 16,
        HirType::National { size } => *size * 2,
    }
}

fn extract_variable_record_depending(program: &CobolProgram) -> HashMap<SmolStr, SmolStr> {
    let Some(data) = &program.data else {
        return HashMap::new();
    };
    data.file_section
        .iter()
        .filter_map(|fd| {
            fd.record_varying
                .as_ref()
                .and_then(|varying| varying.depending_on.as_ref())
                .map(|depending| (fd.file_name.clone(), depending.clone()))
        })
        .collect()
}

fn extract_variable_record_bounds(program: &CobolProgram) -> HashMap<SmolStr, (u32, u32)> {
    let Some(data) = &program.data else {
        return HashMap::new();
    };
    data.file_section
        .iter()
        .filter_map(|fd| {
            let varying = fd.record_varying.as_ref()?;
            let mut item_sizes =
                fd.items.iter().filter(|item| item.level == 1).map(|item| {
                    hir_type_record_size(&determine_hir_type_with_usage(item, None, None))
                });
            let first_size = item_sizes.next();
            let (inferred_min, inferred_max) = if let Some(first_size) = first_size {
                item_sizes.fold((first_size, first_size), |(min, max), size| {
                    (min.min(size), max.max(size))
                })
            } else {
                (1, 1)
            };
            let min = varying.min.unwrap_or(inferred_min);
            let max = varying.max.unwrap_or(inferred_max);
            Some((fd.file_name.clone(), (min, max)))
        })
        .collect()
}

fn patch_open_entries(
    stmts: &mut [HirStatement],
    org_map: &HashMap<SmolStr, u32>,
    assign_map: &HashMap<SmolStr, SmolStr>,
    open_meta_map: &OpenMetadataMap,
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
                if let Some((access_mode, record_key, alternate_keys, relative_key, optional)) =
                    open_meta_map.get(&entry.file_name)
                {
                    entry.access_mode = *access_mode;
                    entry.record_key = record_key.clone();
                    entry.alternate_keys = alternate_keys.clone();
                    entry.relative_key = relative_key.clone();
                    entry.optional = *optional;
                }
            }
        }
    }
}

fn patch_read_keys(stmts: &mut [HirStatement], read_meta_map: &ReadMetadataMap) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStatement::Read {
                file_name,
                is_next,
                key,
                ..
            } => {
                if *is_next || key.is_some() {
                    continue;
                }
                if let Some((organization, access_mode, record_key, relative_key)) =
                    read_meta_map.get(file_name)
                {
                    match (*organization, *access_mode) {
                        (3, 1 | 2) => *key = record_key.clone(),
                        (2, 1 | 2) => *key = relative_key.clone(),
                        _ => {}
                    }
                }
            }
            HirStatement::If {
                then_body,
                else_body,
                ..
            } => {
                patch_read_keys(then_body, read_meta_map);
                patch_read_keys(else_body, read_meta_map);
            }
            HirStatement::Perform { kind, .. } => {
                let body = match kind.as_mut() {
                    HirPerformKind::Inline { body } => body.as_mut_slice(),
                    HirPerformKind::Times { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Until { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Varying { body, .. } => body.as_mut_slice(),
                    HirPerformKind::ProcedureName { .. } => &mut [],
                };
                patch_read_keys(body, read_meta_map);
            }
            _ => {}
        }
    }
}

fn patch_start_keys(stmts: &mut [HirStatement], read_meta_map: &ReadMetadataMap) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStatement::Start { file_name, key, .. } => {
                if key.is_some() {
                    continue;
                }
                if let Some((organization, _, record_key, relative_key)) =
                    read_meta_map.get(file_name)
                {
                    match *organization {
                        3 => *key = record_key.clone(),
                        2 => *key = relative_key.clone(),
                        _ => {}
                    }
                }
            }
            HirStatement::If {
                then_body,
                else_body,
                ..
            } => {
                patch_start_keys(then_body, read_meta_map);
                patch_start_keys(else_body, read_meta_map);
            }
            HirStatement::Perform { kind, .. } => {
                let body = match kind.as_mut() {
                    HirPerformKind::Inline { body } => body.as_mut_slice(),
                    HirPerformKind::Times { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Until { body, .. } => body.as_mut_slice(),
                    HirPerformKind::Varying { body, .. } => body.as_mut_slice(),
                    HirPerformKind::ProcedureName { .. } => &mut [],
                };
                patch_start_keys(body, read_meta_map);
            }
            _ => {}
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
                invalid_key,
                not_invalid_key,
                at_eop,
                not_at_eop,
                ..
            } => {
                if let Some(fn_name) = rec_map.get(record_name.as_str()) {
                    *file_name = fn_name.clone();
                }
                patch_write_file_names(invalid_key, rec_map);
                patch_write_file_names(not_invalid_key, rec_map);
                patch_write_file_names(at_eop, rec_map);
                patch_write_file_names(not_at_eop, rec_map);
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
                let body = match kind.as_mut() {
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
                Some(FileOrganization::Relative) => 2,
                Some(FileOrganization::Indexed) => 3,
            };
            (fc.file_name.clone(), org)
        })
        .collect()
}

fn extract_file_access_modes(program: &CobolProgram) -> HashMap<SmolStr, u32> {
    use cobol_ast::AccessMode;
    let Some(env) = &program.environment else {
        return HashMap::new();
    };
    let Some(io) = &env.input_output else {
        return HashMap::new();
    };
    io.file_controls
        .iter()
        .map(|fc| {
            let access = match fc.access_mode {
                Some(AccessMode::Random) => 1,
                Some(AccessMode::Dynamic) => 2,
                Some(AccessMode::Sequential) | None => 0,
            };
            (fc.file_name.clone(), access)
        })
        .collect()
}

/// Lower DECLARATIVES sections from the PROCEDURE DIVISION.
///
/// Returns `(declaratives, extra_paragraphs)` where `extra_paragraphs` are the
/// section entries and individual paragraphs defined inside each declarative
/// section. These must be appended to the program's `paragraphs` list so that
/// declarative section names and PERFORM references from the declarative body
/// (e.g. `PERFORM DECL-PASS`) are preserved in generated C.
fn lower_declaratives(
    program: &CobolProgram,
    condition_names: &ConditionNameMap,
    next_paragraph_id_start: u32,
    debugging_mode_enabled: bool,
) -> (Vec<HirDeclarative>, Vec<HirParagraph>) {
    let Some(proc) = &program.procedure else {
        return (Vec::new(), Vec::new());
    };
    let mut decls = Vec::new();
    let mut extra_paras = Vec::new();
    let mut next_paragraph_id = next_paragraph_id_start;
    let mut seen_decl_para_names = std::collections::HashSet::new();
    let mut seen_effective_names = std::collections::HashSet::new();
    let mut section_plans = Vec::new();
    let mut transfer_targets = HashMap::new();

    for decl in &proc.declaratives {
        let section_id = HirParagraphId(next_paragraph_id);
        next_paragraph_id += 1;
        let entry = ParagraphPlan {
            id: section_id,
            name: decl.name.clone(),
            kind: HirParagraphKind::Section,
            section_id: None,
            segment_number: None,
            span: decl.span,
        };
        transfer_targets.insert(
            entry.name.clone(),
            HirTransferTarget::Paragraph {
                id: entry.id,
                name: entry.name.clone(),
            },
        );

        let mut paragraph_plans = Vec::with_capacity(decl.paragraphs.len());
        for para in &decl.paragraphs {
            if para.name.is_empty() {
                continue;
            }

            let effective_name = if seen_decl_para_names.contains(&para.name) {
                let base: SmolStr = format!("{}--{}", decl.name, para.name).into();
                if seen_effective_names.insert(base.clone()) {
                    base
                } else {
                    let mut counter = 2usize;
                    loop {
                        let candidate: SmolStr =
                            format!("{}--{}--{}", decl.name, para.name, counter).into();
                        if seen_effective_names.insert(candidate.clone()) {
                            break candidate;
                        }
                        counter += 1;
                    }
                }
            } else {
                seen_decl_para_names.insert(para.name.clone());
                let base = para.name.clone();
                seen_effective_names.insert(base.clone());
                base
            };

            let id = HirParagraphId(next_paragraph_id);
            next_paragraph_id += 1;
            transfer_targets.insert(
                effective_name.clone(),
                HirTransferTarget::Paragraph {
                    id,
                    name: effective_name.clone(),
                },
            );
            paragraph_plans.push(ParagraphPlan {
                id,
                name: effective_name,
                kind: HirParagraphKind::Paragraph,
                section_id: Some(section_id),
                segment_number: None,
                span: para.span,
            });
        }

        section_plans.push(SectionPlan {
            entry,
            paragraphs: paragraph_plans,
        });
    }

    with_transfer_targets(transfer_targets.clone(), || {
        for (decl, plan) in proc.declaratives.iter().zip(section_plans.iter()) {
            extra_paras.push(HirParagraph {
                id: plan.entry.id,
                name: plan.entry.name.clone(),
                kind: plan.entry.kind,
                section_id: None,
                segment_number: plan.entry.segment_number,
                body: Vec::new(),
                span: plan.entry.span,
            });

            let body: Vec<HirStatement> = decl
                .paragraphs
                .iter()
                .flat_map(|para| lower_paragraph(para, condition_names))
                .collect();
            match &decl.use_statement {
                UseStatement::AfterException {
                    file_names,
                    is_global,
                } => {
                    decls.push(HirDeclarative {
                        name: decl.name.clone(),
                        use_kind: HirDeclarativeUse::AfterException,
                        is_global: *is_global,
                        file_names: file_names.clone(),
                        debug_items: Vec::new(),
                        body,
                    });
                }
                UseStatement::ForDebugging { debug_items } => {
                    if !debugging_mode_enabled {
                        continue;
                    }
                    decls.push(HirDeclarative {
                        name: decl.name.clone(),
                        use_kind: HirDeclarativeUse::ForDebugging,
                        is_global: false,
                        file_names: Vec::new(),
                        debug_items: debug_items.clone(),
                        body,
                    });
                }
                UseStatement::BeforeReporting { .. } => {}
            }

            for (para, para_plan) in decl.paragraphs.iter().zip(plan.paragraphs.iter()) {
                let stmts = lower_paragraph(para, condition_names);
                extra_paras.push(HirParagraph {
                    id: para_plan.id,
                    name: para_plan.name.clone(),
                    kind: para_plan.kind,
                    section_id: para_plan.section_id,
                    segment_number: para_plan.segment_number,
                    body: stmts,
                    span: para_plan.span,
                });
            }
        }
    });
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
        HirExpr::DataRef(data_ref) => {
            for sub in data_ref.subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            if let Some(refmod) = &mut data_ref.refmod {
                fix_subscripts_in_expr(&mut refmod.start, occurs_dims);
                if let Some(length) = &mut refmod.length {
                    fix_subscripts_in_expr(length, occurs_dims);
                }
            }
            let var_upper = SmolStr::new(data_ref.name.name.to_uppercase());
            let expected = occurs_dims
                .get(&var_upper)
                .or_else(|| occurs_dims.get(data_ref.name.name.as_str()))
                .copied()
                .unwrap_or(0);
            if expected > data_ref.subscripts.len() {
                let missing = expected - data_ref.subscripts.len();
                let mut new_subs = Vec::new();
                let mut remaining = missing;
                for sub in data_ref.subscripts.iter() {
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
                    data_ref.subscripts = new_subs;
                }
            }
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            // First, recursively fix inner subscript expressions
            for sub in subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            // Check if we need to split subscripts
            let var_upper = SmolStr::new(variable.name.to_uppercase());
            let expected = occurs_dims
                .get(&var_upper)
                .or_else(|| occurs_dims.get(variable.name.as_str()))
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
    match target {
        HirMoveTarget::DataRef(data_ref) => {
            for sub in data_ref.subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            if let Some(refmod) = &mut data_ref.refmod {
                fix_subscripts_in_expr(&mut refmod.start, occurs_dims);
                if let Some(length) = &mut refmod.length {
                    fix_subscripts_in_expr(length, occurs_dims);
                }
            }
            let var_upper = SmolStr::new(data_ref.name.name.to_uppercase());
            let expected = occurs_dims
                .get(&var_upper)
                .or_else(|| occurs_dims.get(data_ref.name.name.as_str()))
                .copied()
                .unwrap_or(0);
            if expected > data_ref.subscripts.len() {
                let missing = expected - data_ref.subscripts.len();
                let mut new_subs = Vec::new();
                let mut remaining = missing;
                for sub in data_ref.subscripts.iter() {
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
                    data_ref.subscripts = new_subs;
                }
            }
        }
        HirMoveTarget::Subscript {
            variable,
            subscripts,
        } => {
            for sub in subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            let var_upper = SmolStr::new(variable.name.to_uppercase());
            let expected = occurs_dims
                .get(&var_upper)
                .or_else(|| occurs_dims.get(variable.name.as_str()))
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
        HirMoveTarget::Variable(_) | HirMoveTarget::ReferenceModification { .. } => {}
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
        HirStatement::MoveCorresponding {
            from_subscripts,
            to_subscripts,
            ..
        } => {
            for sub in from_subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
            }
            for sub in to_subscripts.iter_mut() {
                fix_subscripts_in_expr(sub, occurs_dims);
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
            by_rounded: _,
            giving,
            giving_rounded: _,
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
        HirStatement::StopLiteral { operand, .. } => {
            fix_subscripts_in_expr(operand, occurs_dims);
        }
        HirStatement::Accept { target, .. } => {
            fix_subscripts_in_expr(target, occurs_dims);
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

    fn parse_and_lower_fixed(source: &str) -> HirProgram {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
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
    fn test_lower_relative_key_metadata() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RLTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT RL-FS1 ASSIGN TO \"rl.dat\"
        ORGANIZATION IS RELATIVE
        ACCESS MODE IS SEQUENTIAL
        RELATIVE KEY IS RL-FS1-KEY.
DATA DIVISION.
FILE SECTION.
FD RL-FS1.
01 RL-REC PIC X(10).
WORKING-STORAGE SECTION.
01 RL-FS1-KEY PIC 9(8) COMP.
PROCEDURE DIVISION.
    OPEN INPUT RL-FS1.
    READ RL-FS1.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert_eq!(
            hir.file_relative_keys.get("RL-FS1").map(SmolStr::as_str),
            Some("RL-FS1-KEY")
        );
        let HirStatement::Open { entries, .. } = &hir.body[0] else {
            panic!("Expected OPEN statement");
        };
        assert_eq!(entries[0].organization, 2);
        assert_eq!(entries[0].relative_key.as_deref(), Some("RL-FS1-KEY"));
    }

    #[test]
    fn test_lower_select_optional_metadata() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IXTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT OPTIONAL IX-FS1 ASSIGN TO \"ix.dat\"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY.
DATA DIVISION.
FILE SECTION.
FD IX-FS1.
01 IX-REC.
   05 IX-KEY PIC X(4).
   05 FILLER PIC X(12).
PROCEDURE DIVISION.
    OPEN I-O IX-FS1.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let HirStatement::Open { entries, .. } = &hir.body[0] else {
            panic!("Expected OPEN statement");
        };
        assert!(entries[0].optional);
        assert_eq!(entries[0].organization, 3);
        assert_eq!(entries[0].access_mode, 2);
        assert_eq!(entries[0].record_key.as_deref(), Some("IX-KEY"));
    }

    #[test]
    fn test_lower_start_without_key_uses_indexed_record_key() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IXSTART.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FS1 ASSIGN TO \"ix.dat\"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS SEQUENTIAL
        RECORD KEY IS IX-KEY.
DATA DIVISION.
FILE SECTION.
FD IX-FS1.
01 IX-REC.
   05 IX-KEY PIC X(4).
   05 FILLER PIC X(12).
PROCEDURE DIVISION.
    START IX-FS1.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let HirStatement::Start { key, .. } = &hir.body[0] else {
            panic!("Expected START statement");
        };
        assert_eq!(key.as_deref(), Some("IX-KEY"));
    }

    #[test]
    fn test_lower_qualified_indexed_keys_use_containing_key_area() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IXQUAL.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FS1 ASSIGN TO \"ix.dat\"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY IN IX-REC-KEY-AREA
        ALTERNATE RECORD KEY IS IX-KEY OF IX-ALT-KEY-AREA.
DATA DIVISION.
FILE SECTION.
FD IX-FS1.
01 IX-REC.
   05 IX-REC-KEY-AREA.
      10 IX-KEY PIC X(4).
   05 IX-ALT-KEY-AREA.
      10 IX-KEY PIC X(4).
PROCEDURE DIVISION.
    OPEN I-O IX-FS1.
    READ IX-FS1.
    START IX-FS1 KEY IS EQUAL TO IX-KEY OF IX-ALT-KEY-AREA.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        let HirStatement::Open { entries, .. } = &hir.body[0] else {
            panic!("Expected OPEN statement");
        };
        assert_eq!(entries[0].record_key.as_deref(), Some("IX-REC-KEY-AREA"));
        assert_eq!(
            entries[0].alternate_keys[0].name.as_str(),
            "IX-ALT-KEY-AREA"
        );

        let HirStatement::Read { key, .. } = &hir.body[1] else {
            panic!("Expected READ statement");
        };
        assert_eq!(key.as_deref(), Some("IX-REC-KEY-AREA"));

        let HirStatement::Start { key, .. } = &hir.body[2] else {
            panic!("Expected START statement");
        };
        assert_eq!(key.as_deref(), Some("IX-ALT-KEY-AREA"));
    }

    #[test]
    fn test_lower_marks_fd_with_multiple_record_lengths_as_variable() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VARREC.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SQ-VS7 ASSIGN TO \"sq-vs7.dat\".
DATA DIVISION.
FILE SECTION.
FD SQ-VS7.
01 SHORT-REC PIC X(120).
01 LONG-REC.
   05 LONG-PREFIX PIC X(120).
   05 LONG-SUFFIX PIC X(31).
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(hir.variable_record_files.contains("SQ-VS7"));
    }

    #[test]
    fn test_lower_records_variable_record_bounds() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VARBND.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SQ-VS7 ASSIGN TO \"sq-vs7.dat\".
DATA DIVISION.
FILE SECTION.
FD SQ-VS7
    RECORD IS VARYING IN SIZE FROM 18 TO 2048 CHARACTERS
    DEPENDING ON RECORD-LENGTH.
01 SQ-REC PIC X(2048).
WORKING-STORAGE SECTION.
01 RECORD-LENGTH PIC 9(4).
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert_eq!(
            hir.variable_record_depending
                .get("SQ-VS7")
                .map(SmolStr::as_str),
            Some("RECORD-LENGTH")
        );
        assert_eq!(hir.variable_record_bounds.get("SQ-VS7"), Some(&(18, 2048)));
    }

    #[test]
    fn test_lower_infers_variable_record_bounds_from_fd_records() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VARINF.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SQ-VS7 ASSIGN TO \"sq-vs7.dat\".
DATA DIVISION.
FILE SECTION.
FD SQ-VS7
    RECORD VARYING DEPENDING RECORD-LENGTH.
01 SHORT-REC PIC X(120).
01 LONG-REC.
   05 LONG-PREFIX PIC X(120).
   05 LONG-SUFFIX PIC X(31).
WORKING-STORAGE SECTION.
01 RECORD-LENGTH PIC 9(3).
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert_eq!(hir.variable_record_bounds.get("SQ-VS7"), Some(&(120, 151)));
    }

    #[test]
    fn test_lower_keeps_equal_length_fd_records_fixed() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FIXREC.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SAME-FILE ASSIGN TO \"same.dat\".
DATA DIVISION.
FILE SECTION.
FD SAME-FILE.
01 FIRST-REC PIC X(10).
01 SECOND-REC PIC 9(10).
PROCEDURE DIVISION.
    STOP RUN.
";
        let hir = parse_and_lower(src);
        assert!(!hir.variable_record_files.contains("SAME-FILE"));
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
    fn test_lower_fixed_open_literal_continuation_preserves_margin_spaces() {
        let src = concat!(
            "000100 IDENTIFICATION DIVISION.                                         TST\n",
            "000200 PROGRAM-ID. T.                                                   TST\n",
            "000300 DATA DIVISION.                                                   TST\n",
            "000400 WORKING-STORAGE SECTION.                                         TST\n",
            "000500 01 X PIC X(83).                                                  TST\n",
            "000600 PROCEDURE DIVISION.                                              TST\n",
            "000700     MOVE                                                         TST\n",
            "000800     \"AH YES AH YES W.C                                           TST\n",
            "000900-    \"            BE ALL BAD.\" TO X.                              TST\n",
            "001000     STOP RUN.                                                    TST\n",
        );
        let hir = parse_and_lower_fixed(src);
        if let HirStatement::Move { from, .. } = &hir.body[0] {
            assert_eq!(
                from,
                &HirExpr::Literal(HirLiteral::String(SmolStr::from(format!(
                    "AH YES AH YES W.C{}            BE ALL BAD.",
                    " ".repeat(43)
                ))))
            );
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
                    crate::hir::HirExpr::DataRef(data_ref) => {
                        assert_eq!(data_ref.name.as_str(), "WS-NAME");
                        let refmod = data_ref.refmod.as_ref().expect("expected refmod");
                        assert!(matches!(
                            *refmod.start,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(1))
                        ));
                        assert!(refmod.length.is_some());
                        let len = refmod.length.as_ref().unwrap();
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
                    crate::hir::HirMoveTarget::DataRef(data_ref) => {
                        assert_eq!(data_ref.name.as_str(), "WS-NAME");
                        let refmod = data_ref.refmod.as_ref().expect("expected refmod");
                        assert!(matches!(
                            *refmod.start,
                            crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(3))
                        ));
                        assert!(refmod.length.is_some());
                        let len = refmod.length.as_ref().unwrap();
                        assert!(matches!(
                            **len,
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
                crate::hir::HirExpr::DataRef(data_ref) => {
                    assert_eq!(data_ref.name.as_str(), "WS-NAME");
                    let refmod = data_ref.refmod.as_ref().expect("expected refmod");
                    assert!(matches!(
                        *refmod.start,
                        crate::hir::HirExpr::Literal(crate::hir::HirLiteral::Integer(5))
                    ));
                    assert!(
                        refmod.length.is_none(),
                        "length should be None for start-only ref mod"
                    );
                }
                other => panic!("expected ReferenceModification, got {:?}", other),
            },
            other => panic!("expected Display, got {:?}", other),
        }
    }

    #[test]
    fn test_lower_control_flow_targets_are_resolved_to_paragraph_ids() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOW.
PROCEDURE DIVISION.
    GO TO PARA-2.
PARA-1.
    PERFORM PARA-2 THRU PARA-3.
    STOP RUN.
PARA-2.
    DISPLAY \"TWO\".
PARA-3.
    DISPLAY \"THREE\".
";
        let hir = parse_and_lower(src);
        let para_2 = hir
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.name.as_str() == "PARA-2")
            .expect("expected PARA-2 paragraph");
        let para_3 = hir
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.name.as_str() == "PARA-3")
            .expect("expected PARA-3 paragraph");

        match &hir.body[0] {
            HirStatement::GoTo { targets, .. } => {
                assert_eq!(targets.len(), 1);
                match &targets[0] {
                    HirTransferTarget::Paragraph { id, name } => {
                        assert_eq!(*id, para_2.id);
                        assert_eq!(name.as_str(), "PARA-2");
                    }
                    other => panic!("expected paragraph target, got {:?}", other),
                }
            }
            other => panic!("expected GO TO, got {:?}", other),
        }

        let para_1 = hir
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.name.as_str() == "PARA-1")
            .expect("expected PARA-1 paragraph");
        match &para_1.body[0] {
            HirStatement::Perform { kind, .. } => match kind.as_ref() {
                HirPerformKind::ProcedureName { target, through } => {
                    match target {
                        HirTransferTarget::Paragraph { id, name } => {
                            assert_eq!(*id, para_2.id);
                            assert_eq!(name.as_str(), "PARA-2");
                        }
                        other => panic!("expected paragraph target, got {:?}", other),
                    }
                    match through.as_ref().expect("expected THRU target") {
                        HirTransferTarget::Paragraph { id, name } => {
                            assert_eq!(*id, para_3.id);
                            assert_eq!(name.as_str(), "PARA-3");
                        }
                        other => panic!("expected paragraph THRU target, got {:?}", other),
                    }
                }
                other => panic!("expected procedure-name perform, got {:?}", other),
            },
            other => panic!("expected PERFORM, got {:?}", other),
        }
    }
}
