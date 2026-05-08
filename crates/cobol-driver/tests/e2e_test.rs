// End-to-end integration tests for the COBOL compiler pipeline.
//
// Tests the full flow: Source -> Preprocess -> Lex -> Parse -> Sema -> HIR -> Codegen

use cobol_ast::CobolProgram;
use cobol_codegen::generate_c;
use cobol_common::{FileId, SourceFormat, Span};
use cobol_driver::toolchain::compile_c_to_executable;
use cobol_hir::{lower_analyzed_to_hir, lower_to_hir};
use cobol_hir::{HirDeclarative, HirDeclarativeUse, HirStatement, HirType};
use cobol_lexer::Lexer;
use cobol_parser::Parser;
use cobol_sema::SemanticAnalyzer;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Helper: run the full pipeline up to HIR and return the HIR program.
fn merge_compilation_unit_programs(mut programs: Vec<CobolProgram>) -> CobolProgram {
    let mut root = programs
        .drain(..1)
        .next()
        .expect("parsing should return at least one program");
    root.nested_programs.extend(programs);
    root
}

fn compile_to_hir(source: &str) -> cobol_hir::HirProgram {
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = merge_compilation_unit_programs(
        parser
            .parse_compilation_unit()
            .expect("parsing should succeed"),
    );

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&program);
    assert!(
        !result.has_errors,
        "semantic analysis should not produce errors"
    );

    lower_analyzed_to_hir(&program, &result).expect("HIR lowering should succeed")
}

#[test]
fn analyzed_hir_lowering_rejects_semantic_errors() {
    let source = "IDENTIFICATION DIVISION.
PROGRAM-ID. BAD.
PROCEDURE DIVISION.
    DISPLAY UNKNOWN-NAME.
    STOP RUN.
";
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = parser.parse_program().expect("parsing should succeed");
    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&program);

    assert!(result.has_errors, "source should fail semantic analysis");
    assert!(lower_analyzed_to_hir(&program, &result).is_err());
}

/// Helper: parse and lower without semantic analysis.
/// Useful for testing constructs that may not pass full semantic analysis yet.
fn parse_and_lower(source: &str) -> cobol_hir::HirProgram {
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = merge_compilation_unit_programs(
        parser
            .parse_compilation_unit()
            .expect("parsing should succeed"),
    );
    lower_to_hir(&program)
}

fn parse_and_lower_fixed(source: &str) -> cobol_hir::HirProgram {
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = merge_compilation_unit_programs(
        parser
            .parse_compilation_unit()
            .expect("parsing should succeed"),
    );
    lower_to_hir(&program)
}

/// Helper: run the full pipeline up to C code generation.
fn compile_to_c(source: &str) -> String {
    let hir = compile_to_hir(source);
    generate_c(&hir)
}

