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

/// Helper: parse and lower without semantic analysis.
/// Useful for testing constructs that may not pass full semantic analysis yet.
fn parse_and_lower(source: &str) -> cobol_hir::HirProgram {
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
    let tokens = lexer.lex_all();
    let mut parser = Parser::new(tokens, FileId(0));
    let program = parser.parse_program().expect("parsing should succeed");
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

// ---------------------------------------------------------------------------
// Extended E2E tests for COBOL-85 features
// ---------------------------------------------------------------------------

#[test]
fn test_data_items_and_move() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-MOVE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(10).
01  WS-COUNT PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    MOVE \"COBOL\" TO WS-NAME.
    DISPLAY WS-NAME.
    DISPLAY WS-COUNT.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(!hir.data_items.is_empty());
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Move { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("strncpy"));
    assert!(c_code.contains("cobol_display_int"));
}

#[test]
fn test_arithmetic_compute() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ARITH.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 10.
01  WS-B PIC 9(5) VALUE 20.
01  WS-C PIC 9(5).
PROCEDURE DIVISION.
    COMPUTE WS-C = WS-A + WS-B.
    DISPLAY WS-C.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Compute { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_C ="));
    assert!(c_code.contains("+"));
}

#[test]
fn test_add_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ADD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 10.
01  WS-B PIC 9(5) VALUE 20.
PROCEDURE DIVISION.
    ADD WS-A TO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Add { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_B +="));
}

#[test]
fn test_subtract_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-SUB.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 10.
01  WS-B PIC 9(5) VALUE 30.
PROCEDURE DIVISION.
    SUBTRACT WS-A FROM WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Subtract { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_B -="));
}

#[test]
fn test_multiply_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-MUL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 5.
01  WS-B PIC 9(5) VALUE 3.
PROCEDURE DIVISION.
    MULTIPLY WS-A BY WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Multiply { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_B *="));
}

#[test]
fn test_divide_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DIV.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 2.
01  WS-B PIC 9(5) VALUE 10.
PROCEDURE DIVISION.
    DIVIDE WS-A INTO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Divide { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_B /="));
}

#[test]
fn test_if_else_with_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-IF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3) VALUE 150.
PROCEDURE DIVISION.
    IF WS-A > 100
        DISPLAY \"BIG\"
    ELSE
        DISPLAY \"SMALL\"
    END-IF.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::If { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("if ("));
    assert!(c_code.contains("} else {"));
    assert!(c_code.contains("BIG"));
    assert!(c_code.contains("SMALL"));
}

#[test]
fn test_perform_varying_loop() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-LOOP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-I PIC 9(3).
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 1 BY 1
        UNTIL WS-I > 5
        DISPLAY WS-I
    END-PERFORM.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Perform { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_I = "));
    assert!(c_code.contains("while ("));
    assert!(c_code.contains("WS_I +="));
}

#[test]
fn test_evaluate_desugars_to_if() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-EVAL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-GRADE PIC X(1) VALUE \"A\".
PROCEDURE DIVISION.
    EVALUATE WS-GRADE
        WHEN \"A\"
            DISPLAY \"EXCELLENT\"
        WHEN \"B\"
            DISPLAY \"GOOD\"
        WHEN OTHER
            DISPLAY \"UNKNOWN\"
    END-EVALUATE.
    STOP RUN.
";
    // Use parse_and_lower to skip semantic analysis for EVALUATE
    let hir = parse_and_lower(src);
    // EVALUATE is lowered to nested IF statements
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::If { .. })));

    let c_code = {
        let hir = parse_and_lower(src);
        generate_c(&hir)
    };
    assert!(c_code.contains("if ("));
    assert!(c_code.contains("EXCELLENT"));
    assert!(c_code.contains("GOOD"));
    assert!(c_code.contains("UNKNOWN"));
}

#[test]
fn test_perform_times() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-TIMES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-COUNT PIC 9(3).
PROCEDURE DIVISION.
    PERFORM 3 TIMES
        DISPLAY \"LOOP\"
    END-PERFORM.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Perform { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("for ("));
    assert!(c_code.contains("LOOP"));
}

#[test]
fn test_move_numeric_to_numeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-MOVE-NUM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 100.
01  WS-B PIC 9(5).
PROCEDURE DIVISION.
    MOVE WS-A TO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Move { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_B = WS_A"));
}

