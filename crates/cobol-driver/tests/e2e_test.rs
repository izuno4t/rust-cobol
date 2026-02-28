// End-to-end integration tests for the COBOL compiler pipeline.
//
// Tests the full flow: Source -> Lex -> Parse -> Sema -> HIR -> Codegen

use cobol_codegen::generate_c;
use cobol_common::{FileId, SourceFormat};
use cobol_hir::lower_to_hir;
use cobol_hir::{HirStatement, HirType};
use cobol_lexer::Lexer;
use cobol_parser::Parser;
use cobol_sema::SemanticAnalyzer;

/// Helper: run the full pipeline up to HIR and return the HIR program.
fn compile_to_hir(source: &str) -> cobol_hir::HirProgram {
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = parser.parse_program().expect("parsing should succeed");

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze(&program);
    assert!(
        !result.has_errors,
        "semantic analysis should not produce errors"
    );

    lower_to_hir(&program)
}

/// Helper: run the full pipeline up to C code generation.
fn compile_to_c(source: &str) -> String {
    let hir = compile_to_hir(source);
    generate_c(&hir)
}

#[test]
fn test_parse_and_lower_hello_world() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert_eq!(hir.name.as_str(), "HELLO-WORLD");
    assert!(!hir.body.is_empty());

    // Verify DISPLAY and STOP RUN are in the body
    assert!(
        matches!(&hir.body[0], HirStatement::Display { .. }),
        "first statement should be DISPLAY"
    );
    assert!(
        matches!(&hir.body[1], HirStatement::StopRun { .. }),
        "second statement should be STOP RUN"
    );
}

#[test]
fn test_hello_world_c_output() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
    let c_code = compile_to_c(src);

    // The C code should contain:
    // 1. Runtime declarations
    assert!(c_code.contains("extern void cobol_display_string"));
    assert!(c_code.contains("extern void cobol_stop_run"));

    // 2. A main function
    assert!(c_code.contains("int main("));

    // 3. The display call with the string
    assert!(c_code.contains("Hello, World!"));
    assert!(c_code.contains("cobol_display_string"));
    assert!(c_code.contains("cobol_display_newline"));

    // 4. The stop run call
    assert!(c_code.contains("cobol_stop_run()"));
}

#[test]
fn test_data_items_and_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DATA-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20) VALUE \"COBOL\".
01  WS-COUNT PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    DISPLAY WS-COUNT.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert_eq!(hir.data_items.len(), 2);

    // Check WS-NAME
    assert_eq!(hir.data_items[0].name.as_str(), "WS-NAME");
    assert_eq!(
        hir.data_items[0].data_type,
        HirType::Alphanumeric { size: 20 }
    );

    // Check WS-COUNT
    assert_eq!(hir.data_items[1].name.as_str(), "WS-COUNT");
    assert!(matches!(
        hir.data_items[1].data_type,
        HirType::Numeric { size: 5, .. }
    ));

    // Verify C output
    let c_code = compile_to_c(src);
    assert!(c_code.contains("static char WS_NAME"));
    assert!(c_code.contains("static int64_t WS_COUNT"));
}

#[test]
fn test_if_else_pipeline() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IF-TEST.
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
    let hir = compile_to_hir(src);
    assert!(matches!(&hir.body[0], HirStatement::If { .. }));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("if ("));
    assert!(c_code.contains("} else {"));
}

#[test]
fn test_perform_varying() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LOOP-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-I PIC 9(3).
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 1 BY 1
        UNTIL WS-I > 10
        DISPLAY WS-I
    END-PERFORM.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(matches!(&hir.body[0], HirStatement::Perform { .. }));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("while ("));
}

#[test]
fn test_move_statement_pipeline() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOVE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC X(10).
01  WS-B PIC X(10).
PROCEDURE DIVISION.
    MOVE \"HELLO\" TO WS-A WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    if let HirStatement::Move { to, .. } = &hir.body[0] {
        assert_eq!(to.len(), 2);
    } else {
        panic!("Expected MOVE statement");
    }
}

#[test]
fn test_goback_lowering() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GOBACK-TEST.
PROCEDURE DIVISION.
    DISPLAY \"Done\".
    GOBACK.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Goback { .. })));
}

#[test]
fn test_multiple_display_operands() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MULTI-DISPLAY.
PROCEDURE DIVISION.
    DISPLAY \"A\" \"B\" \"C\".
    STOP RUN.
";
    let hir = compile_to_hir(src);
    if let HirStatement::Display { operands, .. } = &hir.body[0] {
        assert_eq!(operands.len(), 3);
    } else {
        panic!("Expected DISPLAY statement");
    }

    let c_code = compile_to_c(src);
    // Should have 3 display_string calls + 1 from declaration = 4
    let count = c_code.matches("cobol_display_string").count();
    assert_eq!(count, 4);
}

#[test]
fn test_display_no_advancing() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NO-ADV.
PROCEDURE DIVISION.
    DISPLAY \"No newline\" WITH NO ADVANCING.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    if let HirStatement::Display { no_advancing, .. } = &hir.body[0] {
        assert!(*no_advancing);
    } else {
        panic!("Expected DISPLAY statement");
    }

    let c_code = compile_to_c(src);
    // Should have flush instead of newline
    assert!(c_code.contains("cobol_display_flush"));
}
