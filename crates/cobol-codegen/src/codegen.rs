// COBOL Code Generation - C output backend
//
// Translates HIR into C source code that calls the COBOL runtime library.
// The generated C code is then compiled with clang/cc and linked against
// the runtime's static library to produce a native executable.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use cobol_hir::{
    HirAcceptSource, HirBinOp, HirClassType, HirCompareOp, HirCondition, HirDataItem, HirExpr,
    HirFileInfo, HirLiteral, HirMoveTarget, HirOpenMode, HirParagraph, HirPerformKind, HirProgram,
    HirStartRelation, HirStatement, HirType, HirUnaryOp,
};

/// Maps sanitized file name -> sanitized FILE STATUS variable name.
type FileStatusMap = HashMap<String, String>;

thread_local! {
    /// When true, GO TO generates C `goto lbl_XXX;` instead of function call + stop_run.
    /// Set to true while emitting the body of main() where paragraph labels are available.
    static IN_BODY_CONTEXT: RefCell<bool> = const { RefCell::new(false) };
    /// Maps sanitized label names to integer IDs for the goto dispatch mechanism.
    /// Used by paragraph functions to set `_goto_target` so the body can dispatch via goto.
    static GOTO_LABEL_MAP: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    /// Counter for generating unique PERFORM THRU dispatch label names.
    static PERFORM_THRU_COUNTER: RefCell<usize> = const { RefCell::new(0) };
}

/// Maps sanitized file name -> sanitized FD/SD first-record variable name.
type FileRecordMap = HashMap<String, String>;

/// Describes the path segments from a top-level group root to a data item,
/// recording which segments carry an OCCURS dimension.
/// Used by `emit_subscript_access` to generate correct multi-dimensional C access.
#[derive(Debug, Clone)]
struct SubscriptPathInfo {
    /// Ordered list of path segments from the root group to the item.
    /// Each entry: (segment_suffix, has_occurs)
    /// - `segment_suffix`: e.g. `.members._m_GRP_ENTRY`
    /// - `has_occurs`: if true, a subscript `[(idx)-1]` must be inserted after this segment
    segments: Vec<(String, bool)>,
    /// The root group's C name (e.g., `TABLE_1`)
    root: String,
}

thread_local! {
    /// Pre-computed map from sanitized variable name to its subscript path info.
    /// Populated at the start of `generate_c` and used by `emit_subscript_access`.
    static SUBSCRIPT_PATHS: RefCell<HashMap<String, SubscriptPathInfo>> =
        RefCell::new(HashMap::new());

    /// Pre-computed map from sanitized file name to sanitized FD/SD record name.
    /// Used by READ/SORT codegen to resolve the correct record buffer variable.
    static FILE_RECORD_MAP: RefCell<FileRecordMap> =
        RefCell::new(HashMap::new());

    /// Set of sanitized variable names that are CobolDecimal type.
    /// Populated at the start of `generate_c` and used by `emit_expr` to
    /// auto-convert CobolDecimal variables to int64 in expression contexts
    /// (e.g., function call arguments cast to double).
    static DECIMAL_NAMES: RefCell<HashSet<String>> =
        RefCell::new(HashSet::new());

    /// Set of sanitized variable names that are Group type (emitted as C unions).
    /// Used by `emit_expr_as_numeric` to avoid passing a union value where
    /// an arithmetic or pointer type is expected.
    static GROUP_NAMES: RefCell<HashSet<String>> =
        RefCell::new(HashSet::new());
}

/// Build the subscript path map for all data items inside groups with OCCURS.
fn build_subscript_paths(data_items: &[HirDataItem]) -> HashMap<String, SubscriptPathInfo> {
    let mut map = HashMap::new();
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            let root = sanitize_name(&item.name);
            let root_has_occurs = item.occurs.is_some();
            // REDEFINES groups are plain structs (no union/.members wrapper),
            // so their immediate children use `._m_CHILD` not `.members._m_CHILD`.
            let root_is_redefines = item.redefines.is_some();
            collect_subscript_paths(
                &mut map,
                members,
                &root,
                &[],
                root_has_occurs,
                root_is_redefines,
            );
        }
    }
    map
}

/// Build a set of sanitized variable names whose C type is CobolDecimal.
/// Used by `emit_expr` to auto-convert decimal variables to int64 in
/// expression contexts (e.g., function call arguments, casts).
fn build_decimal_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_decimal_names(&mut set, data_items);
    set
}

fn collect_decimal_names(set: &mut HashSet<String>, data_items: &[HirDataItem]) {
    for item in data_items {
        if needs_decimal(&item.data_type) {
            set.insert(sanitize_name(&item.name));
        }
        if let HirType::Group { members, .. } = &item.data_type {
            collect_decimal_names(set, members);
        }
    }
}

fn build_group_names(data_items: &[HirDataItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    collect_group_names(&mut set, data_items);
    set
}

fn collect_group_names(set: &mut HashSet<String>, data_items: &[HirDataItem]) {
    for item in data_items {
        if let HirType::Group { members, .. } = &item.data_type {
            set.insert(sanitize_name(&item.name));
            collect_group_names(set, members);
        }
    }
}

/// Recursively walk group members and build path info for each leaf/member
/// that is reachable through at least one OCCURS ancestor (or has OCCURS itself).
fn collect_subscript_paths(
    map: &mut HashMap<String, SubscriptPathInfo>,
    members: &[HirDataItem],
    root: &str,
    ancestor_segments: &[(String, bool)],
    root_has_occurs: bool,
    parent_is_redefines: bool,
) {
    for member in members {
        if member.redefines.is_some() || member.renames.is_some() {
            continue;
        }
        let c_name = sanitize_name(&member.name);
        let member_has_occurs = member.occurs.is_some();
        // REDEFINES groups are plain structs without union wrapper,
        // so skip `.members` for direct children of a REDEFINES root.
        let segment_suffix = if parent_is_redefines {
            format!("._m_{c_name}")
        } else {
            format!(".members._m_{c_name}")
        };
        let mut segments: Vec<(String, bool)> = ancestor_segments.to_vec();
        segments.push((segment_suffix, member_has_occurs));

        let any_occurs = root_has_occurs || segments.iter().any(|(_, has)| *has);
        if any_occurs {
            let new_occurs_count = segments.iter().filter(|(_, has)| *has).count();
            // Only insert if this path has more OCCURS levels than any existing entry.
            // The flat data_items list may produce the same item via multiple group
            // roots; we want the deepest path (most OCCURS dimensions).
            let should_insert = match map.get(&c_name) {
                Some(existing) => {
                    let existing_count = existing.segments.iter().filter(|(_, has)| *has).count();
                    new_occurs_count > existing_count
                }
                None => true,
            };
            if should_insert {
                map.insert(
                    c_name.clone(),
                    SubscriptPathInfo {
                        segments: segments.clone(),
                        root: root.to_string(),
                    },
                );
            }
        }

        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_subscript_paths(map, sub_members, root, &segments, root_has_occurs, false);
        }
    }
}

/// Generates C source code from a HIR program.
pub fn generate_c(program: &HirProgram) -> String {
    let mut out = String::new();

    // Pre-compute subscript path info for nested OCCURS groups.
    // This allows emit_subscript_access to generate correct multi-dimensional
    // C struct access paths without needing data_items at every call site.
    let paths = build_subscript_paths(&program.data_items);
    SUBSCRIPT_PATHS.with(|cell| {
        *cell.borrow_mut() = paths;
    });

    // Build sanitized file-name → record-name map and store in thread-local.
    let fr_map: FileRecordMap = program
        .file_records
        .iter()
        .map(|(f, r)| (sanitize_name(f), sanitize_name(r)))
        .collect();
    FILE_RECORD_MAP.with(|cell| {
        *cell.borrow_mut() = fr_map;
    });

    // Build the set of decimal variable names for emit_expr auto-conversion.
    let decimal_names = build_decimal_names(&program.data_items);
    DECIMAL_NAMES.with(|cell| {
        *cell.borrow_mut() = decimal_names;
    });

    // Build the set of group variable names (C unions) for emit_expr_as_numeric.
    let group_names = build_group_names(&program.data_items);
    GROUP_NAMES.with(|cell| {
        *cell.borrow_mut() = group_names;
    });

    // Header
    emit_header(&mut out);

    // Runtime function declarations
    emit_runtime_declarations(&mut out);

    // Global data items
    emit_data_items(&mut out, &program.data_items);

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
    GOTO_LABEL_MAP.with(|map| *map.borrow_mut() = label_map.clone());
    PERFORM_THRU_COUNTER.with(|c| *c.borrow_mut() = 0);

    // Emit goto dispatch variable if needed
    if has_labels {
        out.push_str("static int _goto_target = 0;\n\n");
    }

    // Main function
    out.push_str("int main(int argc, char** argv) {\n");

    // Initialize data items
    emit_data_init(&mut out, &program.data_items);

    let has_decl = !program.declaratives.is_empty();

    // Emit body statements (with GO TO -> C goto support)
    IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = true);
    for stmt in &program.body {
        emit_statement(
            &mut out,
            stmt,
            &program.data_items,
            &program.paragraphs,
            &fs_map,
            has_decl,
            1,
        );
    }
    IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = false);

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
    for para in &program.paragraphs {
        let c_name = sanitize_name(&para.name);
        out.push_str(&format!("\nstatic void para_{c_name}(void) {{\n"));
        for stmt in &para.body {
            emit_statement(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                1,
            );
        }
        out.push_str("}\n");
    }

    // Emit declarative handler function definitions
    for decl in &program.declaratives {
        let c_name = sanitize_name(&decl.name);
        out.push_str(&format!("\nstatic void decl_{c_name}(void) {{\n"));
        for stmt in &decl.body {
            emit_statement(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
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
        IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = true);
        for stmt in &program.body {
            emit_statement(
                &mut out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                1,
            );
        }
        IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = false);
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
}

/// Emit a nested (contained) program as a callable C function.
fn emit_nested_program(out: &mut String, program: &HirProgram) {
    let prog_name = sanitize_name(&program.name);

    // Pre-compute subscript path info for this nested program.
    let paths = build_subscript_paths(&program.data_items);
    SUBSCRIPT_PATHS.with(|cell| {
        *cell.borrow_mut() = paths;
    });

    let fr_map: FileRecordMap = program
        .file_records
        .iter()
        .map(|(f, r)| (sanitize_name(f), sanitize_name(r)))
        .collect();
    FILE_RECORD_MAP.with(|cell| {
        *cell.borrow_mut() = fr_map;
    });

    let decimal_names = build_decimal_names(&program.data_items);
    DECIMAL_NAMES.with(|cell| {
        *cell.borrow_mut() = decimal_names;
    });

    let group_names = build_group_names(&program.data_items);
    GROUP_NAMES.with(|cell| {
        *cell.borrow_mut() = group_names;
    });

    // Emit data items as function-scope statics
    emit_data_items(out, &program.data_items);

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

    IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = true);
    for stmt in &program.body {
        emit_statement(
            out,
            stmt,
            &program.data_items,
            &program.paragraphs,
            &fs_map,
            has_decl,
            1,
        );
    }
    IN_BODY_CONTEXT.with(|flag| *flag.borrow_mut() = false);

    if !label_map.is_empty() {
        GOTO_LABEL_MAP.with(|map| *map.borrow_mut() = label_map.clone());
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
            emit_statement(
                out,
                stmt,
                &program.data_items,
                &program.paragraphs,
                &fs_map,
                has_decl,
                1,
            );
        }
        out.push_str("}\n");
    }

    // Recursively emit any further nested programs
    for nested in &program.nested_programs {
        emit_nested_program(out, nested);
    }
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

fn emit_data_items(out: &mut String, items: &[HirDataItem]) {
    if items.is_empty() {
        return;
    }
    // Collect names that are members of groups (emitted inside struct).
    // These should be skipped when they appear as top-level items to avoid
    // redefinition conflicts with the #define macros.
    let group_member_names = collect_group_member_names(items);
    // Collect member names that appear in multiple groups — these should
    // NOT get unqualified #define macros to avoid redefinition warnings.
    let duplicate_member_names = collect_duplicate_member_names(items);

    let mut emitted_typedefs = HashSet::new();
    out.push_str("/* Data items */\n");
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue; // Already emitted as part of a group struct
        }
        emit_single_data_item(out, item, &duplicate_member_names, &mut emitted_typedefs);
    }
    out.push('\n');
}

/// Collect sanitized names of all items that are members of a group.
fn collect_group_member_names(items: &[HirDataItem]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for item in items {
        if let HirType::Group { members, .. } = &item.data_type {
            collect_member_names_recursive(members, &mut names);
        }
    }
    names
}

fn collect_member_names_recursive(members: &[HirDataItem], names: &mut BTreeSet<String>) {
    for member in members {
        // RENAMES (level 66) items are aliases emitted at the top level,
        // not struct members. Exclude them so they're not skipped.
        if member.renames.is_some() {
            continue;
        }
        names.insert(sanitize_name(&member.name));
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            collect_member_names_recursive(sub_members, names);
        }
    }
}

/// Collect member names that appear in more than one top-level group.
/// These names should only get qualified #define macros, not unqualified ones.
/// Sub-groups that are members of other groups are excluded to avoid false duplicates.
fn collect_duplicate_member_names(items: &[HirDataItem]) -> BTreeSet<String> {
    let group_member_names = collect_group_member_names(items);
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for item in items {
        let c_name = sanitize_name(&item.name);
        // Skip sub-groups that are members of other groups
        if group_member_names.contains(&c_name) {
            continue;
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let mut group_names = BTreeSet::new();
            collect_member_names_recursive(members, &mut group_names);
            for name in &group_names {
                if !seen.insert(name.clone()) {
                    duplicates.insert(name.clone());
                }
            }
        }
    }
    duplicates
}