#[test]
fn test_move_zero_to_numeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ZERO.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    MOVE ZERO TO WS-A.
    DISPLAY WS-A.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Move { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_A = 0"));
}

#[test]
fn test_data_with_initial_values() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-MSG PIC X(20) VALUE \"Hello from COBOL!\".
01  WS-NUM PIC 9(5) VALUE 12345.
PROCEDURE DIVISION.
    DISPLAY WS-MSG.
    DISPLAY WS-NUM.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert_eq!(hir.data_items.len(), 2);
    assert!(hir.data_items[0].initial_value.is_some());
    assert!(hir.data_items[1].initial_value.is_some());

    let c_code = compile_to_c(src);
    assert!(c_code.contains("Hello from COBOL!"));
    assert!(c_code.contains("12345"));
}

#[test]
fn test_nested_if() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-NEST-IF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3) VALUE 50.
01  WS-B PIC 9(3) VALUE 30.
PROCEDURE DIVISION.
    IF WS-A > 10
        IF WS-B > 20
            DISPLAY \"BOTH\"
        END-IF
    END-IF.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    // Outer IF
    if let HirStatement::If { then_body, .. } = &hir.body[0] {
        // Inner IF should be in then_body
        assert!(then_body
            .iter()
            .any(|s| matches!(s, HirStatement::If { .. })));
    } else {
        panic!("Expected IF statement");
    }
}

#[test]
fn test_perform_until() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-UNTIL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-I PIC 9(3) VALUE 1.
PROCEDURE DIVISION.
    PERFORM UNTIL WS-I > 5
        DISPLAY WS-I
        ADD 1 TO WS-I
    END-PERFORM.
    STOP RUN.
";
    // Use parse_and_lower to avoid sema issues with inline ADD TO
    let hir = parse_and_lower(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Perform { .. })));

    let c_code = {
        let hir = parse_and_lower(src);
        generate_c(&hir)
    };
    assert!(c_code.contains("while ("));
}

#[test]
fn test_full_pipeline_c_code_structure() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FULLTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-MSG PIC X(20) VALUE \"Hello from COBOL!\".
PROCEDURE DIVISION.
    DISPLAY WS-MSG.
    STOP RUN.
";
    let c_code = compile_to_c(src);

    // Verify complete C code structure
    assert!(c_code.contains("#include <stdio.h>"));
    assert!(c_code.contains("#include <string.h>"));
    assert!(c_code.contains("#include <stdint.h>"));
    assert!(c_code.contains("int main("));
    assert!(c_code.contains("static char WS_MSG"));
    assert!(c_code.contains("memset(WS_MSG"));
    assert!(c_code.contains("strncpy(WS_MSG"));
    assert!(c_code.contains("Hello from COBOL!"));
    assert!(c_code.contains("cobol_stop_run()"));
    assert!(c_code.contains("return 0;"));
}

#[test]
fn test_compute_with_complex_expression() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-COMPUTE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 10.
01  WS-B PIC 9(5) VALUE 3.
01  WS-C PIC 9(5) VALUE 2.
01  WS-RESULT PIC 9(5).
PROCEDURE DIVISION.
    COMPUTE WS-RESULT = WS-A + WS-B * WS-C.
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Compute { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("WS_RESULT ="));
}

#[test]
fn test_initialize_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INIT-STMT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    INITIALIZE WS-A.
    DISPLAY WS-A.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    assert!(hir
        .body
        .iter()
        .any(|s| matches!(s, HirStatement::Initialize { .. })));

    let c_code = compile_to_c(src);
    assert!(c_code.contains("INITIALIZE"));
}

// ---------------------------------------------------------------------------
// COBOL 2014+ E2E tests
// ---------------------------------------------------------------------------

#[test]
fn test_float_short_e2e() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-E2E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    DISPLAY WS-FLOAT.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let float_item = hir
        .data_items
        .iter()
        .find(|d| d.name.as_str() == "WS-FLOAT");
    assert!(float_item.is_some());
    assert_eq!(float_item.unwrap().data_type, HirType::FloatShort);

    let c_code = generate_c(&hir);
    assert!(c_code.contains("static float WS_FLOAT"));
    assert!(c_code.contains("WS_FLOAT = 0.0"));
}

