// COBOL Compiler - CLI driver
//
// Orchestrates the full compilation pipeline:
//   Source -> Lexer -> Parser -> Sema -> HIR -> C codegen -> Executable

use std::path::{Path, PathBuf};

use clap::Parser as ClapParser;
use cobol_ast::CobolProgram;
use cobol_codegen::{compile_c_to_executable, generate_c};
use cobol_common::{CobolStandard, FileId, SourceFormat};
use cobol_diagnostics::{render_diagnostics_to_stderr, WarningLevel};
use cobol_hir::lower_to_hir;
use cobol_lexer::Lexer;
use cobol_parser::Parser;
use cobol_preprocessor::{preprocess, PreprocessorConfig};
use cobol_sema::SemanticAnalyzer;

#[derive(ClapParser)]
#[command(name = "cobolc", about = "COBOL Compiler")]
struct Cli {
    /// Source files to compile
    #[arg(required = true)]
    files: Vec<String>,

    /// Output file
    #[arg(short = 'o', long)]
    output: Option<String>,

    /// Source format (fixed, free, variable)
    #[arg(long, default_value = "free")]
    source_format: String,

    /// COBOL standard mode (cobol85, cobol2002, cobol2014, cobol2023)
    #[arg(long, default_value = "cobol2023")]
    standard: String,

    /// Emit tokens (debug)
    #[arg(long)]
    dump_tokens: bool,

    /// Emit AST (debug)
    #[arg(long)]
    dump_ast: bool,

    /// Emit HIR (debug)
    #[arg(long)]
    dump_hir: bool,

    /// Emit generated C code (debug)
    #[arg(long)]
    emit_c: bool,

    /// Stop after generating C code (do not compile)
    #[arg(long)]
    c_only: bool,

    /// Additional directories to search for COPY copybooks
    #[arg(short = 'I', long = "copy-path", value_name = "DIR")]
    copy_paths: Vec<String>,

    /// Warning control: all, none, error (default: show warnings)
    ///
    /// -Wall    Show all diagnostics including hints and info
    /// -Wnone   Suppress all warnings
    /// -Werror  Treat warnings as errors
    #[arg(short = 'W', long = "warning", default_value = "default")]
    warning_level: String,

    /// Verbose output
    #[arg(short = 'v', long)]
    verbose: bool,
}

fn main() {
    if let Err(code) = run() {
        std::process::exit(code);
    }
}

fn merge_compilation_unit_programs(mut programs: Vec<CobolProgram>) -> Result<CobolProgram, ()> {
    let Some(mut root) = programs.drain(..1).next() else {
        return Err(());
    };
    root.nested_programs.extend(programs);
    Ok(root)
}