fn emit_single_data_item(
    out: &mut String,
    item: &HirDataItem,
    duplicate_member_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
) {
    let c_name = sanitize_name(&item.name);

    // RENAMES (level 66): emit a #define alias, no variable declaration
    if let Some((ref from, ref _thru)) = item.renames {
        let c_from = sanitize_name(from);
        out.push_str(&format!(
            "#define {c_name} {c_from} /* RENAMES {c_from} */\n"
        ));
        return;
    }

    // REDEFINES: overlay on another item's memory via #define with cast
    if let Some(ref redef_name) = item.redefines {
        let c_redef = sanitize_name(redef_name);
        let c_type = c_type_for_hir_type(&item.data_type);
        match &item.data_type {
            HirType::Alphanumeric { .. } | HirType::National { .. } => {
                // Array types: cast to pointer (acts as array base for memset/strncpy)
                out.push_str(&format!(
                    "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                ));
            }
            HirType::Group { members, .. } => {
                // Group REDEFINES: reinterpret as the group's struct type
                let td_name = emit_group_typedefs(out, &c_name, members, emitted_typedefs);
                out.push_str(&format!(
                    "#define {c_name} (*({td_name}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                ));
                // Emit #define macros for children of this REDEFINES group
                // Note: REDEFINES group is a struct, not a union, so no .members wrapper
                emit_group_macros(out, members, &c_name, &c_name, duplicate_member_names);
                // Also emit REDEFINES members within this group so that
                // nested REDEFINES children get their #define macros.
                emit_group_redefines(
                    out,
                    members,
                    &c_name,
                    duplicate_member_names,
                    emitted_typedefs,
                );
            }
            _ => {
                if item.occurs.is_some() {
                    // REDEFINES + OCCURS: cast to pointer so it acts as an array base
                    out.push_str(&format!(
                        "#define {c_name} (({c_type}*)&{c_redef}) /* REDEFINES {c_redef} OCCURS */\n"
                    ));
                } else {
                    // Scalar types: dereference cast for lvalue semantics
                    out.push_str(&format!(
                        "#define {c_name} (*({c_type}*)&{c_redef}) /* REDEFINES {c_redef} */\n"
                    ));
                }
            }
        }
        return;
    }

    let array_suffix = if let Some(n) = item.occurs {
        format!("[{n}]")
    } else {
        String::new()
    };
    match &item.data_type {
        HirType::Alphanumeric { size } => {
            if item.occurs.is_some() {
                out.push_str(&format!(
                    "static char {c_name}{array_suffix}[{}];\n",
                    size + 1
                ));
            } else {
                out.push_str(&format!("static char {}[{}];\n", c_name, size + 1));
            }
        }
        HirType::National { size } => {
            if item.occurs.is_some() {
                out.push_str(&format!(
                    "static uint16_t {c_name}{array_suffix}[{size}];\n"
                ));
            } else {
                out.push_str(&format!("static uint16_t {c_name}[{size}];\n"));
            }
        }
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("static CobolDecimal {c_name}{array_suffix};\n"));
        }
        HirType::Numeric { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Group { members, .. } => {
            // Emit group as union of struct + byte array for group-level operations
            let td_name = emit_group_typedefs(out, &c_name, members, emitted_typedefs);
            out.push_str("static union {\n");
            out.push_str(&format!("    {td_name} members;\n"));
            out.push_str(&format!("    uint8_t _bytes[sizeof({td_name})];\n"));
            out.push_str(&format!("}} {c_name};\n"));
            // Generate macros for group members.
            // Qualified: #define GROUP__FIELD_A GROUP.members._m_FIELD_A (always unique)
            // Unqualified: #define FIELD_A ... (only if name is unique across groups)
            emit_group_macros(
                out,
                members,
                &c_name,
                &format!("{c_name}.members"),
                duplicate_member_names,
            );
            // Emit REDEFINES members as separate static pointers
            emit_group_redefines(
                out,
                members,
                &format!("{c_name}.members"),
                duplicate_member_names,
                emitted_typedefs,
            );
            out.push('\n');
        }
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("static CobolDecimal {c_name}{array_suffix};\n"));
        }
        HirType::Comp3 { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Binary { .. } => {
            out.push_str(&format!("static int64_t {c_name}{array_suffix};\n"));
        }
        HirType::Index => {
            out.push_str(&format!("static int64_t {c_name};\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("static void* {c_name};\n"));
        }
        HirType::Boolean => {
            out.push_str(&format!("static int8_t {c_name}{array_suffix};\n"));
        }
        HirType::FloatShort => {
            out.push_str(&format!("static float {c_name}{array_suffix};\n"));
        }
        HirType::FloatLong => {
            out.push_str(&format!("static double {c_name}{array_suffix};\n"));
        }
        HirType::FloatExtended => {
            out.push_str(&format!("static long double {c_name}{array_suffix};\n"));
        }
    }
}

/// Emit struct typedef(s) for a group and its nested groups (bottom-up).
/// Returns the actual typedef name used (may differ from `_grp_{c_name}_t`
/// if there was a naming collision, e.g. duplicate FILLER groups).
fn emit_group_typedefs(
    out: &mut String,
    c_name: &str,
    members: &[HirDataItem],
    emitted_typedefs: &mut HashSet<String>,
) -> String {
    // First, recurse into nested groups
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            let member_c_name = sanitize_name(&member.name);
            emit_group_typedefs(out, &member_c_name, sub_members, emitted_typedefs);
        }
    }
    // If this typedef name has already been emitted (e.g., duplicate FILLER groups
    // under different REDEFINES), make the name unique by appending a counter.
    let mut typedef_name = format!("_grp_{c_name}_t");
    if emitted_typedefs.contains(&typedef_name) {
        let mut counter = 2u32;
        loop {
            let candidate = format!("_grp_{c_name}_{counter}_t");
            if !emitted_typedefs.contains(&candidate) {
                typedef_name = candidate;
                break;
            }
            counter += 1;
        }
    }
    emitted_typedefs.insert(typedef_name.clone());
    // Emit this level's struct typedef
    out.push_str("typedef struct {\n");
    let mut member_name_counts: HashMap<String, u32> = HashMap::new();
    for member in members {
        if member.redefines.is_some() {
            continue; // REDEFINES handled separately
        }
        if member.renames.is_some() {
            continue; // RENAMES (level 66) are aliases, not separate storage
        }
        emit_group_struct_member(out, member, &mut member_name_counts);
    }
    out.push_str(&format!("}} {typedef_name};\n"));
    typedef_name
}

/// Emit a single member within a group struct typedef.
fn emit_group_struct_member(
    out: &mut String,
    member: &HirDataItem,
    member_name_counts: &mut HashMap<String, u32>,
) {
    let base_c_name = sanitize_name(&member.name);
    // Track member names to avoid duplicates (common with FILLER and implicit FILLER items)
    let count = member_name_counts.entry(base_c_name.clone()).or_insert(0);
    *count += 1;
    let c_name = if *count > 1 {
        format!("{}_{}", base_c_name, count)
    } else {
        base_c_name
    };
    let array_suffix = member.occurs.map_or(String::new(), |n| format!("[{n}]"));
    match &member.data_type {
        HirType::Alphanumeric { size } => {
            if member.occurs.is_some() {
                out.push_str(&format!(
                    "    char _m_{c_name}{array_suffix}[{}];\n",
                    size + 1
                ));
            } else {
                out.push_str(&format!("    char _m_{c_name}[{}];\n", size + 1));
            }
        }
        HirType::National { size } => {
            if member.occurs.is_some() {
                out.push_str(&format!(
                    "    uint16_t _m_{c_name}{array_suffix}[{size}];\n"
                ));
            } else {
                out.push_str(&format!("    uint16_t _m_{c_name}[{size}];\n"));
            }
        }
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("    CobolDecimal _m_{c_name}{array_suffix};\n"));
        }
        HirType::Numeric { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Group { .. } => {
            out.push_str(&format!(
                "    union {{ _grp_{c_name}_t members; uint8_t _bytes[sizeof(_grp_{c_name}_t)]; }} _m_{c_name}{array_suffix};\n"
            ));
        }
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => {
            out.push_str(&format!("    CobolDecimal _m_{c_name}{array_suffix};\n"));
        }
        HirType::Comp3 { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Binary { .. } => {
            out.push_str(&format!("    int64_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::Index => {
            out.push_str(&format!("    int64_t _m_{c_name};\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("    void* _m_{c_name};\n"));
        }
        HirType::Boolean => {
            out.push_str(&format!("    int8_t _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatShort => {
            out.push_str(&format!("    float _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatLong => {
            out.push_str(&format!("    double _m_{c_name}{array_suffix};\n"));
        }
        HirType::FloatExtended => {
            out.push_str(&format!("    long double _m_{c_name}{array_suffix};\n"));
        }
    }
}

/// Emit #define macros for all elementary members in a group.
fn emit_group_macros(
    out: &mut String,
    members: &[HirDataItem],
    group_c_name: &str,
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
) {
    for member in members {
        if member.redefines.is_some() {
            continue;
        }
        // RENAMES (level 66) are aliases — handled at top level
        if member.renames.is_some() {
            continue;
        }
        // FILLER items (and items misnamed "PIC" from implicit FILLER) are unnamed
        // padding; skip macro generation to avoid duplicate #define errors
        if member.name == "FILLER" || member.name == "PIC" {
            continue;
        }
        let c_name = sanitize_name(&member.name);
        let access_path = format!("{path_prefix}._m_{c_name}");
        // Unqualified macro: only if name is unique across all groups
        if !duplicate_names.contains(&c_name) {
            out.push_str(&format!("#define {c_name} {access_path}\n"));
        }
        // Qualified macro: GROUP__FIELD (always unique)
        out.push_str(&format!("#define {group_c_name}__{c_name} {access_path}\n"));
        if let HirType::Group {
            members: sub_members,
            ..
        } = &member.data_type
        {
            // For OCCURS items, child macros access element [0] (first
            // element) as a safe default.  Subscripted access is handled
            // by emit_subscript_access which generates proper indexed
            // paths at each OCCURS level.
            let sub_prefix = if member.occurs.is_some() {
                format!("{access_path}[0].members")
            } else {
                format!("{access_path}.members")
            };
            // Emit macros with both the top-level group qualifier and the
            // immediate sub-group qualifier.  This allows references like
            // `ALPHAN-KEY OF KEY-1` (which becomes KEY_1__ALPHAN_KEY in C)
            // to resolve correctly even when the same leaf name exists
            // under multiple sub-groups of the same top-level group.
            emit_group_macros(out, sub_members, group_c_name, &sub_prefix, duplicate_names);
            // Also emit macros qualified by the immediate parent sub-group
            // (e.g., KEY_1__ALPHAN_KEY) so that COBOL qualified references
            // like `ALPHAN-KEY OF KEY-1` map to the correct macro.
            if c_name != group_c_name {
                emit_group_macros(out, sub_members, &c_name, &sub_prefix, duplicate_names);
            }
        }
    }
}

/// Emit REDEFINES members within a group as #define macros with qualified paths.
fn emit_group_redefines(
    out: &mut String,
    members: &[HirDataItem],
    path_prefix: &str,
    duplicate_names: &BTreeSet<String>,
    emitted_typedefs: &mut HashSet<String>,
) {
    for member in members {
        if let Some(ref redef_name) = member.redefines {
            let c_name = sanitize_name(&member.name);
            let c_redef = sanitize_name(redef_name);
            let c_type = c_type_for_hir_type(&member.data_type);
            let qualified_target = format!("{path_prefix}._m_{c_redef}");
            match &member.data_type {
                HirType::Alphanumeric { .. } | HirType::National { .. } => {
                    out.push_str(&format!(
                        "#define {c_name} (({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */\n"
                    ));
                }
                HirType::Group {
                    members: grp_members,
                    ..
                } => {
                    let td_name = emit_group_typedefs(out, &c_name, grp_members, emitted_typedefs);
                    // Use direct cast expression for #define and child macros
                    // to avoid collisions when multiple REDEFINES groups share
                    // the same name (e.g. duplicate FILLER items).
                    let cast_expr = format!("(*({td_name}*)&{qualified_target})");
                    out.push_str(&format!(
                        "#define {c_name} {cast_expr} /* REDEFINES {c_redef} */\n"
                    ));
                    emit_group_macros(out, grp_members, &c_name, &cast_expr, duplicate_names);
                    // Recurse into this REDEFINES group to emit nested REDEFINES
                    emit_group_redefines(
                        out,
                        grp_members,
                        &cast_expr,
                        duplicate_names,
                        emitted_typedefs,
                    );
                }
                _ => {
                    if member.occurs.is_some() {
                        // REDEFINES + OCCURS: pointer cast (acts as array base)
                        out.push_str(&format!(
                            "#define {c_name} (({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} OCCURS */\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "#define {c_name} (*({c_type}*)&{qualified_target}) /* REDEFINES {c_redef} */\n"
                        ));
                    }
                }
            }
        }
        // Recurse into non-REDEFINES groups to find nested REDEFINES
        if member.redefines.is_none() {
            if let HirType::Group {
                members: sub_members,
                ..
            } = &member.data_type
            {
                let c_name = sanitize_name(&member.name);
                let sub_prefix = if member.occurs.is_some() {
                    format!("{path_prefix}._m_{c_name}[0].members")
                } else {
                    format!("{path_prefix}._m_{c_name}.members")
                };
                emit_group_redefines(
                    out,
                    sub_members,
                    &sub_prefix,
                    duplicate_names,
                    emitted_typedefs,
                );
            }
        }
    }
}

/// Return the C type string for a given HIR type.
fn c_type_for_hir_type(ty: &HirType) -> &'static str {
    match ty {
        HirType::Alphanumeric { .. } => "char",
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0 => "CobolDecimal",
        HirType::Numeric { .. } => "int64_t",
        HirType::Group { .. } => "char",
        HirType::Comp3 { decimal_places, .. } if *decimal_places > 0 => "CobolDecimal",
        HirType::Comp3 { .. } => "int64_t",
        HirType::Binary { .. } => "int64_t",
        HirType::Index => "int64_t",
        HirType::Pointer => "void",
        HirType::Boolean => "int8_t",
        HirType::FloatShort => "float",
        HirType::FloatLong => "double",
        HirType::FloatExtended => "long double",
        HirType::National { .. } => "uint16_t",
    }
}

fn emit_data_init(out: &mut String, items: &[HirDataItem]) {
    // Skip top-level items that are already members of a group
    // (they are initialized through the group's recursive init)
    let group_member_names = collect_group_member_names(items);
    for item in items {
        let c_name = sanitize_name(&item.name);
        if group_member_names.contains(&c_name) {
            continue;
        }
        // Skip REDEFINES/RENAMES items — they share memory with another item
        if item.redefines.is_some() || item.renames.is_some() {
            continue;
        }
        emit_single_data_init(out, item);
    }
}

fn emit_single_data_init(out: &mut String, item: &HirDataItem) {
    emit_single_data_init_with_prefix(out, item, None, None);
}

fn emit_single_data_init_with_prefix(
    out: &mut String,
    item: &HirDataItem,
    group_prefix: Option<&str>,
    disambiguated_name: Option<&str>,
) {
    let base_c_name =
        disambiguated_name.map_or_else(|| sanitize_name(&item.name), |s| s.to_string());
    // Use C struct access path when inside a group, not macro names
    let c_name = if let Some(prefix) = group_prefix {
        format!("{prefix}._m_{base_c_name}")
    } else {
        base_c_name.clone()
    };
    if let HirType::Group { members, .. } = &item.data_type {
        // If this group itself has OCCURS, zero-init the entire array of structs
        // rather than recursing into members (which would fail because we can't
        // access .members on an array element without a subscript).
        if item.occurs.is_some() {
            out.push_str(&format!("    memset(&{c_name}, 0, sizeof({c_name}));\n"));
            return;
        }
        // Initialize group members recursively with C struct access path
        let my_prefix = if let Some(prefix) = group_prefix {
            format!("{prefix}._m_{base_c_name}.members")
        } else {
            format!("{base_c_name}.members")
        };
        // Track member name counts to match the struct member naming
        // (e.g., FILLER -> _m_FILLER, _m_FILLER_2, _m_FILLER_3)
        let mut member_name_counts: HashMap<String, u32> = HashMap::new();
        for member in members {
            // Skip REDEFINES/RENAMES members — they share memory with another item
            if member.redefines.is_some() || member.renames.is_some() {
                continue;
            }
            let member_base = sanitize_name(&member.name);
            let count = member_name_counts.entry(member_base.clone()).or_insert(0);
            *count += 1;
            let deduped = if *count > 1 {
                format!("{}_{}", member_base, count)
            } else {
                member_base
            };
            emit_single_data_init_with_prefix(out, member, Some(&my_prefix), Some(&deduped));
        }
        return;
    }
    // OCCURS items: zero-initialize the entire array
    if let Some(n) = item.occurs {
        match &item.data_type {
            HirType::Numeric { .. }
            | HirType::Comp3 { .. }
            | HirType::Binary { .. }
            | HirType::Boolean => {
                out.push_str(&format!("    memset({c_name}, 0, sizeof({c_name}));\n"));
            }
            HirType::Alphanumeric { size } => {
                out.push_str(&format!(
                    "    for (int _i = 0; _i < {n}; _i++) {{ memset({c_name}[_i], ' ', {size}); {c_name}[_i][{size}] = '\\0'; }}\n"
                ));
            }
            HirType::National { size } => {
                out.push_str(&format!(
                    "    for (int _i = 0; _i < {n}; _i++) {{ for (uint32_t _j = 0; _j < {size}; _j++) {{ {c_name}[_i][_j] = 0x0020; }} }}\n"
                ));
            }
            _ => {
                out.push_str(&format!("    memset({c_name}, 0, sizeof({c_name}));\n"));
            }
        }
        return;
    }
    // CobolDecimal initialization
    if needs_decimal(&item.data_type) {
        let (size, decimal_places, is_signed) = match &item.data_type {
            HirType::Numeric {
                size,
                decimal_places,
                is_signed,
            } => (*size, *decimal_places, *is_signed),
            HirType::Comp3 {
                size,
                decimal_places,
            } => (*size, *decimal_places, true),
            _ => unreachable!(),
        };
        if let Some(init) = &item.initial_value {
            match init {
                HirLiteral::Integer(n) => {
                    // Integer VALUE for a decimal field: scale up
                    let scale = decimal_places;
                    let scaled = *n * 10_i64.pow(scale);
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = {scaled}, .scale = {scale}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                HirLiteral::Decimal(d) => {
                    // Parse decimal literal: "123.45" -> value=12345, scale=2
                    let (value, scale) = parse_decimal_literal(d);
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = {value}, .scale = {scale}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                HirLiteral::Zero => {
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
                _ => {
                    out.push_str(&format!(
                        "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                        if is_signed { 1 } else { 0 }
                    ));
                }
            }
        } else {
            out.push_str(&format!(
                "    {c_name} = (CobolDecimal){{ .value = 0, .scale = {decimal_places}, .size = {size}, .is_signed = {} }};\n",
                if is_signed { 1 } else { 0 }
            ));
        }
        return;
    }
    if let Some(init) = &item.initial_value {
        match (&item.data_type, init) {
            (HirType::Alphanumeric { size }, HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                out.push_str(&format!(
                    "    memset({c_name}, ' ', {size});\n    strncpy({c_name}, \"{escaped}\", {size});\n    {c_name}[{size}] = '\\0';\n"
                ));
            }
            (HirType::Alphanumeric { size }, HirLiteral::Space) => {
                out.push_str(&format!(
                    "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
                ));
            }
            (HirType::Alphanumeric { size }, HirLiteral::Zero) => {
                out.push_str(&format!(
                    "    memset({c_name}, '0', {size});\n    {c_name}[{size}] = '\\0';\n"
                ));
            }
            (
                HirType::Numeric { .. }
                | HirType::Index
                | HirType::Comp3 { .. }
                | HirType::Binary { .. }
                | HirType::Boolean
                | HirType::FloatShort
                | HirType::FloatLong
                | HirType::FloatExtended,
                HirLiteral::Integer(n),
            ) => {
                out.push_str(&format!("    {c_name} = {n};\n"));
            }
            (
                HirType::Numeric { .. }
                | HirType::Index
                | HirType::Comp3 { .. }
                | HirType::Binary { .. }
                | HirType::Boolean
                | HirType::FloatShort
                | HirType::FloatLong
                | HirType::FloatExtended,
                HirLiteral::Zero,
            ) => {
                out.push_str(&format!("    {c_name} = 0;\n"));
            }
            (HirType::National { size }, HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                out.push_str(&format!(
                    "    cobol_move_to_national((const uint8_t*)\"{escaped}\", {src_len}, {c_name}, {size});\n"
                ));
            }
            (HirType::National { size }, HirLiteral::Space) => {
                out.push_str(&format!(
                    "    for (uint32_t _i = 0; _i < {size}; _i++) {{ {c_name}[_i] = 0x0020; }}\n"
                ));
            }
            _ => {
                emit_default_init(out, &item.data_type, &c_name);
            }
        }
    } else {
        emit_default_init(out, &item.data_type, &c_name);
    }
}

fn emit_default_init(out: &mut String, data_type: &HirType, c_name: &str) {
    match data_type {
        HirType::Alphanumeric { size } => {
            out.push_str(&format!(
                "    memset({c_name}, ' ', {size});\n    {c_name}[{size}] = '\\0';\n"
            ));
        }
        HirType::Numeric { .. }
        | HirType::Index
        | HirType::Comp3 { .. }
        | HirType::Binary { .. }
        | HirType::Boolean => {
            out.push_str(&format!("    {c_name} = 0;\n"));
        }
        HirType::FloatShort | HirType::FloatLong | HirType::FloatExtended => {
            out.push_str(&format!("    {c_name} = 0.0;\n"));
        }
        HirType::Pointer => {
            out.push_str(&format!("    {c_name} = NULL;\n"));
        }
        HirType::National { size } => {
            // Fill with UTF-16 spaces (0x0020)
            out.push_str(&format!(
                "    for (uint32_t _i = 0; _i < {size}; _i++) {{ {c_name}[_i] = 0x0020; }}\n"
            ));
        }
        HirType::Group { members, .. } => {
            for member in members {
                emit_single_data_init(out, member);
            }
        }
    }
}

fn emit_statement(
    out: &mut String,
    stmt: &HirStatement,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    match stmt {
        HirStatement::Display {
            operands,
            no_advancing,
            ..
        } => {
            for (i, op) in operands.iter().enumerate() {
                if i > 0 {
                    // COBOL DISPLAY separates operands with a space by default
                    // (some implementations don't; we follow the common convention)
                }
                emit_display_operand(out, op, data_items, &pad);
            }
            if !no_advancing {
                out.push_str(&format!("{pad}cobol_display_newline();\n"));
            } else {
                out.push_str(&format!("{pad}cobol_display_flush();\n"));
            }
        }
        HirStatement::Move { from, to, .. } => {
            for target in to {
                match target {
                    HirMoveTarget::Variable(name) => {
                        let c_target = sanitize_name(name);
                        emit_move_to(out, from, name, &c_target, data_items, &pad);
                    }
                    HirMoveTarget::ReferenceModification {
                        variable,
                        start,
                        length,
                    } => {
                        emit_move_to_refmod(out, from, variable, start, length, data_items, &pad);
                    }
                    HirMoveTarget::Subscript {
                        variable,
                        subscripts,
                    } => {
                        let c_target = emit_subscript_access(variable, subscripts);
                        emit_move_to(out, from, variable, &c_target, data_items, &pad);
                    }
                }
            }
        }
        HirStatement::MoveCorresponding { from, to, .. } => {
            emit_corresponding_move(out, from, to, data_items, &pad);
        }
        HirStatement::AddCorresponding {
            from,
            to,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_err = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_err {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(out, from, to, "+", data_items, &pad);
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_err {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::SubtractCorresponding {
            from,
            to,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_err = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_err {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            emit_corresponding_arith(out, from, to, "-", data_items, &pad);
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_err {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Compute {
            targets,
            expr,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            for target in targets {
                let c_target = emit_expr(target);
                let target_name = match target {
                    HirExpr::Variable(name) => name.as_str(),
                    HirExpr::Subscript { variable, .. } => variable.as_str(),
                    _ => "",
                };
                let target_is_decimal = find_data_item(target_name, data_items)
                    .is_some_and(|i| needs_decimal(&i.data_type));
                if has_size_error {
                    let c_expr = emit_int_compatible_expr(expr, data_items);
                    emit_save_and_check_overflow(
                        out,
                        target_name,
                        &c_target,
                        &c_expr,
                        data_items,
                        &pad,
                    );
                } else if target_is_decimal {
                    emit_assign_to_decimal(out, expr, &c_target, data_items, &pad);
                } else {
                    let c_expr = emit_int_compatible_expr(expr, data_items);
                    out.push_str(&format!("{pad}{c_target} = {c_expr};\n"));
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
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
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // ADD a b GIVING c d -> c = a + b, d = a + b
                // All operands + TO values are summed, result goes to GIVING targets
                let mut all_addends: Vec<String> = operands
                    .iter()
                    .map(|o| emit_int_compatible_expr(o, data_items))
                    .collect();
                for t in to {
                    all_addends.push(emit_int_compatible_expr(t, data_items));
                }
                let sum_expr = all_addends.join(" + ");
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        // For decimal GIVING, build a temp sum then assign
                        out.push_str(&format!("{pad}/* ADD GIVING decimal */\n"));
                        // Use first two addends as decimal add, then chain
                        emit_decimal_giving_add(out, operands, to, &c_target, data_items, &pad);
                    } else if has_size_error {
                        out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                        out.push_str(&format!("{pad}{c_target} = {sum_expr};\n"));
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        out.push_str(&format!("{pad}{c_target} = {sum_expr};\n"));
                    }
                }
            } else {
                for target in to {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        for op in operands {
                            emit_decimal_arith(
                                out,
                                &c_target,
                                op,
                                "cobol_decimal_add",
                                data_items,
                                &pad,
                            );
                        }
                    } else {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} += {sum_expr};\n"));
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}{c_target} += {sum_expr};\n"));
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Subtract {
            operands,
            from,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // SUBTRACT a FROM b GIVING c -> c = b - a
                let sub_vals: Vec<String> = operands
                    .iter()
                    .map(|o| emit_int_compatible_expr(o, data_items))
                    .collect();
                let sub_expr = sub_vals.join(" + ");
                // The FROM value is the minuend
                let from_val = if let Some(f) = from.first() {
                    emit_int_compatible_expr(f, data_items)
                } else {
                    "0".to_string()
                };
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        // SUBTRACT GIVING decimal: result = from - sub
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int(\
                             {from_val} - ({sub_expr}), 0, &{c_target});\n"
                        ));
                    } else if has_size_error {
                        out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                        out.push_str(&format!("{pad}{c_target} = {from_val} - ({sub_expr});\n"));
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        out.push_str(&format!("{pad}{c_target} = {from_val} - ({sub_expr});\n"));
                    }
                }
            } else {
                for target in from {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        for op in operands {
                            emit_decimal_arith(
                                out,
                                &c_target,
                                op,
                                "cobol_decimal_sub",
                                data_items,
                                &pad,
                            );
                        }
                    } else {
                        let sum: Vec<_> = operands
                            .iter()
                            .map(|o| emit_int_compatible_expr(o, data_items))
                            .collect();
                        let sum_expr = sum.join(" + ");
                        if has_size_error {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} -= ({sum_expr});\n"));
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}{c_target} -= ({sum_expr});\n"));
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::If {
            condition,
            then_body,
            else_body,
            ..
        } => {
            let cond = emit_condition(condition, data_items);
            out.push_str(&format!("{pad}if ({cond}) {{\n"));
            for s in then_body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            if !else_body.is_empty() {
                out.push_str(&format!("{pad}}} else {{\n"));
                for s in else_body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                    );
                }
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Perform { kind, .. } => {
            emit_perform(
                out,
                kind,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
        }
        HirStatement::Multiply {
            operand,
            by,
            giving,
            on_size_error,
            not_on_size_error,
            ..
        } => {
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            if !giving.is_empty() {
                // MULTIPLY A BY B GIVING C [D ...]:  C = A * B
                let op_is_dec = is_decimal_expr(operand, data_items);
                let by_is_dec = by.first().is_some_and(|b| is_decimal_expr(b, data_items));
                let any_src_decimal = op_is_dec || by_is_dec;
                // For decimal operands, get raw expr (struct); for non-decimal, get int-compatible
                let c_operand_raw = emit_expr(operand);
                let c_operand_int = emit_int_compatible_expr(operand, data_items);
                let first_by_raw = by.first().map(emit_expr).unwrap_or_default();
                let first_by_int = by
                    .first()
                    .map(|b| emit_int_compatible_expr(b, data_items))
                    .unwrap_or_default();
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal || any_src_decimal {
                        // Decimal path: convert operands to CobolDecimal
                        let init_a = if op_is_dec {
                            format!("CobolDecimal _ma = {c_operand_raw};")
                        } else {
                            format!(
                                "CobolDecimal _ma; cobol_decimal_from_int({c_operand_int}, 0, &_ma);"
                            )
                        };
                        let init_b = if by_is_dec {
                            format!("CobolDecimal _mb = {first_by_raw};")
                        } else {
                            format!(
                                "CobolDecimal _mb; cobol_decimal_from_int({first_by_int}, 0, &_mb);"
                            )
                        };
                        out.push_str(&format!("{pad}{{ {init_a} {init_b} "));
                        out.push_str("CobolDecimal _mr; cobol_decimal_mul(&_ma, &_mb, &_mr); ");
                        if target_is_decimal {
                            out.push_str(&format!("{c_target} = _mr; }}\n"));
                        } else {
                            out.push_str(&format!(
                                "{c_target} = cobol_decimal_to_int64(&_mr); }}\n"
                            ));
                        }
                    } else {
                        let mul_expr = format!("{first_by_int} * {c_operand_int}");
                        if has_size_error {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} = {mul_expr};\n"));
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}{c_target} = {mul_expr};\n"));
                        }
                    }
                }
            } else {
                for target in by {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        emit_decimal_arith(
                            out,
                            &c_target,
                            operand,
                            "cobol_decimal_mul",
                            data_items,
                            &pad,
                        );
                    } else if is_decimal_expr(operand, data_items) {
                        // int64 target *= CobolDecimal operand: use decimal path
                        let c_operand = emit_expr(operand);
                        out.push_str(&format!(
                            "{pad}{{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                             cobol_decimal_mul(&_td, &{c_operand}, &_td); \
                             {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                        ));
                    } else {
                        let c_operand = emit_expr(operand);
                        if has_size_error {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!("{pad}{c_target} *= {c_operand};\n"));
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}{c_target} *= {c_operand};\n"));
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
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
            let has_size_error = !on_size_error.is_empty() || !not_on_size_error.is_empty();
            if has_size_error {
                out.push_str(&format!("{pad}{{ int _size_error = 0;\n"));
            }
            let op_is_dec = is_decimal_expr(operand, data_items);
            let into_is_dec = into.first().is_some_and(|i| is_decimal_expr(i, data_items));
            let any_src_decimal = op_is_dec || into_is_dec;
            let c_operand = emit_expr(operand);
            let c_operand_int = emit_int_compatible_expr(operand, data_items);
            if !giving.is_empty() {
                // DIVIDE A INTO B GIVING C: C = B / A
                let first_into = into.first().map(emit_expr).unwrap_or_default();
                let first_into_int = into
                    .first()
                    .map(|i| emit_int_compatible_expr(i, data_items))
                    .unwrap_or_default();
                for target in giving {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    if let Some(rem) = remainder {
                        let c_rem = emit_expr(rem);
                        out.push_str(&format!(
                            "{pad}{c_rem} = {first_into_int} % {c_operand_int};\n"
                        ));
                    }
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal && any_src_decimal {
                        // Use decimal division
                        let init_a = if into_is_dec {
                            format!("CobolDecimal _da = {first_into};")
                        } else {
                            format!(
                                "CobolDecimal _da; cobol_decimal_from_int({first_into_int}, 0, &_da);"
                            )
                        };
                        let init_b = if op_is_dec {
                            format!("CobolDecimal _db = {c_operand};")
                        } else {
                            format!(
                                "CobolDecimal _db; cobol_decimal_from_int({c_operand_int}, 0, &_db);"
                            )
                        };
                        out.push_str(&format!(
                            "{pad}{{ {init_a} {init_b} cobol_decimal_div(&_da, &_db, &{c_target}); }}\n"
                        ));
                    } else if target_is_decimal {
                        out.push_str(&format!(
                            "{pad}if ({c_operand_int} != 0) {{ \
                             cobol_decimal_from_int(\
                             {first_into_int} / {c_operand_int}, 0, &{c_target}); }}\n"
                        ));
                    } else if any_src_decimal {
                        out.push_str(&format!(
                            "{pad}{c_target} = {first_into_int} / {c_operand_int};\n"
                        ));
                    } else if has_size_error {
                        out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                        out.push_str(&format!(
                            "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} \
                             else {{ {c_target} = {first_into_int} / {c_operand_int}; }}\n"
                        ));
                        emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                        out.push_str(&format!("{pad}}}\n"));
                    } else {
                        out.push_str(&format!(
                            "{pad}{c_target} = {first_into_int} / {c_operand_int};\n"
                        ));
                    }
                }
            } else {
                for target in into {
                    let c_target = emit_expr(target);
                    let var_name = expr_var_name(target);
                    let target_is_decimal = find_data_item(var_name, data_items)
                        .is_some_and(|i| needs_decimal(&i.data_type));
                    if target_is_decimal {
                        emit_decimal_arith(
                            out,
                            &c_target,
                            operand,
                            "cobol_decimal_div",
                            data_items,
                            &pad,
                        );
                    } else if is_decimal_expr(operand, data_items) {
                        // int64 target /= CobolDecimal operand
                        out.push_str(&format!(
                            "{pad}{{ CobolDecimal _td; cobol_decimal_from_int({c_target}, 0, &_td); \
                             cobol_decimal_div(&_td, &{c_operand}, &_td); \
                             {c_target} = cobol_decimal_to_int64(&_td); }}\n"
                        ));
                    } else {
                        if let Some(rem) = remainder {
                            let c_rem = emit_expr(rem);
                            out.push_str(&format!(
                                "{pad}{c_rem} = {c_target} % {c_operand_int};\n"
                            ));
                        }
                        if has_size_error {
                            out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
                            out.push_str(&format!(
                                "{pad}if ({c_operand_int} == 0) {{ _size_error = 1; }} else {{ {c_target} /= {c_operand_int}; }}\n"
                            ));
                            emit_integer_overflow_check(out, var_name, &c_target, data_items, &pad);
                            out.push_str(&format!("{pad}}}\n"));
                        } else {
                            out.push_str(&format!("{pad}{c_target} /= {c_operand_int};\n"));
                        }
                    }
                }
            }
            emit_on_size_error(
                out,
                on_size_error,
                not_on_size_error,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent,
            );
            if has_size_error {
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Call {
            program,
            params,
            on_exception,
            not_on_exception,
            ..
        } => {
            // Extract the program name from the expression.
            // Distinguish static CALL (literal string) from dynamic CALL (variable).
            let (prog_name, is_dynamic) = match program {
                HirExpr::Literal(HirLiteral::String(s)) => (sanitize_name(s), false),
                HirExpr::Variable(name) => {
                    // Check if the variable is a data item (dynamic CALL) or
                    // could be a literal-like reference.
                    let sname = sanitize_name(name);
                    if find_data_item(name, data_items).is_some() {
                        (sname, true)
                    } else {
                        (sname, false)
                    }
                }
                _ => (emit_expr(program), false),
            };
            let has_exception_handlers = !on_exception.is_empty() || !not_on_exception.is_empty();
            out.push_str(&format!("{pad}/* CALL {prog_name} */\n"));
            if has_exception_handlers {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!("{pad}    volatile int _call_failed = 0;\n"));
            }
            let inner_pad = if has_exception_handlers {
                format!("{pad}    ")
            } else {
                pad.to_string()
            };
            if is_dynamic {
                // Dynamic CALL: resolve function at runtime via dlsym.
                // The variable contains the program name as a string.
                let param_count = params.len();
                if param_count == 0 {
                    out.push_str(&format!("{inner_pad}{{\n"));
                    out.push_str(&format!(
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({prog_name}, sizeof({prog_name}), _name, sizeof(_name));\n"
                    ));
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)(void) = (void(*)(void))dlsym(RTLD_DEFAULT, _name);\n"
                    ));
                    if has_exception_handlers {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp(); cobol_call_leave(); }} }}\n"
                        ));
                    }
                    out.push_str(&format!("{inner_pad}}}\n"));
                } else {
                    // Build param values
                    let mut param_values = Vec::new();
                    let mut content_copies = Vec::new();
                    for (i, p) in params.iter().enumerate() {
                        let arg = emit_expr(&p.expr);
                        match p.mode {
                            cobol_hir::HirParamMode::ByReference => {
                                param_values.push(format!("&{arg}"));
                            }
                            cobol_hir::HirParamMode::ByValue => {
                                let arg_int = emit_int_compatible_expr(&p.expr, data_items);
                                param_values.push(format!("(int64_t){arg_int}"));
                            }
                            cobol_hir::HirParamMode::ByContent => {
                                let copy_var = format!("_content_copy_{i}");
                                content_copies.push(format!(
                                    "{inner_pad}typeof({arg}) {copy_var} = {arg};\n"
                                ));
                                param_values.push(format!("&{copy_var}"));
                            }
                        }
                    }
                    out.push_str(&format!("{inner_pad}{{\n"));
                    for copy in &content_copies {
                        out.push_str(copy);
                    }
                    let values_str = param_values.join(", ");
                    // Build typedef for the function pointer type
                    let void_ptrs: Vec<&str> = (0..param_count).map(|_| "void*").collect();
                    let types_str = void_ptrs.join(", ");
                    out.push_str(&format!(
                        "{inner_pad}    char _name[256]; cobol_resolve_call_name({prog_name}, sizeof({prog_name}), _name, sizeof(_name));\n"
                    ));
                    out.push_str(&format!(
                        "{inner_pad}    void (*_fp)({types_str}) = (void(*)({types_str}))dlsym(RTLD_DEFAULT, _name);\n"
                    ));
                    if has_exception_handlers {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }} else {{ _call_failed = 1; }}\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{inner_pad}    if (_fp) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); _fp({values_str}); cobol_call_leave(); }} }}\n"
                        ));
                    }
                    out.push_str(&format!("{inner_pad}}}\n"));
                }
            } else if params.is_empty() {
                if has_exception_handlers {
                    // Use file-scope weak declaration for null check
                    out.push_str(&format!("{inner_pad}if ({prog_name}) {{\n"));
                    out.push_str(&format!(
                        "{inner_pad}    jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}(); cobol_call_leave(); }}\n"
                    ));
                    out.push_str(&format!("{inner_pad}}} else {{ _call_failed = 1; }}\n"));
                } else {
                    // Call via file-scope weak declaration — null-check
                    // to gracefully handle missing sub-programs.
                    out.push_str(&format!(
                        "{inner_pad}if ({prog_name}) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}(); cobol_call_leave(); }} }}\n"
                    ));
                }
            } else {
                // Wrap in a block to scope _content_copy_* variables
                // and avoid redefinition when multiple CALLs in same scope.
                out.push_str(&format!("{inner_pad}{{\n"));
                let call_pad = format!("{inner_pad}    ");
                // Build param types and values based on passing mode
                let mut param_types = Vec::new();
                let mut param_values = Vec::new();
                let mut content_copies = Vec::new();
                for (i, p) in params.iter().enumerate() {
                    let arg = emit_expr(&p.expr);
                    match p.mode {
                        cobol_hir::HirParamMode::ByReference => {
                            param_types.push("void*".to_string());
                            param_values.push(format!("&{arg}"));
                        }
                        cobol_hir::HirParamMode::ByValue => {
                            let arg_int = emit_int_compatible_expr(&p.expr, data_items);
                            param_types.push("int64_t".to_string());
                            param_values.push(format!("(int64_t){arg_int}"));
                        }
                        cobol_hir::HirParamMode::ByContent => {
                            // BY CONTENT: create a copy and pass address of the copy
                            let copy_var = format!("_content_copy_{i}");
                            content_copies
                                .push(format!("{call_pad}typeof({arg}) {copy_var} = {arg};\n"));
                            param_types.push("void*".to_string());
                            param_values.push(format!("&{copy_var}"));
                        }
                    }
                }
                for copy in &content_copies {
                    out.push_str(copy);
                }
                let _types_str = param_types.join(", ");
                let values_str = param_values.join(", ");
                if has_exception_handlers {
                    // Use file-scope weak declaration for null check
                    out.push_str(&format!("{call_pad}if ({prog_name}) {{\n"));
                    out.push_str(&format!(
                        "{call_pad}    jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}({values_str}); cobol_call_leave(); }}\n"
                    ));
                    out.push_str(&format!("{call_pad}}} else {{ _call_failed = 1; }}\n"));
                } else {
                    // Call via file-scope weak declaration — null-check
                    // to gracefully handle missing sub-programs.
                    out.push_str(&format!(
                        "{call_pad}if ({prog_name}) {{ jmp_buf _jbuf; if (setjmp(_jbuf) == 0) {{ cobol_call_enter((uintptr_t)&_jbuf); {prog_name}({values_str}); cobol_call_leave(); }} }}\n"
                    ));
                }
                out.push_str(&format!("{inner_pad}}}\n"));
            }
            if has_exception_handlers {
                emit_on_exception(
                    out,
                    on_exception,
                    not_on_exception,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirStatement::Open { entries, .. } => {
            for entry in entries {
                let c_name = sanitize_name(&entry.file_name);
                let mode_val = match entry.mode {
                    HirOpenMode::Input => 0,
                    HirOpenMode::Output => 1,
                    HirOpenMode::IoMode => 2,
                    HirOpenMode::Extend => 3,
                };
                let mode_comment = match entry.mode {
                    HirOpenMode::Input => "INPUT",
                    HirOpenMode::Output => "OUTPUT",
                    HirOpenMode::IoMode => "I-O",
                    HirOpenMode::Extend => "EXTEND",
                };
                // Determine record length from data items via FD record (default 80)
                let record_var = resolve_file_record(&c_name);
                let rec_len = find_record_len(&record_var, data_items);
                // Use ASSIGN TO path if available, otherwise fall back to file name
                let file_path_str = if entry.assign_to.is_empty() {
                    entry.file_name.as_str()
                } else {
                    entry.assign_to.as_str()
                };
                let escaped_name = escape_c_string(file_path_str);
                let name_len = file_path_str.len();
                let org_val = entry.organization;
                out.push_str(&format!("{pad}/* OPEN {mode_comment} {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, 0, {mode_val}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_open(FILE_ID_{c_name}, (const uint8_t*)\"{escaped_name}\", {name_len}, {org_val}, 0, {mode_val}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Close { files, .. } => {
            for file in files {
                let c_name = sanitize_name(file);
                out.push_str(&format!("{pad}/* CLOSE {c_name} */\n"));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_close(FILE_ID_{c_name});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}cobol_file_close(FILE_ID_{c_name});\n"));
                }
            }
        }
        HirStatement::Read {
            file_name,
            into,
            at_end,
            not_at_end,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            // Determine the target buffer: INTO variable if specified, else
            // look up the FD record name for this file, falling back to the
            // file name itself.
            let (target, target_name) = if let Some((into_var, into_subs)) = into {
                if into_subs.is_empty() {
                    let n = sanitize_name(into_var);
                    (n.clone(), n)
                } else {
                    let access = emit_subscript_access(into_var, into_subs);
                    let n = sanitize_name(into_var);
                    (access, n)
                }
            } else {
                let r = resolve_file_record(&c_name);
                (r.clone(), r)
            };
            let rec_len = find_record_len(&target_name, data_items);
            out.push_str(&format!("{pad}/* READ {c_name} */\n"));
            out.push_str(&format!(
                "{pad}{{\n{pad}    uint32_t _fs = cobol_file_read_next(FILE_ID_{c_name}, (uint8_t*)&{target}, {rec_len});\n"
            ));
            emit_file_status_update(
                out,
                &c_name,
                "_fs",
                fs_map,
                has_declaratives,
                &format!("{pad}    "),
            );
            if !at_end.is_empty() || !not_at_end.is_empty() {
                out.push_str(&format!("{pad}    if (_fs == 10) {{\n"));
                out.push_str(&format!("{pad}        /* AT END */\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                if !not_at_end.is_empty() {
                    out.push_str(&format!("{pad}    }} else {{\n"));
                    out.push_str(&format!("{pad}        /* NOT AT END */\n"));
                    for s in not_at_end {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Write {
            record_name,
            file_name,
            from,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                c_name.clone()
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* WRITE {c_name} */\n"));
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            if needs_rc {
                out.push_str(&format!("{pad}{{\n"));
                out.push_str(&format!(
                    "{pad}    uint32_t _wrc = cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                ));
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_wrc",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                }
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_wrc != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_wrc == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                let has_fs = fs_map.contains_key(&c_file);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_write(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Rewrite {
            record_name,
            file_name,
            from,
            ..
        } => {
            let c_name = sanitize_name(record_name);
            let c_file = if file_name.is_empty() {
                c_name.clone()
            } else {
                sanitize_name(file_name)
            };
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* REWRITE {c_name} */\n"));
            {
                let has_fs = fs_map.contains_key(&c_file);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_file,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!(
                        "{pad}cobol_file_rewrite(FILE_ID_{c_file}, (const uint8_t*)&{source}, {rec_len});\n"
                    ));
                }
            }
        }
        HirStatement::Delete { file_name, .. } => {
            let c_name = sanitize_name(file_name);
            out.push_str(&format!("{pad}/* DELETE {c_name} */\n"));
            {
                let has_fs = fs_map.contains_key(&c_name);
                if has_fs {
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    uint32_t _fs = cobol_file_delete(FILE_ID_{c_name});\n"
                    ));
                    emit_file_status_update(
                        out,
                        &c_name,
                        "_fs",
                        fs_map,
                        has_declaratives,
                        &format!("{pad}    "),
                    );
                    out.push_str(&format!("{pad}}}\n"));
                } else {
                    out.push_str(&format!("{pad}cobol_file_delete(FILE_ID_{c_name});\n"));
                }
            }
        }
        HirStatement::GoTo {
            targets,
            depending_on,
            ..
        } => {
            let in_body = IN_BODY_CONTEXT.with(|flag| *flag.borrow());
            if let Some(dep) = depending_on {
                let c_dep = sanitize_name(dep);
                out.push_str(&format!("{pad}switch ((int){c_dep}) {{\n"));
                for (i, target) in targets.iter().enumerate() {
                    let c_target = sanitize_name(target);
                    if in_body {
                        out.push_str(&format!("{pad}    case {}: goto lbl_{c_target};\n", i + 1));
                    } else {
                        let label_id =
                            GOTO_LABEL_MAP.with(|map| map.borrow().get(&c_target).copied());
                        if let Some(id) = label_id {
                            out.push_str(&format!(
                                "{pad}    case {}: _goto_target = {id}; return;\n",
                                i + 1
                            ));
                        } else {
                            out.push_str(&format!(
                                "{pad}    case {}: para_{c_target}(); return;\n",
                                i + 1
                            ));
                        }
                    }
                }
                out.push_str(&format!("{pad}    default: break;\n"));
                out.push_str(&format!("{pad}}}\n"));
            } else if let Some(target) = targets.first() {
                let c_target = sanitize_name(target);
                if in_body {
                    out.push_str(&format!("{pad}goto lbl_{c_target};\n"));
                } else {
                    let label_id = GOTO_LABEL_MAP.with(|map| map.borrow().get(&c_target).copied());
                    if let Some(id) = label_id {
                        out.push_str(&format!("{pad}_goto_target = {id}; return;\n"));
                    } else {
                        out.push_str(&format!("{pad}para_{c_target}(); return;\n"));
                    }
                }
            }
        }
        HirStatement::Initialize { targets, .. } => {
            for target in targets {
                let c_target = sanitize_name(target);
                emit_initialize_field(out, target, &c_target, data_items, &pad);
            }
        }
        HirStatement::Set { targets, value, .. } => {
            for target in targets {
                let c_target = sanitize_name(target);
                let target_is_decimal = find_data_item(target.as_str(), data_items)
                    .is_some_and(|i| needs_decimal(&i.data_type));
                if target_is_decimal {
                    emit_assign_to_decimal(out, value, &c_target, data_items, &pad);
                } else {
                    let c_value = emit_int_compatible_expr(value, data_items);
                    out.push_str(&format!("{pad}{c_target} = {c_value};\n"));
                }
            }
        }
        HirStatement::SetAddress { target, source, .. } => {
            let c_target = sanitize_name(target);
            let c_source = sanitize_name(source);
            out.push_str(&format!(
                "{pad}{c_target} = (void*){c_source}; /* SET ADDRESS OF */\n"
            ));
        }
        HirStatement::StringStmt {
            into,
            sources,
            on_overflow,
            ..
        } => {
            let c_into = sanitize_name(into);
            let into_size = find_data_item_size(&c_into, data_items);
            out.push_str(&format!("{pad}/* STRING INTO {c_into} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            let src_count = sources.len();
            // Emit source value and optional delimiter for each source
            for (i, src) in sources.iter().enumerate() {
                emit_string_source_value(out, &src.value, i, data_items, &pad);
                emit_string_source_delimiter(out, &src.delimiter, i, data_items, &pad);
            }
            // Build the CobolStringSource array
            out.push_str(&format!(
                "{pad}    struct {{ const uint8_t* ptr; uint32_t len; const uint8_t* delim_ptr; uint32_t delim_len; }} _sources[{src_count}];\n"
            ));
            for i in 0..src_count {
                out.push_str(&format!(
                    "{pad}    _sources[{i}].ptr = (const uint8_t*)_src_ptr_{i}; _sources[{i}].len = _src_len_{i}; _sources[{i}].delim_ptr = _delim_ptr_{i}; _sources[{i}].delim_len = _delim_len_{i};\n"
                ));
            }
            let into_ptr = c_ptr_expr(&c_into, data_items);
            out.push_str(&format!("{pad}    uint32_t _pointer = 1;\n"));
            out.push_str(&format!(
                "{pad}    int32_t _str_rc = cobol_string_concat(_sources, {src_count}, (uint8_t*){into_ptr}, {into_size}, &_pointer);\n"
            ));
            if !on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_str_rc != 0) {{\n"));
                for s in on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::UnstringStmt {
            source,
            delimiters,
            into,
            on_overflow,
            ..
        } => {
            let c_source = sanitize_name(source);
            let src_size = find_data_item_size(&c_source, data_items);
            let targets: Vec<_> = into.iter().map(|s| sanitize_name(s)).collect();
            let tgt_count = targets.len();
            out.push_str(&format!(
                "{pad}/* UNSTRING {c_source} INTO {} */\n",
                targets.join(", ")
            ));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    struct {{ uint8_t* ptr; uint32_t len; uint8_t* delimiter_ptr; uint32_t delimiter_len; uint32_t* count_ptr; }} _targets[{tgt_count}];\n"
            ));
            for (i, tgt) in targets.iter().enumerate() {
                let tgt_size = find_data_item_size(tgt, data_items);
                let tgt_ptr = c_ptr_expr(tgt, data_items);
                out.push_str(&format!(
                    "{pad}    _targets[{i}].ptr = (uint8_t*){tgt_ptr}; _targets[{i}].len = {tgt_size}; _targets[{i}].delimiter_ptr = NULL; _targets[{i}].delimiter_len = 0; _targets[{i}].count_ptr = NULL;\n"
                ));
            }
            out.push_str(&format!(
                "{pad}    uint32_t _pointer = 1; uint32_t _tallying = 0;\n"
            ));
            // Use the first delimiter if specified, otherwise split on spaces
            let (delim_ptr, delim_len) = if let Some(d) = delimiters.first() {
                match &d.value {
                    HirExpr::Literal(HirLiteral::String(s)) => {
                        let escaped = escape_c_string(s);
                        let len = s.len();
                        out.push_str(&format!(
                            "{pad}    static const uint8_t _ustr_delim[] = \"{escaped}\";\n"
                        ));
                        ("(const uint8_t*)_ustr_delim".to_string(), format!("{len}"))
                    }
                    HirExpr::Variable(name) => {
                        let c_d = sanitize_name(name);
                        let d_size = find_data_item_size(&c_d, data_items);
                        let d_ptr = c_ptr_expr(&c_d, data_items);
                        (format!("(const uint8_t*){d_ptr}"), format!("{d_size}"))
                    }
                    _ => ("(const uint8_t*)\" \"".to_string(), "1".to_string()),
                }
            } else {
                ("(const uint8_t*)\" \"".to_string(), "1".to_string())
            };
            let src_ptr = c_ptr_expr(&c_source, data_items);
            out.push_str(&format!(
                "{pad}    int32_t _ustr_rc = cobol_unstring((const uint8_t*){src_ptr}, {src_size}, {delim_ptr}, {delim_len}, _targets, {tgt_count}, &_pointer, &_tallying);\n"
            ));
            if !on_overflow.is_empty() {
                out.push_str(&format!("{pad}    if (_ustr_rc != 0) {{\n"));
                for s in on_overflow {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Accept { target, source, .. } => {
            let c_target = sanitize_name(target);
            let size = find_data_item_size(&c_target, data_items);
            out.push_str(&format!("{pad}/* ACCEPT {c_target} */\n"));
            match source {
                HirAcceptSource::Date => {
                    // ACCEPT FROM DATE: YYMMDD (6 digits)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    time_t _t = time(NULL); struct tm* _tm = localtime(&_t);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    {c_target} = (_tm->tm_year % 100) * 10000 + (_tm->tm_mon + 1) * 100 + _tm->tm_mday;\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::DateYyyymmdd => {
                    // ACCEPT FROM DATE YYYYMMDD: 8 digits
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    time_t _t = time(NULL); struct tm* _tm = localtime(&_t);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    {c_target} = (_tm->tm_year + 1900) * 10000 + (_tm->tm_mon + 1) * 100 + _tm->tm_mday;\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Day => {
                    // ACCEPT FROM DAY: YYDDD (Julian day)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    time_t _t = time(NULL); struct tm* _tm = localtime(&_t);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    {c_target} = (_tm->tm_year % 100) * 1000 + _tm->tm_yday + 1;\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::DayOfWeek => {
                    // ACCEPT FROM DAY-OF-WEEK: 1=Monday ... 7=Sunday
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    time_t _t = time(NULL); struct tm* _tm = localtime(&_t);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    {c_target} = _tm->tm_wday == 0 ? 7 : _tm->tm_wday;\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Time => {
                    // ACCEPT FROM TIME: HHMMSScc (8 digits)
                    out.push_str(&format!("{pad}{{\n"));
                    out.push_str(&format!(
                        "{pad}    time_t _t = time(NULL); struct tm* _tm = localtime(&_t);\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    {c_target} = _tm->tm_hour * 1000000 + _tm->tm_min * 10000 + _tm->tm_sec * 100;\n"
                    ));
                    out.push_str(&format!("{pad}}}\n"));
                }
                HirAcceptSource::Environment(env_name) => {
                    let c_env = sanitize_name(env_name);
                    out.push_str(&format!(
                        "{pad}{{ const char* _env = getenv(\"{c_env}\");\n"
                    ));
                    out.push_str(&format!(
                        "{pad}  if (_env) {{ strncpy((char*)&{c_target}, _env, {size}); }} }}\n"
                    ));
                }
                HirAcceptSource::Console => {
                    // Use &target to handle both char arrays and union (group) types
                    out.push_str(&format!("{pad}fgets((char*)&{c_target}, {size}, stdin);\n"));
                    out.push_str(&format!(
                        "{pad}((char*)&{c_target})[strcspn((char*)&{c_target}, \"\\n\")] = '\\0';\n"
                    ));
                }
            }
        }
        HirStatement::Sort {
            file_name,
            keys,
            using,
            giving,
            input_procedure,
            output_procedure,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let rec_len = find_record_len(&record_var, data_items);
            out.push_str(&format!("{pad}/* SORT {c_name} */\n"));
            let key_count = if keys.is_empty() { 1 } else { keys.len() };
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    struct {{ uint32_t offset; uint32_t length; uint8_t ascending; }} _sort_keys[{key_count}];\n"
            ));
            if keys.is_empty() {
                out.push_str(&format!(
                    "{pad}    _sort_keys[0].offset = 0; _sort_keys[0].length = {rec_len}; _sort_keys[0].ascending = 1;\n"
                ));
            } else {
                for (i, key) in keys.iter().enumerate() {
                    let ascending = matches!(key.order, cobol_hir::HirSortOrder::Ascending);
                    let asc_val: u8 = if ascending { 1 } else { 0 };
                    let field_names: Vec<_> = key.fields.iter().map(|f| f.as_str()).collect();
                    out.push_str(&format!(
                        "{pad}    _sort_keys[{i}].offset = 0; _sort_keys[{i}].length = {rec_len}; _sort_keys[{i}].ascending = {asc_val}; /* key: {} */\n",
                        field_names.join(", ")
                    ));
                }
            }
            if !using.is_empty() {
                // Read records from USING files into a dynamic buffer, then sort
                out.push_str(&format!("{pad}    uint32_t _sort_capacity = 64;\n"));
                out.push_str(&format!("{pad}    uint32_t _sort_count = 0;\n"));
                out.push_str(&format!(
                    "{pad}    uint8_t* _sort_buf = (uint8_t*)malloc(_sort_capacity * {rec_len});\n"
                ));
                for u in using {
                    let c_using = sanitize_name(u);
                    out.push_str(&format!(
                        "{pad}    /* USING {c_using}: read all records */\n"
                    ));
                    out.push_str(&format!(
                        "{pad}    cobol_file_open(FILE_ID_{c_using}, (const uint8_t*)\"{c_using}\", {using_name_len}, 1, 0, 0, {rec_len});\n",
                        using_name_len = c_using.len()
                    ));
                    out.push_str(&format!("{pad}    while (1) {{\n"));
                    out.push_str(&format!(
                        "{pad}        int32_t _rc = cobol_file_read_next(FILE_ID_{c_using}, (uint8_t*)&_sort_buf[_sort_count * {rec_len}], {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        if (_rc != 0) break;\n"));
                    out.push_str(&format!("{pad}        _sort_count++;\n"));
                    out.push_str(&format!(
                        "{pad}        if (_sort_count >= _sort_capacity) {{\n"
                    ));
                    out.push_str(&format!("{pad}            _sort_capacity *= 2;\n"));
                    out.push_str(&format!(
                        "{pad}            _sort_buf = (uint8_t*)realloc(_sort_buf, _sort_capacity * {rec_len});\n"
                    ));
                    out.push_str(&format!("{pad}        }}\n"));
                    out.push_str(&format!("{pad}    }}\n"));
                    out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_using});\n"));
                }
                out.push_str(&format!(
                    "{pad}    cobol_sort(_sort_buf, _sort_count, {rec_len}, _sort_keys, {key_count});\n"
                ));
                if !giving.is_empty() {
                    for g in giving {
                        let c_giving = sanitize_name(g);
                        out.push_str(&format!(
                            "{pad}    /* GIVING {c_giving}: write sorted records */\n"
                        ));
                        out.push_str(&format!(
                            "{pad}    cobol_file_open(FILE_ID_{c_giving}, (const uint8_t*)\"{c_giving}\", {giving_name_len}, 1, 0, 2, {rec_len});\n",
                            giving_name_len = c_giving.len()
                        ));
                        out.push_str(&format!(
                            "{pad}    for (uint32_t _si = 0; _si < _sort_count; _si++) {{\n"
                        ));
                        out.push_str(&format!(
                            "{pad}        cobol_file_write(FILE_ID_{c_giving}, (const uint8_t*)&_sort_buf[_si * {rec_len}], {rec_len});\n"
                        ));
                        out.push_str(&format!("{pad}    }}\n"));
                        out.push_str(&format!("{pad}    cobol_file_close(FILE_ID_{c_giving});\n"));
                    }
                }
                out.push_str(&format!("{pad}    free(_sort_buf);\n"));
            } else if input_procedure.is_some() || output_procedure.is_some() {
                // INPUT/OUTPUT PROCEDURE: call the procedure paragraphs
                if let Some((proc_name, thru)) = input_procedure {
                    let c_proc = sanitize_name(proc_name);
                    out.push_str(&format!("{pad}    /* INPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
                }
                // Sort the accumulated records
                out.push_str(&format!(
                    "{pad}    cobol_sort((uint8_t*)&{record_var}, 0, {rec_len}, _sort_keys, {key_count});\n"
                ));
                if let Some((proc_name, thru)) = output_procedure {
                    let c_proc = sanitize_name(proc_name);
                    out.push_str(&format!("{pad}    /* OUTPUT PROCEDURE {c_proc} */\n"));
                    out.push_str(&format!("{pad}    para_{c_proc}();\n"));
                    if let Some(thru_name) = thru {
                        let c_thru = sanitize_name(thru_name);
                        out.push_str(&format!("{pad}    para_{c_thru}();\n"));
                    }
                }
            } else {
                // No USING: sort in-place (record_count must be managed externally)
                out.push_str(&format!(
                    "{pad}    cobol_sort((uint8_t*)&{record_var}, 0, {rec_len}, _sort_keys, {key_count});\n"
                ));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Inspect { target, kind, .. } => {
            let c_target = sanitize_name(target);
            let target_size = find_data_item_size(&c_target, data_items);
            out.push_str(&format!("{pad}/* INSPECT {c_target} */\n"));
            match kind {
                cobol_hir::HirInspectKind::Tallying { tallying } => {
                    emit_inspect_tallying(out, &c_target, target_size, tallying, data_items, &pad);
                }
                cobol_hir::HirInspectKind::Replacing { replacing } => {
                    emit_inspect_replacing(
                        out,
                        &c_target,
                        target_size,
                        replacing,
                        data_items,
                        &pad,
                    );
                }
                cobol_hir::HirInspectKind::TallyingReplacing {
                    tallying,
                    replacing,
                } => {
                    emit_inspect_tallying(out, &c_target, target_size, tallying, data_items, &pad);
                    emit_inspect_replacing(
                        out,
                        &c_target,
                        target_size,
                        replacing,
                        data_items,
                        &pad,
                    );
                }
                cobol_hir::HirInspectKind::Converting { from, to } => {
                    let c_from = emit_inspect_operand(out, from, "conv_from", data_items, &pad);
                    let c_to = emit_inspect_operand(out, to, "conv_to", data_items, &pad);
                    let insp_tgt_ptr = c_ptr_expr(&c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_inspect_converting((uint8_t*){insp_tgt_ptr}, {target_size}, {}, {}, {}, {});\n",
                        c_from.0, c_from.1, c_to.0, c_to.1
                    ));
                }
            }
        }
        HirStatement::StopRun { .. } => {
            out.push_str(&format!("{pad}cobol_stop_run();\n"));
        }
        HirStatement::Goback { .. } => {
            out.push_str(&format!("{pad}cobol_goback();\n"));
        }
        HirStatement::ExitProgram { .. } => {
            out.push_str(&format!("{pad}exit(0); /* EXIT PROGRAM */\n"));
        }
        HirStatement::ExitParagraph { .. } => {
            out.push_str(&format!("{pad}return; /* EXIT PARAGRAPH */\n"));
        }
        HirStatement::Continue { .. } => {
            out.push_str(&format!("{pad}/* CONTINUE */\n"));
        }
        HirStatement::Label { name } => {
            let c_name = sanitize_name(name);
            out.push_str(&format!("lbl_{c_name}:;\n"));
        }
        // --- COBOL 2002+ statements ---
        HirStatement::Invoke {
            object,
            method,
            params,
            returning,
            ..
        } => {
            let c_obj = emit_expr(object);
            let args: Vec<_> = params.iter().map(emit_expr).collect();
            let args_str = args.join(", ");
            if let Some(ret) = returning {
                let c_ret = sanitize_name(ret);
                out.push_str(&format!(
                    "{pad}{c_ret} = cobol_invoke(&{c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
                    params.len()
                ));
            } else {
                out.push_str(&format!(
                    "{pad}cobol_invoke(&{c_obj}, \"{method}\", (int64_t[]){{{args_str}}}, {});\n",
                    params.len()
                ));
            }
        }
        HirStatement::Raise { exception, .. } => {
            out.push_str(&format!("{pad}cobol_raise(\"{exception}\");\n"));
        }
        HirStatement::Resume { target, .. } => {
            if let Some(t) = target {
                let c_target = sanitize_name(t);
                out.push_str(&format!("{pad}cobol_resume(\"{c_target}\");\n"));
            } else {
                out.push_str(&format!("{pad}cobol_resume(NULL);\n"));
            }
        }
        HirStatement::Allocate {
            target,
            returning,
            char_count,
            ..
        } => {
            let c_target = sanitize_name(target);
            let size_expr = if let Some(count_expr) = char_count {
                emit_int_compatible_expr(count_expr, data_items)
            } else {
                format!("sizeof({c_target})")
            };
            if let Some(ret) = returning {
                let c_ret = sanitize_name(ret);
                out.push_str(&format!(
                    "{pad}{c_ret} = malloc({size_expr}); /* ALLOCATE */\n"
                ));
            } else {
                out.push_str(&format!(
                    "{pad}{c_target} = malloc({size_expr}); /* ALLOCATE */\n"
                ));
            }
        }
        HirStatement::Free { targets, .. } => {
            for target in targets {
                let c_target = sanitize_name(target);
                out.push_str(&format!("{pad}free({c_target}); {c_target} = NULL;\n"));
            }
        }
        // --- COBOL 2014+ statements ---
        HirStatement::Validate { target, .. } => {
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_validate(\"{c_target}\"); /* VALIDATE */\n"
            ));
        }
        HirStatement::JsonGenerate { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_json_generate(&{c_source}, sizeof({c_source}), (uint8_t*){c_target}, sizeof({c_target})); /* JSON GENERATE */\n"
            ));
        }
        HirStatement::JsonParse { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_json_parse((const uint8_t*){c_source}, strlen({c_source}), &{c_target}, sizeof({c_target})); /* JSON PARSE */\n"
            ));
        }
        HirStatement::XmlGenerate { source, target, .. } => {
            let c_source = sanitize_name(source);
            let c_target = sanitize_name(target);
            out.push_str(&format!(
                "{pad}cobol_xml_generate(&{c_source}, sizeof({c_source}), \"{c_source}\", {}, (uint8_t*){c_target}, sizeof({c_target})); /* XML GENERATE */\n",
                c_source.len()
            ));
        }
        HirStatement::XmlParse {
            source,
            processing_procedure,
            ..
        } => {
            let c_source = sanitize_name(source);
            let c_proc = sanitize_name(processing_procedure);
            out.push_str(&format!(
                "{pad}/* XML PARSE {c_source} PROCESSING PROCEDURE {c_proc} */\n"
            ));
            out.push_str(&format!(
                "{pad}cobol_xml_parse((const uint8_t*){c_source}, strlen((const char*){c_source}), _xml_cb_{c_proc});\n"
            ));
        }
        // --- File I/O: additional statements ---
        HirStatement::Start {
            file_name,
            key,
            op,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let mode_val = match op {
                HirStartRelation::Equal => 0,
                HirStartRelation::GreaterThan => 1,
                HirStartRelation::GreaterEqual | HirStartRelation::NotLessThan => 2,
            };
            out.push_str(&format!("{pad}/* START {c_name} */\n"));
            let needs_rc = !invalid_key.is_empty() || !not_invalid_key.is_empty();
            out.push_str(&format!("{pad}{{\n"));
            let start_call = if let Some(key_name) = key {
                let c_key = sanitize_name(key_name);
                let key_size = find_data_item_size(&c_key, data_items);
                let is_key_group = find_data_item(key_name.as_str(), data_items)
                    .is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
                let addr_prefix = if is_key_group { "&" } else { "" };
                format!("cobol_file_start(FILE_ID_{c_name}, (const uint8_t*){addr_prefix}{c_key}, {key_size}, {mode_val})")
            } else {
                format!("cobol_file_start(FILE_ID_{c_name}, NULL, 0, {mode_val})")
            };
            if needs_rc {
                out.push_str(&format!("{pad}    uint32_t _src = {start_call};\n"));
                if !invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_src != 0) {{\n"));
                    for s in invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
                if !not_invalid_key.is_empty() {
                    out.push_str(&format!("{pad}    if (_src == 0) {{\n"));
                    for s in not_invalid_key {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                    out.push_str(&format!("{pad}    }}\n"));
                }
            } else {
                out.push_str(&format!("{pad}    {start_call};\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Return {
            file_name,
            into,
            at_end,
            not_at_end,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let record_var = resolve_file_record(&c_name);
            let rec_len = find_record_len(&record_var, data_items);
            let target = if let Some((into_var, into_subs)) = into {
                if into_subs.is_empty() {
                    sanitize_name(into_var)
                } else {
                    emit_subscript_access(into_var, into_subs)
                }
            } else {
                record_var
            };
            out.push_str(&format!("{pad}/* RETURN {c_name} */\n"));
            out.push_str(&format!(
                "{pad}{{\n{pad}    uint32_t _fs = cobol_file_read_next(FILE_ID_{c_name}, (uint8_t*)&{target}, {rec_len});\n"
            ));
            if !at_end.is_empty() || !not_at_end.is_empty() {
                out.push_str(&format!("{pad}    if (_fs == 10) {{\n"));
                out.push_str(&format!("{pad}        /* AT END */\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                if !not_at_end.is_empty() {
                    out.push_str(&format!("{pad}    }} else {{\n"));
                    out.push_str(&format!("{pad}        /* NOT AT END */\n"));
                    for s in not_at_end {
                        emit_statement(
                            out,
                            s,
                            data_items,
                            paragraphs,
                            fs_map,
                            has_declaratives,
                            indent + 2,
                        );
                    }
                }
                out.push_str(&format!("{pad}    }}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Cancel { programs, .. } => {
            for prog in programs {
                let prog_name = match prog {
                    HirExpr::Literal(HirLiteral::String(s)) => sanitize_name(s),
                    HirExpr::Variable(name) => sanitize_name(name),
                    _ => emit_expr(prog),
                };
                out.push_str(&format!(
                    "{pad}/* CANCEL {prog_name} -- releases loaded program resources */\n"
                ));
            }
        }
        HirStatement::Merge {
            file_name,
            keys,
            using,
            giving,
            ..
        } => {
            let c_name = sanitize_name(file_name);
            let rec_len = find_record_len(&c_name, data_items);
            out.push_str(&format!("{pad}/* MERGE {c_name} */\n"));
            if !using.is_empty() {
                let using_names: Vec<_> = using.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* USING {} */\n", using_names.join(", ")));
            }
            if !giving.is_empty() {
                let giving_names: Vec<_> = giving.iter().map(|f| f.as_str()).collect();
                out.push_str(&format!("{pad}/* GIVING {} */\n", giving_names.join(", ")));
            }
            let key_count = if keys.is_empty() { 1 } else { keys.len() };
            let input_count = using.len();
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!(
                "{pad}    uint32_t _merge_inputs[{input_count}];\n"
            ));
            for (i, input_file) in using.iter().enumerate() {
                let c_input = sanitize_name(input_file);
                out.push_str(&format!(
                    "{pad}    _merge_inputs[{i}] = FILE_ID_{c_input};\n"
                ));
            }
            out.push_str(&format!(
                "{pad}    struct {{ uint32_t offset; uint32_t length; uint8_t ascending; }} _merge_keys[{key_count}];\n"
            ));
            if keys.is_empty() {
                out.push_str(&format!(
                    "{pad}    _merge_keys[0].offset = 0; _merge_keys[0].length = {rec_len}; _merge_keys[0].ascending = 1;\n"
                ));
            } else {
                for (i, key) in keys.iter().enumerate() {
                    let ascending = matches!(key.order, cobol_hir::HirSortOrder::Ascending);
                    let asc_val: u8 = if ascending { 1 } else { 0 };
                    out.push_str(&format!(
                        "{pad}    _merge_keys[{i}].offset = 0; _merge_keys[{i}].length = {rec_len}; _merge_keys[{i}].ascending = {asc_val};\n"
                    ));
                }
            }
            let output_file_id = if let Some(first_giving) = giving.first() {
                let c_giving = sanitize_name(first_giving);
                format!("FILE_ID_{c_giving}")
            } else {
                format!("FILE_ID_{c_name}")
            };
            out.push_str(&format!(
                "{pad}    cobol_merge(_merge_inputs, {input_count}, {output_file_id}, _merge_keys, {key_count}, {rec_len});\n"
            ));
            out.push_str(&format!("{pad}}}\n"));
        }
        HirStatement::Release {
            record_name, from, ..
        } => {
            let c_name = sanitize_name(record_name);
            let rec_len = find_record_len(&c_name, data_items);
            let source = if let Some(from_expr) = from {
                emit_expr(from_expr)
            } else {
                c_name.clone()
            };
            out.push_str(&format!("{pad}/* RELEASE {c_name} */\n"));
            out.push_str(&format!(
                "{pad}cobol_file_write(FILE_ID_{c_name}, (const uint8_t*)&{source}, {rec_len});\n"
            ));
        }
        // --- Table handling: SEARCH ---
        HirStatement::Search {
            table_name,
            all: _,
            varying,
            at_end,
            when_clauses,
            ..
        } => {
            let c_table = sanitize_name(table_name);
            let c_idx = if let Some(ref v) = varying {
                sanitize_name(v)
            } else {
                // Use the first INDEXED BY name from the OCCURS clause
                find_first_index_name(&c_table, data_items)
                    .unwrap_or_else(|| format!("{c_table}_IDX"))
            };
            let max_occurs = find_occurs_count(&c_table, data_items);
            let inner_pad = "    ".repeat(indent + 1);
            let inner2_pad = "    ".repeat(indent + 2);
            out.push_str(&format!("{pad}/* SEARCH {c_table} */\n"));
            out.push_str(&format!("{pad}{{\n"));
            out.push_str(&format!("{inner_pad}int _search_found = 0;\n"));
            out.push_str(&format!(
                "{inner_pad}for (; {c_idx} <= {max_occurs}; {c_idx}++) {{\n"
            ));
            for when in when_clauses {
                let cond = emit_condition(&when.condition, data_items);
                out.push_str(&format!("{inner2_pad}if ({cond}) {{\n"));
                let body_pad_level = indent + 3;
                for s in &when.body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        body_pad_level,
                    );
                }
                out.push_str(&format!("{inner2_pad}    _search_found = 1; break;\n"));
                out.push_str(&format!("{inner2_pad}}}\n"));
            }
            out.push_str(&format!("{inner_pad}}}\n"));
            if !at_end.is_empty() {
                out.push_str(&format!("{inner_pad}if (!_search_found) {{\n"));
                for s in at_end {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 2,
                    );
                }
                out.push_str(&format!("{inner_pad}}}\n"));
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        // --- Report writer statements (stub — emit comments) ---
        HirStatement::Initiate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* INITIATE {c_name} */\n"));
            }
        }
        HirStatement::Generate { report_name, .. } => {
            let c_name = sanitize_name(report_name);
            out.push_str(&format!("{pad}/* GENERATE {c_name} */\n"));
        }
        HirStatement::Terminate { report_names, .. } => {
            for name in report_names {
                let c_name = sanitize_name(name);
                out.push_str(&format!("{pad}/* TERMINATE {c_name} */\n"));
            }
        }
    }
}

fn emit_display_operand(out: &mut String, expr: &HirExpr, data_items: &[HirDataItem], pad: &str) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            out.push_str(&format!("{pad}cobol_display_int({n});\n"));
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            out.push_str(&format!("{pad}cobol_display_int(0);\n"));
        }
        HirExpr::Literal(HirLiteral::Space) => {
            out.push_str(&format!("{pad}cobol_display_space();\n"));
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let escaped = escape_c_string(d);
            let len = d.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\xFF\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\x00\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"\\\"\", 1);\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Null) => {
            out.push_str(&format!("{pad}cobol_display_int(0);\n"));
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
            ));
        }
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let item = find_data_item(name, data_items);

            // If this is a screen item, emit positioning and attribute code
            if let Some(si) = item.and_then(|i| i.screen_info.as_ref()) {
                emit_screen_display(out, si, data_items, pad);
                // After screen attributes, also display children recursively
                // by emitting the screen group content. For leaf items with a
                // VALUE, the value was already emitted by emit_screen_display.
                return;
            }

            let is_alphanumeric =
                item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_group = item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            let is_decimal = item.is_some_and(|i| needs_decimal(&i.data_type));
            if is_decimal {
                // Display decimal using cobol_decimal_to_display
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_name}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else if is_group {
                // Group items are C unions; display their raw bytes
                let size = match &item.unwrap().data_type {
                    HirType::Group { size, .. } => *size,
                    _ => 1,
                };
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*)&{c_name}, {size});\n"
                ));
            } else if is_alphanumeric {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::Alphanumeric { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*){c_name}, {size});\n"
                ));
            } else if item.is_some_and(|i| matches!(i.data_type, HirType::National { .. })) {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::National { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_national((const uint16_t*){c_name}, {size});\n"
                ));
            } else {
                out.push_str(&format!("{pad}cobol_display_int({c_name});\n"));
            }
        }
        HirExpr::BinaryOp { .. } | HirExpr::UnaryOp { .. } => {
            let e = emit_int_compatible_expr(expr, data_items);
            out.push_str(&format!("{pad}cobol_display_int({e});\n"));
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_name = name.to_uppercase();
            match upper_name.as_str() {
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    // In-place string functions: copy arg to temp buffer,
                    // apply function, then display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let func = match upper_name.as_str() {
                            "UPPER-CASE" => "cobol_func_upper_case",
                            "LOWER-CASE" => "cobol_func_lower_case",
                            _ => "cobol_func_reverse",
                        };
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _fbuf[{size}]; memcpy(_fbuf, (const uint8_t*){c_arg}, {size}); {func}(_fbuf, {size}); cobol_display_string(_fbuf, {size}); }}\n"
                        ));
                    }
                }
                "TRIM" => {
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        // mode: 0 = both, 1 = leading, 2 = trailing
                        let mode = if args.len() > 1 {
                            emit_expr(&args[1])
                        } else {
                            "0".to_string()
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _fbuf[256]; uint32_t _flen = cobol_func_trim((const uint8_t*){c_arg}, {size}, _fbuf, 256, {mode}); cobol_display_string(_fbuf, _flen); }}\n"
                        ));
                    }
                }
                "CONCATENATE" => {
                    // For display: concatenate all args into a temp buffer
                    // and display the result
                    let mut total_size = 0u32;
                    let mut arg_parts: Vec<(String, u32)> = Vec::new();
                    for arg in args {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else if let HirExpr::Literal(HirLiteral::String(s)) = arg {
                            s.len() as u32
                        } else {
                            64
                        };
                        total_size += size;
                        arg_parts.push((c_arg, size));
                    }
                    if !arg_parts.is_empty() {
                        let buf_size = total_size.max(1);
                        let mut block =
                            format!("{pad}{{ uint8_t _cbuf[{buf_size}]; uint32_t _coff = 0;\n");
                        for (c_arg, size) in &arg_parts {
                            block.push_str(&format!(
                                "{pad}  memcpy(_cbuf + _coff, \
                                 (const uint8_t*){c_arg}, {size}); \
                                 _coff += {size};\n"
                            ));
                        }
                        block.push_str(&format!(
                            "{pad}  cobol_display_string(_cbuf, {buf_size}); }}\n"
                        ));
                        out.push_str(&block);
                    }
                }
                "NATIONAL-OF" => {
                    // DISPLAY FUNCTION NATIONAL-OF(var) -- convert and display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint16_t _nbuf[{size}]; \
                             cobol_func_national_of(\
                             (const uint8_t*){c_arg}, {size}, _nbuf, {size}); \
                             cobol_display_national(_nbuf, {size}); }}\n"
                        ));
                    }
                }
                "DISPLAY-OF" => {
                    // DISPLAY FUNCTION DISPLAY-OF(var) -- convert and display
                    if let Some(arg) = args.first() {
                        let c_arg = emit_expr(arg);
                        let size: u32 = if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else {
                            64
                        };
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _dbuf[{size}]; \
                             cobol_func_display_of(\
                             (const uint16_t*){c_arg}, {size}, _dbuf, {size}); \
                             cobol_display_string(_dbuf, {size}); }}\n"
                        ));
                    }
                }
                _ => {
                    // Numeric function
                    let e = emit_expr(expr);
                    out.push_str(&format!("{pad}cobol_display_int({e});\n"));
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } => {
            let c_var = sanitize_name(variable);
            let c_start = emit_expr(start);
            let var_size = find_data_item_size(&c_var, data_items);
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({var_size} - ({c_start} - 1))")
            };
            out.push_str(&format!(
                "{pad}cobol_display_string(\
                 (const uint8_t*){c_var} + ({c_start} - 1), {c_len});\n"
            ));
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => {
            let c_access = emit_subscript_access(variable, subscripts);
            let item = find_data_item(variable, data_items);
            let is_alpha =
                item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_decimal = item.is_some_and(|i| needs_decimal(&i.data_type));
            if is_alpha {
                let size = item
                    .and_then(|i| match &i.data_type {
                        HirType::Alphanumeric { size } => Some(*size),
                        _ => None,
                    })
                    .unwrap_or(1);
                out.push_str(&format!(
                    "{pad}cobol_display_string((const uint8_t*){c_access}, {size});\n"
                ));
            } else if is_decimal {
                let pic_str = item
                    .map(|i| generate_pic_string(&i.data_type))
                    .unwrap_or_else(|| "9".to_string());
                let pic_len = pic_str.len();
                out.push_str(&format!(
                    "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(&{c_access}, (uint8_t*)_dbuf, 64, (const uint8_t*)\"{pic_str}\", {pic_len}); cobol_display_string((const uint8_t*)_dbuf, _dlen); }}\n"
                ));
            } else {
                out.push_str(&format!("{pad}cobol_display_int({c_access});\n"));
            }
        }
    }
}

