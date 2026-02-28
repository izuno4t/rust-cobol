// COBOL Compiler - CLI driver
//
// Orchestrates the full compilation pipeline:
//   Source -> Lexer -> Parser -> Sema -> HIR -> C codegen -> Executable

use std::path::{Path, PathBuf};
use std::process;

use clap::Parser as ClapParser;
use cobol_codegen::{compile_c_to_executable, generate_c};
use cobol_common::{FileId, SourceFormat};
use cobol_hir::lower_to_hir;
use cobol_lexer::Lexer;
use cobol_parser::Parser;
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

    /// Verbose output
    #[arg(short = 'v', long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();

    let source_format = match cli.source_format.as_str() {
        "fixed" => SourceFormat::Fixed,
        "free" => SourceFormat::Free,
        "variable" => SourceFormat::Variable,
        other => {
            eprintln!("error: unknown source format '{}'", other);
            process::exit(1);
        }
    };

    for (file_idx, file_path) in cli.files.iter().enumerate() {
        if cli.verbose {
            eprintln!("compiling: {}", file_path);
        }

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read '{}': {}", file_path, e);
                process::exit(1);
            }
        };

        let file_id = FileId(file_idx as u32);

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
            Ok(p) => p,
            Err(_) => {
                eprintln!("error: parsing failed for '{}'", file_path);
                process::exit(1);
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
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&program);
        let diagnostics = analyzer.take_diagnostics();

        // Report diagnostics
        for diag in diagnostics.diagnostics() {
            eprintln!("{:?}: [{}] {}", diag.severity, diag.code, diag.message);
        }

        if result.has_errors {
            eprintln!("error: semantic analysis failed for '{}'", file_path);
            process::exit(1);
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
                    process::exit(1);
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
            process::exit(1);
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
                process::exit(1);
            }
        }
    }
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
/// 2. ./target/debug (development builds)
/// 3. ./target/release (release builds)
/// 4. Current directory
fn find_runtime_lib() -> PathBuf {
    if let Ok(path) = std::env::var("COBOL_RUNTIME_LIB") {
        return PathBuf::from(path);
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