fn run() -> Result<(), i32> {
    let cli = Cli::parse();

    let warning_level = match cli.warning_level.as_str() {
        "all" => WarningLevel::All,
        "none" => WarningLevel::None,
        "error" => WarningLevel::Error,
        "default" => WarningLevel::Default,
        other => {
            eprintln!(
                "error: unknown warning level '{}' (use: all, none, error)",
                other
            );
            return Err(1);
        }
    };

    let source_format = match cli.source_format.as_str() {
        "fixed" => SourceFormat::Fixed,
        "free" => SourceFormat::Free,
        "variable" => SourceFormat::Variable,
        other => {
            eprintln!("error: unknown source format '{}'", other);
            return Err(1);
        }
    };

    let standard = match parse_standard(&cli.standard) {
        Some(standard) => standard,
        None => {
            eprintln!(
                "error: unknown COBOL standard '{}' (use: {})",
                cli.standard,
                CobolStandard::cli_values()
            );
            return Err(1);
        }
    };

    // Build preprocessor configuration from CLI options.
    let pp_config = {
        let mut config = PreprocessorConfig::default();
        for extra_path in &cli.copy_paths {
            config.copy_paths.push(PathBuf::from(extra_path));
        }
        config.source_format = source_format;
        config
    };

    for (file_idx, file_path) in cli.files.iter().enumerate() {
        if cli.verbose {
            eprintln!("compiling: {}", file_path);
        }

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read '{}': {}", file_path, e);
                return Err(1);
            }
        };

        let file_id = FileId(file_idx as u32);

        // ---------------------------------------------------------------
        // Phase 0: Preprocessing (COPY expansion, REPLACE)
        // ---------------------------------------------------------------
        let preprocessed = preprocess(&source, Path::new(file_path), &pp_config);

        // Report preprocessor diagnostics with source-annotated output.
        if !preprocessed.diagnostics.is_empty() {
            render_diagnostics_to_stderr(&preprocessed.diagnostics, file_path, &source);
        }
        if preprocessed.diagnostics.iter().any(|d| d.is_error()) {
            eprintln!("error: preprocessing failed for '{}'", file_path);
            return Err(1);
        }

        let source = preprocessed.source;
        let effective_source_format = preprocessed.effective_source_format;

        // ---------------------------------------------------------------
        // Phase 1: Lexing
        // ---------------------------------------------------------------
        let debug_timing = std::env::var("COBOL_DEBUG_TIMING").as_deref() == Ok("1");
        let t_lex = std::time::Instant::now();
        let mut lexer = Lexer::new(&source, file_id, effective_source_format);
        let tokens = lexer.lex_all();
        if debug_timing {
            eprintln!("[TIMING] Lexing: {:?}", t_lex.elapsed());
        }

        if cli.dump_tokens {
            println!("=== Tokens ({}) ===", file_path);
            for tok in &tokens {
                println!("  {:?}", tok);
            }
            if !cli.dump_ast && !cli.dump_hir && !cli.emit_c {
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Phase 2: Parsing
        // ---------------------------------------------------------------
        let t_parse = std::time::Instant::now();
        let mut parser = Parser::new(tokens, file_id);
        let program = match parser
            .parse_compilation_unit()
            .and_then(merge_compilation_unit_programs)
        {
            Ok(p) => {
                // Render any non-fatal parser diagnostics (warnings, etc.)
                let parser_diags = parser.diagnostics();
                if parser_diags.error_count() > 0 || parser_diags.warning_count() > 0 {
                    render_diagnostics_to_stderr(parser_diags.diagnostics(), file_path, &source);
                }
                p
            }
            Err(_) => {
                // Render parser diagnostics with source-annotated output
                render_diagnostics_to_stderr(
                    parser.diagnostics().diagnostics(),
                    file_path,
                    &source,
                );
                eprintln!("error: parsing failed for '{}'", file_path);
                return Err(1);
            }
        };

        if cli.dump_ast {
            println!("=== AST ({}) ===", file_path);
            println!("{:#?}", program);
            if !cli.dump_hir && !cli.emit_c {
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Phase 3: Semantic analysis
        // ---------------------------------------------------------------
        if debug_timing {
            eprintln!("[TIMING] Parsing: {:?}", t_parse.elapsed());
        }
        let t_sema = std::time::Instant::now();
        let mut analyzer =
            SemanticAnalyzer::with_warning_level_and_standard(warning_level, standard);
        let result = analyzer.analyze(&program);
        let diagnostics = analyzer.take_diagnostics();

        // Report diagnostics with source-annotated colored output
        render_diagnostics_to_stderr(diagnostics.diagnostics(), file_path, &source);

        if result.has_errors {
            eprintln!("error: semantic analysis failed for '{}'", file_path);
            return Err(1);
        }

        // ---------------------------------------------------------------
        // Phase 4: HIR lowering
        // ---------------------------------------------------------------
        if debug_timing {
            eprintln!("[TIMING] Sema + diagnostics: {:?}", t_sema.elapsed());
        }
        let t_hir = std::time::Instant::now();
        let hir = lower_to_hir(&program);
        if debug_timing {
            eprintln!("[TIMING] HIR lowering: {:?}", t_hir.elapsed());
        }

        if cli.dump_hir {
            println!("=== HIR ({}) ===", file_path);
            println!("{}", hir);
            if !cli.emit_c {
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Phase 5: Code generation (C output)
        // ---------------------------------------------------------------
        let t_cg = std::time::Instant::now();
        let c_code = generate_c(&hir);
        if debug_timing {
            eprintln!("[TIMING] C codegen: {:?}", t_cg.elapsed());
        }

        if cli.emit_c {
            println!("=== Generated C ({}) ===", file_path);
            println!("{}", c_code);
            if cli.c_only {
                // Write C to file and stop
                let c_path = output_c_path(file_path, &cli.output);
                if let Err(e) = std::fs::write(&c_path, &c_code) {
                    eprintln!("error: cannot write '{}': {}", c_path.display(), e);
                    return Err(1);
                }
                if cli.verbose {
                    eprintln!("wrote: {}", c_path.display());
                }
                continue;
            }
        }

        // ---------------------------------------------------------------
        // Phase 6: Compile C to executable
        // ---------------------------------------------------------------
        let c_path = output_c_path(file_path, &None);
        if let Err(e) = std::fs::write(&c_path, &c_code) {
            eprintln!("error: cannot write '{}': {}", c_path.display(), e);
            return Err(1);
        }

        let exe_path = output_exe_path(file_path, &cli.output);
        let runtime_lib_path = find_runtime_lib();
        if let Err(e) = ensure_runtime_staticlib_is_fresh(&runtime_lib_path) {
            eprintln!("error: failed to prepare COBOL runtime library: {}", e);
            return Err(1);
        }

        if cli.verbose {
            eprintln!("C source: {}", c_path.display());
            eprintln!("output:   {}", exe_path.display());
            eprintln!("runtime:  {}", runtime_lib_path.display());
        }

        match compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path) {
            Ok(()) => {
                // Clean up the temporary C file
                let _ = std::fs::remove_file(&c_path);
                if cli.verbose {
                    eprintln!("compiled: {}", exe_path.display());
                }
            }
            Err(e) => {
                eprintln!("error: compilation failed: {}", e);
                eprintln!(
                    "hint: the generated C code has been left at '{}'",
                    c_path.display()
                );
                return Err(1);
            }
        }
    }

    Ok(())
}

fn output_c_path(source_path: &str, explicit: &Option<String>) -> PathBuf {
    if let Some(ref out) = explicit {
        PathBuf::from(out).with_extension("c")
    } else {
        let p = Path::new(source_path);
        p.with_extension("c")
    }
}

fn output_exe_path(source_path: &str, explicit: &Option<String>) -> PathBuf {
    if let Some(ref out) = explicit {
        ensure_executable_extension(PathBuf::from(out))
    } else {
        let p = Path::new(source_path);
        ensure_executable_extension(p.with_extension(""))
    }
}

fn ensure_executable_extension(path: PathBuf) -> PathBuf {
    if cfg!(windows) && path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path
    }
}

fn parse_standard(value: &str) -> Option<CobolStandard> {
    CobolStandard::parse_cli(value)
}

/// Locate the directory containing the COBOL runtime static library.
///
/// Search order:
/// 1. COBOL_RUNTIME_LIB environment variable
/// 2. CARGO_TARGET_DIR environment variable
/// 3. Relative to the compiler executable (same dir, then ../lib/)
/// 4. ./target/debug (development builds)
/// 5. ./target/release (release builds)
/// 6. Current directory
fn find_runtime_lib() -> PathBuf {
    if let Ok(path) = std::env::var("COBOL_RUNTIME_LIB") {
        return PathBuf::from(path);
    }

    if let Ok(path) = std::env::var("CARGO_TARGET_DIR") {
        let path = PathBuf::from(path);
        for candidate in [path.join("release"), path.join("debug"), path.clone()] {
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // Search relative to the compiler executable location
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join("libcobol_runtime.a");
            if candidate.exists() {
                return exe_dir.to_path_buf();
            }
            // Also check ../lib/ relative to exe
            let lib_candidate = exe_dir.join("../lib/libcobol_runtime.a");
            if lib_candidate.exists() {
                let lib_dir = exe_dir.join("../lib");
                return lib_dir.canonicalize().unwrap_or(lib_dir);
            }
        }
    }

    let candidates = ["target/debug", "target/release", "."];

    for dir in &candidates {
        let p = PathBuf::from(dir);
        let lib_name = "libcobol_runtime.a";
        if p.join(lib_name).exists() {
            return p;
        }
    }

    // Default to target/debug
    PathBuf::from("target/debug")
}

fn ensure_runtime_staticlib_is_fresh(runtime_lib_dir: &Path) -> Result<(), String> {
    let archive = runtime_lib_dir.join("libcobol_runtime.a");
    let workspace_root = std::env::current_dir().map_err(|e| e.to_string())?;
    let runtime_root = workspace_root.join("crates/cobol-runtime");
    if !runtime_root.join("Cargo.toml").exists() {
        return Ok(());
    }

    let mut newest_source_mtime = std::fs::metadata(runtime_root.join("Cargo.toml"))
        .and_then(|meta| meta.modified())
        .map_err(|e| e.to_string())?;
    let src_root = runtime_root.join("src");
    if src_root.exists() {
        collect_newest_mtime(&src_root, &mut newest_source_mtime)?;
    }

    let archive_is_fresh = std::fs::metadata(&archive)
        .and_then(|meta| meta.modified())
        .map(|mtime| mtime >= newest_source_mtime)
        .unwrap_or(false);
    if archive_is_fresh {
        return Ok(());
    }

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = std::process::Command::new(cargo);
    cmd.current_dir(&workspace_root)
        .arg("build")
        .arg("-p")
        .arg("cobol-runtime");
    if runtime_lib_dir
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "release")
    {
        cmd.arg("--release");
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to invoke cargo build for cobol-runtime: {e}"))?;
    if !status.success() {
        return Err(format!(
            "cargo build -p cobol-runtime exited with status {}",
            status
        ));
    }
    Ok(())
}

fn collect_newest_mtime(dir: &Path, newest: &mut std::time::SystemTime) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            collect_newest_mtime(&path, newest)?;
            continue;
        }
        let modified = metadata.modified().map_err(|e| e.to_string())?;
        if modified > *newest {
            *newest = modified;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_c_path() {
        let p = output_c_path("hello.cob", &None);
        assert_eq!(p, PathBuf::from("hello.c"));
    }

    #[test]
    fn test_output_c_path_explicit() {
        let p = output_c_path("hello.cob", &Some("out.exe".to_string()));
        assert_eq!(p, PathBuf::from("out.c"));
    }

    #[test]
    fn test_output_exe_path() {
        let p = output_exe_path("hello.cob", &None);
        #[cfg(windows)]
        assert_eq!(p, PathBuf::from("hello.exe"));
        #[cfg(not(windows))]
        assert_eq!(p, PathBuf::from("hello"));
    }

    #[test]
    fn test_output_exe_path_explicit() {
        let p = output_exe_path("hello.cob", &Some("my_program".to_string()));
        #[cfg(windows)]
        assert_eq!(p, PathBuf::from("my_program.exe"));
        #[cfg(not(windows))]
        assert_eq!(p, PathBuf::from("my_program"));
    }

    #[test]
    fn test_output_exe_path_preserves_extension() {
        let p = output_exe_path("hello.cob", &Some("my_program.bin".to_string()));
        assert_eq!(p, PathBuf::from("my_program.bin"));
    }

    #[test]
    fn test_parse_standard_cli_values() {
        assert_eq!(parse_standard("cobol85"), Some(CobolStandard::Cobol85));
        assert_eq!(parse_standard("COBOL-2014"), Some(CobolStandard::Cobol2014));
        assert_eq!(parse_standard("2023"), Some(CobolStandard::Cobol2023));
        assert_eq!(parse_standard("latest"), None);
    }
}