/// Emit C code for displaying a screen item with ANSI positioning and attributes.
fn emit_screen_display(
    out: &mut String,
    si: &cobol_hir::HirScreenInfo,
    data_items: &[HirDataItem],
    pad: &str,
) {
    // BLANK SCREEN: clear the whole terminal
    if si.blank_screen {
        out.push_str(&format!("{pad}cobol_screen_clear();\n"));
    }
    // BLANK LINE: clear current line
    if si.blank_line {
        out.push_str(&format!("{pad}cobol_screen_clear_line();\n"));
    }
    // LINE / COLUMN: position cursor
    if si.line.is_some() || si.column.is_some() {
        let line = si.line.unwrap_or(1) as i32;
        let col = si.column.unwrap_or(1) as i32;
        out.push_str(&format!("{pad}cobol_screen_position({line}, {col});\n"));
    }
    // HIGHLIGHT: enable bold
    if si.highlight {
        out.push_str(&format!("{pad}cobol_screen_highlight_on();\n"));
    }
    // REVERSE-VIDEO
    if si.reverse_video {
        out.push_str(&format!("{pad}cobol_screen_reverse_on();\n"));
    }
    // Display the VALUE if present
    if let Some(ref val) = si.value {
        let escaped = escape_c_string(val);
        let len = val.len();
        out.push_str(&format!(
            "{pad}cobol_display_string((const uint8_t*)\"{escaped}\", {len});\n"
        ));
    }
    // Display the SOURCE field if present
    if let Some(ref source) = si.source {
        let c_name = sanitize_name(source);
        let item = find_data_item(source, data_items);
        let is_alpha = item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
        if is_alpha {
            let size = item
                .and_then(|i| match &i.data_type {
                    HirType::Alphanumeric { size } => Some(*size),
                    _ => None,
                })
                .unwrap_or(1);
            out.push_str(&format!(
                "{pad}cobol_display_string((const uint8_t*){c_name}, {size});\n"
            ));
        } else {
            out.push_str(&format!("{pad}cobol_display_int({c_name});\n"));
        }
    }
    // Reset attributes if we turned any on
    if si.highlight || si.reverse_video {
        out.push_str(&format!("{pad}cobol_screen_reset_attrs();\n"));
    }
}

