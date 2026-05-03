// COBOL Code Generation - C output backend
//
// Translates HIR into C source code that calls the COBOL runtime library.
// The generated C code is then compiled with clang/cc and linked against
// the runtime's static library to produce a native executable.

#[path = "compiler.rs"]
mod compiler;
#[path = "context.rs"]
mod context;
#[path = "data.rs"]
mod data;
#[path = "expr.rs"]
mod expr;
#[path = "stmt.rs"]
mod stmt;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use cobol_hir::{
    HirAcceptSource, HirBinOp, HirClassType, HirCompareOp, HirCondition, HirDataItem,
    HirDeclarative, HirDeclarativeUse, HirExpr, HirFileInfo, HirLiteral, HirMoveTarget,
    HirOpenMode, HirParagraph, HirParagraphId, HirParagraphKind, HirPerformKind, HirProgram,
    HirStartRelation, HirStatement, HirTransferTarget, HirType, HirUnaryOp,
};

pub use self::compiler::compile_c_to_executable;
pub(crate) use self::context::*;
pub(crate) use self::data::*;
pub(crate) use self::expr::*;
pub(crate) use self::stmt::*;

/// Compute a fingerprint hash for a group's member structure.
/// Used to disambiguate groups with the same local name but different members.
fn compute_group_fingerprint(members: &[HirDataItem]) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for m in members {
        if m.redefines.is_some() {
            continue;
        }
        m.name.hash(&mut hasher);
        std::mem::discriminant(&m.data_type).hash(&mut hasher);
        m.occurs.hash(&mut hasher);
        match &m.data_type {
            HirType::Alphanumeric { size } => size.hash(&mut hasher),
            HirType::Numeric {
                size,
                decimal_places,
                ..
            } => {
                size.hash(&mut hasher);
                decimal_places.hash(&mut hasher);
            }
            HirType::Group {
                members: sub_members,
                ..
            } => {
                compute_group_fingerprint(sub_members).hash(&mut hasher);
            }
            _ => {}
        }
    }
    (hasher.finish() & 0xFFFF_FFFF) as u32
}

fn group_typedef_name_for_layout(
    c_name: &str,
    members: &[HirDataItem],
    raw_display_layout: bool,
) -> String {
    let fp = compute_group_fingerprint(members);
    let layout = if raw_display_layout { "raw" } else { "val" };
    format!("_grp_{c_name}_{layout}_{fp:08x}_t")
}

