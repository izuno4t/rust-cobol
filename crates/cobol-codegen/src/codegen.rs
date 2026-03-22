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
    HirFileInfo, HirLiteral, HirMoveTarget, HirOpenMode, HirParagraph, HirPerformKind, HirProgram,
    HirStartRelation, HirStatement, HirType, HirUnaryOp,
};

pub(crate) use self::context::*;
pub(crate) use self::data::*;
pub(crate) use self::expr::*;
pub use self::compiler::compile_c_to_executable;
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
    emit_data_items(&mut out, &program.data_items, &HashSet::new());
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
        out.push_str("static void _check_file_declarative(const char* file_c_name, int fs) {\n");
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
        emit_statement_with_ctx(
            &mut out,
            stmt,
            &program.data_items,
            &program.paragraphs,
            &fs_map,
            has_decl,
            &ctx,
            1,
        );
    }
    with_active_context(|ctx| ctx.set_in_body_context(false));
    cg_timing!("emit_body_statements", t_body);

    // Fallthrough return
    out.push_str("    return 0;\n");

    // Emit goto dispatch table if labels exist
    if has_labels {
        out.push_str("_goto_dispatch:\n");
        out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
        out.push_str("      switch(_t) {\n");
        for (name, id) in &label_map {
            out.push_str(&format!("        case {id}: goto lbl_{name};\n"));
        }
        out.push_str("        default: return 0;\n");
        out.push_str("      }\n");
        out.push_str("    }\n");
    }

    out.push_str("}\n");

    // Emit paragraph function definitions
    let t_para = std::time::Instant::now();
    for para in &program.paragraphs {
        let c_name = sanitize_name(&para.name);
        out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
        for stmt in &para.body {
            emit_statement_with_ctx(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                &ctx,
                1,
            );
        }
        out.push_str("}\n");
    }

    cg_timing!("emit_paragraphs", t_para);

    // Emit declarative handler function definitions
    for decl in &program.declaratives {
        let c_name = sanitize_name(&decl.name);
        out.push_str(&format!("\nstatic void decl_{c_name}(void) {{\n"));
        for stmt in &decl.body {
            emit_statement_with_ctx(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                &ctx,
                1,
            );
        }
        out.push_str("}\n");
    }

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
            emit_statement_with_ctx(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                &ctx,
                1,
            );
        }
        with_active_context(|ctx| ctx.set_in_body_context(false));
        // Emit goto dispatch table for sub-program if labels exist
        if has_labels {
            out.push_str("_goto_dispatch:\n");
            out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
            out.push_str("      switch(_t) {\n");
            for (name, id) in &label_map {
                out.push_str(&format!("        case {id}: goto lbl_{name};\n"));
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
        out.push_str(&format!("#define para_{c_name} para_{prog_name}__{c_name}\n"));
    }
    if !program.paragraphs.is_empty() {
        out.push('\n');
    }

    // Emit data items as function-scope statics
    let nested_fd_aliases: HashSet<String> = program
        .fd_record_aliases
        .keys()
        .map(|k| sanitize_name(k))
        .collect();
    emit_data_items(out, &program.data_items, &nested_fd_aliases);

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
        emit_statement_with_ctx(
            out,
            stmt,
            &program.data_items,
            &program.paragraphs,
            &fs_map,
            has_decl,
            &ctx,
            1,
        );
    }
    with_active_context(|ctx| ctx.set_in_body_context(false));

    if !label_map.is_empty() {
        out.push_str("_goto_dispatch:\n");
        out.push_str("    { int _t = _goto_target; _goto_target = 0;\n");
        out.push_str("      switch(_t) {\n");
        for (name, id) in &label_map {
            out.push_str(&format!("        case {id}: goto lbl_{name};\n"));
        }
        out.push_str("        default: return;\n");
        out.push_str("      }\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");

    // Emit paragraph function definitions for nested program
    for para in &program.paragraphs {
        let c_name = sanitize_name(&para.name);
        out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
        for stmt in &para.body {
            emit_statement_with_ctx(
                out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                &ctx,
                1,
            );
        }
        out.push_str("}\n");
    }

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

fn collect_top_level_data_item_c_names(program: &HirProgram) -> Vec<String> {
    let group_member_names = collect_group_member_names(&program.data_items);
    let fd_aliases: HashSet<String> = program
        .fd_record_aliases
        .keys()
        .map(|k| sanitize_name(k))
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

/// Collect all Label statements from the body and assign each a unique integer ID.
fn build_body_label_map(body: &[HirStatement]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    let mut id = 1usize;
    for stmt in body {
        if let HirStatement::Label { name } = stmt {
            let c_name = sanitize_name(name);
            map.entry(c_name).or_insert_with(|| {
                let current = id;
                id += 1;
                current
            });
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
    out.push_str("/* Runtime library declarations */\n");
    out.push_str("extern void cobol_display_string(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_display_int(int64_t value);\n");
    out.push_str("extern void cobol_display_newline(void);\n");
    out.push_str("extern void cobol_display_space(void);\n");
    out.push_str("extern void cobol_display_flush(void);\n");
    out.push_str("extern void cobol_stop_run(void) __attribute__((noreturn));\n");
    out.push_str("extern void cobol_goback(void);\n");
    out.push_str("extern void cobol_call_enter(uintptr_t jmp_buf_ptr);\n");
    out.push_str("extern void cobol_call_leave(void);\n");
    // File I/O runtime declarations
    out.push_str("/* File I/O runtime declarations */\n");
    out.push_str(
        "extern uint32_t cobol_file_open(uint32_t file_id, const uint8_t* path_ptr, uint32_t path_len, uint32_t org, uint32_t access, uint32_t mode, uint32_t record_len);\n",
    );
    out.push_str("extern uint32_t cobol_file_close(uint32_t file_id);\n");
    out.push_str(
        "extern uint32_t cobol_file_read_next(uint32_t file_id, uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_write(uint32_t file_id, const uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_file_rewrite(uint32_t file_id, const uint8_t* record_ptr, uint32_t record_len);\n",
    );
    out.push_str("extern uint32_t cobol_file_delete(uint32_t file_id);\n");
    out.push_str(
        "extern uint32_t cobol_file_start(uint32_t file_id, const uint8_t* key_ptr, uint32_t key_len, uint32_t mode);\n",
    );
    // String/INSPECT runtime declarations
    out.push_str("/* Class condition runtime declarations */\n");
    out.push_str("extern int32_t cobol_is_numeric(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic_lower(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int32_t cobol_is_alphabetic_upper(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("/* Alphanumeric comparison runtime declaration */\n");
    out.push_str(
        "extern int32_t cobol_compare_alphanumeric(const uint8_t* a, uint32_t a_len, const uint8_t* b, uint32_t b_len);\n",
    );
    out.push_str("/* String operations runtime declarations */\n");
    out.push_str(
        "extern void cobol_move_string(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_move_numeric_to_display(int64_t value, int32_t scale, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_store_numeric_display(int64_t value, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern int64_t cobol_display_to_int64(const uint8_t* src, uint32_t src_len);\n");
    out.push_str(
        "extern int32_t cobol_string_concat(const void* sources, uint32_t source_count, uint8_t* dst, uint32_t dst_len, uint32_t* pointer);\n",
    );
    out.push_str(
        "extern int32_t cobol_unstring(const uint8_t* src, uint32_t src_len, const uint8_t* delim, uint32_t delim_len, void* targets, uint32_t target_count, uint32_t* pointer, uint32_t* tallying);\n",
    );
    out.push_str(
        "extern uint32_t cobol_inspect_tallying(const uint8_t* src, uint32_t src_len, const uint8_t* search, uint32_t search_len, uint32_t mode);\n",
    );
    out.push_str(
        "extern void cobol_inspect_replacing(uint8_t* src, uint32_t src_len, const uint8_t* search, uint32_t search_len, const uint8_t* replace, uint32_t replace_len, uint32_t mode);\n",
    );
    out.push_str(
        "extern void cobol_inspect_converting(uint8_t* src, uint32_t src_len, const uint8_t* from, uint32_t from_len, const uint8_t* to, uint32_t to_len);\n",
    );
    // Sort/Merge runtime declarations
    out.push_str("/* Sort/Merge runtime declarations */\n");
    out.push_str(
        "extern void cobol_sort(uint8_t* records, uint32_t count, uint32_t rec_len, const void* keys, uint32_t key_count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_merge(const uint32_t* inputs, uint32_t input_count, uint32_t output, const void* keys, uint32_t key_count, uint32_t rec_len);\n",
    );
    // Intrinsic function runtime declarations
    out.push_str("/* Intrinsic function runtime declarations */\n");
    out.push_str("extern uint32_t cobol_func_current_date(uint8_t* buf, uint32_t buf_len);\n");
    out.push_str("extern uint32_t cobol_func_length(const uint8_t* ptr, uint32_t len);\n");
    out.push_str(
        "extern uint32_t cobol_func_trim(const uint8_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len, uint32_t mode);\n",
    );
    out.push_str("extern void cobol_func_upper_case(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_func_lower_case(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern void cobol_func_reverse(uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int64_t cobol_func_numval(const uint8_t* ptr, uint32_t len);\n");
    out.push_str("extern int64_t cobol_func_max_int(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_min_int(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_mod(int64_t a, int64_t b);\n");
    out.push_str("extern int64_t cobol_func_integer(int64_t value, int32_t scale);\n");
    out.push_str("extern uint32_t cobol_func_ord(uint8_t c);\n");
    out.push_str("extern uint8_t cobol_func_char(uint32_t ord);\n");
    out.push_str("/* Mathematical intrinsic function declarations */\n");
    out.push_str("extern int64_t cobol_func_abs(int64_t value);\n");
    out.push_str("extern double cobol_func_abs_float(double value);\n");
    out.push_str("extern double cobol_func_sqrt(double value);\n");
    out.push_str("extern double cobol_func_exp(double value);\n");
    out.push_str("extern double cobol_func_exp10(double value);\n");
    out.push_str("extern double cobol_func_log(double value);\n");
    out.push_str("extern double cobol_func_log10(double value);\n");
    out.push_str("extern double cobol_func_sin(double value);\n");
    out.push_str("extern double cobol_func_cos(double value);\n");
    out.push_str("extern double cobol_func_tan(double value);\n");
    out.push_str("extern double cobol_func_asin(double value);\n");
    out.push_str("extern double cobol_func_acos(double value);\n");
    out.push_str("extern double cobol_func_atan(double value);\n");
    out.push_str("extern int64_t cobol_func_ceiling(double value);\n");
    out.push_str("extern int64_t cobol_func_floor(double value);\n");
    out.push_str("extern int64_t cobol_func_factorial(int64_t n);\n");
    out.push_str("extern double cobol_func_rem(double a, double b);\n");
    out.push_str("extern double cobol_func_random(int64_t seed);\n");
    out.push_str("extern int64_t cobol_func_sign(int64_t value);\n");
    out.push_str("extern double cobol_func_mean(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_median(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_midrange(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_range(const double* values, int32_t count);\n");
    out.push_str(
        "extern double cobol_func_standard_deviation(const double* values, int32_t count);\n",
    );
    out.push_str("extern double cobol_func_variance(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_sum_float(const double* values, int32_t count);\n");
    out.push_str("extern double cobol_func_annuity(double rate, int64_t periods);\n");
    out.push_str("extern double cobol_func_present_value(double rate, const double* values, int32_t count);\n");
    out.push_str("/* Date/time intrinsic function declarations */\n");
    out.push_str("extern int64_t cobol_func_integer_of_date(int64_t yyyymmdd);\n");
    out.push_str("extern int64_t cobol_func_date_of_integer(int64_t day_count);\n");
    out.push_str("extern int64_t cobol_func_integer_of_day(int64_t yyyyddd);\n");
    out.push_str("extern int64_t cobol_func_day_of_integer(int64_t day_count);\n");
    out.push_str("extern int64_t cobol_func_date_to_yyyymmdd(int64_t yymmdd, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_year_to_yyyy(int64_t yy, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_day_to_yyyyddd(int64_t yyddd, int64_t pivot);\n");
    out.push_str("extern int64_t cobol_func_test_date_yyyymmdd(int64_t yyyymmdd);\n");
    out.push_str("extern int64_t cobol_func_test_day_yyyyddd(int64_t yyyyddd);\n");
    out.push_str("extern uint32_t cobol_func_when_compiled(uint8_t* buf, uint32_t buf_len);\n");
    out.push_str("extern int64_t cobol_func_max_int_n(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_min_int_n(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_ord_max(const int64_t* values, int32_t count);\n");
    out.push_str("extern int64_t cobol_func_ord_min(const int64_t* values, int32_t count);\n");
    out.push_str(
        "extern int32_t cobol_func_max_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int32_t cobol_func_min_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int64_t cobol_func_ord_max_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern int64_t cobol_func_ord_min_alpha(const uint8_t** ptrs, const uint32_t* lens, int32_t count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_func_stored_char_length(const uint8_t* ptr, uint32_t len);\n",
    );
    out.push_str("/* COBOL 2002+ runtime declarations */\n");
    out.push_str(
        "extern void cobol_raise(const char* exception_name) __attribute__((noreturn));\n",
    );
    out.push_str("extern void cobol_resume(const char* target);\n");
    out.push_str("extern void cobol_exception_push(uintptr_t jmp_buf_ptr);\n");
    out.push_str("extern void cobol_exception_pop(void);\n");
    out.push_str("extern int32_t cobol_exception_code(void);\n");
    out.push_str("extern void cobol_exception_clear(void);\n");
    out.push_str(
        "extern int64_t cobol_invoke(void* obj, const char* method, int64_t* args, int32_t argc);\n",
    );
    out.push_str("/* COBOL 2014+ runtime declarations */\n");
    out.push_str("extern void cobol_validate(const char* target_name);\n");
    out.push_str(
        "extern uint32_t cobol_json_generate(const void* fields, uint32_t field_count, uint8_t* output, uint32_t output_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_json_parse(const uint8_t* json, uint32_t json_len, void* fields, uint32_t field_count);\n",
    );
    out.push_str(
        "extern uint32_t cobol_xml_generate(const void* fields, uint32_t field_count, const uint8_t* root_name, uint32_t root_name_len, uint8_t* output, uint32_t output_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_xml_parse(const uint8_t* xml, uint32_t xml_len, void (*callback)(uint32_t, const uint8_t*, uint32_t, const uint8_t*, uint32_t));\n",
    );
    out.push_str("/* COBOL 2023+ runtime declarations */\n");
    out.push_str("extern uint32_t cobol_utf8_char_count(const uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str(
        "extern uint32_t cobol_utf8_substring(const uint8_t* src, uint32_t src_len, uint32_t start_char, uint32_t char_count, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern uint32_t cobol_utf8_upper(uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str("extern uint32_t cobol_utf8_lower(uint8_t* ptr, uint32_t byte_len);\n");
    out.push_str("extern uint64_t cobol_thread_create(void (*func)(void*), void* arg);\n");
    out.push_str("extern uint32_t cobol_thread_join(uint64_t handle);\n");
    out.push_str("extern uint64_t cobol_mutex_create(void);\n");
    out.push_str("extern void cobol_mutex_lock(uint64_t handle);\n");
    out.push_str("extern void cobol_mutex_unlock(uint64_t handle);\n");
    out.push_str("extern void cobol_mutex_destroy(uint64_t handle);\n");
    // CobolDecimal arithmetic
    out.push_str("/* Decimal arithmetic runtime declarations */\n");
    out.push_str("typedef struct { int64_t value; int32_t scale; int32_t size; int8_t is_signed; } CobolDecimal;\n");
    out.push_str("extern void cobol_decimal_add(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n");
    out.push_str("extern void cobol_decimal_sub(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n");
    out.push_str("extern void cobol_decimal_mul(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n");
    out.push_str("extern void cobol_decimal_div(const CobolDecimal* a, const CobolDecimal* b, CobolDecimal* result);\n");
    out.push_str(
        "extern int32_t cobol_decimal_cmp(const CobolDecimal* a, const CobolDecimal* b);\n",
    );
    out.push_str(
        "extern void cobol_decimal_from_int(int64_t value, int32_t scale, CobolDecimal* result);\n",
    );
    out.push_str("extern int64_t cobol_decimal_to_int64(const CobolDecimal* d);\n");
    out.push_str("extern double cobol_decimal_to_double(const CobolDecimal* d);\n");
    out.push_str("extern void cobol_decimal_from_double(double val, CobolDecimal* result);\n");
    out.push_str("extern void cobol_decimal_from_string(const uint8_t* ptr, uint32_t len, CobolDecimal* result);\n");
    out.push_str("extern uint32_t cobol_decimal_to_display(const CobolDecimal* dec, uint8_t* buf, uint32_t buf_len, const uint8_t* pic_ptr, uint32_t pic_len);\n");
    // Screen section runtime declarations
    out.push_str("/* Screen section runtime declarations */\n");
    out.push_str("extern void cobol_screen_position(int32_t line, int32_t col);\n");
    out.push_str("extern void cobol_screen_clear(void);\n");
    out.push_str("extern void cobol_screen_clear_line(void);\n");
    out.push_str("extern void cobol_screen_highlight_on(void);\n");
    out.push_str("extern void cobol_screen_highlight_off(void);\n");
    out.push_str("extern void cobol_screen_reverse_on(void);\n");
    out.push_str("extern void cobol_screen_reverse_off(void);\n");
    out.push_str("extern void cobol_screen_reset_attrs(void);\n");
    // NATIONAL (PIC N) runtime declarations
    out.push_str("/* NATIONAL (PIC N) runtime declarations */\n");
    out.push_str(
        "extern uint32_t cobol_func_national_of(const uint8_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern uint32_t cobol_func_display_of(const uint16_t* src, uint32_t src_len, uint8_t* dst, uint32_t dst_len);\n",
    );
    out.push_str(
        "extern void cobol_move_to_national(const uint8_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push_str("extern void cobol_display_national(const uint16_t* ptr, uint32_t len);\n");
    out.push_str(
        "extern void cobol_move_national_to_national(const uint16_t* src, uint32_t src_len, uint16_t* dst, uint32_t dst_len);\n",
    );
    out.push('\n');
}

fn emit_classes(out: &mut String, classes: &[cobol_hir::HirClass]) {
    let empty_records: HashMap<smol_str::SmolStr, smol_str::SmolStr> = HashMap::new();
    let ctx = CodegenContext::new(&[], &empty_records);
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
                emit_statement_with_ctx(out, stmt, &[], &[], &HashMap::new(), false, &ctx, 1);
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
                emit_statement_with_ctx(out, stmt, &[], &[], &HashMap::new(), false, &ctx, 1);
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
    let ctx = CodegenContext::new(&[], &empty_records);
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
            emit_statement_with_ctx(out, stmt, &[], &[], &HashMap::new(), false, &ctx, 1);
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