fn emit_move_to(
    out: &mut String,
    from: &HirExpr,
    target_name: &smol_str::SmolStr,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_type = find_data_item(target_name.as_str(), data_items).map(|item| &item.data_type);
    let is_target_alpha = matches!(target_type, Some(HirType::Alphanumeric { .. }));
    let is_target_group = matches!(target_type, Some(HirType::Group { .. }));
    let is_target_national = matches!(target_type, Some(HirType::National { .. }));
    let is_target_decimal = target_type.is_some_and(needs_decimal);

    // NATIONAL target: convert source to national
    if is_target_national {
        let tgt_size = match target_type {
            Some(HirType::National { size }) => *size,
            _ => 1,
        };
        match from {
            HirExpr::Literal(HirLiteral::String(s)) => {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                out.push_str(&format!(
                    "{pad}cobol_move_to_national(\
                     (const uint8_t*)\"{escaped}\", {src_len}, \
                     {c_target}, {tgt_size});\n"
                ));
            }
            HirExpr::Literal(HirLiteral::Space) => {
                out.push_str(&format!(
                    "{pad}for (uint32_t _i = 0; _i < {tgt_size}; _i++) \
                     {{ {c_target}[_i] = 0x0020; }}\n"
                ));
            }
            HirExpr::Variable(src_name) => {
                let c_src = sanitize_name(src_name);
                let src_item = find_data_item(src_name.as_str(), data_items).map(|i| &i.data_type);
                if matches!(src_item, Some(HirType::National { .. })) {
                    let src_size = match src_item {
                        Some(HirType::National { size }) => *size,
                        _ => 1,
                    };
                    out.push_str(&format!(
                        "{pad}cobol_move_national_to_national(\
                         (const uint16_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                } else {
                    let src_size = find_data_item_size(&c_src, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_to_national(\
                         (const uint8_t*){c_src}, {src_size}, \
                         {c_target}, {tgt_size});\n"
                    ));
                }
            }
            _ => {
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!("{pad}{c_target}[0] = (uint16_t){e};\n"));
            }
        }
        return;
    }

    // Group-to-group move: use memcpy with space-padding per COBOL rules.
    // Use sizeof() for C struct sizes to account for null terminators and padding.
    if is_target_group {
        if let HirExpr::Variable(src_name) = from {
            let c_src = sanitize_name(src_name);
            let is_source_group = find_data_item(src_name.as_str(), data_items)
                .is_some_and(|item| matches!(item.data_type, HirType::Group { .. }));
            if is_source_group {
                // Both are groups: use sizeof() for correct C-level byte copy
                out.push_str(&format!(
                    "{pad}{{\n\
                     {pad}    size_t _src_sz = sizeof({c_src});\n\
                     {pad}    size_t _tgt_sz = sizeof({c_target});\n\
                     {pad}    size_t _cp_sz = _src_sz < _tgt_sz ? _src_sz : _tgt_sz;\n\
                     {pad}    memcpy(&{c_target}, &{c_src}, _cp_sz);\n\
                     {pad}    if (_src_sz < _tgt_sz) {{\n\
                     {pad}        memset((uint8_t*)&{c_target} + _src_sz, ' ', \
                     _tgt_sz - _src_sz);\n\
                     {pad}    }}\n\
                     {pad}}}\n"
                ));
            } else {
                // Non-group source to group target: copy by COBOL data size
                let src_size = find_data_item_size(&c_src, data_items);
                let tgt_size = find_data_item_size(c_target, data_items);
                let copy_size = src_size.min(tgt_size);
                out.push_str(&format!(
                    "{pad}memcpy(&{c_target}, &{c_src}, {copy_size});\n"
                ));
            }
        } else if let HirExpr::Subscript { variable, .. } = from {
            // Subscripted source to group target: check type and use memcpy
            let c_src = emit_expr(from);
            let src_item = find_data_item(variable.as_str(), data_items);
            let is_src_alpha =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }));
            let is_src_group =
                src_item.is_some_and(|i| matches!(i.data_type, HirType::Group { .. }));
            if is_src_alpha || is_src_group {
                let src_size = find_data_item_size(&sanitize_name(variable), data_items);
                let tgt_size = find_data_item_size(c_target, data_items);
                let copy_size = src_size.min(tgt_size);
                let addr_prefix = if is_src_group { "&" } else { "" };
                out.push_str(&format!(
                    "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                     {pad}memcpy(&{c_target}, {addr_prefix}{c_src}, {copy_size});\n"
                ));
            } else {
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!(
                    "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                     {pad}{{ int64_t _v = {e}; memcpy(&{c_target}, &_v, \
                     sizeof(_v) < sizeof({c_target}) ? sizeof(_v) : sizeof({c_target})); }}\n"
                ));
            }
        } else if let HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } = from
        {
            // Reference-modified source to group target: copy substring
            let c_src = sanitize_name(variable);
            let c_start = emit_expr(start);
            let src_full_size = find_data_item_size(&c_src, data_items);
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({src_full_size} - ({c_start} - 1))")
            };
            out.push_str(&format!(
                "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                 {pad}memcpy(&{c_target}, (const uint8_t*){c_src} + ({c_start} - 1), \
                 {c_len} < sizeof({c_target}) ? {c_len} : sizeof({c_target}));\n"
            ));
        } else {
            // Non-variable to group: handle figurative constants
            match from {
                HirExpr::Literal(HirLiteral::Space) => {
                    out.push_str(&format!(
                        "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::Zero) => {
                    out.push_str(&format!(
                        "{pad}memset(&{c_target}, '0', sizeof({c_target}));\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::HighValue) => {
                    out.push_str(&format!(
                        "{pad}memset(&{c_target}, 0xFF, sizeof({c_target}));\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::LowValue) => {
                    out.push_str(&format!(
                        "{pad}memset(&{c_target}, 0x00, sizeof({c_target}));\n"
                    ));
                }
                HirExpr::Literal(HirLiteral::String(s)) => {
                    let escaped = escape_c_string(s);
                    let src_len = s.len();
                    out.push_str(&format!(
                        "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                         {pad}memcpy(&{c_target}, \"{escaped}\", \
                         {src_len} < sizeof({c_target}) ? {src_len} : sizeof({c_target}));\n"
                    ));
                }
                _ => {
                    // Check if source expr refers to an alpha/group field
                    if is_alpha_expr(from, data_items) || is_group_expr(from, data_items) {
                        let e = emit_expr(from);
                        let src_name = expr_var_name(from);
                        let src_size = find_data_item_size(&sanitize_name(src_name), data_items);
                        let tgt_size = find_data_item_size(c_target, data_items);
                        let copy_size = src_size.min(tgt_size);
                        let addr_prefix = if is_group_expr(from, data_items) {
                            "&"
                        } else {
                            ""
                        };
                        out.push_str(&format!(
                            "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                             {pad}memcpy(&{c_target}, {addr_prefix}{e}, {copy_size});\n"
                        ));
                    } else {
                        let e = emit_int_compatible_expr(from, data_items);
                        out.push_str(&format!(
                            "{pad}memset(&{c_target}, ' ', sizeof({c_target}));\n\
                             {pad}{{ int64_t _v = {e}; memcpy(&{c_target}, &_v, \
                             sizeof(_v) < sizeof({c_target}) ? sizeof(_v) : sizeof({c_target})); }}\n"
                        ));
                    }
                }
            }
        }
        return;
    }

    // CobolDecimal target: use proper conversion functions
    if is_target_decimal {
        emit_assign_to_decimal(out, from, c_target, data_items, pad);
        return;
    }

    // Detect source type for cross-type moves (handles Variable and Subscript)
    let src_var_name = expr_var_name(from);
    let src_type = if !src_var_name.is_empty() {
        find_data_item(src_var_name, data_items).map(|i| &i.data_type)
    } else {
        None
    };
    let is_source_index =
        !src_var_name.is_empty() && src_type.is_none() && is_index_name(src_var_name, data_items);
    let is_source_numeric_var = is_source_index
        || matches!(
            src_type,
            Some(
                HirType::Numeric { .. }
                    | HirType::Binary { .. }
                    | HirType::Comp3 { .. }
                    | HirType::Index
            )
        );
    let is_source_decimal_var = src_type.is_some_and(needs_decimal);
    let is_source_alpha_var = matches!(src_type, Some(HirType::Alphanumeric { .. }));
    let is_source_group_var = matches!(src_type, Some(HirType::Group { .. }));
    let is_source_national_var = matches!(src_type, Some(HirType::National { .. }));

    // National source -> alphanumeric target: use DISPLAY-OF conversion
    if is_target_alpha && is_source_national_var {
        if let HirExpr::Variable(name) = from {
            let c_src = sanitize_name(name);
            let src_size = match find_data_item(name.as_str(), data_items).map(|i| &i.data_type) {
                Some(HirType::National { size }) => *size,
                _ => 1,
            };
            let tgt_size = find_data_item_size(c_target, data_items);
            out.push_str(&format!(
                "{pad}cobol_func_display_of(\
                 (const uint16_t*){c_src}, {src_size}, \
                 (uint8_t*){c_target}, {tgt_size});\n"
            ));
            out.push_str(&format!("{pad}{c_target}[{tgt_size}] = '\\0';\n"));
        }
        return;
    }

    match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            if is_target_alpha {
                let escaped = escape_c_string(s);
                let src_len = s.len();
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_string((const uint8_t*)\"{escaped}\", {src_len}, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else {
                let escaped = escape_c_string(s);
                out.push_str(&format!(
                    "{pad}strncpy({c_target}, \"{escaped}\", sizeof({c_target}) - 1);\n"
                ));
                out.push_str(&format!(
                    "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
                ));
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            if is_target_alpha {
                // Numeric literal → alphanumeric: right-justify with leading spaces
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_numeric_to_display({n}, 0, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else {
                out.push_str(&format!("{pad}{c_target} = {n};\n"));
            }
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            if is_target_alpha {
                // Decimal literal → alphanumeric: format and move as display string
                let tgt_size = find_data_item_size(c_target, data_items);
                let display_str = d.to_string();
                let display_len = display_str.len();
                out.push_str(&format!(
                    "{pad}cobol_move_string((const uint8_t*)\"{display_str}\", {display_len}, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if is_target_decimal {
                // Parse the decimal and compute integer value + scale
                let parts: Vec<&str> = d.split('.').collect();
                let int_part: i64 = parts[0].parse().unwrap_or(0);
                let frac_str = parts.get(1).copied().unwrap_or("");
                let scale = frac_str.len() as i64;
                let frac: i64 = frac_str.parse().unwrap_or(0);
                let val = if int_part < 0 {
                    int_part * 10i64.pow(scale as u32) - frac
                } else {
                    int_part * 10i64.pow(scale as u32) + frac
                };
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int({val}, {scale}, &{c_target});\n"
                ));
            } else {
                out.push_str(&format!("{pad}{c_target} = {d};\n"));
            }
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}memset({c_target}, '0', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                ));
            } else {
                out.push_str(&format!("{pad}{c_target} = 0;\n"));
            }
        }
        HirExpr::Literal(HirLiteral::Space) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}memset({c_target}, ' ', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                ));
            } else {
                out.push_str(&format!(
                    "{pad}memset({c_target}, ' ', sizeof({c_target}) - 1);\n"
                ));
                out.push_str(&format!(
                    "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
                ));
            }
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            out.push_str(&format!(
                "{pad}memset({c_target}, 0xFF, sizeof({c_target}) - 1);\n"
            ));
            out.push_str(&format!(
                "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
            ));
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            out.push_str(&format!(
                "{pad}memset({c_target}, 0x00, sizeof({c_target}));\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            out.push_str(&format!(
                "{pad}memset({c_target}, '\"', sizeof({c_target}) - 1);\n"
            ));
            out.push_str(&format!(
                "{pad}{c_target}[sizeof({c_target}) - 1] = '\\0';\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Null) => {
            out.push_str(&format!("{pad}{c_target} = 0;\n"));
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            if is_target_alpha {
                let tgt_size = find_data_item_size(c_target, data_items);
                if s.len() == 1 {
                    let ch = s.chars().next().unwrap();
                    out.push_str(&format!(
                        "{pad}memset({c_target}, '{ch}', {tgt_size}); {c_target}[{tgt_size}] = '\\0';\n"
                    ));
                } else {
                    let escaped = escape_c_string(s);
                    let slen = s.len();
                    out.push_str(&format!(
                        "{pad}{{ const char* _all = \"{escaped}\"; int _al = {slen};\n"
                    ));
                    out.push_str(&format!(
                        "{pad}  for (int _i = 0; _i < {tgt_size}; _i++) {c_target}[_i] = _all[_i % _al];\n"
                    ));
                    out.push_str(&format!("{pad}  {c_target}[{tgt_size}] = '\\0'; }}\n"));
                }
            } else if let Some(ch) = s.chars().next() {
                out.push_str(&format!("{pad}{c_target} = '{ch}';\n"));
            }
        }
        _ => {
            // Handle string-returning intrinsic functions in MOVE context
            if let HirExpr::FunctionCall { name, args } = from {
                let upper_fn = name.to_uppercase();
                match upper_fn.as_str() {
                    "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                        if let Some(arg) = args.first() {
                            let func = match upper_fn.as_str() {
                                "UPPER-CASE" => "cobol_func_upper_case",
                                "LOWER-CASE" => "cobol_func_lower_case",
                                _ => "cobol_func_reverse",
                            };
                            let tgt_size = find_data_item_size(c_target, data_items);
                            let (c_src, src_size_str) = emit_string_func_arg(arg);
                            out.push_str(&format!(
                                "{pad}{{ uint8_t _fbuf[{src_size_str}]; memcpy(_fbuf, (const uint8_t*){c_src}, {src_size_str}); {func}(_fbuf, {src_size_str}); cobol_move_string(_fbuf, {src_size_str}, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        }
                        return;
                    }
                    "CURRENT-DATE" => {
                        let tgt_size = find_data_item_size(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _cdbuf[21]; cobol_func_current_date(_cdbuf, 21); cobol_move_string(_cdbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "WHEN-COMPILED" => {
                        let tgt_size = find_data_item_size(c_target, data_items);
                        out.push_str(&format!(
                            "{pad}{{ uint8_t _wcbuf[21]; cobol_func_when_compiled(_wcbuf, 21); cobol_move_string(_wcbuf, 21, (uint8_t*){c_target}, {tgt_size}); }}\n"
                        ));
                        return;
                    }
                    "CHAR" => {
                        if let Some(arg) = args.first() {
                            let c_arg = emit_expr_as_numeric(arg);
                            let tgt_size = find_data_item_size(c_target, data_items);
                            out.push_str(&format!(
                                "{pad}{{ uint8_t _chval = cobol_func_char((uint32_t){c_arg}); cobol_move_string(&_chval, 1, (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                        }
                        return;
                    }
                    "MAX" | "MIN" => {
                        let has_alpha = args
                            .iter()
                            .any(|a| matches!(a, HirExpr::Literal(HirLiteral::String(_))));
                        if has_alpha && is_target_alpha {
                            let func = if upper_fn == "MAX" {
                                "cobol_func_max_alpha"
                            } else {
                                "cobol_func_min_alpha"
                            };
                            let tgt_size = find_data_item_size(c_target, data_items);
                            let n = args.len();
                            let mut ptrs = Vec::new();
                            let mut lens = Vec::new();
                            for arg in args {
                                let (c_src, c_len) = emit_string_func_arg(arg);
                                ptrs.push(format!("(const uint8_t*){c_src}"));
                                lens.push(c_len);
                            }
                            let ptrs_init = ptrs.join(", ");
                            let lens_init = lens.join(", ");
                            out.push_str(&format!(
                                "{pad}{{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
                                 uint32_t _al[] = {{{lens_init}}}; \
                                 int32_t _ai = {func}(_ap, _al, {n}); \
                                 cobol_move_string(_ap[_ai], _al[_ai], \
                                 (uint8_t*){c_target}, {tgt_size}); }}\n"
                            ));
                            return;
                        }
                    }
                    _ => {} // Fall through to other handling below
                }
            }
            if is_target_alpha && is_source_decimal_var {
                // CobolDecimal variable → alphanumeric: use cobol_decimal_to_display
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    let src_type = find_data_item(name.as_str(), data_items).map(|i| &i.data_type);
                    let pic_str = src_type
                        .map(generate_pic_string)
                        .unwrap_or_else(|| "9".to_string());
                    let pic_len = pic_str.len();
                    out.push_str(&format!(
                        "{pad}{{ char _dbuf[64]; uint32_t _dlen = cobol_decimal_to_display(\
                         &{c_src}, (uint8_t*)_dbuf, 64, \
                         (const uint8_t*)\"{pic_str}\", {pic_len}); \
                         cobol_move_string((const uint8_t*)_dbuf, _dlen, \
                         (uint8_t*){c_target}, {tgt_size}); }}\n"
                    ));
                }
            } else if is_target_alpha && is_source_numeric_var {
                // Numeric variable → alphanumeric: use cobol_move_numeric_to_display
                let e = emit_expr(from);
                let tgt_size = find_data_item_size(c_target, data_items);
                out.push_str(&format!(
                    "{pad}cobol_move_numeric_to_display({e}, 0, (uint8_t*){c_target}, {tgt_size});\n"
                ));
            } else if !is_target_alpha && is_source_alpha_var {
                // Alphanumeric variable/subscript → numeric: use cobol_func_numval
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    out.push_str(&format!(
                        "{pad}{c_target} = cobol_func_numval((const uint8_t*){c_src}, {src_size});\n"
                    ));
                } else if let HirExpr::Subscript { variable, .. } = from {
                    let e = emit_expr(from);
                    let src_size = find_data_item_size(&sanitize_name(variable), data_items);
                    out.push_str(&format!(
                        "{pad}{c_target} = cobol_func_numval((const uint8_t*){e}, {src_size});\n"
                    ));
                } else {
                    let e = emit_expr(from);
                    out.push_str(&format!("{pad}{c_target} = {e};\n"));
                }
            } else if is_target_alpha && is_source_group_var {
                // Group variable → alphanumeric: copy bytes with & prefix (group is a C union)
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*)&{c_src}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                }
            } else if is_target_alpha {
                // Alphanumeric → alphanumeric: use cobol_move_string
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){c_src}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if let HirExpr::ReferenceModification {
                    variable,
                    start,
                    length,
                } = from
                {
                    let c_src = sanitize_name(variable);
                    let c_start = emit_expr(start);
                    let src_full_size = find_data_item_size(&c_src, data_items);
                    let c_len = if let Some(len) = length {
                        emit_expr(len)
                    } else {
                        format!("({src_full_size} - ({c_start} - 1))")
                    };
                    let tgt_size = find_data_item_size(c_target, data_items);
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){c_src} + ({c_start} - 1), {c_len}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else if is_source_alpha_var || is_source_group_var {
                    // Subscripted or other alphanumeric/group source
                    let e = emit_expr(from);
                    let src_size = find_data_item_size(&sanitize_name(src_var_name), data_items);
                    let tgt_size = find_data_item_size(c_target, data_items);
                    // When source is a subscript expression the result of
                    // emit_expr is an element value (e.g. char), not a
                    // pointer.  We need to take its address with '&'.
                    let addr_prefix = if matches!(from, HirExpr::Subscript { .. }) {
                        "&"
                    } else {
                        ""
                    };
                    out.push_str(&format!(
                        "{pad}cobol_move_string((const uint8_t*){addr_prefix}{e}, {src_size}, (uint8_t*){c_target}, {tgt_size});\n"
                    ));
                } else {
                    let e = emit_int_compatible_expr(from, data_items);
                    out.push_str(&format!("{pad}{c_target} = {e};\n"));
                }
            } else if is_source_group_var {
                // Group variable → numeric target: treat group as alphanumeric bytes
                // and convert via cobol_func_numval (group is a C union).
                if let HirExpr::Variable(name) = from {
                    let c_src = sanitize_name(name);
                    let src_size = find_data_item_size(&c_src, data_items);
                    if is_target_decimal {
                        // Target is CobolDecimal
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int(\
                             cobol_func_numval((const uint8_t*)&{c_src}, {src_size}), \
                             0, &{c_target});\n"
                        ));
                    } else {
                        out.push_str(&format!(
                            "{pad}{c_target} = cobol_func_numval((const uint8_t*)&{c_src}, {src_size});\n"
                        ));
                    }
                }
            } else if is_source_decimal_var {
                // CobolDecimal variable → integer target: use cobol_decimal_to_int64
                let e = emit_expr(from);
                out.push_str(&format!(
                    "{pad}{c_target} = cobol_decimal_to_int64(&{e});\n"
                ));
            } else {
                // Use emit_int_compatible_expr to handle compound expressions
                // that may contain CobolDecimal sub-expressions.
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!("{pad}{c_target} = {e};\n"));
            }
        }
    }
}