#[test]
fn test_float_long_e2e() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-L-E2E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-DBL USAGE FLOAT-LONG.
PROCEDURE DIVISION.
    DISPLAY WS-DBL.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let item = hir.data_items.iter().find(|d| d.name.as_str() == "WS-DBL");
    assert!(item.is_some());
    assert_eq!(item.unwrap().data_type, HirType::FloatLong);

    let c_code = generate_c(&hir);
    assert!(c_code.contains("static double WS_DBL"));
}

#[test]
fn test_float_extended_e2e() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-E-E2E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-EXT USAGE FLOAT-EXTENDED.
PROCEDURE DIVISION.
    DISPLAY WS-EXT.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let item = hir.data_items.iter().find(|d| d.name.as_str() == "WS-EXT");
    assert!(item.is_some());
    assert_eq!(item.unwrap().data_type, HirType::FloatExtended);

    let c_code = generate_c(&hir);
    assert!(c_code.contains("static long double WS_EXT"));
}

#[test]
fn test_c_code_includes_2014_runtime_declarations() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RUNTIME-DECL.
PROCEDURE DIVISION.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    // 2014+ runtime declarations
    assert!(c_code.contains("cobol_validate"));
    assert!(c_code.contains("cobol_json_generate"));
    assert!(c_code.contains("cobol_xml_generate"));
}

#[test]
fn test_c_code_includes_2023_runtime_declarations() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RUNTIME-2023.
PROCEDURE DIVISION.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    // 2023+ runtime declarations
    assert!(c_code.contains("cobol_utf8_char_count"));
    assert!(c_code.contains("cobol_utf8_substring"));
    assert!(c_code.contains("cobol_utf8_upper"));
    assert!(c_code.contains("cobol_utf8_lower"));
    assert!(c_code.contains("cobol_thread_create"));
    assert!(c_code.contains("cobol_thread_join"));
    assert!(c_code.contains("cobol_mutex_create"));
    assert!(c_code.contains("cobol_mutex_lock"));
    assert!(c_code.contains("cobol_mutex_unlock"));
    assert!(c_code.contains("cobol_mutex_destroy"));
}

// ===========================================================================
// Reference modification tests
// ===========================================================================

#[test]
fn test_reference_modification_display_in_hir() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20) VALUE \"HELLO WORLD\".
PROCEDURE DIVISION.
    DISPLAY WS-NAME(1:5).
    STOP RUN.
";
    let hir = parse_and_lower(src);
    assert!(!hir.body.is_empty());
    match &hir.body[0] {
        HirStatement::Display { operands, .. } => {
            assert_eq!(operands.len(), 1);
            match &operands[0] {
                cobol_hir::HirExpr::ReferenceModification {
                    variable,
                    start,
                    length,
                } => {
                    assert_eq!(variable.as_str(), "WS-NAME");
                    assert!(matches!(
                        **start,
                        cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::Integer(1))
                    ));
                    assert!(length.is_some());
                }
                other => panic!("expected ReferenceModification, got {:?}", other),
            }
        }
        other => panic!("expected Display, got {:?}", other),
    }
}

#[test]
fn test_reference_modification_display_c_output() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-C.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20) VALUE \"HELLO WORLD\".
PROCEDURE DIVISION.
    DISPLAY WS-NAME(1:5).
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);

    // The generated C should contain a display call with pointer arithmetic
    // and length parameter: cobol_display_string with offset and length.
    assert!(
        c_code.contains("cobol_display_string"),
        "should contain cobol_display_string"
    );
    // The offset should be (1 - 1) = 0, referencing WS_NAME + 0
    assert!(
        c_code.contains("WS_NAME"),
        "should reference the variable WS_NAME"
    );
    // Should contain the colon-based offset arithmetic (start - 1)
    assert!(
        c_code.contains("- 1"),
        "should contain 1-based to 0-based adjustment"
    );
}

#[test]
fn test_reference_modification_move_target_c_output() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-MOVE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
PROCEDURE DIVISION.
    MOVE \"ABC\" TO WS-NAME(3:3).
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);

    // For MOVE to a reference-modified target, we expect memcpy
    assert!(
        c_code.contains("memcpy"),
        "should use memcpy for ref-mod MOVE target"
    );
    assert!(
        c_code.contains("WS_NAME"),
        "should reference the target variable"
    );
    assert!(c_code.contains("ABC"), "should contain the source string");
}