fn compact_c_code(c_code: &str) -> String {
    c_code.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn empty_hir_program(name: &str) -> cobol_hir::HirProgram {
    cobol_hir::HirProgram {
        name: name.into(),
        data_items: Vec::new(),
        communication_descriptions: Vec::new(),
        paragraphs: Vec::new(),
        body: Vec::new(),
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        using_params: Vec::new(),
        file_organizations: std::collections::HashMap::new(),
        file_assignments: std::collections::HashMap::new(),
        file_optionals: std::collections::HashSet::new(),
        file_relative_keys: std::collections::HashMap::new(),
        file_access_modes: std::collections::HashMap::new(),
        file_status_vars: Vec::new(),
        declaratives: Vec::new(),
        file_records: std::collections::HashMap::new(),
        fd_record_aliases: std::collections::HashMap::new(),
        variable_record_files: std::collections::HashSet::new(),
        variable_record_depending: std::collections::HashMap::new(),
        variable_record_bounds: std::collections::HashMap::new(),
        same_record_areas: Vec::new(),
        decimal_point_is_comma: false,
        special_class_conditions: std::collections::HashMap::new(),
        program_collating_sequence: None,
        nested_programs: Vec::new(),
        span: Span::dummy(),
    }
}

fn assert_display_numeric_update(c_code: &str, c_name: &str, op: &str) {
    assert!(c_code.contains("cobol_store_numeric_display"));
    assert!(c_code.contains("cobol_display_to_int64"));
    assert!(c_code.contains(c_name));
    assert!(c_code.contains(op));
}

/// Helper: compile COBOL source to a native binary and run it.
/// Returns (stdout, stderr, exit_code).
fn compile_and_run(source: &str) -> (String, String, i32) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

    // Run the full pipeline: lex -> parse -> sema -> hir -> codegen
    let hir = compile_to_hir(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    // Find the runtime library path
    let runtime_lib_path = find_test_runtime_lib();

    compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
        .expect("C compilation should succeed");

    let output = Command::new(&exe_path)
        .output()
        .expect("execute compiled binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

fn compile_c_and_run(c_code: &str) -> (String, String, i32) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

    std::fs::write(&c_path, c_code).expect("write C file");

    let runtime_lib_path = find_test_runtime_lib();
    compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
        .expect("C compilation should succeed");

    let output = Command::new(&exe_path)
        .output()
        .expect("execute compiled binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

/// Helper: compile COBOL source without sema (for programs that may
/// use features sema doesn't fully support yet).
fn compile_and_run_no_sema(source: &str) -> (String, String, i32) {
    compile_and_run_no_sema_with_env(source, &[])
}

fn compile_and_run_no_sema_with_stdin(source: &str, stdin: &str) -> (String, String, i32) {
    use std::io::Write;

    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

    let hir = parse_and_lower(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    let runtime_lib_path = find_test_runtime_lib();

    compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
        .expect("C compilation should succeed");

    let mut child = Command::new(&exe_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("execute compiled binary");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait for compiled binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

fn compile_and_run_no_sema_with_env(source: &str, envs: &[(&str, &str)]) -> (String, String, i32) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

    let hir = parse_and_lower(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    let runtime_lib_path = find_test_runtime_lib();

    compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
        .expect("C compilation should succeed");

    let mut command = Command::new(&exe_path);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("execute compiled binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

fn find_test_runtime_lib() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(profile_dir) = exe.parent().and_then(|deps| deps.parent()) {
            if has_runtime_archive(profile_dir) {
                return profile_dir.to_path_buf();
            }
        }
    }
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        let debug_dir = PathBuf::from(target_dir).join("debug");
        if has_runtime_archive(&debug_dir) {
            return debug_dir;
        }
    }
    // Check common locations relative to the test working directory
    let candidates = [
        PathBuf::from("target/debug"),
        PathBuf::from("../../target/debug"),
        PathBuf::from("../../../target/debug"),
    ];
    for dir in &candidates {
        if has_runtime_archive(dir) {
            return dir.clone();
        }
    }
    // Fallback: try CARGO_MANIFEST_DIR
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace_root = PathBuf::from(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("target/debug"))
            .unwrap_or_default();
        if has_runtime_archive(&workspace_root) {
            return workspace_root;
        }
    }
    PathBuf::from("target/debug")
}

fn has_runtime_archive(dir: &std::path::Path) -> bool {
    if dir.join("libcobol_runtime.a").exists() {
        return true;
    }
    let deps_dir = dir.join("deps");
    let Ok(entries) = std::fs::read_dir(deps_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.extension().is_some_and(|ext| ext == "a")
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("libcobol_runtime"))
    })
}

fn temp_exe_path(dir: &std::path::Path, stem: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{stem}.exe"))
    } else {
        dir.join(stem)
    }
}

#[path = "e2e/core_pipeline.rs"]
mod core_pipeline;

#[path = "e2e/native_runtime.rs"]
mod native_runtime;

#[path = "e2e/compatibility_and_declaratives.rs"]
mod compatibility_and_declaratives;

#[path = "e2e/language_operations.rs"]
mod language_operations;

#[path = "e2e/validate_data_exchange.rs"]
mod validate_data_exchange;

#[path = "e2e/files_nist_sort.rs"]
mod files_nist_sort;

#[path = "e2e/debug_communication_screen.rs"]
mod debug_communication_screen;

#[path = "e2e/advanced_runtime.rs"]
mod advanced_runtime;