/// Emit a MOVE into a reference-modified target: `MOVE src TO VAR(start:length)`.
///
/// Generated C uses `memcpy` with pointer arithmetic.
fn emit_move_to_refmod(
    out: &mut String,
    from: &HirExpr,
    variable: &smol_str::SmolStr,
    start: &HirExpr,
    length: &Option<HirExpr>,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let c_var = sanitize_name(variable);
    let c_start = emit_expr(start);
    let var_size = find_data_item_size(&c_var, data_items);
    let c_len = if let Some(len) = length {
        emit_expr(len)
    } else {
        // No length: remaining bytes from start
        format!("({var_size} - ({c_start} - 1))")
    };

    match from {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let src_len = s.len();
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), \"{escaped}\", \
                 ({src_len} < (size_t)({c_len}) ? {src_len} : (size_t)({c_len})));\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Space) => {
            out.push_str(&format!(
                "{pad}memset({c_var} + ({c_start} - 1), ' ', {c_len});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            out.push_str(&format!(
                "{pad}memset({c_var} + ({c_start} - 1), '0', {c_len});\n"
            ));
        }
        HirExpr::Variable(src_name) => {
            let c_src = sanitize_name(src_name);
            let src_size = find_data_item_size(&c_src, data_items);
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), {c_src}, \
                 ({src_size} < (uint32_t)({c_len}) ? {src_size} : (uint32_t)({c_len})));\n"
            ));
        }
        HirExpr::ReferenceModification {
            variable: src_var,
            start: src_start,
            length: src_length,
        } => {
            let c_src_var = sanitize_name(src_var);
            let c_src_start = emit_expr(src_start);
            let src_size = find_data_item_size(&c_src_var, data_items);
            let c_src_len = if let Some(sl) = src_length {
                emit_expr(sl)
            } else {
                format!("({src_size} - ({c_src_start} - 1))")
            };
            out.push_str(&format!(
                "{pad}memcpy({c_var} + ({c_start} - 1), \
                 {c_src_var} + ({c_src_start} - 1), \
                 ({c_src_len} < ({c_len}) ? ({c_src_len}) : ({c_len})));\n"
            ));
        }
        _ => {
            // Fallback: evaluate expression, store temporarily, then memcpy
            let e = emit_expr(from);
            out.push_str(&format!(
                "{pad}{{ int64_t _tmp = {e}; \
                 memcpy({c_var} + ({c_start} - 1), &_tmp, \
                 (sizeof(_tmp) < (size_t)({c_len}) ? sizeof(_tmp) : (size_t)({c_len}))); }}\n"
            ));
        }
    }
}

fn emit_perform(
    out: &mut String,
    kind: &HirPerformKind,
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    let pad = "    ".repeat(indent);
    match kind {
        HirPerformKind::Inline { body } => {
            out.push_str(&format!("{pad}{{\n"));
            for s in body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirPerformKind::Times { count, body } => {
            let c_count = emit_int_compatible_expr(count, data_items);
            out.push_str(&format!(
                "{pad}for (int64_t _cobol_i = 0; _cobol_i < ({c_count}); _cobol_i++) {{\n"
            ));
            for s in body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirPerformKind::Until { condition, body } => {
            let cond = emit_condition(condition, data_items);
            out.push_str(&format!("{pad}while (!({cond})) {{\n"));
            for s in body {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
            out.push_str(&format!("{pad}}}\n"));
        }
        HirPerformKind::Varying {
            var,
            from,
            by,
            until,
            body,
        } => {
            let c_var = emit_expr(var);
            let var_name = expr_var_name(var);
            let var_is_decimal =
                find_data_item(var_name, data_items).is_some_and(|i| needs_decimal(&i.data_type));
            let cond = emit_condition(until, data_items);

            if var_is_decimal {
                // CobolDecimal loop variable: use decimal arithmetic
                let from_is_decimal = is_decimal_expr(from, data_items);
                if from_is_decimal {
                    let c_from = emit_expr(from);
                    out.push_str(&format!("{pad}{c_var} = {c_from};\n"));
                } else {
                    // Try to extract value and scale from literal
                    let from_info: Option<(i64, u32)> = match from {
                        HirExpr::Literal(HirLiteral::Decimal(s)) => Some(parse_decimal_literal(s)),
                        HirExpr::Literal(HirLiteral::Integer(n)) => Some((*n, 0)),
                        _ => None,
                    };
                    if let Some((val, scale)) = from_info {
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int({val}, {scale}, &{c_var});\n"
                        ));
                    } else {
                        let c_from = emit_int_compatible_expr(from, data_items);
                        out.push_str(&format!(
                            "{pad}cobol_decimal_from_int({c_from}, 0, &{c_var});\n"
                        ));
                    }
                }
                out.push_str(&format!("{pad}while (!({cond})) {{\n"));
                for s in body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                    );
                }
                // Increment by decimal amount
                let by_is_decimal = is_decimal_expr(by, data_items);
                if by_is_decimal {
                    let c_by = emit_expr(by);
                    out.push_str(&format!(
                        "{pad}    cobol_decimal_add(&{c_var}, &{c_by}, &{c_var});\n"
                    ));
                } else {
                    // Extract value and scale from literal, or fall back to generic
                    let by_info: Option<(i64, u32)> = match by {
                        HirExpr::Literal(HirLiteral::Decimal(s)) => Some(parse_decimal_literal(s)),
                        HirExpr::UnaryOp {
                            op: HirUnaryOp::Neg,
                            operand,
                        } => {
                            if let HirExpr::Literal(HirLiteral::Decimal(s)) = operand.as_ref() {
                                let (v, s) = parse_decimal_literal(s);
                                Some((-v, s))
                            } else {
                                None
                            }
                        }
                        HirExpr::Literal(HirLiteral::Integer(n)) => Some((*n, 0)),
                        _ => None,
                    };
                    if let Some((by_val, by_scale)) = by_info {
                        out.push_str(&format!(
                            "{pad}    {{ CobolDecimal _by; cobol_decimal_from_int({by_val}, {by_scale}, &_by); cobol_decimal_add(&{c_var}, &_by, &{c_var}); }}\n"
                        ));
                    } else {
                        let c_by = emit_int_compatible_expr(by, data_items);
                        out.push_str(&format!(
                            "{pad}    {{ CobolDecimal _by; cobol_decimal_from_int({c_by}, 0, &_by); cobol_decimal_add(&{c_var}, &_by, &{c_var}); }}\n"
                        ));
                    }
                }
                out.push_str(&format!("{pad}}}\n"));
            } else {
                let c_from = emit_int_compatible_expr(from, data_items);
                let c_by = emit_int_compatible_expr(by, data_items);
                out.push_str(&format!(
                    "{pad}{c_var} = {c_from};\n{pad}while (!({cond})) {{\n"
                ));
                for s in body {
                    emit_statement(
                        out,
                        s,
                        data_items,
                        paragraphs,
                        fs_map,
                        has_declaratives,
                        indent + 1,
                    );
                }
                out.push_str(&format!("{pad}    {c_var} += {c_by};\n"));
                out.push_str(&format!("{pad}}}\n"));
            }
        }
        HirPerformKind::ProcedureName { name, through } => {
            let c_name = sanitize_name(name);
            let in_body = IN_BODY_CONTEXT.with(|flag| *flag.borrow());
            let has_labels = GOTO_LABEL_MAP.with(|map| !map.borrow().is_empty());
            let need_body_dispatch = in_body && has_labels;
            if let Some(thru) = through {
                // PERFORM name THRU through: call all paragraphs from name to through
                let c_thru = sanitize_name(thru);
                out.push_str(&format!("{pad}/* PERFORM {c_name} THRU {c_thru} */\n"));
                let start_idx = paragraphs
                    .iter()
                    .position(|p| sanitize_name(&p.name) == c_name);
                let end_idx = paragraphs
                    .iter()
                    .position(|p| sanitize_name(&p.name) == c_thru);
                if let (Some(si), Some(ei)) = (start_idx, end_idx) {
                    let (lo, hi) = if si <= ei { (si, ei) } else { (ei, si) };
                    let thru_paras: Vec<_> = paragraphs[lo..=hi]
                        .iter()
                        .map(|p| sanitize_name(&p.name))
                        .collect();

                    if has_labels && thru_paras.len() > 1 {
                        // Generate unique label suffix for this PERFORM THRU
                        let pt_id = PERFORM_THRU_COUNTER.with(|c| {
                            let mut v = c.borrow_mut();
                            *v += 1;
                            *v
                        });
                        let suffix = format!("pt{pt_id}");
                        // Collect label IDs for paragraphs in the THRU range
                        let thru_ids: Vec<(String, usize)> = GOTO_LABEL_MAP.with(|map| {
                            let m = map.borrow();
                            thru_paras
                                .iter()
                                .filter_map(|pn| m.get(pn).map(|id| (pn.clone(), *id)))
                                .collect()
                        });

                        // Emit each paragraph call with goto dispatch
                        for (idx, pn) in thru_paras.iter().enumerate() {
                            out.push_str(&format!("_pt_{suffix}_{pn}:\n"));
                            out.push_str(&format!("{pad}para_{pn}();\n"));
                            if idx < thru_paras.len() - 1 {
                                // After each call (except last), check _goto_target
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                ));
                            } else {
                                // After last call, check for out-of-range goto
                                if has_labels {
                                    out.push_str(&format!(
                                        "{pad}if (_goto_target) goto _pt_disp_{suffix};\n"
                                    ));
                                }
                            }
                        }
                        out.push_str(&format!("{pad}goto _pt_end_{suffix};\n"));

                        // Dispatch table for this PERFORM THRU
                        out.push_str(&format!("_pt_disp_{suffix}:\n"));
                        out.push_str(&format!("{pad}{{ int _t = _goto_target;\n"));
                        for (pn, id) in &thru_ids {
                            out.push_str(&format!(
                                "{pad}  if (_t == {id}) {{ _goto_target = 0; goto _pt_{suffix}_{pn}; }}\n"
                            ));
                        }
                        // Not in range: propagate
                        if need_body_dispatch {
                            out.push_str(&format!("{pad}  goto _goto_dispatch;\n"));
                        } else {
                            out.push_str(&format!("{pad}  return;\n"));
                        }
                        out.push_str(&format!("{pad}}}\n"));
                        out.push_str(&format!("_pt_end_{suffix}:;\n"));
                    } else {
                        for pn in &thru_paras {
                            out.push_str(&format!("{pad}para_{pn}();\n"));
                            if need_body_dispatch {
                                out.push_str(&format!(
                                    "{pad}if (_goto_target) goto _goto_dispatch;\n"
                                ));
                            }
                        }
                    }
                } else {
                    // Fallback: just call the named paragraph
                    out.push_str(&format!("{pad}para_{c_name}();\n"));
                    if need_body_dispatch {
                        out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                    }
                }
            } else {
                out.push_str(&format!("{pad}para_{c_name}();\n"));
                if need_body_dispatch {
                    out.push_str(&format!("{pad}if (_goto_target) goto _goto_dispatch;\n"));
                } else if has_labels {
                    // In paragraph function: propagate _goto_target via return
                    out.push_str(&format!("{pad}if (_goto_target) return;\n"));
                }
            }
        }
    }
}