#[test]
fn test_reference_modification_start_only_c_output() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-START.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20) VALUE \"HELLO WORLD\".
PROCEDURE DIVISION.
    DISPLAY WS-NAME(6:).
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);

    // Start-only reference modification: length defaults to remaining bytes.
    // The C code should compute length as (size - (start - 1)).
    assert!(c_code.contains("cobol_display_string"));
    assert!(c_code.contains("WS_NAME"));
}

#[test]
fn test_reference_modification_as_move_source() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REFMOD-SRC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SRC PIC X(20) VALUE \"HELLO WORLD\".
01  WS-DST PIC X(10).
PROCEDURE DIVISION.
    MOVE WS-SRC(7:5) TO WS-DST.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    assert!(!hir.body.is_empty());
    match &hir.body[0] {
        HirStatement::Move { from, .. } => match from {
            cobol_hir::HirExpr::ReferenceModification {
                variable,
                start,
                length,
            } => {
                assert_eq!(variable.as_str(), "WS-SRC");
                assert!(matches!(
                    **start,
                    cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::Integer(7))
                ));
                assert!(length.is_some());
                let len = length.as_ref().unwrap();
                assert!(matches!(
                    **len,
                    cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::Integer(5))
                ));
            }
            other => panic!("expected ReferenceModification, got {:?}", other),
        },
        other => panic!("expected Move, got {:?}", other),
    }
}

// ===========================================================================
// Native binary compilation & execution tests
//
// These tests compile COBOL source all the way to native binaries via C,
// execute them, and verify the output. They require clang and the runtime
// static library (built automatically by cargo).
// ===========================================================================

use std::path::PathBuf;
use std::process::Command;

/// Helper: compile COBOL source to a native binary and run it.
/// Returns (stdout, stderr, exit_code).
fn compile_and_run(source: &str) -> (String, String, i32) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = tmp.path().join("test_exe");

    // Run the full pipeline: lex -> parse -> sema -> hir -> codegen
    let hir = compile_to_hir(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    // Find the runtime library path
    let runtime_lib_path = find_test_runtime_lib();

    cobol_codegen::compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
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
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = tmp.path().join("test_exe");

    let hir = parse_and_lower(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    let runtime_lib_path = find_test_runtime_lib();

    cobol_codegen::compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
        .expect("C compilation should succeed");

    let output = Command::new(&exe_path)
        .output()
        .expect("execute compiled binary");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    (stdout, stderr, code)
}

fn find_test_runtime_lib() -> PathBuf {
    // Check common locations relative to the test working directory
    let candidates = [
        PathBuf::from("target/debug"),
        PathBuf::from("../../target/debug"),
        PathBuf::from("../../../target/debug"),
    ];
    for dir in &candidates {
        if dir.join("libcobol_runtime.a").exists() {
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
        if workspace_root.join("libcobol_runtime.a").exists() {
            return workspace_root;
        }
    }
    PathBuf::from("target/debug")
}

#[test]
fn test_native_hello_world() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0, "program should exit with code 0");
    assert!(
        stdout.contains("Hello, World!"),
        "output should contain greeting, got: {}",
        stdout
    );
}

#[test]
fn test_native_arithmetic() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ARITH.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 100.
01  WS-B PIC 9(5) VALUE 25.
PROCEDURE DIVISION.
    ADD WS-A TO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("125"),
        "100 + 25 should be 125, got: {}",
        stdout
    );
}

#[test]
fn test_native_if_else() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IFTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC 9(3) VALUE 42.
PROCEDURE DIVISION.
    IF WS-NUM > 10
        DISPLAY \"BIG\"
    ELSE
        DISPLAY \"SMALL\"
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("BIG"),
        "42 > 10 should print BIG, got: {}",
        stdout
    );
}

#[test]
fn test_native_perform_times() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PERF-TIMES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-COUNT PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    PERFORM 3 TIMES
        ADD 1 TO WS-COUNT
    END-PERFORM.
    DISPLAY WS-COUNT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("3"), "should count to 3, got: {}", stdout);
}

