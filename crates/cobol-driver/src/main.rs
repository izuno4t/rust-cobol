// COBOL Compiler - CLI driver
//
// Orchestrates the full compilation pipeline:
//   Source -> Lexer -> Parser -> Sema -> HIR -> C codegen -> Executable

use std::path::{Path, PathBuf};

use clap::Parser as ClapParser;
use cobol_codegen::{compile_c_to_executable, generate_c};
use cobol_common::{FileId, SourceFormat};
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

        // ---------------------------------------------------------------
        // Phase 1: Lexing
        // ---------------------------------------------------------------
        let mut lexer = Lexer::new(&source, file_id, source_format);
        let tokens = lexer.lex_all();

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
        let mut parser = Parser::new(tokens, file_id);
        let program = match parser.parse_program() {
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
        let mut analyzer = SemanticAnalyzer::with_warning_level(warning_level);
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
        let hir = lower_to_hir(&program);

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
        let c_code = generate_c(&hir);

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
        PathBuf::from(out)
    } else {
        let p = Path::new(source_path);
        p.with_extension("")
    }
}

/// Locate the directory containing the COBOL runtime static library.
///
/// Search order:
/// 1. COBOL_RUNTIME_LIB environment variable
/// 2. Relative to the compiler executable (same dir, then ../lib/)
/// 3. ./target/debug (development builds)
/// 4. ./target/release (release builds)
/// 5. Current directory
fn find_runtime_lib() -> PathBuf {
    if let Ok(path) = std::env::var("COBOL_RUNTIME_LIB") {
        return PathBuf::from(path);
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
        assert_eq!(p, PathBuf::from("hello"));
    }

    #[test]
    fn test_output_exe_path_explicit() {
        let p = output_exe_path("hello.cob", &Some("my_program".to_string()));
        assert_eq!(p, PathBuf::from("my_program"));
    }
}