/// Emit ON SIZE ERROR / NOT ON SIZE ERROR handlers.
///
/// Uses a simplified approach: the arithmetic is already emitted, so we
/// emit the NOT ON SIZE ERROR body unconditionally and add a TODO comment
/// for actual overflow detection.
#[allow(clippy::too_many_arguments)]
fn emit_on_size_error(
    out: &mut String,
    on_size_error: &[HirStatement],
    not_on_size_error: &[HirStatement],
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    if on_size_error.is_empty() && not_on_size_error.is_empty() {
        return;
    }
    let pad = "    ".repeat(indent);
    // The caller is responsible for setting `_size_error` flag before calling this.
    // We emit: if (_size_error) { ON SIZE ERROR body } else { NOT ON SIZE ERROR body }
    if !on_size_error.is_empty() || !not_on_size_error.is_empty() {
        out.push_str(&format!("{pad}if (_size_error) {{\n"));
        for s in on_size_error {
            emit_statement(
                out,
                s,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent + 1,
            );
        }
        if !not_on_size_error.is_empty() {
            out.push_str(&format!("{pad}}} else {{\n"));
            for s in not_on_size_error {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

/// Emit ON EXCEPTION / NOT ON EXCEPTION handlers for CALL.
///
/// Uses `_call_failed` flag (declared in caller scope) to branch.
/// Currently the flag is always 0 (success) since we don't yet detect
/// dynamic-link failures, but the code path is now reachable.
#[allow(clippy::too_many_arguments)]
fn emit_on_exception(
    out: &mut String,
    on_exception: &[HirStatement],
    not_on_exception: &[HirStatement],
    data_items: &[HirDataItem],
    paragraphs: &[HirParagraph],
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    indent: usize,
) {
    if on_exception.is_empty() && not_on_exception.is_empty() {
        return;
    }
    let pad = "    ".repeat(indent);
    if !on_exception.is_empty() || !not_on_exception.is_empty() {
        out.push_str(&format!("{pad}if (_call_failed) {{\n"));
        for s in on_exception {
            emit_statement(
                out,
                s,
                data_items,
                paragraphs,
                fs_map,
                has_declaratives,
                indent + 1,
            );
        }
        if !not_on_exception.is_empty() {
            out.push_str(&format!("{pad}}} else {{\n"));
            for s in not_on_exception {
                emit_statement(
                    out,
                    s,
                    data_items,
                    paragraphs,
                    fs_map,
                    has_declaratives,
                    indent + 1,
                );
            }
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

/// Emit an expression as a numeric C value, auto-converting CobolDecimal
/// variables to int64 using the DECIMAL_NAMES thread-local set.
/// Used for function call arguments where `(double)CobolDecimal` casts are invalid.
fn emit_expr_as_numeric(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let is_dec = DECIMAL_NAMES.with(|cell| cell.borrow().contains(&c_name));
            let is_grp = GROUP_NAMES.with(|cell| cell.borrow().contains(&c_name));
            if is_dec {
                format!("cobol_decimal_to_int64(&{c_name})")
            } else if is_grp {
                // Group variables are C unions; cast to 0 in numeric context
                // (groups used in arithmetic are unusual; default to 0).
                "((int64_t)0)".to_string()
            } else {
                c_name
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = emit_expr_as_numeric(left);
            let r = emit_expr_as_numeric(right);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("((int64_t)pow((double){l}, (double){r}))"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_expr_as_numeric(operand);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        _ => emit_expr(expr),
    }
}

/// Emit an expression as a `double`, preserving decimal fractional parts.
/// Used for math intrinsic function arguments (ACOS, ASIN, COS, SIN, TAN,
/// LOG, SQRT, etc.) where truncating to int64 loses precision.
fn emit_expr_as_double(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let is_dec = DECIMAL_NAMES.with(|cell| cell.borrow().contains(&c_name));
            if is_dec {
                format!("cobol_decimal_to_double(&{c_name})")
            } else {
                format!("(double){c_name}")
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = emit_expr_as_double(left);
            let r = emit_expr_as_double(right);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("pow({l}, {r})"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_expr_as_double(operand);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => format!("(double){n}"),
        HirExpr::Literal(HirLiteral::Decimal(d)) => d.to_string(),
        _ => {
            let e = emit_expr_as_numeric(expr);
            format!("(double)({e})")
        }
    }
}

/// Emit alphanumeric MAX/MIN: builds arrays of pointers and lengths, calls runtime,
/// returns pointer to the winning element.
fn emit_alpha_max_min(args: &[HirExpr], func: &str) -> String {
    let n = args.len();
    let mut ptrs = Vec::new();
    let mut lens = Vec::new();
    for arg in args {
        let (c_src, c_len) = emit_string_func_arg(arg);
        ptrs.push(format!("(const uint8_t*){c_src}"));
        lens.push(c_len);
    }
    let ptrs_init = ptrs.join(", ");
    let lens_init = lens.join(", ");
    format!(
        "({{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
         uint32_t _al[] = {{{lens_init}}}; \
         int32_t _ai = {func}(_ap, _al, {n}); \
         _ap[_ai]; }})"
    )
}

/// Emit alphanumeric ORD-MAX/ORD-MIN: returns 1-based position.
fn emit_alpha_ord_max_min(args: &[HirExpr], func: &str) -> String {
    let n = args.len();
    let mut ptrs = Vec::new();
    let mut lens = Vec::new();
    for arg in args {
        let (c_src, c_len) = emit_string_func_arg(arg);
        ptrs.push(format!("(const uint8_t*){c_src}"));
        lens.push(c_len);
    }
    let ptrs_init = ptrs.join(", ");
    let lens_init = lens.join(", ");
    format!(
        "({{ const uint8_t* _ap[] = {{{ptrs_init}}}; \
         uint32_t _al[] = {{{lens_init}}}; \
         {func}(_ap, _al, {n}); }})"
    )
}

/// Helper to extract (c_source_ptr, byte_size) for a string function argument.
/// For string literals, returns ("\"escaped\"", len).
/// For variables, returns (c_name, sizeof(c_name)).
/// For nested string functions, returns the expression and its size.
fn emit_string_func_arg(expr: &HirExpr) -> (String, String) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            (format!("\"{escaped}\""), format!("{}", s.len()))
        }
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            (c_name.clone(), format!("sizeof({c_name})"))
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_fn = name.to_uppercase();
            match upper_fn.as_str() {
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    if let Some(inner) = args.first() {
                        let (_, size) = emit_string_func_arg(inner);
                        let e = emit_expr(expr);
                        (e, size)
                    } else {
                        ("((uint8_t*)0)".to_string(), "0".to_string())
                    }
                }
                "CHAR" => (emit_expr(expr), "1".to_string()),
                "CURRENT-DATE" | "WHEN-COMPILED" => (emit_expr(expr), "21".to_string()),
                _ => {
                    let e = emit_expr(expr);
                    (format!("&{e}"), format!("sizeof({e})"))
                }
            }
        }
        _ => {
            let e = emit_expr(expr);
            (e.clone(), format!("sizeof({e})"))
        }
    }
}

fn emit_expr(expr: &HirExpr) -> String {
    match expr {
        HirExpr::Literal(HirLiteral::Integer(n)) => format!("((int64_t){n})"),
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            // Validate that the decimal literal contains only safe characters
            // to prevent injection into the generated C source.
            if d.chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+')
            {
                d.to_string()
            } else {
                "0 /* invalid decimal */".to_string()
            }
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            // Return string literal as a C string (used as pointer in intrinsic
            // function arguments such as FUNCTION LOWER-CASE("text"))
            let escaped = escape_c_string(s);
            format!("\"{}\"", escaped)
        }
        HirExpr::Literal(HirLiteral::Zero) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::Space) => "((int64_t)32)".to_string(),
        HirExpr::Literal(HirLiteral::HighValue) => "((int64_t)0xFF)".to_string(),
        HirExpr::Literal(HirLiteral::LowValue) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::Quote) => "((int64_t)'\"')".to_string(),
        HirExpr::Literal(HirLiteral::Null) => "((int64_t)0)".to_string(),
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            if let Some(ch) = s.chars().next() {
                format!("((int64_t)'{}')", ch)
            } else {
                "((int64_t)' ')".to_string()
            }
        }
        HirExpr::Variable(name) => sanitize_name(name),
        HirExpr::BinaryOp { op, left, right } => {
            // Use emit_expr_as_numeric to auto-convert CobolDecimal sub-expressions
            let l = emit_expr_as_numeric(left);
            let r = emit_expr_as_numeric(right);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("((int64_t)pow((double){l}, (double){r}))"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_expr_as_numeric(operand);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_name = name.to_uppercase();
            let c_args: Vec<_> = args.iter().map(emit_expr_as_numeric).collect();
            // Map COBOL intrinsic function names to runtime function calls.
            match upper_name.as_str() {
                "LENGTH" => {
                    // FUNCTION LENGTH(var) -- returns the byte length.
                    if let Some(arg_expr) = args.first() {
                        // Check if the arg is a string-returning function
                        if let HirExpr::FunctionCall {
                            name: inner_name,
                            args: inner_args,
                        } = arg_expr
                        {
                            let inner_upper = inner_name.to_uppercase();
                            match inner_upper.as_str() {
                                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                                    if let Some(inner_arg) = inner_args.first() {
                                        let size = if let HirExpr::Literal(HirLiteral::String(s)) =
                                            inner_arg
                                        {
                                            format!("{}", s.len())
                                        } else {
                                            let c_arg = emit_expr(inner_arg);
                                            format!("sizeof({c_arg})")
                                        };
                                        return format!("((int64_t){size})");
                                    }
                                }
                                "CHAR" => return "((int64_t)1)".to_string(),
                                "CURRENT-DATE" | "WHEN-COMPILED" => {
                                    return "((int64_t)21)".to_string()
                                }
                                _ => {}
                            }
                        }
                        if let HirExpr::Literal(HirLiteral::String(s)) = arg_expr {
                            return format!("((int64_t){})", s.len());
                        }
                        let c_arg = &c_args[0];
                        format!("cobol_func_length((const uint8_t*){c_arg}, sizeof({c_arg}))")
                    } else {
                        "0".to_string()
                    }
                }
                "NUMVAL" | "NUMVAL-C" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_numval((const uint8_t*){arg}, sizeof({arg}))")
                    } else {
                        "0".to_string()
                    }
                }
                "MAX" => {
                    let has_alpha = args
                        .iter()
                        .any(|a| matches!(a, HirExpr::Literal(HirLiteral::String(_))));
                    if has_alpha && !args.is_empty() {
                        emit_alpha_max_min(args, "cobol_func_max_alpha")
                    } else if c_args.len() >= 2 {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_max_int_n(_mv, {}); }})",
                            c_args.len()
                        )
                    } else {
                        c_args.first().cloned().unwrap_or_else(|| "0".to_string())
                    }
                }
                "MIN" => {
                    let has_alpha = args
                        .iter()
                        .any(|a| matches!(a, HirExpr::Literal(HirLiteral::String(_))));
                    if has_alpha && !args.is_empty() {
                        emit_alpha_max_min(args, "cobol_func_min_alpha")
                    } else if c_args.len() >= 2 {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _mv[] = {{{arg_list}}}; \
                             cobol_func_min_int_n(_mv, {}); }})",
                            c_args.len()
                        )
                    } else {
                        c_args.first().cloned().unwrap_or_else(|| "0".to_string())
                    }
                }
                "MOD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_mod({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "INTEGER" | "INTEGER-PART" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer({arg}, 0)")
                    } else {
                        "0".to_string()
                    }
                }
                "ORD" => {
                    if let Some(arg_expr) = args.first() {
                        match arg_expr {
                            HirExpr::Literal(HirLiteral::String(s)) => {
                                if let Some(ch) = s.bytes().next() {
                                    format!("cobol_func_ord({ch})")
                                } else {
                                    "cobol_func_ord(0)".to_string()
                                }
                            }
                            HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
                                // Variable may be a char array; dereference
                                // the first byte.
                                let c = emit_expr(arg_expr);
                                format!("cobol_func_ord((uint8_t)*((const uint8_t*){c}))")
                            }
                            _ => {
                                if let Some(arg) = c_args.first() {
                                    format!("cobol_func_ord((uint8_t){arg})")
                                } else {
                                    "0".to_string()
                                }
                            }
                        }
                    } else {
                        "0".to_string()
                    }
                }
                "CHAR" => {
                    if let Some(arg) = c_args.first() {
                        format!("({{ static uint8_t _chbuf[2]; _chbuf[0] = cobol_func_char((uint32_t){arg}); _chbuf[1] = '\\0'; _chbuf; }})")
                    } else {
                        "0".to_string()
                    }
                }
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    // String-returning functions: copy arg to temp buffer, apply, return buffer
                    if let Some(arg_expr) = args.first() {
                        let func = match upper_name.as_str() {
                            "UPPER-CASE" => "cobol_func_upper_case",
                            "LOWER-CASE" => "cobol_func_lower_case",
                            _ => "cobol_func_reverse",
                        };
                        let (c_src, size) = emit_string_func_arg(arg_expr);
                        format!(
                            "({{ static uint8_t _sfbuf[{size}]; \
                             memcpy(_sfbuf, (const uint8_t*){c_src}, {size}); \
                             {func}(_sfbuf, {size}); _sfbuf; }})"
                        )
                    } else {
                        "((uint8_t*)0)".to_string()
                    }
                }
                "ABS" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_abs({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "SQRT" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_sqrt({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "EXP" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_exp({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "EXP10" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_exp10({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "LOG" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_log({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "LOG10" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_log10({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "SIN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_sin({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "COS" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_cos({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "TAN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_tan({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ASIN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_asin({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ACOS" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_acos({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "ATAN" => {
                    if let Some(arg) = args.first() {
                        let d = emit_expr_as_double(arg);
                        format!("cobol_func_atan({d})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "FACTORIAL" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_factorial({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "REM" | "REMAINDER" => {
                    if args.len() >= 2 {
                        let d0 = emit_expr_as_double(&args[0]);
                        let d1 = emit_expr_as_double(&args[1]);
                        format!("cobol_func_rem({d0}, {d1})")
                    } else {
                        "0.0".to_string()
                    }
                }
                "RANDOM" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_random({arg})")
                    } else {
                        "cobol_func_random(0)".to_string()
                    }
                }
                "SIGN" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_sign({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "CEILING" | "CEIL" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_ceiling((double){arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "FLOOR" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_floor((double){arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "ANNUITY" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_annuity((double){}, {})", c_args[0], c_args[1])
                    } else {
                        "0.0".to_string()
                    }
                }
                "STORED-CHAR-LENGTH" => {
                    if let Some(arg) = c_args.first() {
                        format!(
                            "cobol_func_stored_char_length(\
                             (const uint8_t*){arg}, sizeof({arg}))"
                        )
                    } else {
                        "0".to_string()
                    }
                }
                "MEAN" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_mean(_mv, {}); }})",
                        c_args.len()
                    )
                }
                "MEDIAN" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_median(_mv, {}); }})",
                        c_args.len()
                    )
                }
                "RANGE" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _rv[] = {{{joined}}}; \
                         cobol_func_range(_rv, {}); }})",
                        c_args.len()
                    )
                }
                "MIDRANGE" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_midrange(_mv, {}); }})",
                        c_args.len()
                    )
                }
                "STANDARD-DEVIATION" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_standard_deviation(_mv, {}); }})",
                        c_args.len()
                    )
                }
                "VARIANCE" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _mv[] = {{{joined}}}; \
                         cobol_func_variance(_mv, {}); }})",
                        c_args.len()
                    )
                }
                "PRESENT-VALUE" => {
                    if c_args.len() >= 2 {
                        let rate = &c_args[0];
                        let rest: Vec<_> =
                            c_args[1..].iter().map(|a| format!("(double){a}")).collect();
                        let joined = rest.join(", ");
                        format!(
                            "({{ double _pv[] = {{{joined}}}; \
                             cobol_func_present_value((double){rate}, _pv, {}); }})",
                            rest.len()
                        )
                    } else {
                        "0.0".to_string()
                    }
                }
                "SUM" => {
                    let arg_list = c_args
                        .iter()
                        .map(|a| format!("(double){a}"))
                        .collect::<Vec<_>>();
                    let joined = arg_list.join(", ");
                    format!(
                        "({{ double _sv[] = {{{joined}}}; \
                         cobol_func_sum_float(_sv, {}); }})",
                        c_args.len()
                    )
                }
                "ORD-MAX" => {
                    let has_alpha = args
                        .iter()
                        .any(|a| matches!(a, HirExpr::Literal(HirLiteral::String(_))));
                    if has_alpha && !args.is_empty() {
                        emit_alpha_ord_max_min(args, "cobol_func_ord_max_alpha")
                    } else {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _om[] = {{{arg_list}}}; \
                             cobol_func_ord_max(_om, {}); }})",
                            c_args.len()
                        )
                    }
                }
                "ORD-MIN" => {
                    let has_alpha = args
                        .iter()
                        .any(|a| matches!(a, HirExpr::Literal(HirLiteral::String(_))));
                    if has_alpha && !args.is_empty() {
                        emit_alpha_ord_max_min(args, "cobol_func_ord_min_alpha")
                    } else {
                        let arg_list = c_args.join(", ");
                        format!(
                            "({{ int64_t _om[] = {{{arg_list}}}; \
                             cobol_func_ord_min(_om, {}); }})",
                            c_args.len()
                        )
                    }
                }
                "INTEGER-OF-DATE" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer_of_date({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DATE-OF-INTEGER" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_date_of_integer({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "INTEGER-OF-DAY" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_integer_of_day({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DAY-OF-INTEGER" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_day_of_integer({arg})")
                    } else {
                        "0".to_string()
                    }
                }
                "DATE-TO-YYYYMMDD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_date_to_yyyymmdd({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "YEAR-TO-YYYY" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_year_to_yyyy({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "DAY-TO-YYYYDDD" => {
                    if c_args.len() >= 2 {
                        format!("cobol_func_day_to_yyyyddd({}, {})", c_args[0], c_args[1])
                    } else {
                        "0".to_string()
                    }
                }
                "TEST-DATE-YYYYMMDD" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_test_date_yyyymmdd({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "TEST-DAY-YYYYDDD" => {
                    if let Some(arg) = c_args.first() {
                        format!("cobol_func_test_day_yyyyddd({arg})")
                    } else {
                        "1".to_string()
                    }
                }
                "CURRENT-DATE" => {
                    "({ static uint8_t _cdbuf[22]; cobol_func_current_date(_cdbuf, 21); _cdbuf; })"
                        .to_string()
                }
                "WHEN-COMPILED" => {
                    "({ static uint8_t _wcbuf[22]; cobol_func_when_compiled(_wcbuf, 21); _wcbuf; })"
                        .to_string()
                }
                "NATIONAL-OF" => {
                    // FUNCTION NATIONAL-OF(alphanumeric-var)
                    // Returns a national value; in expression context, emit
                    // as a statement expression that fills a temp buffer.
                    if let Some(arg) = c_args.first() {
                        format!(
                            "({{ static uint16_t _ntmp[256]; \
                             cobol_func_national_of(\
                             (const uint8_t*){arg}, sizeof({arg}), \
                             _ntmp, 256); _ntmp; }})"
                        )
                    } else {
                        "((uint16_t*)0)".to_string()
                    }
                }
                "DISPLAY-OF" => {
                    // FUNCTION DISPLAY-OF(national-var)
                    // Returns an alphanumeric value.
                    if let Some(arg) = c_args.first() {
                        format!(
                            "({{ static char _dtmp[256]; \
                             cobol_func_display_of(\
                             (const uint16_t*){arg}, sizeof({arg})/sizeof(uint16_t), \
                             (uint8_t*)_dtmp, 256); _dtmp; }})"
                        )
                    } else {
                        "((char*)0)".to_string()
                    }
                }
                _ => {
                    // User-defined or unhandled intrinsic function:
                    // use cobol_func_ prefix with lowercase name so it matches
                    // the runtime function naming convention.
                    let c_name = sanitize_name(name).to_lowercase();
                    format!("cobol_func_{c_name}({})", c_args.join(", "))
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length: _,
        } => {
            // In numeric expression context, reference modification returns
            // a pointer expression. This is unusual but we emit it for
            // completeness. Callers like emit_display_operand handle the
            // display case directly.
            let c_var = sanitize_name(variable);
            let c_start = emit_expr_as_numeric(start);
            format!("({c_var} + ({c_start} - 1))")
        }
        HirExpr::Subscript {
            variable,
            subscripts,
        } => emit_subscript_access(variable, subscripts),
    }
}

/// Generates C code for subscripted table access.
/// COBOL subscripts are 1-based; C arrays are 0-based.
///
/// For items inside groups with nested OCCURS, generates proper C struct
/// access paths with subscripts at each OCCURS level.  For example:
///   01 TABLE-1. 05 GRP OCCURS 3. 10 ITEM PIC 9 OCCURS 4.
/// `ITEM(I, J)` becomes:
///   `TABLE_1.members._m_GRP[(I)-1].members._m_ITEM[(J)-1]`
fn emit_subscript_access(variable: &smol_str::SmolStr, subscripts: &[HirExpr]) -> String {
    let c_name = sanitize_name(variable);
    // Check if we have pre-computed path info for this variable (nested OCCURS)
    let path_info = SUBSCRIPT_PATHS.with(|cell| cell.borrow().get(&c_name).cloned());

    if let Some(ref info) = path_info {
        let occurs_count = info.segments.iter().filter(|(_, has)| *has).count();
        if occurs_count > 0 && subscripts.len() >= occurs_count {
            // Build the full struct access path, inserting subscripts at OCCURS levels
            let mut access = info.root.clone();
            let mut sub_idx = 0;
            for (segment_suffix, has_occurs) in &info.segments {
                access.push_str(segment_suffix);
                if *has_occurs && sub_idx < subscripts.len() {
                    let idx = emit_expr_as_numeric(&subscripts[sub_idx]);
                    access.push_str(&format!("[({idx}) - 1]"));
                    sub_idx += 1;
                }
            }
            return access;
        }
    }

    // Fallback: simple flat array subscript (top-level OCCURS without group nesting)
    if subscripts.len() == 1 {
        let idx = emit_expr_as_numeric(&subscripts[0]);
        format!("{c_name}[({idx}) - 1]")
    } else {
        let mut access = c_name;
        for sub in subscripts {
            let idx = emit_expr_as_numeric(sub);
            access = format!("{access}[({idx}) - 1]");
        }
        access
    }
}

/// Returns true if the given HirType requires CobolDecimal representation
/// (i.e., has fractional decimal places).
fn needs_decimal(data_type: &HirType) -> bool {
    matches!(
        data_type,
        HirType::Numeric { decimal_places, .. } if *decimal_places > 0
    ) || matches!(data_type, HirType::Comp3 { decimal_places, .. } if *decimal_places > 0)
}

/// Returns true if the given expression refers to a CobolDecimal variable.
fn is_decimal_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    let name = expr_var_name(expr);
    if name.is_empty() {
        return false;
    }
    find_data_item(name, data_items).is_some_and(|i| needs_decimal(&i.data_type))
}

/// Check whether an expression refers to a group variable (emitted as a C union).
fn is_group_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    let name = expr_var_name(expr);
    if name.is_empty() {
        return false;
    }
    find_data_item(name, data_items).is_some_and(|i| matches!(i.data_type, HirType::Group { .. }))
}

/// Check whether an expression refers to an alphanumeric variable (emitted as `char[]`).
fn is_alpha_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    let name = expr_var_name(expr);
    if name.is_empty() {
        return false;
    }
    find_data_item(name, data_items)
        .is_some_and(|i| matches!(i.data_type, HirType::Alphanumeric { .. }))
}

/// Check whether an expression tree contains any decimal variable or decimal
/// literal, meaning that converting to int64 would lose fractional precision.
fn expr_contains_decimal(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            DECIMAL_NAMES.with(|cell| cell.borrow().contains(&c_name))
        }
        HirExpr::Literal(HirLiteral::Decimal(_)) => true,
        HirExpr::BinaryOp { left, right, .. } => {
            expr_contains_decimal(left) || expr_contains_decimal(right)
        }
        HirExpr::UnaryOp { operand, .. } => expr_contains_decimal(operand),
        HirExpr::FunctionCall { args, .. } => args.iter().any(expr_contains_decimal),
        _ => false,
    }
}

/// Emit an expression as int64, converting CobolDecimal to int64 if needed.
/// For simple variable/subscript expressions, wraps with cobol_decimal_to_int64.
/// For compound expressions (BinaryOp etc), recursively converts sub-expressions.
fn emit_int_compatible_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> String {
    match expr {
        HirExpr::Variable(_) | HirExpr::Subscript { .. } => {
            if is_decimal_expr(expr, data_items) {
                let c = emit_expr(expr);
                format!("cobol_decimal_to_int64(&{c})")
            } else if is_group_expr(expr, data_items) {
                // Group variables are C unions; convert via cobol_func_numval
                // (treat group bytes as alphanumeric and parse as a number).
                let c = emit_expr(expr);
                let var_name = expr_var_name(expr);
                let size = find_data_item_size(&sanitize_name(var_name), data_items);
                format!("cobol_func_numval((const uint8_t*)&{c}, {size})")
            } else if is_alpha_expr(expr, data_items) {
                // Alphanumeric fields are char[] in C; convert to int via numval.
                let c = emit_expr(expr);
                let var_name = expr_var_name(expr);
                let size = find_data_item_size(&sanitize_name(var_name), data_items);
                format!("cobol_func_numval((const uint8_t*){c}, {size})")
            } else {
                emit_expr(expr)
            }
        }
        HirExpr::BinaryOp { op, left, right } => {
            let l = emit_int_compatible_expr(left, data_items);
            let r = emit_int_compatible_expr(right, data_items);
            let op_str = match op {
                HirBinOp::Add => "+",
                HirBinOp::Sub => "-",
                HirBinOp::Mul => "*",
                HirBinOp::Div => "/",
                HirBinOp::Pow => return format!("((int64_t)pow((double){l}, (double){r}))"),
            };
            format!("({l} {op_str} {r})")
        }
        HirExpr::UnaryOp { op, operand } => {
            let o = emit_int_compatible_expr(operand, data_items);
            match op {
                HirUnaryOp::Neg => format!("(-{o})"),
            }
        }
        HirExpr::FunctionCall { .. } => {
            // FunctionCall results are always numeric C types (int64_t/double).
            // emit_expr now uses emit_expr_as_numeric for function arguments,
            // which auto-converts CobolDecimal variables via the DECIMAL_NAMES
            // thread-local, so we can safely delegate.
            emit_expr(expr)
        }
        _ => emit_expr(expr),
    }
}

/// Emit code to assign a value to a CobolDecimal target.
/// Handles integer literals, decimal literals, Zero, CobolDecimal sources,
/// and integer variable sources.
fn emit_assign_to_decimal(
    out: &mut String,
    from: &HirExpr,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match from {
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            out.push_str(&format!(
                "{pad}cobol_decimal_from_int({n}, 0, &{c_target});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let (scaled, scale) = parse_decimal_literal(d);
            out.push_str(&format!(
                "{pad}cobol_decimal_from_int({scaled}, {scale}, &{c_target});\n"
            ));
        }
        HirExpr::Literal(HirLiteral::Zero) | HirExpr::Literal(HirLiteral::Null) => {
            // Only zero the value; preserve scale/size/is_signed so that
            // subsequent double-precision arithmetic (cobol_decimal_to_double,
            // cobol_decimal_from_double) still knows the field's precision.
            out.push_str(&format!("{pad}{c_target}.value = 0;\n"));
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}cobol_decimal_from_string(\
                 (const uint8_t*)\"{escaped}\", {len}, &{c_target});\n"
            ));
        }
        _ => {
            if is_decimal_expr(from, data_items) {
                // CobolDecimal to CobolDecimal: struct copy
                let c_src = emit_expr(from);
                out.push_str(&format!("{pad}{c_target} = {c_src};\n"));
            } else if expr_contains_decimal(from)
                || matches!(from, HirExpr::BinaryOp { .. } | HirExpr::UnaryOp { .. }
                    if expr_contains_decimal(from))
            {
                // Expression contains decimal sub-expressions or fractional
                // literals: use double arithmetic to preserve precision, then
                // convert back via cobol_decimal_from_double which respects the
                // target's existing scale.
                let e = emit_expr_as_double(from);
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_double({e}, &{c_target});\n"
                ));
            } else {
                // Integer variable or expression -> CobolDecimal
                // Use emit_int_compatible_expr to handle BinaryOp/UnaryOp
                // that may contain CobolDecimal sub-expressions.
                let e = emit_int_compatible_expr(from, data_items);
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int({e}, 0, &{c_target});\n"
                ));
            }
        }
    }
}

/// Extract the base variable name from a `HirExpr` for data-item lookups.
/// Returns the variable name for `Variable` and `Subscript` variants,
/// or an empty string for other expression types.
fn expr_var_name(expr: &HirExpr) -> &str {
    match expr {
        HirExpr::Variable(name) => name.as_str(),
        HirExpr::Subscript { variable, .. } => variable.as_str(),
        _ => "",
    }
}

/// Look up a data item by name (searching flattened items including group members).
fn find_data_item<'a>(name: &str, data_items: &'a [HirDataItem]) -> Option<&'a HirDataItem> {
    // Handle qualified names like "WS-DST::FIELD-A"
    if let Some(pos) = name.find("::") {
        let group_name = &name[..pos];
        let member_name = &name[pos + 2..];
        // Find the group, then search within it
        for item in data_items {
            if item.name.as_str() == group_name {
                if let HirType::Group { members, .. } = &item.data_type {
                    return find_data_item(member_name, members);
                }
            }
        }
        // Fallback: try unqualified search
        return find_data_item(member_name, data_items);
    }
    for item in data_items {
        if item.name.as_str() == name {
            return Some(item);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_data_item(name, members) {
                return Some(found);
            }
        }
    }
    None
}

/// Check if a name refers to an INDEX variable (declared via INDEXED BY).
fn is_index_name(name: &str, data_items: &[HirDataItem]) -> bool {
    for item in data_items {
        for idx_name in &item.indexed_by {
            if idx_name.as_str() == name {
                return true;
            }
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if is_index_name(name, members) {
                return true;
            }
        }
    }
    false
}

/// Check if a sanitized C variable name corresponds to a group item.
fn is_group_item_c(c_name: &str, data_items: &[HirDataItem]) -> bool {
    is_group_item_c_in(c_name, data_items)
}

fn is_group_item_c_in(c_name: &str, items: &[HirDataItem]) -> bool {
    for item in items {
        let item_c = sanitize_name(&item.name);
        if item_c == c_name {
            return matches!(&item.data_type, HirType::Group { .. });
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if is_group_item_c_in(c_name, members) {
                return true;
            }
        }
    }
    false
}

/// Check if a sanitized C variable name corresponds to a numeric, binary,
/// comp3, index, or other non-array type stored as int64_t/CobolDecimal.
fn is_numeric_item_c(c_name: &str, data_items: &[HirDataItem]) -> bool {
    is_numeric_item_c_in(c_name, data_items)
}

fn is_numeric_item_c_in(c_name: &str, items: &[HirDataItem]) -> bool {
    for item in items {
        let item_c = sanitize_name(&item.name);
        if item_c == c_name {
            return matches!(
                &item.data_type,
                HirType::Numeric { .. }
                    | HirType::Comp3 { .. }
                    | HirType::Binary { .. }
                    | HirType::Index
                    | HirType::FloatShort
                    | HirType::FloatLong
                    | HirType::FloatExtended
                    | HirType::Boolean
            );
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if is_numeric_item_c_in(c_name, members) {
                return true;
            }
        }
    }
    false
}

/// Return a C expression suitable for use in pointer casts.
/// For group items (C unions), returns `&name` since unions cannot be cast
/// to pointers directly. For elementary items (arrays/scalars), returns `name`.
fn c_ptr_expr(c_name: &str, data_items: &[HirDataItem]) -> String {
    if is_group_item_c(c_name, data_items) {
        format!("&{c_name}")
    } else if is_numeric_item_c(c_name, data_items) {
        // Numeric items are stored as int64_t or CobolDecimal, so we need &
        // to get a pointer to the storage (not cast the value itself)
        format!("&{c_name}")
    } else {
        // Alphanumeric and National are char[] / uint16_t[], which decay to pointers
        c_name.to_string()
    }
}

/// Resolve a variable name to its fully-qualified C name.
/// If the variable is a group member, returns the qualified path
/// (e.g., `WS_SRC.members._m_FIELD_A`).
/// If it's a top-level variable, returns `sanitize_name(name)`.
/// Get the group members of a data item by COBOL name.
fn get_group_members<'a>(name: &str, data_items: &'a [HirDataItem]) -> &'a [HirDataItem] {
    if let Some(item) = find_data_item(name, data_items) {
        if let HirType::Group { members, .. } = &item.data_type {
            return members;
        }
    }
    &[]
}

