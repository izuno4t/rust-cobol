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
    HirAcceptSource, HirBinOp, HirClassType, HirCompareOp, HirCondition, HirDataItem, HirExpr,
    HirFileInfo, HirLiteral, HirMoveTarget, HirOpenMode, HirParagraph, HirParagraphId,
    HirParagraphKind, HirPerformKind, HirProgram, HirStartRelation, HirStatement,
    HirTransferTarget, HirType, HirUnaryOp,
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

/// Generate the typedef name for a group struct, unique per member layout.
fn group_typedef_name(c_name: &str, members: &[HirDataItem]) -> String {
    let fp = compute_group_fingerprint(members);
    format!("_grp_{c_name}_{fp:08x}_t")
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
        emit_fd_alias_macros(&mut out, &program.data_items, &program.fd_record_aliases);
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

        // Forward-declare CALL targets with weak stub definitions.  When the
        // real sub-program is linked, the real definition overrides the stub.
        // Otherwise the stub (which does nothing) is used, preventing link
        // errors for absent sub-programs.
        let call_targets = collect_call_targets(program);
        if !call_targets.is_empty() {
            out.push_str("/* Weak stubs for CALL targets (overridden by real sub-programs) */\n");
            out.push_str(
                "#pragma clang diagnostic push\n\
             #pragma clang diagnostic ignored \"-Wdeprecated-non-prototype\"\n",
            );
            for target in &call_targets {
                out.push_str(&format!(
                    "__attribute__((weak)) void {target}() {{ /* stub */ }}\n"
                ));
            }
            out.push_str("#pragma clang diagnostic pop\n");
            out.push('\n');
        }

        // Forward-declare nested program entry points
        if !program.nested_programs.is_empty() {
            out.push_str("/* Forward declarations for nested programs */\n");
            for nested in &program.nested_programs {
                let nested_name = sanitize_name(&nested.name);
                let param_sig = if nested.using_params.is_empty() {
                    "void".to_string()
                } else {
                    nested
                        .using_params
                        .iter()
                        .map(|_| "void*".to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
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

            // Emit dispatcher function: called after each file I/O to invoke matching
            // USE AFTER EXCEPTION handler if status is non-zero.
            out.push_str(
                "static void _check_file_declarative(const char* file_c_name, int fs) {\n",
            );
            out.push_str("    if (fs == 0) return;\n");
            for decl in &program.declaratives {
                let c_decl = sanitize_name(&decl.name);
                // Check if any file_name is a mode keyword (I-O, INPUT, OUTPUT,
                // EXTEND) rather than a specific file name.  Mode-based USE AFTER
                // handlers apply to ALL files opened in that mode; as a
                // simplification we treat them as unconditional catch-alls.
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
        let label_map = build_body_label_map(&program.body);
        let has_labels = !label_map.is_empty();
        with_active_context(|ctx| ctx.set_body_label_map(label_map.clone()));
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        // Emit goto dispatch variable if needed
        if has_labels {
            out.push_str("static int _goto_target = 0;\n\n");
        }

        // Main function
        out.push_str("int main(int argc, char** argv) {\n");

        // Initialize data items
        let t_init = std::time::Instant::now();
        emit_data_init(&mut out, &program.data_items);
        cg_timing!("emit_data_init", t_init);

        let has_decl = !program.declaratives.is_empty();

        // Emit body statements (with GO TO -> C goto support)
        let t_body = std::time::Instant::now();
        with_active_context(|ctx| ctx.set_in_body_context(true));
        for stmt in &program.body {
            let env = StmtEmitEnv {
                data_items: &program.data_items,
                paragraphs: &program.paragraphs,
                fs_map: &fs_map,
                has_declaratives: has_decl,
                ctx: &ctx,
            };
            emit_statement_with_ctx(&mut out, stmt, &env, 1);
        }
        with_active_context(|ctx| ctx.set_in_body_context(false));
        cg_timing!("emit_body_statements", t_body);

        // Falling off the main procedure should behave like STOP RUN so that
        // buffered files are flushed consistently.
        out.push_str("    cobol_stop_run();\n");

        // Emit goto dispatch table if labels exist
        if has_labels {
            out.push_str("_goto_dispatch:\n");
            out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
            out.push_str("      switch(_t) {\n");
            for paragraph in &program.paragraphs {
                if let Some(id) = label_map.get(&paragraph.id) {
                    let c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!("        case {id}: goto lbl_{c_name};\n"));
                }
            }
            out.push_str("        default: cobol_stop_run();\n");
            out.push_str("      }\n");
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
            for stmt in &decl.body {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                };
                emit_statement_with_ctx(&mut out, stmt, &env, 1);
            }
            out.push_str("}\n");
        }
        with_active_context(|ctx| ctx.set_label_map(label_map.clone()));

        // For sub-programs (those with USING params), emit a callable entry point.
        // This allows other programs to CALL this program by name.
        if !program.using_params.is_empty() {
            let prog_name = sanitize_name(&program.name);
            out.push_str(&format!("\nvoid {prog_name}(void) {{\n"));
            out.push_str("    /* Sub-program entry point */\n");
            if has_labels {
                with_active_context(|ctx| ctx.set_label_map(label_map.clone()));
            }
            with_active_context(|ctx| ctx.set_in_body_context(true));
            for stmt in &program.body {
                let env = StmtEmitEnv {
                    data_items: &program.data_items,
                    paragraphs: &program.paragraphs,
                    fs_map: &fs_map,
                    has_declaratives: has_decl,
                    ctx: &ctx,
                };
                emit_statement_with_ctx(&mut out, stmt, &env, 1);
            }
            with_active_context(|ctx| ctx.set_in_body_context(false));
            // Emit goto dispatch table for sub-program if labels exist
            if has_labels {
                out.push_str("_goto_dispatch:\n");
                out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
                out.push_str("      switch(_t) {\n");
                for paragraph in &program.paragraphs {
                    if let Some(id) = label_map.get(&paragraph.id) {
                        let c_name = sanitize_name(&paragraph.name);
                        out.push_str(&format!("        case {id}: goto lbl_{c_name};\n"));
                    }
                }
                out.push_str("        default: return;\n");
                out.push_str("      }\n");
                out.push_str("    }\n");
            }
            out.push_str("}\n");
        }

        // Emit nested programs as separate callable functions
        for nested in &program.nested_programs {
            emit_nested_program(&mut out, nested);
        }

        out
    })
}

/// Emit a nested (contained) program as a callable C function.
fn emit_nested_program(out: &mut String, program: &HirProgram) {
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

        for nested in &program.nested_programs {
            let nested_name = sanitize_name(&nested.name);
            out.push_str(&format!("void {nested_name}(void);\n"));
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

        let fs_map = build_file_status_map(&program.file_status_vars);
        let label_map = build_body_label_map(&program.body);
        let has_decl = !program.declaratives.is_empty();
        if !label_map.is_empty() {
            with_active_context(|ctx| ctx.set_body_label_map(label_map.clone()));
            with_active_context(|ctx| ctx.set_label_map(label_map.clone()));
        }

        // Generate the param signature based on USING params
        let param_sig = if program.using_params.is_empty() {
            "void".to_string()
        } else {
            program
                .using_params
                .iter()
                .map(|_| "void*".to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!("\nvoid {prog_name}({param_sig}) {{\n"));
        out.push_str("    /* Nested program entry point */\n");

        // Initialize data items
        emit_data_init(out, &program.data_items);

        with_active_context(|ctx| ctx.set_in_body_context(true));
        for stmt in &program.body {
            let env = StmtEmitEnv {
                data_items: &program.data_items,
                paragraphs: &program.paragraphs,
                fs_map: &fs_map,
                has_declaratives: has_decl,
                ctx: &ctx,
            };
            emit_statement_with_ctx(out, stmt, &env, 1);
        }
        with_active_context(|ctx| ctx.set_in_body_context(false));

        if !label_map.is_empty() {
            out.push_str("_goto_dispatch:\n");
            out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
            out.push_str("      switch(_t) {\n");
            for paragraph in &program.paragraphs {
                if let Some(id) = label_map.get(&paragraph.id) {
                    let c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!("        case {id}: goto lbl_{c_name};\n"));
                }
            }
            out.push_str("        default: return;\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
        }
        out.push_str("}\n");

        emit_program_paragraph_definitions(out, program, &fs_map, has_decl);

        // Recursively emit any further nested programs
        for nested in &program.nested_programs {
            emit_nested_program(out, nested);
        }
        if !program.paragraphs.is_empty() {
            out.push('\n');
            for para in &program.paragraphs {
                let c_name = sanitize_name(&para.name);
                out.push_str(&format!("#undef para_{c_name}\n"));
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
            let is_section_header = matches!(paragraphs[i].kind, HirParagraphKind::Section);

            let section_start = i;
            let mut section_end = i + 1;
            if is_section_header {
                while section_end < paragraphs.len() {
                    if paragraphs[section_end].section_id == Some(paragraphs[i].id) {
                        section_end += 1;
                        continue;
                    }
                    break;
                }
            }

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
                out.push_str("    if (_goto_target) goto _goto_dispatch;\n");
                for paragraph in section_paras {
                    let paragraph_c_name = sanitize_name(&paragraph.name);
                    out.push_str(&format!("lbl_{paragraph_c_name}:;\n"));
                    ctx.set_in_body_context(true);
                    for stmt in &paragraph.body {
                        let env = StmtEmitEnv {
                            data_items: &program.data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives: has_decl,
                            ctx,
                        };
                        emit_statement_with_ctx(out, stmt, &env, 1);
                    }
                    ctx.set_in_body_context(false);
                }
                if !merged_label_map.is_empty() {
                    out.push_str("_goto_dispatch:\n");
                    out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
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
                    out.push_str("    }\n");
                }
                out.push_str("}\n");

                for paragraph in &section_paras[1..] {
                    let paragraph_c_name = sanitize_name(&paragraph.name);
                    if let Some(id) = merged_label_map.get(&paragraph.id) {
                        out.push_str(&format!(
                            "\nstatic void para_{paragraph_c_name}(void) {{ _goto_target = {id}; para_{c_name}(); }}\n"
                        ));
                    }
                }
            } else {
                let para_label_map = build_paragraph_label_map(&paragraphs[i]);
                ctx.set_label_map(para_label_map.clone());
                out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
                out.push_str(&format!("lbl_{c_name}:;\n"));
                for stmt in &paragraphs[i].body {
                    let env = StmtEmitEnv {
                        data_items: &program.data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives: has_decl,
                        ctx,
                    };
                    emit_statement_with_ctx(out, stmt, &env, 1);
                }
                if !para_label_map.is_empty() {
                    out.push_str("_goto_dispatch:\n");
                    out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
                    out.push_str("      switch(_t) {\n");
                    if let Some(id) = para_label_map.get(&paragraphs[i].id) {
                        out.push_str(&format!("        case {id}: goto lbl_{c_name};\n"));
                    }
                    out.push_str("        default: _goto_target = _t; return;\n");
                    out.push_str("      }\n");
                    out.push_str("    }\n");
                }
                out.push_str("}\n");
            }

            i = section_end;
        }
    });
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

/// Collect all Label statements from the body and assign each a unique integer ID.
fn build_body_label_map(body: &[HirStatement]) -> HashMap<HirParagraphId, usize> {
    let mut map = HashMap::new();
    let mut id = 1usize;
    for stmt in body {
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

fn build_paragraph_label_map(paragraph: &HirParagraph) -> HashMap<HirParagraphId, usize> {
    let mut map = HashMap::new();
    let mut id = 1usize;
    map.insert(paragraph.id, id);
    id += 1;
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
}

fn emit_runtime_declarations(out: &mut String) {
    cobol_runtime::abi::emit_c_declarations(out);
}

fn emit_classes(out: &mut String, classes: &[cobol_hir::HirClass]) {
    let empty_records: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let ctx = CodegenContext::new(&[], &empty_records, &[], &HashMap::new());
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
    let ctx = CodegenContext::new(&[], &empty_records, &[], &HashMap::new());
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