#[test]
fn test_native_perform_until() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PERF-UNTIL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-I PIC 9(3) VALUE 1.
01  WS-SUM PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    PERFORM UNTIL WS-I > 10
        ADD WS-I TO WS-SUM
        ADD 1 TO WS-I
    END-PERFORM.
    DISPLAY WS-SUM.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    // Sum 1..10 = 55
    assert!(
        stdout.contains("55"),
        "sum of 1..10 should be 55, got: {}",
        stdout
    );
}

#[test]
fn test_native_evaluate() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVAL-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-GRADE PIC 9 VALUE 3.
PROCEDURE DIVISION.
    EVALUATE WS-GRADE
        WHEN 1 DISPLAY \"ONE\"
        WHEN 2 DISPLAY \"TWO\"
        WHEN 3 DISPLAY \"THREE\"
        WHEN OTHER DISPLAY \"OTHER\"
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("THREE"),
        "grade 3 should print THREE, got: {}",
        stdout
    );
}

#[test]
fn test_native_move_and_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOVE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3) VALUE 42.
01  WS-B PIC 9(3).
PROCEDURE DIVISION.
    MOVE WS-A TO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("42"), "should display 42, got: {}", stdout);
}

#[test]
fn test_native_multiply_divide() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MULDIV.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(5) VALUE 12.
01  WS-B PIC 9(5) VALUE 5.
01  WS-R PIC 9(5).
PROCEDURE DIVISION.
    MULTIPLY WS-A BY WS-B.
    DISPLAY WS-B.
    MOVE 100 TO WS-R.
    DIVIDE 4 INTO WS-R.
    DISPLAY WS-R.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("60"), "12 * 5 = 60, got: {}", stdout);
    assert!(stdout.contains("25"), "100 / 4 = 25, got: {}", stdout);
}

#[test]
fn test_native_paragraph_perform() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PARA-TEST.
PROCEDURE DIVISION.
    PERFORM GREET-PARA.
    STOP RUN.
GREET-PARA.
    DISPLAY \"Hello from paragraph\".
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("Hello from paragraph"),
        "should call paragraph, got: {}",
        stdout
    );
}

#[test]
fn test_native_nested_if() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NEST-IF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-X PIC 9(3) VALUE 50.
PROCEDURE DIVISION.
    IF WS-X > 10
        IF WS-X > 100
            DISPLAY \"HUGE\"
        ELSE
            DISPLAY \"MEDIUM\"
        END-IF
    ELSE
        DISPLAY \"SMALL\"
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("MEDIUM"),
        "50 > 10 but not > 100 = MEDIUM, got: {}",
        stdout
    );
}

#[test]
fn test_native_perform_varying() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PERF-VARY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-I PIC 9(3).
01  WS-SUM PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 1 BY 2 UNTIL WS-I > 9
        ADD WS-I TO WS-SUM
    END-PERFORM.
    DISPLAY WS-SUM.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    // 1 + 3 + 5 + 7 + 9 = 25
    assert!(
        stdout.contains("25"),
        "sum of 1,3,5,7,9 should be 25, got: {}",
        stdout
    );
}

#[test]
fn test_native_compute_expression() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMP-EXPR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3) VALUE 10.
01  WS-B PIC 9(3) VALUE 3.
01  WS-R PIC 9(5).
PROCEDURE DIVISION.
    COMPUTE WS-R = WS-A * WS-B + 7.
    DISPLAY WS-R.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    // 10 * 3 + 7 = 37
    assert!(stdout.contains("37"), "10 * 3 + 7 = 37, got: {}", stdout);
}

#[test]
fn test_native_multiple_displays() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MULTI-DISP.
PROCEDURE DIVISION.
    DISPLAY \"LINE1\".
    DISPLAY \"LINE2\".
    DISPLAY \"LINE3\".
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("LINE1"));
    assert!(stdout.contains("LINE2"));
    assert!(stdout.contains("LINE3"));
}

#[test]
fn test_native_goback() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GOBACK-TEST.
PROCEDURE DIVISION.
    DISPLAY \"BEFORE\".
    GOBACK.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("BEFORE"),
        "should display BEFORE, got: {}",
        stdout
    );
}