/// Emit MOVE CORRESPONDING: for each member name in `from` group that also
/// exists in `to` group, generate a MOVE from from.member to to.member.
fn emit_corresponding_move(
    out: &mut String,
    from: &str,
    to: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let from_members = get_group_members(from, data_items);
    let to_members = get_group_members(to, data_items);
    let c_from = sanitize_name(from);
    let c_to = sanitize_name(to);
    out.push_str(&format!(
        "{pad}/* MOVE CORRESPONDING {c_from} TO {c_to} */\n"
    ));
    for src_item in from_members {
        for tgt_item in to_members {
            if src_item.name == tgt_item.name {
                let member_c = sanitize_name(&src_item.name);
                // Use qualified macros to avoid collision
                let src_q = format!("{c_from}__{member_c}");
                let tgt_q = format!("{c_to}__{member_c}");
                // For OCCURS items, use memcpy instead of direct assignment
                if src_item.occurs.is_some() || tgt_item.occurs.is_some() {
                    out.push_str(&format!(
                        "{pad}memcpy(&{tgt_q}, &{src_q}, sizeof({tgt_q}));\n"
                    ));
                    continue;
                }
                match (&src_item.data_type, &tgt_item.data_type) {
                    (HirType::Numeric { .. }, HirType::Numeric { .. })
                    | (HirType::Binary { .. }, HirType::Binary { .. })
                    | (HirType::Comp3 { .. }, HirType::Comp3 { .. }) => {
                        out.push_str(&format!("{pad}{tgt_q} = {src_q};\n"));
                    }
                    (
                        HirType::Alphanumeric { size: src_sz },
                        HirType::Alphanumeric { size: tgt_sz },
                    ) => {
                        let copy_len = std::cmp::min(*src_sz, *tgt_sz);
                        out.push_str(&format!("{pad}memcpy({tgt_q}, {src_q}, {copy_len});\n"));
                        if *tgt_sz > *src_sz {
                            out.push_str(&format!(
                                "{pad}memset({tgt_q} + {src_sz}, ' ', {});\n",
                                tgt_sz - src_sz
                            ));
                        }
                    }
                    _ => {
                        out.push_str(&format!(
                            "{pad}memcpy(&{tgt_q}, &{src_q}, sizeof({tgt_q}));\n"
                        ));
                    }
                }
            }
        }
    }
}

/// Emit ADD/SUBTRACT CORRESPONDING: for each matching numeric member,
/// generate target.member = target.member op source.member.
fn emit_corresponding_arith(
    out: &mut String,
    from: &str,
    to: &str,
    op: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let from_members = get_group_members(from, data_items);
    let to_members = get_group_members(to, data_items);
    let c_from = sanitize_name(from);
    let c_to = sanitize_name(to);
    let op_name = if op == "+" { "ADD" } else { "SUBTRACT" };
    out.push_str(&format!(
        "{pad}/* {op_name} CORRESPONDING {c_from} TO {c_to} */\n"
    ));
    for src_item in from_members {
        for tgt_item in to_members {
            if src_item.name == tgt_item.name && is_numeric_type(&tgt_item.data_type) {
                let member_c = sanitize_name(&src_item.name);
                // Use qualified names: GROUP.members._m_MEMBER for disambiguation
                let src_ref = format!("{c_from}.members._m_{member_c}");
                let tgt_ref = format!("{c_to}.members._m_{member_c}");
                if needs_decimal(&tgt_item.data_type) {
                    // CobolDecimal: use runtime functions
                    let func = if op == "+" {
                        "cobol_decimal_add"
                    } else {
                        "cobol_decimal_sub"
                    };
                    out.push_str(&format!(
                        "{pad}{func}(&{src_ref}, &{tgt_ref}, &{tgt_ref});\n"
                    ));
                } else {
                    out.push_str(&format!("{pad}{tgt_ref} = {tgt_ref} {op} {src_ref};\n"));
                }
            }
        }
    }
}

/// Check if a HirType is a numeric type (suitable for arithmetic CORRESPONDING).
fn is_numeric_type(ty: &HirType) -> bool {
    matches!(
        ty,
        HirType::Numeric { .. }
            | HirType::Binary { .. }
            | HirType::Comp3 { .. }
            | HirType::FloatShort
            | HirType::FloatLong
            | HirType::FloatExtended
    )
}

/// Get the maximum integer value for a PIC 9(N) field.
fn get_pic_max(name: &str, data_items: &[HirDataItem]) -> Option<i64> {
    let item = find_data_item(name, data_items)?;
    match &item.data_type {
        HirType::Numeric { size, .. } => Some(10_i64.pow(*size) - 1),
        HirType::Binary { size } => Some(10_i64.pow(*size) - 1),
        _ => None,
    }
}

/// Emit overflow check for integer (non-decimal) arithmetic targets.
/// Expects `_prev` and `_size_error` to be in scope.
fn emit_integer_overflow_check(
    out: &mut String,
    target_name: &str,
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(max_val) = get_pic_max(target_name, data_items) {
        out.push_str(&format!(
            "{pad}if (llabs({c_target}) > {max_val}) {{ _size_error = 1; {c_target} = _prev; }}\n"
        ));
    }
}

/// Emit COMPUTE with overflow check: save, assign, check, restore on overflow.
fn emit_save_and_check_overflow(
    out: &mut String,
    target_name: &str,
    c_target: &str,
    c_expr: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    let target_is_decimal =
        find_data_item(target_name, data_items).is_some_and(|i| needs_decimal(&i.data_type));
    if target_is_decimal {
        // CobolDecimal target: save/restore struct, convert expression
        out.push_str(&format!("{pad}{{ CobolDecimal _prev = {c_target};\n"));
        out.push_str(&format!(
            "{pad}cobol_decimal_from_int((int64_t)({c_expr}), 0, &{c_target});\n"
        ));
        if let Some(max_val) = get_pic_max(target_name, data_items) {
            out.push_str(&format!(
                "{pad}if (llabs({c_target}.value) > {max_val}) \
                 {{ _size_error = 1; {c_target} = _prev; }}\n"
            ));
        }
        out.push_str(&format!("{pad}}}\n"));
    } else {
        out.push_str(&format!("{pad}{{ int64_t _prev = {c_target};\n"));
        out.push_str(&format!("{pad}{c_target} = {c_expr};\n"));
        if let Some(max_val) = get_pic_max(target_name, data_items) {
            out.push_str(&format!(
                "{pad}if (llabs({c_target}) > {max_val}) \
                 {{ _size_error = 1; {c_target} = _prev; }}\n"
            ));
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

/// Generate a PICTURE string for use with cobol_decimal_to_display.
/// E.g., Numeric { size: 5, decimal_places: 2, is_signed: true } => "-999.99"
fn generate_pic_string(data_type: &HirType) -> String {
    match data_type {
        HirType::Numeric {
            size,
            decimal_places,
            ..
        }
        | HirType::Comp3 {
            size,
            decimal_places,
        } => {
            let is_signed = match data_type {
                HirType::Numeric { is_signed, .. } => *is_signed,
                _ => true,
            };
            let int_digits = *size as usize - *decimal_places as usize;
            let mut pic = String::new();
            if is_signed {
                pic.push('-');
            }
            for _ in 0..int_digits {
                pic.push('9');
            }
            if *decimal_places > 0 {
                pic.push('.');
                for _ in 0..*decimal_places {
                    pic.push('9');
                }
            }
            pic
        }
        _ => "9".to_string(),
    }
}

/// Emit a decimal arithmetic operation.
/// Converts the operand to a CobolDecimal temporary if needed, then calls the runtime function.
fn emit_decimal_arith(
    out: &mut String,
    c_target: &str,
    operand: &HirExpr,
    func: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    // Check if operand is already a decimal variable
    let op_is_decimal = is_decimal_expr(operand, data_items);

    if op_is_decimal {
        let c_op = emit_expr(operand);
        out.push_str(&format!(
            "{pad}{func}(&{c_target}, &{c_op}, &{c_target});\n"
        ));
    } else {
        // Convert operand to a temporary CobolDecimal (use int-compatible for mixed exprs)
        let c_op = emit_int_compatible_expr(operand, data_items);
        out.push_str(&format!(
            "{pad}{{ CobolDecimal _tmp; cobol_decimal_from_int({c_op}, 0, &_tmp); {func}(&{c_target}, &_tmp, &{c_target}); }}\n"
        ));
    }
}

/// Emit ADD GIVING for decimal: add all operands and TO values, store in GIVING target.
fn emit_decimal_giving_add(
    out: &mut String,
    operands: &[HirExpr],
    to: &[HirExpr],
    c_target: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    // Start by copying the first addend to the target
    let mut first = true;
    for op in operands {
        if first {
            // Initialize target with first operand
            let op_is_decimal = is_decimal_expr(op, data_items);
            if op_is_decimal {
                let c_op = emit_expr(op);
                out.push_str(&format!("{pad}{c_target} = {c_op};\n"));
            } else {
                let c_op = emit_expr(op);
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int({c_op}, 0, &{c_target});\n"
                ));
            }
            first = false;
        } else {
            emit_decimal_arith(out, c_target, op, "cobol_decimal_add", data_items, pad);
        }
    }
    for t in to {
        if first {
            let c_t = emit_expr(t);
            let t_is_decimal = is_decimal_expr(t, data_items);
            if t_is_decimal {
                out.push_str(&format!("{pad}{c_target} = {c_t};\n"));
            } else {
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int({c_t}, 0, &{c_target});\n"
                ));
            }
            first = false;
        } else {
            emit_decimal_arith(out, c_target, t, "cobol_decimal_add", data_items, pad);
        }
    }
}

/// Parse a decimal literal string like "123.45" into (scaled_value, scale).
/// E.g., "123.45" -> (12345, 2), "10.5" -> (105, 1), "100" -> (100, 0).
fn parse_decimal_literal(s: &str) -> (i64, u32) {
    let negative = s.starts_with('-');
    let body = s.trim_start_matches(['+', '-']);
    if let Some(dot_pos) = body.find('.') {
        let int_part = &body[..dot_pos];
        let frac_part = &body[dot_pos + 1..];
        let scale = frac_part.len() as u32;
        let combined: String = int_part.chars().chain(frac_part.chars()).collect();
        let abs_value: i64 = combined.parse().unwrap_or(0);
        if negative {
            (-abs_value, scale)
        } else {
            (abs_value, scale)
        }
    } else {
        let abs_value: i64 = body.parse().unwrap_or(0);
        if negative {
            (-abs_value, 0)
        } else {
            (abs_value, 0)
        }
    }
}

/// Emit INITIALIZE for a single field, choosing the correct default by type.
///
/// COBOL rules: ALPHANUMERIC → spaces, NUMERIC → zero, GROUP → recurse members.
fn emit_initialize_field(
    out: &mut String,
    name: &smol_str::SmolStr,
    c_name: &str,
    data_items: &[HirDataItem],
    pad: &str,
) {
    if let Some(item) = find_data_item(name.as_str(), data_items) {
        // OCCURS items: memset the entire array
        if item.occurs.is_some() {
            out.push_str(&format!(
                "{pad}memset({c_name}, 0, sizeof({c_name})); /* INITIALIZE */\n"
            ));
            return;
        }
        match &item.data_type {
            HirType::Alphanumeric { size } => {
                out.push_str(&format!(
                    "{pad}memset({c_name}, ' ', {size}); {c_name}[{size}] = '\\0'; /* INITIALIZE */\n"
                ));
            }
            HirType::Group { members, .. } => {
                out.push_str(&format!("{pad}/* INITIALIZE group {c_name} */\n"));
                for member in members {
                    if member.redefines.is_some() || member.renames.is_some() {
                        continue;
                    }
                    let member_c = sanitize_name(&member.name);
                    emit_initialize_field(out, &member.name, &member_c, data_items, pad);
                }
            }
            dt if needs_decimal(dt) => {
                // CobolDecimal → zero via runtime
                out.push_str(&format!(
                    "{pad}cobol_decimal_from_int(0, 0, &{c_name}); /* INITIALIZE */\n"
                ));
            }
            _ => {
                // Numeric, Binary, Index, etc. → zero
                out.push_str(&format!("{pad}{c_name} = 0; /* INITIALIZE */\n"));
            }
        }
    } else {
        // Unknown field, default to zero
        out.push_str(&format!("{pad}{c_name} = 0; /* INITIALIZE */\n"));
    }
}

/// Emit an INSPECT operand (pattern string) as a C pointer+length pair.
/// Returns (ptr_expr, len_expr) for use in runtime calls.
fn emit_inspect_operand(
    _out: &mut str,
    expr: &HirExpr,
    _label: &str,
    data_items: &[HirDataItem],
    _pad: &str,
) -> (String, String) {
    match expr {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            (format!("(const uint8_t*)\"{escaped}\""), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Space) => {
            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
        }
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let size = find_data_item_size(&c_name, data_items);
            let ptr = c_ptr_expr(&c_name, data_items);
            (format!("(const uint8_t*){ptr}"), format!("{size}"))
        }
        _ => ("NULL".to_string(), "0".to_string()),
    }
}

/// Emit INSPECT TALLYING phrases.
fn emit_inspect_tallying(
    out: &mut String,
    c_target: &str,
    target_size: u32,
    tallying: &[cobol_hir::HirInspectTallying],
    data_items: &[HirDataItem],
    pad: &str,
) {
    let tgt_ptr = c_ptr_expr(c_target, data_items);
    if tallying.is_empty() {
        // Fallback: count all characters
        out.push_str(&format!(
            "{pad}cobol_inspect_tallying((const uint8_t*){tgt_ptr}, {target_size}, NULL, 0, 0);\n"
        ));
        return;
    }
    for (i, t) in tallying.iter().enumerate() {
        let counter = emit_expr_as_numeric(&t.counter);
        let (mode, search_ptr, search_len) = match &t.kind {
            cobol_hir::HirTallyingKind::Characters => (0u32, "NULL".to_string(), "0".to_string()),
            cobol_hir::HirTallyingKind::All(expr) => {
                let label = format!("tally_s{i}");
                let (ptr, len) = emit_inspect_operand(out, expr, &label, data_items, pad);
                (1, ptr, len)
            }
            cobol_hir::HirTallyingKind::Leading(expr) => {
                let label = format!("tally_s{i}");
                let (ptr, len) = emit_inspect_operand(out, expr, &label, data_items, pad);
                (2, ptr, len)
            }
        };
        out.push_str(&format!(
            "{pad}{counter} += cobol_inspect_tallying((const uint8_t*){tgt_ptr}, {target_size}, {search_ptr}, {search_len}, {mode});\n"
        ));
    }
}

/// Emit INSPECT REPLACING phrases.
fn emit_inspect_replacing(
    out: &mut String,
    c_target: &str,
    target_size: u32,
    replacing: &[cobol_hir::HirInspectReplacing],
    data_items: &[HirDataItem],
    pad: &str,
) {
    let tgt_ptr = c_ptr_expr(c_target, data_items);
    if replacing.is_empty() {
        // Fallback: replace all characters with space
        out.push_str(&format!(
            "{pad}cobol_inspect_replacing((uint8_t*){tgt_ptr}, {target_size}, NULL, 0, (const uint8_t*)\" \", 1, 0);\n"
        ));
        return;
    }
    for (i, r) in replacing.iter().enumerate() {
        let (mode, search_ptr, search_len, replace_ptr, replace_len) = match &r.kind {
            cobol_hir::HirReplacingKind::Characters(to_expr) => {
                let label = format!("rep_to{i}");
                let (to_ptr, to_len) = emit_inspect_operand(out, to_expr, &label, data_items, pad);
                (0u32, "NULL".to_string(), "0".to_string(), to_ptr, to_len)
            }
            cobol_hir::HirReplacingKind::All { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (1, f_ptr, f_len, t_ptr, t_len)
            }
            cobol_hir::HirReplacingKind::Leading { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (2, f_ptr, f_len, t_ptr, t_len)
            }
            cobol_hir::HirReplacingKind::First { from, to } => {
                let from_label = format!("rep_from{i}");
                let to_label = format!("rep_to{i}");
                let (f_ptr, f_len) = emit_inspect_operand(out, from, &from_label, data_items, pad);
                let (t_ptr, t_len) = emit_inspect_operand(out, to, &to_label, data_items, pad);
                (3, f_ptr, f_len, t_ptr, t_len)
            }
        };
        out.push_str(&format!(
            "{pad}cobol_inspect_replacing((uint8_t*){tgt_ptr}, {target_size}, {search_ptr}, {search_len}, {replace_ptr}, {replace_len}, {mode});\n"
        ));
    }
}

/// Emit the value part of a STRING source operand.
fn emit_string_source_value(
    out: &mut String,
    value: &HirExpr,
    i: usize,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match value {
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = \"{escaped}\"; uint32_t _src_len_{i} = {len};\n"
            ));
        }
        HirExpr::Variable(name) => {
            let c_var = sanitize_name(name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = {ptr}; uint32_t _src_len_{i} = {var_size};\n"
            ));
        }
        _ => {
            let e = emit_expr(value);
            out.push_str(&format!("{pad}    int64_t _src_tmp_{i} = {e};\n"));
            out.push_str(&format!(
                "{pad}    const void* _src_ptr_{i} = &_src_tmp_{i}; uint32_t _src_len_{i} = sizeof(int64_t);\n"
            ));
        }
    }
}

/// Emit the delimiter part of a STRING source operand.
fn emit_string_source_delimiter(
    out: &mut String,
    delimiter: &Option<HirExpr>,
    i: usize,
    data_items: &[HirDataItem],
    pad: &str,
) {
    match delimiter {
        Some(HirExpr::Literal(HirLiteral::String(s))) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = (const uint8_t*)\"{escaped}\"; uint32_t _delim_len_{i} = {len};\n"
            ));
        }
        Some(HirExpr::Variable(name)) => {
            let c_var = sanitize_name(name);
            let var_size = find_data_item_size(&c_var, data_items);
            let ptr = c_ptr_expr(&c_var, data_items);
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = (const uint8_t*){ptr}; uint32_t _delim_len_{i} = {var_size};\n"
            ));
        }
        _ => {
            // DELIMITED BY SIZE (no delimiter)
            out.push_str(&format!(
                "{pad}    const uint8_t* _delim_ptr_{i} = NULL; uint32_t _delim_len_{i} = 0;\n"
            ));
        }
    }
}

/// Check whether an HIR expression refers to an alphanumeric field or string literal.
fn is_alphanumeric_expr(expr: &HirExpr, data_items: &[HirDataItem]) -> bool {
    match expr {
        HirExpr::Variable(name) => {
            if let Some(item) = find_data_item(name.as_str(), data_items) {
                matches!(
                    item.data_type,
                    HirType::Alphanumeric { .. } | HirType::Group { .. }
                )
            } else {
                false
            }
        }
        HirExpr::Literal(HirLiteral::String(_))
        | HirExpr::Literal(HirLiteral::Space)
        | HirExpr::Literal(HirLiteral::HighValue)
        | HirExpr::Literal(HirLiteral::LowValue)
        | HirExpr::Literal(HirLiteral::Quote) => true,
        HirExpr::ReferenceModification { variable, .. } => {
            if let Some(item) = find_data_item(variable.as_str(), data_items) {
                matches!(item.data_type, HirType::Alphanumeric { .. })
            } else {
                false
            }
        }
        HirExpr::FunctionCall { name, .. } => {
            let upper_fn = name.to_uppercase();
            matches!(
                upper_fn.as_str(),
                "CHAR" | "CURRENT-DATE" | "WHEN-COMPILED" | "UPPER-CASE" | "LOWER-CASE" | "REVERSE"
            )
        }
        _ => false,
    }
}

/// Produce `(ptr_expr, len_expr)` for an alphanumeric comparison operand.
fn emit_alphanumeric_operand(expr: &HirExpr, data_items: &[HirDataItem]) -> (String, String) {
    match expr {
        HirExpr::Variable(name) => {
            let c_name = sanitize_name(name);
            let size = find_data_item_size(&c_name, data_items);
            let ptr = c_ptr_expr(&c_name, data_items);
            (format!("(const uint8_t*){ptr}"), format!("{size}"))
        }
        HirExpr::Literal(HirLiteral::String(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            (format!("(const uint8_t*)\"{}\"", escaped), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Space) => {
            ("(const uint8_t*)\" \"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::HighValue) => {
            ("(const uint8_t*)\"\\xFF\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::LowValue) => {
            ("(const uint8_t*)\"\\x00\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Quote) => {
            ("(const uint8_t*)\"\\\"\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Integer(n)) => {
            // Numeric literal compared with alphanumeric: convert to string
            let s = n.to_string();
            let len = s.len();
            (format!("(const uint8_t*)\"{}\"", s), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::Zero) => {
            ("(const uint8_t*)\"0\"".to_string(), "1".to_string())
        }
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let len = d.len();
            (format!("(const uint8_t*)\"{}\"", d), format!("{len}"))
        }
        HirExpr::Literal(HirLiteral::AllChar(s)) => {
            let escaped = escape_c_string(s);
            let len = s.len();
            (format!("(const uint8_t*)\"{}\"", escaped), format!("{len}"))
        }
        HirExpr::Subscript { .. } => {
            let c_name = emit_expr(expr);
            let var_name = expr_var_name(expr);
            let size = if !var_name.is_empty() {
                let sz = find_data_item_size(&sanitize_name(var_name), data_items);
                format!("{sz}")
            } else {
                format!("sizeof({c_name})")
            };
            let ptr = format!("(const uint8_t*)&{c_name}");
            (ptr, size)
        }
        HirExpr::FunctionCall { name, args } => {
            let upper_fn = name.to_uppercase();
            match upper_fn.as_str() {
                "CHAR" => {
                    // Returns a 1-byte buffer pointer
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*){e}"), "1".to_string())
                }
                "CURRENT-DATE" | "WHEN-COMPILED" => {
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*){e}"), "21".to_string())
                }
                "UPPER-CASE" | "LOWER-CASE" | "REVERSE" => {
                    let size: u32 = if let Some(arg) = args.first() {
                        if let HirExpr::Variable(v) = arg {
                            find_data_item_size(&sanitize_name(v), data_items)
                        } else if let HirExpr::Literal(HirLiteral::String(s)) = arg {
                            s.len() as u32
                        } else {
                            64
                        }
                    } else {
                        64
                    };
                    let func = match upper_fn.as_str() {
                        "UPPER-CASE" => "cobol_func_upper_case",
                        "LOWER-CASE" => "cobol_func_lower_case",
                        _ => "cobol_func_reverse",
                    };
                    let c_arg = if let Some(arg) = args.first() {
                        emit_expr(arg)
                    } else {
                        "\"\"".to_string()
                    };
                    (
                        format!(
                            "({{ static uint8_t _fbuf[{size}]; \
                             memcpy(_fbuf, (const uint8_t*){c_arg}, {size}); \
                             {func}(_fbuf, {size}); \
                             (const uint8_t*)_fbuf; }})"
                        ),
                        format!("{size}"),
                    )
                }
                _ => {
                    let e = emit_expr(expr);
                    (format!("(const uint8_t*)&{e}"), format!("sizeof({e})"))
                }
            }
        }
        HirExpr::ReferenceModification {
            variable,
            start,
            length,
        } => {
            let c_src = sanitize_name(variable);
            let c_start = emit_expr(start);
            let src_full_size = find_data_item_size(&c_src, data_items);
            let c_len = if let Some(len) = length {
                emit_expr(len)
            } else {
                format!("({src_full_size} - ({c_start} - 1))")
            };
            (format!("(const uint8_t*){c_src} + ({c_start} - 1)"), c_len)
        }
        _ => {
            // Fallback for non-alphanumeric expressions used in mixed comparisons
            let e = emit_expr(expr);
            (format!("(const uint8_t*)&{e}"), format!("sizeof({e})"))
        }
    }
}

/// Generate code to initialize a CobolDecimal _tcmp from a non-decimal expression.
/// Handles decimal literals properly via cobol_decimal_from_string.
fn emit_decimal_init_expr(expr: &HirExpr, c_expr: &str) -> String {
    match expr {
        HirExpr::Literal(HirLiteral::Decimal(d)) => {
            let len = d.len();
            format!("cobol_decimal_from_string((const uint8_t*)\"{d}\", {len}, &_tcmp);")
        }
        _ => {
            format!("cobol_decimal_from_int({c_expr}, 0, &_tcmp);")
        }
    }
}

fn emit_condition(cond: &HirCondition, data_items: &[HirDataItem]) -> String {
    match cond {
        HirCondition::Compare { left, op, right } => {
            if is_alphanumeric_expr(left, data_items) || is_alphanumeric_expr(right, data_items) {
                // Alphanumeric comparison via runtime function
                let (a_ptr, a_len) = emit_alphanumeric_operand(left, data_items);
                let (b_ptr, b_len) = emit_alphanumeric_operand(right, data_items);
                let cmp = format!("cobol_compare_alphanumeric({a_ptr}, {a_len}, {b_ptr}, {b_len})");
                let op_str = match op {
                    HirCompareOp::Eq => "== 0",
                    HirCompareOp::Ne => "!= 0",
                    HirCompareOp::Gt => "> 0",
                    HirCompareOp::Lt => "< 0",
                    HirCompareOp::Ge => ">= 0",
                    HirCompareOp::Le => "<= 0",
                };
                format!("({cmp} {op_str})")
            } else if is_decimal_expr(left, data_items) || is_decimal_expr(right, data_items) {
                // CobolDecimal comparison via runtime function
                let left_is_dec = is_decimal_expr(left, data_items);
                let right_is_dec = is_decimal_expr(right, data_items);
                // For decimal sides, use emit_expr to get the struct;
                // for non-decimal sides, use emit_int_compatible_expr to
                // ensure CobolDecimal sub-expressions are converted to int64.
                let l = if left_is_dec {
                    emit_expr(left)
                } else {
                    emit_int_compatible_expr(left, data_items)
                };
                let r = if right_is_dec {
                    emit_expr(right)
                } else {
                    emit_int_compatible_expr(right, data_items)
                };
                let op_str = match op {
                    HirCompareOp::Eq => "== 0",
                    HirCompareOp::Ne => "!= 0",
                    HirCompareOp::Gt => "> 0",
                    HirCompareOp::Lt => "< 0",
                    HirCompareOp::Ge => ">= 0",
                    HirCompareOp::Le => "<= 0",
                };
                if left_is_dec && right_is_dec {
                    format!("(cobol_decimal_cmp(&{l}, &{r}) {op_str})")
                } else if left_is_dec {
                    // left is decimal, right is not: convert right to temp
                    let init = emit_decimal_init_expr(right, &r);
                    format!(
                        "(({{ CobolDecimal _tcmp; {init} \
                         cobol_decimal_cmp(&{l}, &_tcmp); }}) {op_str})"
                    )
                } else {
                    // right is decimal, left is not: convert left to temp
                    let init = emit_decimal_init_expr(left, &l);
                    format!(
                        "(({{ CobolDecimal _tcmp; {init} \
                         cobol_decimal_cmp(&_tcmp, &{r}); }}) {op_str})"
                    )
                }
            } else {
                let l = emit_int_compatible_expr(left, data_items);
                let r = emit_int_compatible_expr(right, data_items);
                let op_str = match op {
                    HirCompareOp::Eq => "==",
                    HirCompareOp::Ne => "!=",
                    HirCompareOp::Gt => ">",
                    HirCompareOp::Lt => "<",
                    HirCompareOp::Ge => ">=",
                    HirCompareOp::Le => "<=",
                };
                format!("{l} {op_str} {r}")
            }
        }
        HirCondition::ClassCondition { operand, class } => {
            let (ptr, len) = emit_alphanumeric_operand(operand, data_items);
            let func = match class {
                HirClassType::Numeric => "cobol_is_numeric",
                HirClassType::Alphabetic => "cobol_is_alphabetic",
                HirClassType::AlphabeticLower => "cobol_is_alphabetic_lower",
                HirClassType::AlphabeticUpper => "cobol_is_alphabetic_upper",
            };
            format!("({func}({ptr}, {len}))")
        }
        HirCondition::And(a, b) => {
            let a = emit_condition(a, data_items);
            let b = emit_condition(b, data_items);
            format!("({a} && {b})")
        }
        HirCondition::Or(a, b) => {
            let a = emit_condition(a, data_items);
            let b = emit_condition(b, data_items);
            format!("({a} || {b})")
        }
        HirCondition::Not(inner) => {
            let c = emit_condition(inner, data_items);
            format!("(!({c}))")
        }
    }
}

/// Build a map from sanitized file name to sanitized FILE STATUS variable name.
fn build_file_status_map(file_status_vars: &[HirFileInfo]) -> FileStatusMap {
    file_status_vars
        .iter()
        .map(|info| {
            (
                sanitize_name(&info.file_name),
                sanitize_name(&info.status_var),
            )
        })
        .collect()
}

/// Emit a FILE STATUS variable update after a file I/O operation.
/// `fs_val` is the C expression (typically `_fs`) holding the uint32_t status.
fn emit_file_status_update(
    out: &mut String,
    file_c_name: &str,
    fs_val: &str,
    fs_map: &FileStatusMap,
    has_declaratives: bool,
    pad: &str,
) {
    if let Some(status_var) = fs_map.get(file_c_name) {
        // Convert numeric status code to 2-digit string: e.g. 0 → "00", 10 → "10"
        // Use an intermediate buffer + memcpy so this works even when the status
        // variable is a union (group item with REDEFINES) rather than a plain char[].
        out.push_str(&format!(
            "{pad}{{ char _fs_buf[4]; snprintf(_fs_buf, sizeof(_fs_buf), \"%02u\", (unsigned){fs_val}); memcpy(&{status_var}, _fs_buf, 2); }}\n"
        ));
    }
    if has_declaratives {
        out.push_str(&format!(
            "{pad}_check_file_declarative(\"{file_c_name}\", {fs_val});\n"
        ));
    }
}

/// Collect all CALL target program names across the program body and
/// paragraphs. Returns sanitized, unique C identifiers for weak forward
/// declarations.
fn collect_call_targets(program: &HirProgram) -> Vec<String> {
    let mut targets = BTreeSet::new();
    for stmt in &program.body {
        collect_call_targets_stmt(stmt, &mut targets);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_call_targets_stmt(stmt, &mut targets);
        }
    }
    for decl in &program.declaratives {
        for stmt in &decl.body {
            collect_call_targets_stmt(stmt, &mut targets);
        }
    }
    // Exclude nested program names (they are defined in this compilation unit)
    let nested_names: BTreeSet<String> = program
        .nested_programs
        .iter()
        .map(|p| sanitize_name(&p.name))
        .collect();
    targets
        .into_iter()
        .filter(|t| !nested_names.contains(t))
        .collect()
}

fn collect_call_targets_stmt(stmt: &HirStatement, targets: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::Call {
            program,
            on_exception,
            not_on_exception,
            ..
        } => {
            let prog_name = match program {
                HirExpr::Literal(HirLiteral::String(s)) => Some(sanitize_name(s)),
                _ => None,
            };
            if let Some(name) = prog_name {
                targets.insert(name);
            }
            for s in on_exception {
                collect_call_targets_stmt(s, targets);
            }
            for s in not_on_exception {
                collect_call_targets_stmt(s, targets);
            }
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_call_targets_stmt(s, targets);
            }
            for s in else_body {
                collect_call_targets_stmt(s, targets);
            }
        }
        HirStatement::Perform {
            kind:
                HirPerformKind::Inline { body }
                | HirPerformKind::Times { body, .. }
                | HirPerformKind::Until { body, .. }
                | HirPerformKind::Varying { body, .. },
            ..
        } => {
            for s in body {
                collect_call_targets_stmt(s, targets);
            }
        }
        _ => {}
    }
}

/// Collect all file names referenced in file I/O statements across
/// the program body,  and nested constructs. Returns a
/// sorted, deduplicated list of file names.
fn collect_file_names(program: &HirProgram) -> Vec<String> {
    let mut names = BTreeSet::new();
    for stmt in &program.body {
        collect_file_names_stmt(stmt, &mut names);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_file_names_stmt(stmt, &mut names);
        }
    }
    names.into_iter().collect()
}