fn using_param_signature(program: &HirProgram) -> String {
    if program.using_params.is_empty() {
        return "void".to_string();
    }

    program
        .using_params
        .iter()
        .enumerate()
        .map(|(idx, param)| match param.mode {
            cobol_hir::HirParamMode::ByValue => format!("int64_t _arg{idx}"),
            cobol_hir::HirParamMode::ByReference | cobol_hir::HirParamMode::ByContent => {
                format!("void* _arg{idx}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn using_param_excluded_inits(program: &HirProgram) -> HashSet<String> {
    let mut excluded: HashSet<String> = program
        .using_params
        .iter()
        .map(|param| sanitize_name(&param.name))
        .collect();
    for item in &program.data_items {
        if item.is_external {
            excluded.insert(sanitize_name(&item.name));
        }
    }
    excluded
}

fn emit_using_param_bindings(out: &mut String, program: &HirProgram) {
    if program.using_params.is_empty() {
        return;
    }

    out.push_str("/* PROCEDURE DIVISION USING bindings */\n");
    for param in &program.using_params {
        let c_name = sanitize_name(&param.name);
        out.push_str(&format!("#undef {c_name}\n"));
        match &param.data_type {
            HirType::Alphanumeric { size } => {
                out.push_str(&format!("static char* _link_{c_name} = NULL;\n"));
                out.push_str(&format!(
                    "#define {c_name} (*((char (*)[{len}])_link_{c_name}))\n",
                    len = size + 1
                ));
            }
            HirType::National { size } => {
                out.push_str(&format!("static uint16_t* _link_{c_name} = NULL;\n"));
                out.push_str(&format!(
                    "#define {c_name} (*((uint16_t (*)[{size}])_link_{c_name}))\n"
                ));
            }
            HirType::Group { members, .. } => {
                let raw_display_layout = find_data_item(&c_name, &program.data_items)
                    .is_some_and(|item| group_needs_raw_display_layout(item, &program.data_items));
                let td = group_typedef_name_for_layout(&c_name, members, raw_display_layout);
                let binding_ty = format!("_link_{c_name}_t");
                out.push_str(&format!(
                    "typedef union {{ {td} members; uint8_t _bytes[sizeof({td})]; }} {binding_ty};\n"
                ));
                out.push_str(&format!("static {binding_ty}* _link_{c_name} = NULL;\n"));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
                out.push_str(&format!("static CobolDecimal* _link_{c_name} = NULL;\n"));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::Numeric { .. }
            | HirType::Comp3 { .. }
            | HirType::Binary { .. }
            | HirType::Index => {
                out.push_str(&format!("static int64_t _link_{c_name}_value = 0;\n"));
                out.push_str(&format!(
                    "static int64_t* _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::Pointer => {
                out.push_str(&format!("static void* _link_{c_name}_value = NULL;\n"));
                out.push_str(&format!(
                    "static void** _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::Boolean => {
                out.push_str(&format!("static int8_t _link_{c_name}_value = 0;\n"));
                out.push_str(&format!(
                    "static int8_t* _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::FloatShort => {
                out.push_str(&format!("static float _link_{c_name}_value = 0;\n"));
                out.push_str(&format!(
                    "static float* _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::FloatLong => {
                out.push_str(&format!("static double _link_{c_name}_value = 0;\n"));
                out.push_str(&format!(
                    "static double* _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
            HirType::FloatExtended => {
                out.push_str(&format!("static long double _link_{c_name}_value = 0;\n"));
                out.push_str(&format!(
                    "static long double* _link_{c_name} = &_link_{c_name}_value;\n"
                ));
                out.push_str(&format!("#define {c_name} (*_link_{c_name})\n"));
            }
        }
    }
    out.push('\n');
}

fn emit_using_param_binding_setup(out: &mut String, program: &HirProgram, indent: &str) {
    for (idx, param) in program.using_params.iter().enumerate() {
        let c_name = sanitize_name(&param.name);
        match param.mode {
            cobol_hir::HirParamMode::ByValue => match &param.data_type {
                HirType::Boolean => {
                    out.push_str(&format!(
                        "{indent}_link_{c_name}_value = (int8_t)_arg{idx};\n"
                    ));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
                HirType::FloatShort => {
                    out.push_str(&format!(
                        "{indent}_link_{c_name}_value = (float)_arg{idx};\n"
                    ));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
                HirType::FloatLong => {
                    out.push_str(&format!(
                        "{indent}_link_{c_name}_value = (double)_arg{idx};\n"
                    ));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
                HirType::FloatExtended => {
                    out.push_str(&format!(
                        "{indent}_link_{c_name}_value = (long double)_arg{idx};\n"
                    ));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
                HirType::Pointer => {
                    out.push_str(&format!(
                        "{indent}_link_{c_name}_value = (void*)(uintptr_t)_arg{idx};\n"
                    ));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
                _ => {
                    out.push_str(&format!("{indent}_link_{c_name}_value = _arg{idx};\n"));
                    out.push_str(&format!(
                        "{indent}_link_{c_name} = &_link_{c_name}_value;\n"
                    ));
                }
            },
            cobol_hir::HirParamMode::ByReference | cobol_hir::HirParamMode::ByContent => {
                match &param.data_type {
                    HirType::Alphanumeric { .. } => {
                        out.push_str(&format!("{indent}_link_{c_name} = (char*)_arg{idx};\n"));
                    }
                    HirType::National { .. } => {
                        out.push_str(&format!("{indent}_link_{c_name} = (uint16_t*)_arg{idx};\n"));
                    }
                    HirType::Group { .. } => {
                        out.push_str(&format!(
                            "{indent}_link_{c_name} = (_link_{c_name}_t*)_arg{idx};\n"
                        ));
                    }
                    HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
                        out.push_str(&format!(
                            "{indent}_link_{c_name} = (CobolDecimal*)_arg{idx};\n"
                        ));
                    }
                    HirType::Numeric { .. }
                    | HirType::Comp3 { .. }
                    | HirType::Binary { .. }
                    | HirType::Index => {
                        out.push_str(&format!("{indent}_link_{c_name} = (int64_t*)_arg{idx};\n"));
                    }
                    HirType::Pointer => {
                        out.push_str(&format!("{indent}_link_{c_name} = (void**)_arg{idx};\n"));
                    }
                    HirType::Boolean => {
                        out.push_str(&format!("{indent}_link_{c_name} = (int8_t*)_arg{idx};\n"));
                    }
                    HirType::FloatShort => {
                        out.push_str(&format!("{indent}_link_{c_name} = (float*)_arg{idx};\n"));
                    }
                    HirType::FloatLong => {
                        out.push_str(&format!("{indent}_link_{c_name} = (double*)_arg{idx};\n"));
                    }
                    HirType::FloatExtended => {
                        out.push_str(&format!(
                            "{indent}_link_{c_name} = (long double*)_arg{idx};\n"
                        ));
                    }
                }
            }
        }
    }
}

fn emit_using_param_binding_cleanup(out: &mut String, program: &HirProgram) {
    if program.using_params.is_empty() {
        return;
    }

    for param in &program.using_params {
        let c_name = sanitize_name(&param.name);
        out.push_str(&format!("#undef {c_name}\n"));
    }
    out.push('\n');
}

fn emit_file_declarative_dispatch(
    out: &mut String,
    fn_name: &str,
    declaratives: &[HirDeclarative],
    inherited_global_declaratives: &[HirDeclarative],
) {
    if declaratives.is_empty() && inherited_global_declaratives.is_empty() {
        return;
    }

    out.push_str(&format!(
        "static void {fn_name}(const char* file_c_name, int fs) {{\n"
    ));
    out.push_str("    if (fs == 0) return;\n");
    for decl in declaratives
        .iter()
        .chain(inherited_global_declaratives.iter())
        .filter(|decl| decl.use_kind == HirDeclarativeUse::AfterException)
    {
        let c_decl = sanitize_name(&decl.name);
        let is_mode_based = decl.file_names.iter().any(|f| {
            let upper = f.to_uppercase();
            matches!(upper.as_str(), "I-O" | "INPUT" | "OUTPUT" | "EXTEND")
        });
        if is_mode_based {
            out.push_str(&format!("    decl_{c_decl}(); return;\n"));
        } else {
            for fname in &decl.file_names {
                let c_file = sanitize_name(fname);
                out.push_str(&format!(
                    "    if (strcmp(file_c_name, \"{c_file}\") == 0) {{ decl_{c_decl}(); return; }}\n"
                ));
            }
        }
    }
    out.push_str("}\n\n");
}

macro_rules! cg_timing {
    ($label:expr, $start:expr) => {
        if std::env::var("COBOL_DEBUG_TIMING").as_deref() == Ok("1") {
            eprintln!("[CG-TIMING] {}: {:?}", $label, $start.elapsed());
        }
    };
}

/// Generates C source code from a HIR program.
pub fn generate_c(program: &HirProgram) -> String {
    reset_group_typedef_registry();
    let ctx = CodegenContext::from_program(program);
    with_pushed_context(&ctx, || {
        let mut out = String::new();

        // Header
        emit_header(&mut out);

        // Runtime function declarations
        emit_runtime_declarations(&mut out);

        // Global data items
        let t_data = std::time::Instant::now();
        let top_level_fd_aliases: HashSet<String> = program
            .fd_record_aliases
            .keys()
            .map(sanitize_name)
            .collect();
        emit_data_items(
            &mut out,
            &program.data_items,
            &top_level_fd_aliases,
            &program.fd_record_aliases,
        );
        emit_debug_special_registers(&mut out, program);
        emit_fd_alias_macros(&mut out, &program.data_items, &program.fd_record_aliases);
        emit_using_param_bindings(&mut out, program);
        cg_timing!("emit_data_items", t_data);

        // COBOL 2002+: Emit class definitions (struct + vtable)
        emit_classes(&mut out, &program.classes);

        // COBOL 2002+: Emit user-defined function definitions
        emit_functions(&mut out, &program.functions);

        // COBOL 2014+: Emit type definitions
        emit_typedefs(&mut out, &program.typedefs);

        // COBOL 2023+: Emit interface definitions
        emit_interfaces(&mut out, &program.interfaces);

        // Collect file names used in the program and emit file handle globals
        let file_names = collect_file_names(program);
        if !file_names.is_empty() {
            out.push_str("/* File handles (each COBOL file gets a unique compile-time ID) */\n");
            for (id, name) in file_names.iter().enumerate() {
                let c_name = sanitize_name(name);
                out.push_str(&format!(
                    "static const uint32_t FILE_ID_{c_name} = {};\n",
                    id + 1
                ));
            }
            out.push('\n');
        }

        // Forward-declare CALL targets as weak externs.  If the real
        // sub-program is not linked, the symbol resolves to NULL so CALL ...
        // ON EXCEPTION can observe the missing target.
        let call_targets = collect_call_targets(program);
        if !call_targets.is_empty() {
            out.push_str("/* Weak externs for CALL targets */\n");
            out.push_str(
                "#pragma clang diagnostic push\n\
             #pragma clang diagnostic ignored \"-Wdeprecated-non-prototype\"\n",
            );
            out.push_str("#if defined(__APPLE__)\n");
            for target in &call_targets {
                out.push_str(&format!(
                    "extern void {target}() __attribute__((weak_import));\n"
                ));
            }
            out.push_str("#else\n");
            for target in &call_targets {
                out.push_str(&format!("extern void {target}() __attribute__((weak));\n"));
            }
            out.push_str("#endif\n");
            out.push_str("#pragma clang diagnostic pop\n");
            out.push('\n');
        }

        // Forward-declare nested program entry points
        if !program.nested_programs.is_empty() {
            out.push_str("/* Forward declarations for nested programs */\n");
            for nested in &program.nested_programs {
                let nested_name = sanitize_name(&nested.name);
                let param_sig = using_param_signature(nested);
                out.push_str(&format!("void {nested_name}({param_sig});\n"));
            }
            out.push('\n');
        }

        // Forward-declare paragraph functions
        for para in &program.paragraphs {
            let c_name = sanitize_name(&para.name);
            out.push_str(&format!("static void para_{c_name}(void);\n"));
        }
        if !program.paragraphs.is_empty() {
            out.push('\n');
        }

        // Forward-declare and emit declarative handler functions
        if !program.declaratives.is_empty() {
            for decl in &program.declaratives {
                let c_name = sanitize_name(&decl.name);
                out.push_str(&format!("static void decl_{c_name}(void);\n"));
            }
            out.push('\n');
        }
        emit_file_declarative_dispatch(
            &mut out,
            "_check_file_declarative",
            &program.declaratives,
            &[],
        );

        emit_debug_declarative_support(&mut out, program);

        // XML PARSE support: emit special registers and callback functions
        let xml_procs = collect_xml_parse_procedures(program);
        if !xml_procs.is_empty() {
            out.push_str("/* XML PARSE special registers */\n");
            out.push_str("static char XML_EVENT[30] = {0};\n");
            out.push_str("static char XML_TEXT[1024] = {0};\n\n");

            for proc_name in &xml_procs {
                out.push_str(&format!(
                "static void _xml_cb_{proc_name}(uint32_t event, const uint8_t* name, uint32_t name_len, const uint8_t* value, uint32_t value_len) {{\n"
            ));
                out.push_str("    switch(event) {\n");
                out.push_str("        case 1: strcpy(XML_EVENT, \"START-OF-ELEMENT\"); break;\n");
                out.push_str("        case 2: strcpy(XML_EVENT, \"END-OF-ELEMENT\"); break;\n");
                out.push_str("        case 3: strcpy(XML_EVENT, \"CONTENT-CHARACTERS\"); break;\n");
                out.push_str("        case 4: strcpy(XML_EVENT, \"ATTRIBUTE-NAME\"); break;\n");
                out.push_str("        default: strcpy(XML_EVENT, \"EXCEPTION\"); break;\n");
                out.push_str("    }\n");
                out.push_str("    if (name && name_len > 0) {\n");
                out.push_str("        uint32_t n = name_len < 1023 ? name_len : 1023;\n");
                out.push_str("        memcpy(XML_TEXT, name, n);\n");
                out.push_str("        XML_TEXT[n] = '\\0';\n");
                out.push_str("    } else if (value && value_len > 0) {\n");
                out.push_str("        uint32_t n = value_len < 1023 ? value_len : 1023;\n");
                out.push_str("        memcpy(XML_TEXT, value, n);\n");
                out.push_str("        XML_TEXT[n] = '\\0';\n");
                out.push_str("    } else {\n");
                out.push_str("        XML_TEXT[0] = '\\0';\n");
                out.push_str("    }\n");
                out.push_str(&format!("    para_{proc_name}();\n"));
                out.push_str("}\n\n");
            }
        }

        // Build FILE STATUS variable mapping (file name → status variable C name)
        let fs_map = build_file_status_map(&program.file_status_vars);

        // Collect labels from body and build label ID map for goto dispatch
        let label_map = build_entry_label_map(&program.paragraphs, &program.body);
        let has_labels = !label_map.is_empty();
        with_active_context(|ctx| ctx.set_body_label_map(label_map.clone()));
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        // Shared goto-dispatch state used by main and nested program entry flows.
        out.push_str("static int _goto_target = 0;\n\n");
        emit_alterable_paragraph_state(&mut out, &ctx);

        // Main function
        out.push_str("int main(int argc, char** argv) {\n");

        // Initialize data items
        let t_init = std::time::Instant::now();
        emit_data_init(&mut out, &program.data_items);
        cg_timing!("emit_data_init", t_init);

        let has_decl = !program.declaratives.is_empty();

        // Emit only the top-level paragraph/section entry flow for the main
        // entry point. Fine-grained transfers continue through _goto_target.
        let t_body = std::time::Instant::now();
        let use_top_level_entry_flow = !program.paragraphs.is_empty();
        let body_prefix = top_level_body_prefix(&program.body);
        if !body_prefix.is_empty() {
            with_active_context(|ctx| ctx.set_in_body_context(!use_top_level_entry_flow));
            for stmt in body_prefix {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(&mut out, stmt, &env, 1);
            }
            with_active_context(|ctx| ctx.set_in_body_context(false));
        }
        if !use_top_level_entry_flow {
            with_active_context(|ctx| ctx.set_in_body_context(true));
            for stmt in &program.body[body_prefix.len()..] {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(&mut out, stmt, &env, 1);
            }
            with_active_context(|ctx| ctx.set_in_body_context(false));
        } else {
            emit_top_level_entry_flow(&mut out, &program.paragraphs, &label_map);
        }
        cg_timing!("emit_body_statements", t_body);

        // Falling off the main procedure should behave like STOP RUN so that
        // buffered files are flushed consistently.
        out.push_str("    cobol_stop_run();\n");

        // Emit goto dispatch table if labels exist
        if has_labels && !use_top_level_entry_flow {
            out.push_str("_goto_dispatch:\n");
            out.push_str("    while (_goto_target) {\n");
            out.push_str("        int _t = _goto_target;\n");
            out.push_str("        _goto_target = 0;\n");
            out.push_str("        switch(_t) {\n");
            for paragraph in &program.paragraphs {
                if let Some(id) = label_map.get(&paragraph.id) {
                    let c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!("        case {id}: para_{c_name}(); break;\n"));
                }
            }
            out.push_str("        default: cobol_stop_run();\n");
            out.push_str("        }\n");
            out.push_str("    }\n");
        }

        out.push_str("}\n");

        let t_para = std::time::Instant::now();
        emit_program_paragraph_definitions(&mut out, program, &fs_map, has_decl);
        with_active_context(|ctx| ctx.set_body_label_map(label_map.clone()));
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        cg_timing!("emit_paragraphs", t_para);

        // Emit declarative handler function definitions
        for decl in &program.declaratives {
            let c_name = sanitize_name(&decl.name);
            with_active_context(|ctx| ctx.set_label_map(HashMap::new()));
            out.push_str(&format!("\nstatic void decl_{c_name}(void) {{\n"));
            let is_debug_decl = decl.use_kind == HirDeclarativeUse::ForDebugging;
            let has_decl_entry = program.paragraphs.iter().any(|p| p.name == decl.name);
            if is_debug_decl {
                out.push_str("    int _prev_suppress_debug_event = _suppress_debug_event;\n");
                out.push_str("    _suppress_debug_event = 1;\n");
                with_active_context(|ctx| ctx.set_in_debug_declarative(true));
            }
            if has_decl_entry {
                out.push_str(&format!("    para_{c_name}();\n"));
            } else {
                for stmt in &decl.body {
                    let env = StmtEmitEnv {
                        data_items: &program.data_items,
                        paragraphs: &program.paragraphs,
                        fs_map: &fs_map,
                        has_declaratives: has_decl,
                        ctx: &ctx,
                        current_paragraph: None,
                    };
                    emit_statement_with_ctx(&mut out, stmt, &env, 1);
                }
            }
            if is_debug_decl {
                with_active_context(|ctx| ctx.set_in_debug_declarative(false));
                out.push_str("    _suppress_debug_event = _prev_suppress_debug_event;\n");
            }
            // Declaratives can leave a pending transfer in `_goto_target`,
            // but the actual dispatch loop lives in the owning procedure.
            // Keep a local landing label so emitted `goto _goto_dispatch`
            // statements compile and simply return control to the caller.
            out.push_str("_goto_dispatch:\n");
            out.push_str("    while (_goto_target) {\n");
            out.push_str("      return;\n");
            out.push_str("    }\n");
            out.push_str("}\n");
        }
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        // For sub-programs (those with USING params), emit a callable entry point.
        // This allows other programs to CALL this program by name.
        if !program.using_params.is_empty() {
            let prog_name = sanitize_name(&program.name);
            let param_sig = using_param_signature(program);
            out.push_str(&format!("\nvoid {prog_name}({param_sig}) {{\n"));
            out.push_str("    /* Sub-program entry point */\n");
            emit_using_param_binding_setup(&mut out, program, "    ");
            let excluded_inits = using_param_excluded_inits(program);
            emit_data_init_excluding(&mut out, &program.data_items, &excluded_inits);
            if has_labels && !use_top_level_entry_flow {
                with_active_context(|ctx| ctx.set_label_map(label_map.clone()));
            }
            let use_top_level_entry_flow = !program.paragraphs.is_empty();
            let body_prefix = top_level_body_prefix(&program.body);
            if !body_prefix.is_empty() {
                with_active_context(|ctx| ctx.set_in_body_context(!use_top_level_entry_flow));
                for stmt in body_prefix {
                    let env = StmtEmitEnv {
                        data_items: &program.data_items,
                        paragraphs: &program.paragraphs,
                        fs_map: &fs_map,
                        has_declaratives: has_decl,
                        ctx: &ctx,
                        current_paragraph: None,
                    };
                    emit_statement_with_ctx(&mut out, stmt, &env, 1);
                }
                with_active_context(|ctx| ctx.set_in_body_context(false));
            }
            if !use_top_level_entry_flow {
                with_active_context(|ctx| ctx.set_in_body_context(true));
                for stmt in &program.body[body_prefix.len()..] {
                    let env = StmtEmitEnv {
                        data_items: &program.data_items,
                        paragraphs: &program.paragraphs,
                        fs_map: &fs_map,
                        has_declaratives: has_decl,
                        ctx: &ctx,
                        current_paragraph: None,
                    };
                    emit_statement_with_ctx(&mut out, stmt, &env, 1);
                }
                with_active_context(|ctx| ctx.set_in_body_context(false));
            } else {
                emit_top_level_entry_flow(&mut out, &program.paragraphs, &label_map);
            }
            // Emit goto dispatch table for sub-program if labels exist
            if has_labels && !use_top_level_entry_flow {
                out.push_str("_goto_dispatch:\n");
                out.push_str("    while (_goto_target) {\n");
                out.push_str("        int _t = _goto_target;\n");
                out.push_str("        _goto_target = 0;\n");
                out.push_str("        switch(_t) {\n");
                for paragraph in &program.paragraphs {
                    if let Some(id) = label_map.get(&paragraph.id) {
                        let c_name = sanitize_name(&paragraph.name);
                        out.push_str(&format!("        case {id}: para_{c_name}(); break;\n"));
                    }
                }
                out.push_str("        default: return;\n");
                out.push_str("        }\n");
                out.push_str("    }\n");
            }
            out.push_str("}\n");
            emit_using_param_binding_cleanup(&mut out, program);
        }

        let inherited_global_declaratives: Vec<_> = program
            .declaratives
            .iter()
            .filter(|decl| decl.is_global && decl.use_kind == HirDeclarativeUse::AfterException)
            .cloned()
            .collect();

        // Emit nested programs as separate callable functions
        for nested in &program.nested_programs {
            emit_nested_program(&mut out, nested, &inherited_global_declaratives);
        }

        out
    })
}

fn emit_debug_special_registers(out: &mut String, program: &HirProgram) {
    let needs = collect_debug_register_needs(program);
    let declared: HashSet<String> = program
        .data_items
        .iter()
        .map(|item| sanitize_name(&item.name))
        .collect();
    let mut emitted_any = false;

    for (c_name, needed) in [
        ("DEBUG_LINE", needs.line),
        ("DEBUG_NAME", needs.name),
        ("DEBUG_CONTENTS", needs.contents),
        ("DEBUG_SUB_1", needs.sub_1),
        ("DEBUG_SUB_2", needs.sub_2),
        ("DEBUG_SUB_3", needs.sub_3),
    ] {
        if declared.contains(c_name) || !needed {
            continue;
        }
        if !emitted_any {
            out.push_str("/* Debug special registers */\n");
            emitted_any = true;
        }
        out.push_str(&format!("static char {c_name}[81];\n"));
    }

    if emitted_any {
        out.push('\n');
    }
}

#[derive(Clone, Copy, Default)]
struct DebugRegisterNeeds {
    line: bool,
    name: bool,
    contents: bool,
    sub_1: bool,
    sub_2: bool,
    sub_3: bool,
}

fn has_debug_declaratives(program: &HirProgram) -> bool {
    program
        .declaratives
        .iter()
        .any(|decl| decl.use_kind == HirDeclarativeUse::ForDebugging)
}

fn collect_debug_register_needs(program: &HirProgram) -> DebugRegisterNeeds {
    let hir_dump = format!("{program:#?}");
    let has_debug_decl = has_debug_declaratives(program);
    DebugRegisterNeeds {
        line: has_debug_decl || hir_dump.contains("DEBUG-LINE"),
        name: has_debug_decl || hir_dump.contains("DEBUG-NAME"),
        contents: has_debug_decl || hir_dump.contains("DEBUG-CONTENTS"),
        sub_1: hir_dump.contains("DEBUG-SUB-1"),
        sub_2: hir_dump.contains("DEBUG-SUB-2"),
        sub_3: hir_dump.contains("DEBUG-SUB-3"),
    }
}

fn is_all_procedures_debug_decl(debug_items: &[smol_str::SmolStr]) -> bool {
    debug_items.len() >= 2
        && debug_items[0].eq_ignore_ascii_case("ALL")
        && debug_items[1].eq_ignore_ascii_case("PROCEDURES")
}

fn emit_debug_declarative_support(out: &mut String, program: &HirProgram) {
    if !has_debug_declaratives(program) {
        out.push_str("static void _set_debug_event(const char* name, const char* contents, const char* line) {\n");
        out.push_str("    (void)name;\n");
        out.push_str("    (void)contents;\n");
        out.push_str("    (void)line;\n");
        out.push_str("}\n");
        out.push_str("static void _set_fallthrough_debug_event(const char* name, const char* contents, const char* line) {\n");
        out.push_str("    (void)name;\n");
        out.push_str("    (void)contents;\n");
        out.push_str("    (void)line;\n");
        out.push_str("}\n");
        out.push_str("static void _dispatch_debug_declarative(const char* paragraph_name) {\n");
        out.push_str("    (void)paragraph_name;\n");
        out.push_str("}\n\n");
        return;
    }

    let needs = collect_debug_register_needs(program);
    out.push_str("/* Debug declarative dispatch support */\n");
    out.push_str("static char _debug_event_name[81];\n");
    out.push_str("static char _debug_event_contents[81];\n");
    out.push_str("static char _debug_event_line[81];\n");
    out.push_str("static int _suppress_debug_event = 0;\n");
    out.push_str("static int _debug_event_explicit = 0;\n");
    out.push_str(
        "static void _debug_copy_text_field(char* dst, size_t dst_size, const char* src) {\n",
    );
    out.push_str("    if (!dst || dst_size == 0) return;\n");
    out.push_str("    memset(dst, ' ', dst_size);\n");
    out.push_str("    if (!src) return;\n");
    out.push_str("    size_t n = strlen(src);\n");
    out.push_str("    if (n > dst_size) n = dst_size;\n");
    out.push_str("    memcpy(dst, src, n);\n");
    out.push_str("}\n");
    out.push_str(
        "static void _set_debug_event(const char* name, const char* contents, const char* line) {\n",
    );
    out.push_str("    if (_suppress_debug_event) return;\n");
    out.push_str("    _debug_event_explicit = 1;\n");
    out.push_str("    _debug_copy_text_field(_debug_event_name, sizeof(_debug_event_name), name ? name : \"\");\n");
    out.push_str("    _debug_copy_text_field(_debug_event_contents, sizeof(_debug_event_contents), contents ? contents : \"\");\n");
    out.push_str("    _debug_copy_text_field(_debug_event_line, sizeof(_debug_event_line), line ? line : \"\");\n");
    out.push_str("}\n");
    out.push_str(
        "static void _set_fallthrough_debug_event(const char* name, const char* contents, const char* line) {\n",
    );
    out.push_str("    if (_suppress_debug_event || _debug_event_explicit) return;\n");
    out.push_str("    _debug_copy_text_field(_debug_event_name, sizeof(_debug_event_name), name ? name : \"\");\n");
    out.push_str("    _debug_copy_text_field(_debug_event_contents, sizeof(_debug_event_contents), contents ? contents : \"\");\n");
    out.push_str("    _debug_copy_text_field(_debug_event_line, sizeof(_debug_event_line), line ? line : \"\");\n");
    out.push_str("}\n");
    out.push_str("static void _dispatch_debug_declarative(const char* paragraph_name) {\n");
    out.push_str("    if (_suppress_debug_event) return;\n");
    out.push_str("    if (!paragraph_name || paragraph_name[0] == '\\0') return;\n");
    out.push_str("    const char* _debug_switch = getenv(\"COBOL_DEBUGGING_MODE\");\n");
    out.push_str("    if (_debug_switch && (strcmp(_debug_switch, \"0\") == 0 || strcmp(_debug_switch, \"OFF\") == 0 || strcmp(_debug_switch, \"off\") == 0 || strcmp(_debug_switch, \"false\") == 0 || strcmp(_debug_switch, \"FALSE\") == 0)) { _debug_event_explicit = 0; return; }\n");
    for decl in &program.declaratives {
        if decl.use_kind != HirDeclarativeUse::ForDebugging {
            continue;
        }
        let c_decl = sanitize_name(&decl.name);
        if is_all_procedures_debug_decl(&decl.debug_items) {
            emit_debug_declarative_dispatch_call(out, &c_decl, needs);
            continue;
        }
        for debug_item in &decl.debug_items {
            let upper = debug_item.to_uppercase();
            if matches!(
                upper.as_str(),
                "ALL" | "PROCEDURES" | "REFERENCES" | "OF" | "IN"
            ) {
                continue;
            }
            let escaped = escape_c_string(debug_item);
            out.push_str(&format!(
                "    if (strcmp(paragraph_name, \"{escaped}\") == 0) {{\n"
            ));
            emit_debug_declarative_dispatch_call(out, &c_decl, needs);
            out.push_str("    }\n");
        }
    }
    out.push_str("    _debug_event_explicit = 0;\n");
    out.push_str("}\n\n");
}

fn emit_debug_declarative_dispatch_call(out: &mut String, c_decl: &str, needs: DebugRegisterNeeds) {
    if needs.line {
        out.push_str(
            "        _debug_copy_text_field(DEBUG_LINE, sizeof(DEBUG_LINE), _debug_event_line);\n",
        );
    }
    if needs.name {
        out.push_str(
            "        _debug_copy_text_field(DEBUG_NAME, sizeof(DEBUG_NAME), _debug_event_name);\n",
        );
    }
    if needs.contents {
        out.push_str(
            "        _debug_copy_text_field(DEBUG_CONTENTS, sizeof(DEBUG_CONTENTS), _debug_event_contents);\n",
        );
    }
    out.push_str(&format!("        decl_{c_decl}();\n"));
    out.push_str("        return;\n");
}

/// Emit a nested (contained) program as a callable C function.
fn emit_nested_program(
    out: &mut String,
    program: &HirProgram,
    inherited_global_declaratives: &[HirDeclarative],
) {
    let prog_name = sanitize_name(&program.name);
    let ctx = with_active_context(|parent| CodegenContext::merged_with_program(parent, program));
    with_pushed_context(&ctx, || {
        let nested_data_names = collect_top_level_data_item_c_names(program);
        for c_name in &nested_data_names {
            out.push_str(&format!("#undef {c_name}\n"));
            out.push_str(&format!("#define {c_name} {prog_name}__{c_name}\n"));
        }
        if !nested_data_names.is_empty() {
            out.push('\n');
        }

        for para in &program.paragraphs {
            let c_name = sanitize_name(&para.name);
            out.push_str(&format!("#undef para_{c_name}\n"));
            out.push_str(&format!(
                "#define para_{c_name} para_{prog_name}__{c_name}\n"
            ));
        }
        if !program.paragraphs.is_empty() {
            out.push('\n');
        }

        for decl in &program.declaratives {
            let c_name = sanitize_name(&decl.name);
            out.push_str(&format!("#undef decl_{c_name}\n"));
            out.push_str(&format!(
                "#define decl_{c_name} decl_{prog_name}__{c_name}\n"
            ));
        }
        if !program.declaratives.is_empty() {
            out.push('\n');
        }

        // Emit data items as function-scope statics
        let nested_fd_aliases: HashSet<String> = program
            .fd_record_aliases
            .keys()
            .map(sanitize_name)
            .collect();
        emit_data_items(
            out,
            &program.data_items,
            &nested_fd_aliases,
            &program.fd_record_aliases,
        );
        emit_fd_alias_macros(out, &program.data_items, &program.fd_record_aliases);
        emit_using_param_bindings(out, program);

        for nested in &program.nested_programs {
            let nested_name = sanitize_name(&nested.name);
            let param_sig = using_param_signature(nested);
            out.push_str(&format!("void {nested_name}({param_sig});\n"));
        }
        if !program.nested_programs.is_empty() {
            out.push('\n');
        }

        // Forward-declare paragraph functions for this nested program.
        // Use the same para_{name} convention; if names collide with the parent,
        // the nested definition overrides (last definition wins for static fns).
        for para in &program.paragraphs {
            let c_name = sanitize_name(&para.name);
            out.push_str(&format!("static void para_{c_name}(void);\n"));
        }
        if !program.paragraphs.is_empty() {
            out.push('\n');
        }

        if !program.declaratives.is_empty() {
            for decl in &program.declaratives {
                let c_name = sanitize_name(&decl.name);
                out.push_str(&format!("static void decl_{c_name}(void);\n"));
            }
            out.push('\n');
        }

        emit_file_declarative_dispatch(
            out,
            ctx.file_declarative_dispatch_fn(),
            &program.declaratives,
            inherited_global_declaratives,
        );

        let fs_map = build_file_status_map(&program.file_status_vars);
        let label_map = build_entry_label_map(&program.paragraphs, &program.body);
        let has_decl =
            !program.declaratives.is_empty() || !inherited_global_declaratives.is_empty();
        if !label_map.is_empty() {
            with_active_context(|ctx| ctx.set_body_label_map(label_map.clone()));
            with_active_context(|ctx| ctx.set_label_map(label_map.clone()));
        }
        emit_alterable_paragraph_state(out, &ctx);

        // Generate the param signature based on USING params
        let param_sig = using_param_signature(program);
        out.push_str(&format!("\nvoid {prog_name}({param_sig}) {{\n"));
        out.push_str("    /* Nested program entry point */\n");
        emit_using_param_binding_setup(out, program, "    ");

        // Initialize data items
        let excluded_inits = using_param_excluded_inits(program);
        emit_data_init_excluding(out, &program.data_items, &excluded_inits);

        let use_top_level_entry_flow = !program.paragraphs.is_empty();
        let body_prefix = top_level_body_prefix(&program.body);
        if !body_prefix.is_empty() {
            with_active_context(|ctx| ctx.set_in_body_context(!use_top_level_entry_flow));
            for stmt in body_prefix {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(out, stmt, &env, 1);
            }
            with_active_context(|ctx| ctx.set_in_body_context(false));
        }
        if !use_top_level_entry_flow {
            with_active_context(|ctx| ctx.set_in_body_context(true));
            for stmt in &program.body[body_prefix.len()..] {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(out, stmt, &env, 1);
            }
            with_active_context(|ctx| ctx.set_in_body_context(false));
        } else {
            emit_top_level_entry_flow(out, &program.paragraphs, &label_map);
        }

        if !label_map.is_empty() && !use_top_level_entry_flow {
            out.push_str("_goto_dispatch:\n");
            out.push_str("    while (_goto_target) {\n");
            out.push_str("        int _t = _goto_target;\n");
            out.push_str("        _goto_target = 0;\n");
            out.push_str("        switch(_t) {\n");
            for paragraph in &program.paragraphs {
                if let Some(id) = label_map.get(&paragraph.id) {
                    let c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!("        case {id}: para_{c_name}(); break;\n"));
                }
            }
            out.push_str("        default: return;\n");
            out.push_str("        }\n");
            out.push_str("    }\n");
        }
        out.push_str("}\n");

        emit_program_paragraph_definitions(out, program, &fs_map, has_decl);

        for decl in &program.declaratives {
            let c_name = sanitize_name(&decl.name);
            with_active_context(|ctx| ctx.set_label_map(HashMap::new()));
            out.push_str(&format!("\nstatic void decl_{c_name}(void) {{\n"));
            let has_decl_entry = program.paragraphs.iter().any(|p| p.name == decl.name);
            if has_decl_entry {
                out.push_str(&format!("    para_{c_name}();\n"));
            } else {
                for stmt in &decl.body {
                    let env = StmtEmitEnv {
                        data_items: &program.data_items,
                        paragraphs: &program.paragraphs,
                        fs_map: &fs_map,
                        has_declaratives: has_decl,
                        ctx: &ctx,
                        current_paragraph: None,
                    };
                    emit_statement_with_ctx(out, stmt, &env, 1);
                }
            }
            out.push_str("_goto_dispatch:\n");
            out.push_str("    while (_goto_target) {\n");
            out.push_str("      return;\n");
            out.push_str("    }\n");
            out.push_str("}\n");
        }
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        emit_using_param_binding_cleanup(out, program);

        let mut next_inherited_global_declaratives = inherited_global_declaratives.to_vec();
        next_inherited_global_declaratives.extend(
            program
                .declaratives
                .iter()
                .filter(|decl| decl.is_global && decl.use_kind == HirDeclarativeUse::AfterException)
                .cloned(),
        );

        // Recursively emit any further nested programs
        for nested in &program.nested_programs {
            emit_nested_program(out, nested, &next_inherited_global_declaratives);
        }
        if !program.paragraphs.is_empty() {
            out.push('\n');
            for para in &program.paragraphs {
                let c_name = sanitize_name(&para.name);
                out.push_str(&format!("#undef para_{c_name}\n"));
            }
        }
        if !program.declaratives.is_empty() {
            out.push('\n');
            for decl in &program.declaratives {
                let c_name = sanitize_name(&decl.name);
                out.push_str(&format!("#undef decl_{c_name}\n"));
            }
        }
        if !nested_data_names.is_empty() {
            out.push('\n');
            for c_name in &nested_data_names {
                out.push_str(&format!("#undef {c_name}\n"));
            }
        }
    });
}

fn emit_program_paragraph_definitions(
    out: &mut String,
    program: &HirProgram,
    fs_map: &FileStatusMap,
    has_decl: bool,
) {
    with_active_context(|ctx| {
        let paragraphs = &program.paragraphs;
        let mut i = 0;
        while i < paragraphs.len() {
            let c_name = sanitize_name(&paragraphs[i].name);
            let section_start = i;
            let section_end = paragraph_group_end(paragraphs, i);
            let section_paras = &paragraphs[section_start..section_end];
            let section_len = section_end - section_start;

            if section_len > 1 {
                let mut merged_label_map: HashMap<HirParagraphId, usize> = HashMap::new();
                let mut next_id = 1usize;
                for paragraph in section_paras {
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        merged_label_map.entry(paragraph.id)
                    {
                        entry.insert(next_id);
                        next_id += 1;
                    }
                    for (paragraph_id, _) in build_paragraph_label_map(paragraph) {
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            merged_label_map.entry(paragraph_id)
                        {
                            entry.insert(next_id);
                            next_id += 1;
                        }
                    }
                }
                ctx.set_label_map(merged_label_map.clone());

                out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
                out.push_str(&format!(
                    "    cobol_trace_paragraph(\"{}\", \"{}\");\n",
                    sanitize_name(&program.name),
                    section_paras[0].name
                ));
                out.push_str("    if (_goto_target) goto _goto_dispatch;\n");
                for paragraph in section_paras {
                    let paragraph_c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!(
                        "    _set_fallthrough_debug_event(\"{}\", \"FALL THROUGH\", \"\");\n",
                        escape_c_string(&paragraph.name)
                    ));
                    out.push_str(&format!("lbl_{paragraph_c_name}:;\n"));
                    out.push_str(&format!(
                        "    _dispatch_debug_declarative(\"{}\");\n",
                        escape_c_string(&paragraph.name)
                    ));
                    ctx.set_in_body_context(true);
                    for stmt in &paragraph.body {
                        let env = StmtEmitEnv {
                            data_items: &program.data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives: has_decl,
                            ctx,
                            current_paragraph: Some(paragraph.id),
                        };
                        emit_statement_with_ctx(out, stmt, &env, 1);
                    }
                    ctx.set_in_body_context(false);
                }
                out.push_str("_goto_dispatch:\n");
                out.push_str("    while (_goto_target) {\n");
                out.push_str("      int _t = _goto_target; _goto_target = 0;\n");
                if !merged_label_map.is_empty() {
                    out.push_str("      switch(_t) {\n");
                    for paragraph in section_paras {
                        if let Some(id) = merged_label_map.get(&paragraph.id) {
                            let paragraph_c_name = sanitize_name(&paragraph.name);
                            out.push_str(&format!(
                                "        case {id}: goto lbl_{paragraph_c_name};\n"
                            ));
                        }
                    }
                    out.push_str("        default: _goto_target = _t; return;\n");
                    out.push_str("      }\n");
                }
                out.push_str("      return;\n");
                out.push_str("    }\n");
                out.push_str("}\n");

                for paragraph in &section_paras[1..] {
                    emit_isolated_paragraph_definition(
                        out,
                        paragraph,
                        &program.name,
                        paragraphs,
                        &program.data_items,
                        fs_map,
                        has_decl,
                        ctx,
                    );
                }
            } else {
                emit_isolated_paragraph_definition(
                    out,
                    &paragraphs[i],
                    &program.name,
                    paragraphs,
                    &program.data_items,
                    fs_map,
                    has_decl,
                    ctx,
                );
            }

            i = section_end;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn emit_isolated_paragraph_definition(
    out: &mut String,
    paragraph: &HirParagraph,
    program_name: &str,
    paragraphs: &[HirParagraph],
    data_items: &[HirDataItem],
    fs_map: &FileStatusMap,
    has_decl: bool,
    ctx: &CodegenContext,
) {
    let c_name = sanitize_name(&paragraph.name);
    let para_label_map = build_paragraph_label_map(paragraph);
    ctx.set_label_map(para_label_map.clone());
    out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
    out.push_str(&format!(
        "    _set_fallthrough_debug_event(\"{}\", \"FALL THROUGH\", \"\");\n",
        escape_c_string(&paragraph.name)
    ));
    out.push_str(&format!(
        "    _dispatch_debug_declarative(\"{}\");\n",
        escape_c_string(&paragraph.name)
    ));
    out.push_str(&format!(
        "    cobol_trace_paragraph(\"{}\", \"{}\");\n",
        sanitize_name(program_name),
        paragraph.name
    ));
    out.push_str("    _goto_target = 0;\n");
    out.push_str(&format!("lbl_{c_name}:;\n"));
    ctx.set_in_body_context(true);
    for stmt in &paragraph.body {
        let env = StmtEmitEnv {
            data_items,
            paragraphs,
            fs_map,
            has_declaratives: has_decl,
            ctx,
            current_paragraph: Some(paragraph.id),
        };
        emit_statement_with_ctx(out, stmt, &env, 1);
    }
    ctx.set_in_body_context(false);
    out.push_str("_goto_dispatch:\n");
    if para_label_map.is_empty() {
        out.push_str("    while (_goto_target) {\n");
        out.push_str("      return;\n");
        out.push_str("    }\n");
    } else {
        out.push_str("    while (_goto_target) {\n");
        out.push_str("      int _t = _goto_target; _goto_target = 0;\n");
        out.push_str("      switch(_t) {\n");
        for paragraph in paragraphs {
            if let Some(id) = para_label_map.get(&paragraph.id) {
                let paragraph_c_name = sanitize_name(&paragraph.name);
                out.push_str(&format!(
                    "        case {id}: goto lbl_{paragraph_c_name};\n"
                ));
            }
        }
        out.push_str("        default: _goto_target = _t; return;\n");
        out.push_str("      }\n");
        out.push_str("      return;\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");
}

fn collect_top_level_data_item_c_names(program: &HirProgram) -> Vec<String> {
    let group_member_names = collect_group_member_names(&program.data_items);
    let fd_aliases: HashSet<String> = program
        .fd_record_aliases
        .keys()
        .map(sanitize_name)
        .collect();
    let mut names = BTreeSet::new();
    for item in &program.data_items {
        if item.is_external {
            continue;
        }
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue;
        }
        if fd_aliases.contains(&c_name) {
            continue;
        }
        names.insert(c_name);
    }
    names.into_iter().collect()
}

pub(crate) fn paragraph_c_name(paragraphs: &[HirParagraph], id: HirParagraphId) -> Option<String> {
    paragraphs
        .iter()
        .find(|paragraph| paragraph.id == id)
        .map(|paragraph| sanitize_name(&paragraph.name))
}

pub(crate) fn transfer_target_c_name(
    target: &HirTransferTarget,
    paragraphs: &[HirParagraph],
) -> String {
    match target {
        HirTransferTarget::Paragraph { id, name } => {
            paragraph_c_name(paragraphs, *id).unwrap_or_else(|| sanitize_name(name))
        }
        HirTransferTarget::Label { name, .. } => sanitize_name(name),
    }
}

fn emit_alterable_paragraph_state(out: &mut String, ctx: &CodegenContext) {
    let alterable_paragraphs = ctx.alterable_paragraphs();
    if alterable_paragraphs.is_empty() {
        return;
    }
    out.push_str("/* ALTER paragraph dispatch state */\n");
    for info in alterable_paragraphs {
        let Some(default_target_id) = info.default_target.paragraph_id() else {
            continue;
        };
        out.push_str(&format!(
            "static uint32_t {} = {};\n",
            info.dispatch_var, default_target_id.0
        ));
    }
    out.push('\n');
}

fn emit_inline_dispatch_loop(
    out: &mut String,
    paragraphs: &[HirParagraph],
    label_map: &HashMap<HirParagraphId, usize>,
    next_override_map: Option<&HashMap<HirParagraphId, usize>>,
    handled_ids: Option<&HashSet<HirParagraphId>>,
) {
    if label_map.is_empty() {
        return;
    }
    out.push_str("    while (_goto_target) {\n");
    out.push_str("        int _t = _goto_target;\n");
    out.push_str("        _goto_target = 0;\n");
    out.push_str("        switch(_t) {\n");
    let next_label_map = build_next_label_map(paragraphs, label_map);
    for paragraph in paragraphs {
        if handled_ids.is_some_and(|ids| !ids.contains(&paragraph.id)) {
            continue;
        }
        if let Some(id) = label_map.get(&paragraph.id) {
            let c_name = sanitize_name(&paragraph.name);
            let next_id = if let Some(map) = next_override_map {
                map.get(&paragraph.id)
                    .copied()
                    .or_else(|| next_label_map.get(&paragraph.id).copied())
            } else {
                next_label_map.get(&paragraph.id).copied()
            };
            if let Some(next_id) = next_id {
                out.push_str(&format!(
                    "        case {id}: para_{c_name}(); if (!_goto_target) {{ _set_fallthrough_debug_event(\"{}\", \"FALL THROUGH\", \"\"); _goto_target = {next_id}; }} break;\n",
                    escape_c_string(&paragraph.name)
                ));
            } else {
                out.push_str(&format!("        case {id}: para_{c_name}(); break;\n"));
            }
        }
    }
    out.push_str("        default: cobol_stop_run();\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
}

fn emit_top_level_entry_flow(
    out: &mut String,
    paragraphs: &[HirParagraph],
    label_map: &HashMap<HirParagraphId, usize>,
) {
    let top_level_next_map = build_top_level_next_label_map(paragraphs, label_map);
    let top_level_entry_ids = top_level_group_entry_ids(paragraphs, label_map);
    if let Some(first_id) = top_level_entry_ids.first().copied() {
        if let Some(first_name) = paragraphs
            .iter()
            .find(|paragraph| paragraph.id == first_id)
            .map(|paragraph| escape_c_string(&paragraph.name))
        {
            out.push_str(&format!(
                "    _set_debug_event(\"{first_name}\", \"START PROGRAM\", \"\");\n"
            ));
        }
        out.push_str("_goto_dispatch:\n");
        if let Some(first_label_id) = label_map.get(&first_id) {
            out.push_str(&format!(
                "    if (!_goto_target) _goto_target = {first_label_id};\n"
            ));
        }
        emit_inline_dispatch_loop(out, paragraphs, label_map, Some(&top_level_next_map), None);
    }
}

fn top_level_body_prefix(body: &[HirStatement]) -> &[HirStatement] {
    let label_start = body
        .iter()
        .position(|stmt| matches!(stmt, HirStatement::Label { .. }))
        .unwrap_or(body.len());
    &body[..label_start]
}

fn build_top_level_next_label_map(
    paragraphs: &[HirParagraph],
    label_map: &HashMap<HirParagraphId, usize>,
) -> HashMap<HirParagraphId, usize> {
    let top_level = top_level_group_entry_ids(paragraphs, label_map);

    let mut next_map = HashMap::new();
    for pair in top_level.windows(2) {
        let current = pair[0];
        let next = pair[1];
        if let Some(next_id) = label_map.get(&next) {
            next_map.insert(current, *next_id);
        }
    }
    next_map
}

fn build_next_label_map(
    paragraphs: &[HirParagraph],
    label_map: &HashMap<HirParagraphId, usize>,
) -> HashMap<HirParagraphId, usize> {
    let mut next_map = HashMap::new();
    let mut id_to_paragraph = HashMap::new();
    for paragraph in paragraphs {
        if label_map.contains_key(&paragraph.id) {
            id_to_paragraph.insert(paragraph.id, paragraph);
        }
    }

    let mut ordered: Vec<_> = label_map.iter().map(|(pid, id)| (*id, *pid)).collect();
    ordered.sort_unstable_by_key(|(id, _)| *id);
    for pair in ordered.windows(2) {
        let (_current_id, current_pid) = pair[0];
        let (next_id, _next_pid) = pair[1];
        if id_to_paragraph.contains_key(&current_pid) {
            next_map.insert(current_pid, next_id);
        }
    }
    next_map
}

fn build_entry_label_map(
    paragraphs: &[HirParagraph],
    body: &[HirStatement],
) -> HashMap<HirParagraphId, usize> {
    let mut map = HashMap::new();
    let mut next_id = 1usize;

    for paragraph in paragraphs {
        map.entry(paragraph.id).or_insert_with(|| {
            let current = next_id;
            next_id += 1;
            current
        });
    }

    for stmt in body {
        if let HirStatement::Label { target } = stmt {
            if let Some(paragraph_id) = target.paragraph_id() {
                map.entry(paragraph_id).or_insert_with(|| {
                    let current = next_id;
                    next_id += 1;
                    current
                });
            }
        }
    }

    map
}

fn build_paragraph_label_map(paragraph: &HirParagraph) -> HashMap<HirParagraphId, usize> {
    let mut map = HashMap::new();
    let mut id = 1usize;
    for stmt in &paragraph.body {
        if let HirStatement::Label { target } = stmt {
            if let Some(paragraph_id) = target.paragraph_id() {
                map.entry(paragraph_id).or_insert_with(|| {
                    let current = id;
                    id += 1;
                    current
                });
            }
        }
    }
    map
}

fn paragraph_group_end(paragraphs: &[HirParagraph], start: usize) -> usize {
    let paragraph = &paragraphs[start];
    if matches!(paragraph.kind, HirParagraphKind::Section) {
        let mut end = start + 1;
        while end < paragraphs.len() {
            if paragraphs[end].section_id == Some(paragraph.id) {
                end += 1;
                continue;
            }
            break;
        }
        return end;
    }

    start + 1
}

fn top_level_group_entry_ids(
    paragraphs: &[HirParagraph],
    label_map: &HashMap<HirParagraphId, usize>,
) -> Vec<HirParagraphId> {
    let mut ids = Vec::new();
    let mut i = 0usize;
    while i < paragraphs.len() {
        let paragraph = &paragraphs[i];
        if paragraph.section_id.is_none() && label_map.contains_key(&paragraph.id) {
            ids.push(paragraph.id);
        }
        i = paragraph_group_end(paragraphs, i);
    }
    ids
}

fn emit_header(out: &mut String) {
    out.push_str("/* Generated by cobolc - COBOL Compiler */\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdlib.h>\n");
    out.push_str("#include <string.h>\n");
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <setjmp.h>\n");
    out.push_str("#include <math.h>\n");
    out.push_str("#include <time.h>\n");
    out.push_str("#include <dlfcn.h>\n");
    out.push('\n');
    // Helper for dynamic CALL: convert COBOL program name (space-padded,
    // may contain hyphens) to a C identifier suitable for dlsym lookup.
    out.push_str("static void cobol_resolve_call_name(const char* src, size_t src_len, char* dst, size_t dst_len) {\n");
    out.push_str("    size_t j = 0;\n");
    out.push_str("    for (size_t i = 0; i < src_len && j < dst_len - 1; i++) {\n");
    out.push_str("        if (src[i] == ' ' || src[i] == '\\0') break;\n");
    out.push_str("        dst[j++] = (src[i] == '-') ? '_' : src[i];\n");
    out.push_str("    }\n");
    out.push_str("    dst[j] = '\\0';\n");
    out.push_str("}\n\n");
    out.push_str(
        "static void cobol_trace_paragraph(const char* program, const char* paragraph) {\n",
    );
    out.push_str("    const char* enabled = getenv(\"COBOL_TRACE_PARAGRAPHS\");\n");
    out.push_str(
        "    if (!enabled || enabled[0] == '\\0' || strcmp(enabled, \"0\") == 0) return;\n",
    );
    out.push_str("    const char* trace_file = getenv(\"COBOL_TRACE_PARAGRAPHS_FILE\");\n");
    out.push_str("    if (trace_file && trace_file[0] != '\\0') {\n");
    out.push_str("        FILE* fp = fopen(trace_file, \"a\");\n");
    out.push_str("        if (fp) {\n");
    out.push_str("            fprintf(fp, \"[COBOL-TRACE] %s::%s\\n\", program, paragraph);\n");
    out.push_str("            fclose(fp);\n");
    out.push_str("        }\n");
    out.push_str("        return;\n");
    out.push_str("    }\n");
    out.push_str("    fprintf(stderr, \"[COBOL-TRACE] %s::%s\\n\", program, paragraph);\n");
    out.push_str("}\n\n");
}

fn emit_runtime_declarations(out: &mut String) {
    cobol_runtime::abi::emit_c_declarations(out);
}

fn emit_classes(out: &mut String, classes: &[cobol_hir::HirClass]) {
    let empty_records: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let empty_orgs: HashMap<smol_str::SmolStr, u32> = HashMap::new();
    let empty_relative_keys: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let empty_aliases: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let ctx = CodegenContext::new(
        &[],
        &empty_records,
        &empty_orgs,
        &empty_relative_keys,
        &[],
        &empty_aliases,
        "_check_file_declarative".to_string(),
    );
    for class in classes {
        let c_name = sanitize_name(&class.name);
        out.push_str(&format!("/* CLASS {} */\n", c_name));

        // Forward-declare the dispatch function (needed for vtable struct)
        out.push_str(&format!(
            "static int64_t {c_name}_dispatch(void* obj, const char* method, int64_t* args, int32_t argc);\n\n"
        ));

        // Emit instance data as a struct
        out.push_str(&format!("typedef struct {c_name}_s {{\n"));
        out.push_str("    void* _vtable; /* vtable pointer */\n");
        for item in &class.instance_data {
            let member_name = sanitize_name(&item.name);
            let c_type = hir_type_to_c(&item.data_type);
            out.push_str(&format!("    {c_type} {member_name};\n"));
        }
        out.push_str(&format!("}} {c_name};\n\n"));

        // Emit vtable struct: first entry is the dispatch function pointer
        out.push_str(&format!("typedef struct {c_name}_vtable_s {{\n"));
        out.push_str(
            "    int64_t (*_dispatch)(void* obj, const char* method, int64_t* args, int32_t argc);\n",
        );
        for method in &class.instance_methods {
            let method_name = sanitize_name(&method.name);
            out.push_str(&format!("    int64_t (*{method_name})({c_name}* self);\n"));
        }
        out.push_str(&format!("}} {c_name}_vtable;\n\n"));

        // Emit method implementations
        for method in &class.instance_methods {
            let method_name = sanitize_name(&method.name);
            out.push_str(&format!(
                "int64_t {c_name}_{method_name}({c_name}* self) {{\n"
            ));
            for item in &method.data_items {
                let local_name = sanitize_name(&item.name);
                let c_type = hir_type_to_c(&item.data_type);
                out.push_str(&format!("    {c_type} {local_name};\n"));
            }
            for stmt in &method.body {
                let empty_fs_map = HashMap::new();
                let env = StmtEmitEnv {
                    data_items: &[],
                    paragraphs: &[],
                    fs_map: &empty_fs_map,
                    has_declaratives: false,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(out, stmt, &env, 1);
            }
            out.push_str("    return 0;\n");
            out.push_str("}\n\n");
        }

        // Emit dispatch function: maps method name to function pointer
        out.push_str(&format!(
            "static int64_t {c_name}_dispatch(void* obj, const char* method, int64_t* args, int32_t argc) {{\n"
        ));
        out.push_str("    (void)args; (void)argc;\n");
        for method in &class.instance_methods {
            let method_name = sanitize_name(&method.name);
            let method_str = &method.name;
            out.push_str(&format!(
                "    if (strcmp(method, \"{method_str}\") == 0) return {c_name}_{method_name}(({c_name}*)obj);\n"
            ));
        }
        out.push_str(&format!(
            "    fprintf(stderr, \"COBOL INVOKE: unknown method '%s' on class {}\\n\", method);\n",
            class.name
        ));
        out.push_str("    return 0;\n");
        out.push_str("}\n\n");

        // Emit factory methods
        for method in &class.factory_methods {
            let method_name = sanitize_name(&method.name);
            out.push_str(&format!(
                "int64_t {c_name}_factory_{method_name}(void) {{\n"
            ));
            for stmt in &method.body {
                let empty_fs_map = HashMap::new();
                let env = StmtEmitEnv {
                    data_items: &[],
                    paragraphs: &[],
                    fs_map: &empty_fs_map,
                    has_declaratives: false,
                    ctx: &ctx,
                    current_paragraph: None,
                };
                emit_statement_with_ctx(out, stmt, &env, 1);
            }
            out.push_str("    return 0;\n");
            out.push_str("}\n\n");
        }

        // Emit vtable instance: dispatch function first, then methods
        out.push_str(&format!(
            "static {c_name}_vtable {c_name}_vtable_instance = {{\n"
        ));
        out.push_str(&format!("    ._dispatch = {c_name}_dispatch,\n"));
        for method in &class.instance_methods {
            let method_name = sanitize_name(&method.name);
            out.push_str(&format!("    .{method_name} = {c_name}_{method_name},\n"));
        }
        out.push_str("};\n\n");

        // Emit constructor (NEW)
        out.push_str(&format!("{c_name}* {c_name}_new(void) {{\n"));
        out.push_str(&format!(
            "    {c_name}* obj = ({c_name}*)malloc(sizeof({c_name}));\n"
        ));
        out.push_str(&format!("    obj->_vtable = &{c_name}_vtable_instance;\n"));
        out.push_str("    return obj;\n");
        out.push_str("}\n\n");
    }
}

fn emit_functions(out: &mut String, functions: &[cobol_hir::HirFunction]) {
    let empty_records: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let empty_orgs: HashMap<smol_str::SmolStr, u32> = HashMap::new();
    let empty_relative_keys: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let empty_aliases: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let ctx = CodegenContext::new(
        &[],
        &empty_records,
        &empty_orgs,
        &empty_relative_keys,
        &[],
        &empty_aliases,
        "_check_file_declarative".to_string(),
    );
    for func in functions {
        let c_name = sanitize_name(&func.name).to_lowercase();
        let ret_type = hir_type_to_c(&func.returning);

        // Build parameter list
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| {
                let p_name = sanitize_name(&p.name);
                let p_type = hir_type_to_c(&p.data_type);
                format!("{p_type} {p_name}")
            })
            .collect();
        let params_str = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };

        out.push_str(&format!(
            "{ret_type} cobol_func_{c_name}({params_str}) {{\n"
        ));

        // Local data items
        for item in &func.data_items {
            let local_name = sanitize_name(&item.name);
            let c_type = hir_type_to_c(&item.data_type);
            out.push_str(&format!("    {c_type} {local_name};\n"));
        }
        emit_data_init(out, &func.data_items);

        // Body
        for stmt in &func.body {
            let empty_fs_map = HashMap::new();
            let env = StmtEmitEnv {
                data_items: &[],
                paragraphs: &[],
                fs_map: &empty_fs_map,
                has_declaratives: false,
                ctx: &ctx,
                current_paragraph: None,
            };
            emit_statement_with_ctx(out, stmt, &env, 1);
        }

        out.push_str(&format!("    return ({ret_type})0;\n"));
        out.push_str("}\n\n");
    }
}

fn emit_typedefs(out: &mut String, typedefs: &[cobol_hir::HirTypedef]) {
    for td in typedefs {
        let c_name = sanitize_name(&td.name);
        let c_type = hir_type_to_c(&td.base_type);
        out.push_str(&format!("typedef {c_type} {c_name}; /* TYPEDEF */\n"));
    }
    if !typedefs.is_empty() {
        out.push('\n');
    }
}

fn emit_interfaces(out: &mut String, interfaces: &[cobol_hir::HirInterface]) {
    for iface in interfaces {
        let c_name = sanitize_name(&iface.name);
        out.push_str(&format!("/* INTERFACE {} */\n", c_name));

        // Emit interface as a vtable struct
        out.push_str(&format!("typedef struct {c_name}_vtable_s {{\n"));
        for method in &iface.methods {
            let method_name = sanitize_name(&method.name);
            out.push_str(&format!("    int64_t (*{method_name})(void* self);\n"));
        }
        out.push_str(&format!("}} {c_name}_vtable;\n\n"));
    }
}

/// Map a HIR type to its C representation.
pub(crate) fn hir_type_to_c(data_type: &HirType) -> &'static str {
    match data_type {
        HirType::Alphanumeric { .. } => "char*",
        HirType::Numeric { .. } => "int64_t",
        HirType::Group { .. } => "int64_t",
        HirType::Comp3 { .. } => "int64_t",
        HirType::Binary { .. } => "int64_t",
        HirType::Index => "int64_t",
        HirType::Pointer => "void*",
        HirType::Boolean => "int8_t",
        HirType::FloatShort => "float",
        HirType::FloatLong => "double",
        HirType::FloatExtended => "long double",
        HirType::National { .. } => "uint16_t*",
    }
}