fn collect_file_names_stmt(stmt: &HirStatement, names: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::Open { entries, .. } => {
            for entry in entries {
                names.insert(entry.file_name.to_string());
            }
        }
        HirStatement::Close { files, .. } => {
            for f in files {
                names.insert(f.to_string());
            }
        }
        HirStatement::Read {
            file_name,
            at_end,
            not_at_end,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for s in not_at_end {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Write {
            file_name,
            record_name,
            ..
        } => {
            if file_name.is_empty() {
                names.insert(record_name.to_string());
            } else {
                names.insert(file_name.to_string());
            }
        }
        HirStatement::Rewrite {
            file_name,
            record_name,
            ..
        } => {
            if file_name.is_empty() {
                names.insert(record_name.to_string());
            } else {
                names.insert(file_name.to_string());
            }
        }
        HirStatement::Delete { file_name, .. } => {
            names.insert(file_name.to_string());
        }
        HirStatement::Sort {
            file_name,
            using,
            giving,
            ..
        } => {
            names.insert(file_name.to_string());
            for u in using {
                names.insert(u.to_string());
            }
            for g in giving {
                names.insert(g.to_string());
            }
        }
        HirStatement::Search {
            at_end,
            when_clauses,
            ..
        } => {
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for w in when_clauses {
                for s in &w.body {
                    collect_file_names_stmt(s, names);
                }
            }
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_file_names_stmt(s, names);
            }
            for s in else_body {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Perform { kind, .. } => {
            let body = match kind {
                HirPerformKind::Inline { body } => body.as_slice(),
                HirPerformKind::Times { body, .. } => body.as_slice(),
                HirPerformKind::Until { body, .. } => body.as_slice(),
                HirPerformKind::Varying { body, .. } => body.as_slice(),
                HirPerformKind::ProcedureName { .. } => &[],
            };
            for s in body {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Start {
            file_name,
            invalid_key,
            not_invalid_key,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in invalid_key {
                collect_file_names_stmt(s, names);
            }
            for s in not_invalid_key {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Return {
            file_name,
            at_end,
            not_at_end,
            ..
        } => {
            names.insert(file_name.to_string());
            for s in at_end {
                collect_file_names_stmt(s, names);
            }
            for s in not_at_end {
                collect_file_names_stmt(s, names);
            }
        }
        HirStatement::Merge {
            file_name,
            using,
            giving,
            ..
        } => {
            names.insert(file_name.to_string());
            for f in using {
                names.insert(f.to_string());
            }
            for f in giving {
                names.insert(f.to_string());
            }
        }
        HirStatement::Release { record_name, .. } => {
            names.insert(record_name.to_string());
        }
        _ => {}
    }
}

/// Collect unique XML PARSE processing procedure names from the program.
fn collect_xml_parse_procedures(program: &HirProgram) -> Vec<String> {
    let mut procs = BTreeSet::new();
    for stmt in &program.body {
        collect_xml_parse_stmt(stmt, &mut procs);
    }
    for para in &program.paragraphs {
        for stmt in &para.body {
            collect_xml_parse_stmt(stmt, &mut procs);
        }
    }
    procs.into_iter().collect()
}

fn collect_xml_parse_stmt(stmt: &HirStatement, procs: &mut BTreeSet<String>) {
    match stmt {
        HirStatement::XmlParse {
            processing_procedure,
            ..
        } => {
            procs.insert(sanitize_name(processing_procedure));
        }
        HirStatement::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_xml_parse_stmt(s, procs);
            }
            for s in else_body {
                collect_xml_parse_stmt(s, procs);
            }
        }
        HirStatement::Perform { kind, .. } => {
            let body = match kind {
                HirPerformKind::Inline { body } => body.as_slice(),
                HirPerformKind::Times { body, .. } => body.as_slice(),
                HirPerformKind::Until { body, .. } => body.as_slice(),
                HirPerformKind::Varying { body, .. } => body.as_slice(),
                HirPerformKind::ProcedureName { .. } => &[],
            };
            for s in body {
                collect_xml_parse_stmt(s, procs);
            }
        }
        _ => {}
    }
}

/// Find the record length for a file/record name by looking up
/// the data item with a matching (sanitized) name. Returns the
/// size in bytes (default 80 if not found).
fn find_record_len(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    find_data_item_size(c_name, data_items)
}

/// Find the OCCURS count for a table by its sanitized C name.
/// Returns a reasonable default (10) if the item is not found.
fn find_occurs_count(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            return item.occurs.unwrap_or(10);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_occurs_count_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    10
}

fn find_occurs_count_in(c_name: &str, members: &[HirDataItem]) -> u32 {
    for item in members {
        if sanitize_name(&item.name) == c_name {
            return item.occurs.unwrap_or(0);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_occurs_count_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    0
}

/// Find the first INDEXED BY name for a given table (OCCURS item) name.
fn find_first_index_name(c_name: &str, data_items: &[HirDataItem]) -> Option<String> {
    for item in data_items {
        if sanitize_name(&item.name) == c_name {
            if let Some(first) = item.indexed_by.first() {
                return Some(sanitize_name(first));
            }
        }
        if let HirType::Group { members, .. } = &item.data_type {
            if let Some(found) = find_first_index_name(c_name, members) {
                return Some(found);
            }
        }
    }
    None
}

/// Find the byte size of a data item by its sanitized C name.
/// Returns a reasonable default (80) if the item is not found.
fn find_data_item_size(c_name: &str, data_items: &[HirDataItem]) -> u32 {
    // Handle subscript/struct-access expressions like
    // "TABLE.members._m_FOO[(I)-1].members._m_BAR" by extracting the
    // final member name ("_m_BAR") and looking it up without the prefix.
    if c_name.contains('[') || c_name.contains(".members.") {
        if let Some(pos) = c_name.rfind(".members._m_") {
            let leaf = &c_name[pos + ".members._m_".len()..];
            // Remove any trailing subscript like "[(I)-1]"
            let leaf_name = if let Some(br) = leaf.find('[') {
                &leaf[..br]
            } else {
                leaf
            };
            if !leaf_name.is_empty() {
                let found = find_data_item_size_in(leaf_name, data_items);
                if found > 0 {
                    return found;
                }
            }
        }
    }
    // Handle qualified C names like "WS_DST__FIELD_B"
    if let Some(pos) = c_name.find("__") {
        let group_c = &c_name[..pos];
        let member_c = &c_name[pos + 2..];
        for item in data_items {
            if sanitize_name(&item.name) == group_c {
                if let HirType::Group { members, .. } = &item.data_type {
                    let found = find_data_item_size_in(member_c, members);
                    if found > 0 {
                        return found;
                    }
                }
            }
        }
    }
    for item in data_items {
        let item_c_name = sanitize_name(&item.name);
        if item_c_name == c_name {
            return data_item_byte_size(&item.data_type);
        }
        // Also search in group members
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_data_item_size_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    80 // Default record length
}

fn find_data_item_size_in(c_name: &str, items: &[HirDataItem]) -> u32 {
    for item in items {
        let item_c_name = sanitize_name(&item.name);
        if item_c_name == c_name {
            return data_item_byte_size(&item.data_type);
        }
        if let HirType::Group { members, .. } = &item.data_type {
            let found = find_data_item_size_in(c_name, members);
            if found > 0 {
                return found;
            }
        }
    }
    0
}

/// Compute the byte size of an HIR type.
fn data_item_byte_size(data_type: &HirType) -> u32 {
    match data_type {
        HirType::Alphanumeric { size } => *size,
        HirType::Numeric { size, .. } => *size,
        HirType::Group { size, .. } => *size,
        HirType::Comp3 { size, .. } => *size,
        HirType::Binary { size } => *size,
        HirType::Index => 8,
        HirType::Pointer => 8,
        HirType::Boolean => 1,
        HirType::FloatShort => 4,
        HirType::FloatLong => 8,
        HirType::FloatExtended => 16,
        HirType::National { size } => size * 2, // UTF-16: 2 bytes per character
    }
}

/// Convert a COBOL data name to a valid C identifier.
///
/// COBOL names use hyphens which are not valid in C, so we replace
/// them with underscores. Additionally, names starting with a digit
/// are prefixed with `cob_`, and C reserved words are prefixed to
/// avoid collisions.
fn sanitize_name(name: &str) -> String {
    let mut result = name.replace("::", "__").replace('-', "_");
    // C identifiers cannot start with a digit
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert_str(0, "cob_");
    }
    // Avoid C reserved words
    match result.as_str() {
        "auto" | "break" | "case" | "char" | "const" | "continue" | "default" | "do" | "double"
        | "else" | "enum" | "extern" | "float" | "for" | "goto" | "if" | "int" | "long"
        | "register" | "return" | "short" | "signed" | "sizeof" | "static" | "struct"
        | "switch" | "typedef" | "union" | "unsigned" | "void" | "volatile" | "while"
        | "inline" | "restrict" | "_Bool" | "_Complex" | "_Imaginary" | "main" => {
            result.insert_str(0, "cob_");
        }
        _ => {}
    }
    result
}

/// Resolve the FD/SD record buffer variable name for a file.
/// Returns the first record name from the FILE_RECORD_MAP if available,
/// otherwise falls back to the file name itself.
fn resolve_file_record(sanitized_file_name: &str) -> String {
    FILE_RECORD_MAP.with(|cell| {
        let map = cell.borrow();
        map.get(sanitized_file_name)
            .cloned()
            .unwrap_or_else(|| sanitized_file_name.to_string())
    })
}

/// Escape special characters for use in a C string literal.
fn escape_c_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            c => escaped.push(c),
        }
    }
    escaped
}

// ---------------------------------------------------------------------------
// COBOL 2002+: Class and function code generation
// ---------------------------------------------------------------------------

fn emit_classes(out: &mut String, classes: &[cobol_hir::HirClass]) {
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
                emit_statement(out, stmt, &[], &[], &HashMap::new(), false, 1);
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
                emit_statement(out, stmt, &[], &[], &HashMap::new(), false, 1);
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
            emit_statement(out, stmt, &[], &[], &HashMap::new(), false, 1);
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
fn hir_type_to_c(data_type: &HirType) -> &'static str {
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

/// Compile a generated C source file into a native executable.
///
/// Returns `Ok(())` on success, or an error message on failure.
pub fn compile_c_to_executable(
    c_source_path: &std::path::Path,
    output_path: &std::path::Path,
    runtime_lib_path: &std::path::Path,
) -> Result<(), String> {
    // Try clang first, then cc
    let compiler = find_c_compiler()?;

    let status = std::process::Command::new(&compiler)
        .arg(c_source_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", runtime_lib_path.display()))
        .arg("-lcobol_runtime")
        .arg("-lpthread")
        .arg("-ldl")
        .arg("-lm")
        .status()
        .map_err(|e| format!("Failed to run C compiler '{}': {}", compiler, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "C compiler '{}' exited with status: {}",
            compiler, status
        ))
    }
}

fn find_c_compiler() -> Result<String, String> {
    // Check CC environment variable
    if let Ok(cc) = std::env::var("CC") {
        return Ok(cc);
    }

    // Try clang, then gcc, then cc
    for compiler in &["clang", "gcc", "cc"] {
        if std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok(compiler.to_string());
        }
    }

    Err("No C compiler found. Install clang or gcc.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::{FileId, SourceFormat};
    use cobol_hir::lower_to_hir;
    use cobol_lexer::Lexer;
    use cobol_parser::Parser;

    fn parse_lower_generate(source: &str) -> String {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        let program = parser.parse_program().unwrap();
        let hir = lower_to_hir(&program);
        generate_c(&hir)
    }

    #[test]
    fn test_generate_hello_world() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("cobol_display_string"));
        assert!(c_code.contains("Hello, World!"));
        assert!(c_code.contains("cobol_display_newline"));
        assert!(c_code.contains("cobol_stop_run"));
        assert!(c_code.contains("int main"));
    }

    #[test]
    fn test_generate_with_data_items() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DATA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
01  WS-COUNT PIC 9(5).
PROCEDURE DIVISION.
    DISPLAY WS-COUNT.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("static char WS_NAME"));
        assert!(c_code.contains("static int64_t WS_COUNT"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("WS-NAME"), "WS_NAME");
        assert_eq!(sanitize_name("HELLO-WORLD"), "HELLO_WORLD");
        assert_eq!(sanitize_name("SIMPLE"), "SIMPLE");
        // C reserved words are prefixed with cob_
        assert_eq!(sanitize_name("int"), "cob_int");
        assert_eq!(sanitize_name("main"), "cob_main");
        assert_eq!(sanitize_name("return"), "cob_return");
        // Names starting with a digit are prefixed with cob_
        assert_eq!(sanitize_name("1ST-FIELD"), "cob_1ST_FIELD");
    }

    #[test]
    fn test_escape_c_string() {
        assert_eq!(escape_c_string("hello"), "hello");
        assert_eq!(escape_c_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_c_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_generate_if_statement() {
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
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("if ("));
        assert!(c_code.contains("} else {"));
    }

    // -----------------------------------------------------------------------
    // COBOL 2002+ codegen tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_raise() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RAISE.
PROCEDURE DIVISION.
    RAISE EXCEPTION \"EC-SIZE-OVERFLOW\".
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_raise"),
            "Generated C should contain cobol_raise call"
        );
    }

    #[test]
    fn test_generate_resume() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RESUME.
PROCEDURE DIVISION.
    RESUME.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_resume"),
            "Generated C should contain cobol_resume call"
        );
    }

    #[test]
    fn test_generate_invoke() {
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
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_invoke"),
            "Generated C should contain cobol_invoke call"
        );
        assert!(
            c_code.contains("DO-SOMETHING"),
            "Generated C should reference the method name"
        );
    }

    #[test]
    fn test_generate_allocate_and_free() {
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
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("malloc"),
            "Generated C should contain malloc for ALLOCATE"
        );
        assert!(
            c_code.contains("free("),
            "Generated C should contain free for FREE"
        );
    }

    #[test]
    fn test_generate_setjmp_header() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-HDR.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("#include <setjmp.h>"),
            "Generated C should include setjmp.h"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2002() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_raise"),
            "Runtime declarations should include cobol_raise"
        );
        assert!(
            c_code.contains("cobol_resume"),
            "Runtime declarations should include cobol_resume"
        );
        assert!(
            c_code.contains("cobol_invoke"),
            "Runtime declarations should include cobol_invoke"
        );
    }

    #[test]
    fn test_generate_class_struct() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-CLASS".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: vec![cobol_hir::HirClass {
                name: "MY-CLASS".into(),
                parent: None,
                factory_methods: Vec::new(),
                instance_methods: vec![cobol_hir::HirMethod {
                    name: "DO-WORK".into(),
                    params: Vec::new(),
                    returning: None,
                    data_items: Vec::new(),
                    body: Vec::new(),
                    span: Span::dummy(),
                }],
                factory_data: Vec::new(),
                instance_data: vec![HirDataItem {
                    name: "MY-FIELD".into(),
                    data_type: HirType::Numeric {
                        size: 5,
                        decimal_places: 0,
                        is_signed: false,
                    },
                    initial_value: None,
                    occurs: None,
                    indexed_by: Vec::new(),
                    redefines: None,
                    renames: None,
                    screen_info: None,
                    span: Span::dummy(),
                }],
                span: Span::dummy(),
            }],
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("typedef struct MY_CLASS_s"),
            "Should generate struct for class"
        );
        assert!(c_code.contains("_vtable"), "Should generate vtable");
        assert!(
            c_code.contains("MY_CLASS_new"),
            "Should generate constructor"
        );
        assert!(
            c_code.contains("MY_CLASS_DO_WORK"),
            "Should generate method implementation"
        );
    }

    #[test]
    fn test_generate_function() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-FUNC".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: vec![cobol_hir::HirFunction {
                name: "ADD-NUMBERS".into(),
                params: vec![
                    cobol_hir::HirParam {
                        name: "A".into(),
                        mode: cobol_hir::HirParamMode::ByValue,
                        data_type: HirType::Numeric {
                            size: 5,
                            decimal_places: 0,
                            is_signed: false,
                        },
                    },
                    cobol_hir::HirParam {
                        name: "B".into(),
                        mode: cobol_hir::HirParamMode::ByValue,
                        data_type: HirType::Numeric {
                            size: 5,
                            decimal_places: 0,
                            is_signed: false,
                        },
                    },
                ],
                returning: HirType::Numeric {
                    size: 5,
                    decimal_places: 0,
                    is_signed: false,
                },
                data_items: Vec::new(),
                body: Vec::new(),
                span: Span::dummy(),
            }],
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_func_add_numbers"),
            "Should generate function with cobol_func_ prefix (lowercase)"
        );
        assert!(c_code.contains("int64_t A"), "Should generate parameter A");
        assert!(c_code.contains("int64_t B"), "Should generate parameter B");
    }

    #[test]
    fn test_hir_type_to_c_boolean() {
        assert_eq!(hir_type_to_c(&HirType::Boolean), "int8_t");
    }

    #[test]
    fn test_generate_local_storage() {
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
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("LS_COUNTER"),
            "Should emit LOCAL-STORAGE variable"
        );
        assert!(
            c_code.contains("WS_COUNTER"),
            "Should emit WORKING-STORAGE variable"
        );
    }

    // -----------------------------------------------------------------------
    // COBOL 2014+ codegen tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_float_short() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static float WS_FLOAT"),
            "FLOAT-SHORT should generate C float type"
        );
    }

    #[test]
    fn test_generate_float_long() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-L.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-L USAGE FLOAT-LONG.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static double WS_FLOAT_L"),
            "FLOAT-LONG should generate C double type"
        );
    }

    #[test]
    fn test_generate_float_extended() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-E USAGE FLOAT-EXTENDED.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static long double WS_FLOAT_E"),
            "FLOAT-EXTENDED should generate C long double type"
        );
    }

    #[test]
    fn test_generate_float_init() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-INIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-F USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("WS_F = 0.0"),
            "Float data items should be initialized to 0.0"
        );
    }

    #[test]
    fn test_hir_type_to_c_float_short() {
        assert_eq!(hir_type_to_c(&HirType::FloatShort), "float");
    }

    #[test]
    fn test_hir_type_to_c_float_long() {
        assert_eq!(hir_type_to_c(&HirType::FloatLong), "double");
    }

    #[test]
    fn test_hir_type_to_c_float_extended() {
        assert_eq!(hir_type_to_c(&HirType::FloatExtended), "long double");
    }

    #[test]
    fn test_generate_validate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-VALIDATE".into(),
            data_items: vec![HirDataItem {
                name: "WS-NAME".into(),
                data_type: HirType::Alphanumeric { size: 20 },
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                span: Span::dummy(),
            }],
            paragraphs: Vec::new(),
            body: vec![HirStatement::Validate {
                target: "WS-NAME".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_validate"),
            "VALIDATE should generate cobol_validate call"
        );
    }

    #[test]
    fn test_generate_json_generate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-JSON-GEN".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::JsonGenerate {
                source: "WS-DATA".into(),
                target: "WS-JSON".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_json_generate"),
            "JSON GENERATE should emit cobol_json_generate call"
        );
    }

    #[test]
    fn test_generate_json_parse_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-JSON-PARSE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::JsonParse {
                source: "WS-JSON".into(),
                target: "WS-DATA".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_json_parse"),
            "JSON PARSE should emit cobol_json_parse call"
        );
    }

    #[test]
    fn test_generate_xml_generate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-XML-GEN".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::XmlGenerate {
                source: "WS-DATA".into(),
                target: "WS-XML".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_xml_generate"),
            "XML GENERATE should emit cobol_xml_generate call"
        );
    }

    #[test]
    fn test_generate_xml_parse_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-XML-PARSE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::XmlParse {
                source: "WS-XML".into(),
                processing_procedure: "XML-HANDLER".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("XML PARSE"),
            "XML PARSE should emit XML PARSE comment"
        );
        assert!(
            c_code.contains("XML_HANDLER"),
            "XML PARSE should reference processing procedure"
        );
    }

    #[test]
    fn test_generate_typedef() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-TYPEDEF".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: vec![cobol_hir::HirTypedef {
                name: "MONEY-TYPE".into(),
                base_type: HirType::Numeric {
                    size: 9,
                    decimal_places: 2,
                    is_signed: true,
                },
                span: Span::dummy(),
            }],
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("typedef int64_t MONEY_TYPE"),
            "TYPEDEF should generate C typedef"
        );
    }

    #[test]
    fn test_generate_interface() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-IFACE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: vec![cobol_hir::HirInterface {
                name: "IComparable".into(),
                methods: vec![cobol_hir::HirMethod {
                    name: "CompareTo".into(),
                    params: Vec::new(),
                    returning: None,
                    data_items: Vec::new(),
                    body: Vec::new(),
                    span: Span::dummy(),
                }],
                span: Span::dummy(),
            }],
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("INTERFACE IComparable"),
            "Should generate interface comment"
        );
        assert!(
            c_code.contains("IComparable_vtable"),
            "Should generate vtable for interface"
        );
        assert!(
            c_code.contains("CompareTo"),
            "Should include method in vtable"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2014() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL-2014.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_validate"),
            "Should declare cobol_validate"
        );
        assert!(
            c_code.contains("cobol_json_generate"),
            "Should declare cobol_json_generate"
        );
        assert!(
            c_code.contains("cobol_json_parse"),
            "Should declare cobol_json_parse"
        );
        assert!(
            c_code.contains("cobol_xml_generate"),
            "Should declare cobol_xml_generate"
        );
        assert!(
            c_code.contains("cobol_xml_parse"),
            "Should declare cobol_xml_parse"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2023() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL-2023.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_utf8_char_count"),
            "Should declare cobol_utf8_char_count"
        );
        assert!(
            c_code.contains("cobol_utf8_substring"),
            "Should declare cobol_utf8_substring"
        );
        assert!(
            c_code.contains("cobol_thread_create"),
            "Should declare cobol_thread_create"
        );
        assert!(
            c_code.contains("cobol_mutex_create"),
            "Should declare cobol_mutex_create"
        );
    }
}
