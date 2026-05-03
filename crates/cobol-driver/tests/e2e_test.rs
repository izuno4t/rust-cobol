// End-to-end integration tests for the COBOL compiler pipeline.
//
// Tests the full flow: Source -> Lex -> Parse -> Sema -> HIR -> Codegen

use cobol_ast::CobolProgram;
use cobol_codegen::generate_c;
use cobol_common::{FileId, SourceFormat, Span};
use cobol_hir::lower_to_hir;
use cobol_hir::{HirDeclarative, HirDeclarativeUse, HirStatement, HirType};
use cobol_lexer::Lexer;
use cobol_parser::Parser;
use cobol_sema::SemanticAnalyzer;

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

    lower_to_hir(&program)
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
    assert!(c_code.contains("cobol_move_string"));
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
    assert!(c_code.contains("WS_B = llabs"));
    assert!(c_code.contains("WS_A"));
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
    assert!(c_code.contains("WS_A = llabs"));
    assert!(c_code.contains("((int64_t)(0))"));
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
    assert!(c_code.contains("memcpy(WS_MSG"));
    assert!(c_code.contains("Hello from COBOL!"));
    assert!(c_code.contains("cobol_stop_run()"));
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
                cobol_hir::HirExpr::DataRef(data_ref) => {
                    assert_eq!(data_ref.name.as_str(), "WS-NAME");
                    let refmod = data_ref.refmod.as_ref().expect("expected refmod");
                    assert!(matches!(
                        *refmod.start,
                        cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::Integer(1))
                    ));
                    assert!(refmod.length.is_some());
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
            cobol_hir::HirExpr::DataRef(data_ref) => {
                assert_eq!(data_ref.name.as_str(), "WS-SRC");
                let refmod = data_ref.refmod.as_ref().expect("expected refmod");
                assert!(matches!(
                    *refmod.start,
                    cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::Integer(7))
                ));
                assert!(refmod.length.is_some());
                let len = refmod.length.as_ref().unwrap();
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
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

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
    compile_and_run_no_sema_with_env(source, &[])
}

fn compile_and_run_no_sema_with_env(source: &str, envs: &[(&str, &str)]) -> (String, String, i32) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let c_path = tmp.path().join("test.c");
    let exe_path = temp_exe_path(tmp.path(), "test_exe");

    let hir = parse_and_lower(source);
    let c_code = generate_c(&hir);

    std::fs::write(&c_path, &c_code).expect("write C file");

    let runtime_lib_path = find_test_runtime_lib();

    cobol_codegen::compile_c_to_executable(&c_path, &exe_path, &runtime_lib_path)
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

fn temp_exe_path(dir: &std::path::Path, stem: &str) -> PathBuf {
    if cfg!(windows) {
        dir.join(format!("{stem}.exe"))
    } else {
        dir.join(stem)
    }
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

// ===========================================================================
// Phase 1: OCCURS / Table (array) tests
// ===========================================================================

#[test]
fn test_native_occurs_basic() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. OCCURS-BASIC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-TABLE PIC 9(3) OCCURS 10 TIMES.
PROCEDURE DIVISION.
    MOVE 42 TO WS-TABLE(3).
    DISPLAY WS-TABLE(3).
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("42"),
        "WS-TABLE(3) should be 42, got: {}",
        stdout
    );
}

#[test]
fn test_native_occurs_variable_subscript() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. OCCURS-VAR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-TABLE PIC 9(3) OCCURS 10 TIMES.
01  WS-IDX PIC 9(2) VALUE 5.
PROCEDURE DIVISION.
    MOVE 99 TO WS-TABLE(WS-IDX).
    DISPLAY WS-TABLE(5).
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("99"),
        "WS-TABLE(5) should be 99, got: {}",
        stdout
    );
}

#[test]
fn test_native_occurs_loop() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. OCCURS-LOOP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-TABLE PIC 9(3) OCCURS 5 TIMES.
01  WS-IDX PIC 9(2).
PROCEDURE DIVISION.
    PERFORM VARYING WS-IDX FROM 1 BY 1
        UNTIL WS-IDX > 5
        MOVE WS-IDX TO WS-TABLE(WS-IDX)
    END-PERFORM.
    DISPLAY WS-TABLE(1).
    DISPLAY WS-TABLE(3).
    DISPLAY WS-TABLE(5).
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 3, "should have 3 lines, got: {}", stdout);
    assert!(
        lines[0].contains('1'),
        "first element should be 1, got: {}",
        lines[0]
    );
    assert!(
        lines[1].contains('3'),
        "third element should be 3, got: {}",
        lines[1]
    );
    assert!(
        lines[2].contains('5'),
        "fifth element should be 5, got: {}",
        lines[2]
    );
}

// ===========================================================================
// Phase 2: Decimal arithmetic tests
// ===========================================================================

#[test]
fn test_native_decimal_add() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEC-ADD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3)V99 VALUE 10.50.
01  WS-B PIC 9(3)V99 VALUE 20.25.
PROCEDURE DIVISION.
    ADD WS-A TO WS-B.
    DISPLAY WS-B.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("30.75"),
        "10.50 + 20.25 should be 030.75, got: {}",
        stdout
    );
}

#[test]
fn test_native_decimal_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEC-DISP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-AMOUNT PIC 9(5)V99 VALUE 123.45.
PROCEDURE DIVISION.
    DISPLAY WS-AMOUNT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("123.45"),
        "should display 123.45, got: {}",
        stdout
    );
}

#[test]
fn test_native_decimal_subtract() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEC-SUB.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3)V99 VALUE 50.00.
01  WS-B PIC 9(3)V99 VALUE 12.75.
PROCEDURE DIVISION.
    SUBTRACT WS-B FROM WS-A.
    DISPLAY WS-A.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("37.25"),
        "50.00 - 12.75 should be 037.25, got: {}",
        stdout
    );
}

// ===========================================================================
// Phase 3: Error handling tests
// ===========================================================================

#[test]
fn test_native_on_size_error_overflow() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIZE-ERR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SMALL PIC 9(2) VALUE 99.
PROCEDURE DIVISION.
    ADD 1 TO WS-SMALL
        ON SIZE ERROR DISPLAY \"OVERFLOW\"
        NOT ON SIZE ERROR DISPLAY \"OK\"
    END-ADD.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("OVERFLOW"),
        "99 + 1 should overflow PIC 9(2), got: {}",
        stdout
    );
}

#[test]
fn test_native_on_size_error_no_overflow() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIZE-OK.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-VAL PIC 9(3) VALUE 50.
PROCEDURE DIVISION.
    ADD 10 TO WS-VAL
        ON SIZE ERROR DISPLAY \"OVERFLOW\"
        NOT ON SIZE ERROR DISPLAY \"OK\"
    END-ADD.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("OK"),
        "50 + 10 should not overflow PIC 9(3), got: {}",
        stdout
    );
}

// -----------------------------------------------------------------------
// Phase 4: Figurative constants (HIGH-VALUE, LOW-VALUE, QUOTE, NULL)
// -----------------------------------------------------------------------

#[test]
fn test_native_high_value() {
    // Verify HIGH-VALUES and LOW-VALUES are generated correctly by checking
    // the generated C code for proper memset calls.
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HV-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-HV PIC X(3).
01  WS-LV PIC X(3).
PROCEDURE DIVISION.
    MOVE HIGH-VALUES TO WS-HV.
    MOVE LOW-VALUES TO WS-LV.
    DISPLAY \"DONE\".
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("memset(WS_HV, 0xFF,"),
        "HIGH-VALUES should generate memset with 0xFF, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("memset(WS_LV, 0x00,"),
        "LOW-VALUES should generate memset with 0x00, got:\n{}",
        c_code
    );

    // Also verify the native binary runs successfully
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("DONE"),
        "HIGH-VALUE/LOW-VALUE program should complete, got: {}",
        stdout
    );
}

#[test]
fn test_native_low_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LV-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-LV PIC X(3).
01  WS-NUM PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    MOVE LOW-VALUES TO WS-LV.
    IF WS-NUM = 0
        DISPLAY \"LV-OK\"
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("LV-OK"), "LOW-VALUE test, got: {}", stdout);
}

// -----------------------------------------------------------------------
// Phase 4: FILE STATUS variable - C codegen verification
// -----------------------------------------------------------------------

#[test]
fn test_file_status_codegen() {
    // Test that FILE STATUS clause generates proper status updates in C code.
    // We use parse_and_lower which includes the environment division lowering.
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FS-TEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT MYFILE ASSIGN TO \"test.dat\"
        FILE STATUS IS WS-FS.
DATA DIVISION.
FILE SECTION.
FD  MYFILE.
01  MY-REC PIC X(80).
WORKING-STORAGE SECTION.
01  WS-FS PIC XX.
PROCEDURE DIVISION.
    OPEN INPUT MYFILE.
    READ MYFILE.
    CLOSE MYFILE.
    STOP RUN.
";
    let hir = parse_and_lower(src);

    // Verify file_status_vars was populated
    assert_eq!(hir.file_status_vars.len(), 1);
    assert_eq!(hir.file_status_vars[0].file_name.as_str(), "MYFILE");
    assert_eq!(hir.file_status_vars[0].status_var.as_str(), "WS-FS");

    let c_code = generate_c(&hir);

    // OPEN should capture return value and set FILE STATUS via intermediate buffer
    assert!(
        c_code.contains("memcpy(&WS_FS, _fs_buf, 2)"),
        "OPEN should update FILE STATUS variable via memcpy, got:\n{}",
        c_code
    );
}

// -----------------------------------------------------------------------
// Phase A: Critical correctness tests
// -----------------------------------------------------------------------

#[test]
fn test_a1_alphanumeric_comparison_generates_memcmp() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CMP-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC X(10).
01  WS-B PIC X(10).
PROCEDURE DIVISION.
    IF WS-A = WS-B
        DISPLAY \"EQUAL\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_compare_alphanumeric"),
        "Alphanumeric comparison should use cobol_compare_alphanumeric, got:\n{}",
        c_code
    );
    assert!(
        !c_code.contains("(WS_A == WS_B)"),
        "Should not generate pointer comparison for alphanumeric fields"
    );
}

#[test]
fn test_a1_alphanumeric_comparison_with_literal() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CMP-LIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STATUS PIC X(5).
PROCEDURE DIVISION.
    IF WS-STATUS = \"DONE\"
        DISPLAY \"FINISHED\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_compare_alphanumeric"),
        "Alphanumeric-to-literal comparison should use runtime function, got:\n{}",
        c_code
    );
}

#[test]
fn test_a1_numeric_comparison_unchanged() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NUM-CMP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-X PIC 9(5).
01  WS-Y PIC 9(5).
PROCEDURE DIVISION.
    IF WS-X > WS-Y
        DISPLAY \"GREATER\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    // The declaration always exists; check that no *call* is generated in the main body
    let main_body = c_code.split("int main(").nth(1).unwrap_or("");
    assert!(
        !main_body.contains("cobol_compare_alphanumeric("),
        "Numeric comparison should NOT call cobol_compare_alphanumeric, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("(WS_X > WS_Y)"),
        "Numeric comparison should use direct operators, got:\n{}",
        c_code
    );
}

#[test]
fn test_a2_initialize_alphanumeric_spaces() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INIT-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
PROCEDURE DIVISION.
    INITIALIZE WS-NAME.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("memset(WS_NAME, ' ', 20)"),
        "INITIALIZE of alphanumeric should fill with spaces, got:\n{}",
        c_code
    );
}

#[test]
fn test_a2_initialize_numeric_zero() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INIT-NUM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-COUNT PIC 9(5).
PROCEDURE DIVISION.
    INITIALIZE WS-COUNT.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("WS_COUNT = 0; /* INITIALIZE */"),
        "INITIALIZE of numeric should set to zero, got:\n{}",
        c_code
    );
}

#[test]
fn test_a3_sign_condition_positive() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIGN-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC S9(5).
PROCEDURE DIVISION.
    IF WS-NUM IS POSITIVE
        DISPLAY \"POS\"
    END-IF.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    let c_code = generate_c(&hir);
    // POSITIVE should generate > 0
    assert!(
        c_code.contains("> 0") || c_code.contains("> ((int64_t)0)"),
        "POSITIVE condition should generate > 0, got:\n{}",
        c_code
    );
}

#[test]
fn test_a3_sign_condition_negative() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIGN-NEG.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC S9(5).
PROCEDURE DIVISION.
    IF WS-NUM IS NEGATIVE
        DISPLAY \"NEG\"
    END-IF.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("< 0") || c_code.contains("< ((int64_t)0)"),
        "NEGATIVE condition should generate < 0, got:\n{}",
        c_code
    );
}

#[test]
fn test_a3_sign_condition_zero() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIGN-ZERO.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC S9(5).
PROCEDURE DIVISION.
    IF WS-NUM IS ZERO
        DISPLAY \"ZERO\"
    END-IF.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("== 0") || c_code.contains("== ((int64_t)0)"),
        "ZERO condition should generate == 0, got:\n{}",
        c_code
    );
}

#[test]
fn test_a5_goto_depending_on() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GOTO-DEP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-IDX PIC 9.
PROCEDURE DIVISION.
    GO TO PARA-A PARA-B PARA-C
        DEPENDING ON WS-IDX.
    STOP RUN.
PARA-A.
    DISPLAY \"A\".
PARA-B.
    DISPLAY \"B\".
PARA-C.
    DISPLAY \"C\".
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("switch"),
        "GO TO DEPENDING ON should generate switch statement, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("case 1:") && c_code.contains("case 2:") && c_code.contains("case 3:"),
        "switch should have cases 1, 2, 3, got:\n{}",
        c_code
    );
}

#[test]
fn test_a6_88_level_thru_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRU-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SCORE PIC 9(3).
    88 PASSING VALUES 60 THRU 100.
PROCEDURE DIVISION.
    IF PASSING
        DISPLAY \"PASS\"
    END-IF.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    // Should generate range comparison: score >= 60 AND score <= 100
    assert!(
        c_code.contains(">= ") && c_code.contains("<= "),
        "88 THRU should generate range comparison (>= and <=), got:\n{}",
        c_code
    );
}

#[test]
fn test_a4_class_condition_numeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CLASS-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-INPUT PIC X(10).
PROCEDURE DIVISION.
    IF WS-INPUT IS NUMERIC
        DISPLAY \"NUMBER\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_is_numeric"),
        "IS NUMERIC should call cobol_is_numeric, got:\n{}",
        c_code
    );
}

#[test]
fn test_a4_class_condition_alphabetic() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CLASS-ALPHA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-INPUT PIC X(10).
PROCEDURE DIVISION.
    IF WS-INPUT IS ALPHABETIC
        DISPLAY \"ALPHA\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_is_alphabetic"),
        "IS ALPHABETIC should call cobol_is_alphabetic, got:\n{}",
        c_code
    );
}

#[test]
fn test_a4_class_condition_not_numeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CLASS-NOT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-INPUT PIC X(10).
PROCEDURE DIVISION.
    IF WS-INPUT IS NOT NUMERIC
        DISPLAY \"NOT NUMBER\"
    END-IF.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("(!") && c_code.contains("cobol_is_numeric"),
        "NOT NUMERIC should generate negated call, got:\n{}",
        c_code
    );
}

#[test]
fn test_a7_call_on_exception_not_dead() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CALL-EXC.
DATA DIVISION.
WORKING-STORAGE SECTION.
PROCEDURE DIVISION.
    CALL \"SUBPROG\"
        ON EXCEPTION DISPLAY \"ERROR\"
        NOT ON EXCEPTION DISPLAY \"OK\"
    END-CALL.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        !c_code.contains("if (0)"),
        "ON EXCEPTION should not generate dead if(0) code, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("_call_failed"),
        "ON EXCEPTION should use _call_failed flag, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("extern void SUBPROG() __attribute__((weak_import));")
            || c_code.contains("extern void SUBPROG() __attribute__((weak));"),
        "missing subprogram should be emitted as platform weak extern, got:\n{}",
        c_code
    );
    assert!(
        !c_code.contains("void SUBPROG() { /* stub */ }"),
        "missing subprogram should not be emitted as executable stub, got:\n{}",
        c_code
    );
}

// -----------------------------------------------------------------------
// Phase B: Common pattern completion tests
// -----------------------------------------------------------------------

#[test]
fn test_b1_string_with_delimiter() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. STR-DELIM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-RESULT PIC X(30).
01  WS-FIRST  PIC X(10) VALUE \"HELLO\".
01  WS-SECOND PIC X(10) VALUE \"WORLD\".
PROCEDURE DIVISION.
    STRING WS-FIRST DELIMITED BY SPACE
           WS-SECOND DELIMITED BY SIZE
           INTO WS-RESULT.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_string_concat"),
        "STRING should call cobol_string_concat, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("_delim_ptr_") || c_code.contains("delim"),
        "STRING with DELIMITED BY should set up delimiter info, got:\n{}",
        c_code
    );
}

#[test]
fn test_b2_move_string_literal_to_alphanumeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOV-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(10).
PROCEDURE DIVISION.
    MOVE \"HELLO\" TO WS-NAME.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_move_string"),
        "String literal to alphanumeric MOVE should use cobol_move_string, got:\n{}",
        c_code
    );
}

#[test]
fn test_b2_move_numeric_to_alphanumeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOV-NUM2ALPHA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM   PIC 9(5) VALUE 42.
01  WS-DISP  PIC X(10).
PROCEDURE DIVISION.
    MOVE WS-NUM TO WS-DISP.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_move_numeric_to_display"),
        "Numeric to alphanumeric MOVE should use cobol_move_numeric_to_display, got:\n{}",
        c_code
    );
}

#[test]
fn test_b2_move_alphanumeric_to_numeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOV-ALPHA2NUM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STR   PIC X(5) VALUE \"12345\".
01  WS-NUM   PIC 9(5).
PROCEDURE DIVISION.
    MOVE WS-STR TO WS-NUM.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_func_numval"),
        "Alphanumeric to numeric MOVE should use cobol_func_numval, got:\n{}",
        c_code
    );
}

#[test]
fn test_b4_inspect_tallying_all() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSP-TALLY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STR   PIC X(10) VALUE \"AABBAACCA\".
01  WS-COUNT PIC 9(3)  VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-STR TALLYING WS-COUNT FOR ALL \"A\".
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_inspect_tallying"),
        "INSPECT TALLYING should call cobol_inspect_tallying, got:\n{}",
        c_code
    );
    // mode 1 = ALL
    assert!(
        c_code.contains(", 1);"),
        "INSPECT TALLYING ALL should use mode 1, got:\n{}",
        c_code
    );
    // Counter variable should be incremented
    assert!(
        c_code.contains("WS_COUNT +="),
        "INSPECT TALLYING should increment counter variable, got:\n{}",
        c_code
    );
}

#[test]
fn test_b4_inspect_replacing_all() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSP-REPL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STR PIC X(10) VALUE \"AABBAACCA\".
PROCEDURE DIVISION.
    INSPECT WS-STR REPLACING ALL \"A\" BY \"X\".
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_inspect_replacing"),
        "INSPECT REPLACING should call cobol_inspect_replacing, got:\n{}",
        c_code
    );
    // mode 1 = ALL
    assert!(
        c_code.contains(", 1);"),
        "INSPECT REPLACING ALL should use mode 1, got:\n{}",
        c_code
    );
}

#[test]
fn test_b5_sort_with_using() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SORT-TEST.
DATA DIVISION.
FILE SECTION.
FD  SORT-FILE.
01  SORT-REC PIC X(80).
FD  INPUT-FILE.
01  INPUT-REC PIC X(80).
WORKING-STORAGE SECTION.
01  WS-KEY PIC X(10).
PROCEDURE DIVISION.
    SORT SORT-FILE ON ASCENDING KEY WS-KEY
        USING INPUT-FILE
        GIVING SORT-FILE.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    // Should read records from USING file
    assert!(
        c_code.contains("_sort_count"),
        "SORT with USING should track record count, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_sort("),
        "SORT should call cobol_sort, got:\n{}",
        c_code
    );
}

// -----------------------------------------------------------------------
// Phase C: Completeness improvement tests
// -----------------------------------------------------------------------

#[test]
fn test_c4_refmod_dynamic_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. REFMOD-DYN.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STR PIC X(20) VALUE \"HELLO WORLD\".
01  WS-POS PIC 9(3) VALUE 7.
01  WS-LEN PIC 9(3) VALUE 5.
PROCEDURE DIVISION.
    DISPLAY WS-STR(WS-POS:WS-LEN).
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    // Dynamic start/length should use variable names, not literals
    assert!(
        c_code.contains("WS_POS"),
        "Reference modification should use variable WS_POS, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("WS_LEN"),
        "Reference modification should use variable WS_LEN, got:\n{}",
        c_code
    );
}

#[test]
fn test_c1_redefines_pointer_cast() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. REDEF-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM   PIC 9(5) VALUE 12345.
01  WS-STR   REDEFINES WS-NUM PIC X(5).
PROCEDURE DIVISION.
    DISPLAY WS-STR.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("REDEFINES"),
        "REDEFINES should be annotated in generated C, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("WS_STR") && c_code.contains("WS_NUM"),
        "Both WS_STR and WS_NUM should be in generated C, got:\n{}",
        c_code
    );
}

#[test]
fn test_c5_call_by_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CALL-VAL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    CALL \"SUBPROG\" USING BY VALUE WS-NUM.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    // BY VALUE should pass the value directly, not &var
    let main_body = c_code.split("int main(").nth(1).unwrap_or("");
    assert!(
        main_body.contains("int64_t"),
        "CALL BY VALUE should declare int64_t param type, got:\n{}",
        c_code
    );
}

#[test]
fn test_c5_call_by_content() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CALL-CONT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC 9(5) VALUE 42.
PROCEDURE DIVISION.
    CALL \"SUBPROG\" USING BY CONTENT WS-NUM.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    // BY CONTENT should create a copy variable
    let main_body = c_code.split("int main(").nth(1).unwrap_or("");
    assert!(
        main_body.contains("_content_copy_"),
        "CALL BY CONTENT should create a copy variable, got:\n{}",
        c_code
    );
}

#[test]
fn test_c6_on_size_error_overflow_check() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SIZE-ERR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3) VALUE 999.
01  WS-B PIC 9(3).
PROCEDURE DIVISION.
    ADD 1 TO WS-A
        ON SIZE ERROR
            DISPLAY \"OVERFLOW\"
        NOT ON SIZE ERROR
            DISPLAY \"OK\"
    END-ADD.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("_size_error"),
        "ON SIZE ERROR should use _size_error flag, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("llabs"),
        "ON SIZE ERROR should check absolute value with llabs, got:\n{}",
        c_code
    );
}

// -----------------------------------------------------------------------
// Phase C-3: DECLARATIVES section
// -----------------------------------------------------------------------

#[test]
fn test_c3_declaratives_handler_generation() {
    // Build HIR directly with a declarative section since the parser
    // doesn't yet support DECLARATIVES syntax.
    let mut hir = parse_and_lower(
        "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STATUS PIC XX.
PROCEDURE DIVISION.
    DISPLAY \"MAIN\".
    STOP RUN.
",
    );

    // Inject a declarative handler for a file named "MY-FILE"
    hir.declaratives.push(HirDeclarative {
        name: "FILE-ERR-SECTION".into(),
        use_kind: HirDeclarativeUse::AfterException,
        is_global: false,
        file_names: vec!["MY-FILE".into()],
        debug_items: vec![],
        body: vec![HirStatement::Display {
            operands: vec![cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::String(
                "FILE ERROR HANDLER".into(),
            ))],
            no_advancing: false,
            span: Span::dummy(),
        }],
    });

    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("decl_FILE_ERR_SECTION"),
        "Should generate declarative handler function, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("_check_file_declarative"),
        "Should generate dispatcher function, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("MY_FILE"),
        "Dispatcher should match file name MY_FILE, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("FILE ERROR HANDLER"),
        "Handler body should contain the DISPLAY statement, got:\n{}",
        c_code
    );
}

// -----------------------------------------------------------------------
// Phase C-2: CORRESPONDING (CORR) matching
// -----------------------------------------------------------------------

#[test]
fn test_c2_move_corresponding() {
    use cobol_hir::HirDataItem;
    // Build HIR directly with group items having matching members.
    let hir = cobol_hir::HirProgram {
        name: "CORR-TEST".into(),
        data_items: vec![
            HirDataItem {
                name: "WS-SRC".into(),
                data_type: HirType::Group {
                    members: vec![
                        HirDataItem {
                            name: "FIELD-A".into(),
                            data_type: HirType::Numeric {
                                size: 5,
                                decimal_places: 0,
                                is_signed: false,
                            },
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            occurs: None,
                            indexed_by: Vec::new(),
                            redefines: None,
                            renames: None,
                            screen_info: None,
                            justified: false,
                            span: Span::dummy(),
                        },
                        HirDataItem {
                            name: "FIELD-B".into(),
                            data_type: HirType::Alphanumeric { size: 10 },
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            occurs: None,
                            indexed_by: Vec::new(),
                            redefines: None,
                            renames: None,
                            screen_info: None,
                            justified: false,
                            span: Span::dummy(),
                        },
                    ],
                    size: 15,
                },
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                justified: false,
                span: Span::dummy(),
            },
            HirDataItem {
                name: "WS-DST".into(),
                data_type: HirType::Group {
                    members: vec![
                        HirDataItem {
                            name: "FIELD-A".into(),
                            data_type: HirType::Numeric {
                                size: 5,
                                decimal_places: 0,
                                is_signed: false,
                            },
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            occurs: None,
                            indexed_by: Vec::new(),
                            redefines: None,
                            renames: None,
                            screen_info: None,
                            justified: false,
                            span: Span::dummy(),
                        },
                        HirDataItem {
                            name: "FIELD-C".into(),
                            data_type: HirType::Alphanumeric { size: 10 },
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            occurs: None,
                            indexed_by: Vec::new(),
                            redefines: None,
                            renames: None,
                            screen_info: None,
                            justified: false,
                            span: Span::dummy(),
                        },
                    ],
                    size: 15,
                },
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                justified: false,
                span: Span::dummy(),
            },
        ],
        communication_descriptions: Vec::new(),
        paragraphs: Vec::new(),
        body: vec![HirStatement::MoveCorresponding {
            from: cobol_hir::HirDataName::simple("WS-SRC"),
            to: cobol_hir::HirDataName::simple("WS-DST"),
            span: Span::dummy(),
        }],
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        using_params: Vec::new(),
        file_organizations: std::collections::HashMap::new(),
        file_assignments: std::collections::HashMap::new(),
        file_relative_keys: std::collections::HashMap::new(),
        file_status_vars: Vec::new(),
        declaratives: Vec::new(),
        file_records: std::collections::HashMap::new(),
        fd_record_aliases: std::collections::HashMap::new(),
        nested_programs: Vec::new(),
        span: Span::dummy(),
    };

    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("MOVE CORRESPONDING"),
        "Should have MOVE CORRESPONDING comment, got:\n{}",
        c_code
    );
    // FIELD-A is numeric in both groups → should generate qualified
    // store via cobol_store_numeric_display (group members are char[])
    assert!(
        c_code.contains("WS_DST__FIELD_A")
            && c_code.contains("WS_SRC__FIELD_A")
            && c_code.contains("cobol_store_numeric_display"),
        "Should move matching FIELD-A with qualified names via display store, got:\n{}",
        c_code
    );
    assert!(
        !c_code.contains("#define FIELD_A "),
        "Duplicate group members should not emit unqualified FIELD_A macros, got:\n{}",
        c_code
    );
    // FIELD-B and FIELD-C don't match → should NOT appear in MOVE section
    assert!(
        !c_code.contains("FIELD_B = ") && !c_code.contains("FIELD_C = "),
        "Should NOT move non-matching fields, got:\n{}",
        c_code
    );
}

#[test]
fn test_c2_add_corresponding() {
    use cobol_hir::HirDataItem;
    let hir = cobol_hir::HirProgram {
        name: "ADD-CORR-TEST".into(),
        data_items: vec![
            HirDataItem {
                name: "GRP-A".into(),
                data_type: HirType::Group {
                    members: vec![HirDataItem {
                        name: "AMT".into(),
                        data_type: HirType::Numeric {
                            size: 9,
                            decimal_places: 2,
                            is_signed: false,
                        },
                        picture: None,
                        is_numeric_edited: false,
                        blank_when_zero: false,
                        scale_adjustment: 0,
                        is_external: false,
                        initial_value: None,
                        occurs: None,
                        indexed_by: Vec::new(),
                        redefines: None,
                        renames: None,
                        screen_info: None,
                        justified: false,
                        span: Span::dummy(),
                    }],
                    size: 9,
                },
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                justified: false,
                span: Span::dummy(),
            },
            HirDataItem {
                name: "GRP-B".into(),
                data_type: HirType::Group {
                    members: vec![HirDataItem {
                        name: "AMT".into(),
                        data_type: HirType::Numeric {
                            size: 9,
                            decimal_places: 2,
                            is_signed: false,
                        },
                        picture: None,
                        is_numeric_edited: false,
                        blank_when_zero: false,
                        scale_adjustment: 0,
                        is_external: false,
                        initial_value: None,
                        occurs: None,
                        indexed_by: Vec::new(),
                        redefines: None,
                        renames: None,
                        screen_info: None,
                        justified: false,
                        span: Span::dummy(),
                    }],
                    size: 9,
                },
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                justified: false,
                span: Span::dummy(),
            },
        ],
        communication_descriptions: Vec::new(),
        paragraphs: Vec::new(),
        body: vec![HirStatement::AddCorresponding {
            from: cobol_hir::HirDataName::simple("GRP-A"),
            to: cobol_hir::HirDataName::simple("GRP-B"),
            on_size_error: Vec::new(),
            not_on_size_error: Vec::new(),
            span: Span::dummy(),
        }],
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        using_params: Vec::new(),
        file_organizations: std::collections::HashMap::new(),
        file_assignments: std::collections::HashMap::new(),
        file_relative_keys: std::collections::HashMap::new(),
        file_status_vars: Vec::new(),
        declaratives: Vec::new(),
        file_records: std::collections::HashMap::new(),
        fd_record_aliases: std::collections::HashMap::new(),
        nested_programs: Vec::new(),
        span: Span::dummy(),
    };

    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("ADD CORRESPONDING"),
        "Should have ADD CORRESPONDING comment, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("GRP_B.members._m_AMT") && c_code.contains("GRP_A.members._m_AMT"),
        "Should add matching AMT field via qualified member access, got:\n{}",
        c_code
    );
}

#[test]
fn test_c2_move_corresponding_uses_fully_qualified_nested_names() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-QUAL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC-ROOT.
   05 SRC-GRP.
      10 FIELD-A PIC 9(3) VALUE 111.
01 DST-ROOT.
   05 DST-GRP.
      10 FIELD-A PIC 9(3) VALUE 222.
PROCEDURE DIVISION.
    MOVE CORRESPONDING SRC-GRP OF SRC-ROOT TO DST-GRP OF DST-ROOT.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("SRC_ROOT__SRC_GRP__FIELD_A"),
        "nested corresponding should use fully qualified source name, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("DST_ROOT__DST_GRP__FIELD_A"),
        "nested corresponding should use fully qualified destination name, got:\n{}",
        c_code
    );
}

#[test]
fn test_c2_add_corresponding_skips_filler_members() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-FILLER.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 GRP-SRC.
   05 FILLER PIC 99 VALUE 11.
   05 KEEP-A PIC 99 VALUE 22.
01 GRP-DST.
   05 FILLER PIC 99 VALUE 33.
   05 KEEP-A PIC 99 VALUE 44.
PROCEDURE DIVISION.
    ADD CORRESPONDING GRP-SRC TO GRP-DST.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        !c_code.contains("GRP_DST__FILLER), 2) +") && !c_code.contains("GRP_SRC__FILLER), 2)"),
        "corresponding arithmetic must not target filler members in emitted operation, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("GRP_DST__KEEP_A"),
        "non-filler corresponding members should still be emitted, got:\n{}",
        c_code
    );
}

// ============================================================
// Native execution E2E tests for additional COBOL features
// ============================================================

#[test]
fn test_native_string_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. STRING-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-FIRST  PIC X(5) VALUE 'HELLO'.
01 WS-SECOND PIC X(6) VALUE ' WORLD'.
01 WS-RESULT PIC X(20) VALUE SPACES.
01 WS-PTR    PIC 9(2) VALUE 1.
PROCEDURE DIVISION.
    STRING WS-FIRST DELIMITED BY SIZE
           WS-SECOND DELIMITED BY SIZE
           INTO WS-RESULT
           WITH POINTER WS-PTR
    END-STRING.
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("HELLO WORLD"),
        "STRING should concatenate: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_inspect_tallying() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA  PIC X(10) VALUE 'AABBAACCAA'.
01 WS-COUNT PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-DATA TALLYING WS-COUNT
        FOR ALL 'A'.
    DISPLAY WS-COUNT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    assert!(
        trimmed.contains("006") || trimmed.contains("6"),
        "INSPECT TALLYING should count 6 A's: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_reference_modification() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. REFMOD-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SOURCE PIC X(10) VALUE 'ABCDEFGHIJ'.
01 WS-TARGET PIC X(5) VALUE SPACES.
PROCEDURE DIVISION.
    MOVE WS-SOURCE(3:5) TO WS-TARGET.
    DISPLAY WS-TARGET.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("CDEFG"),
        "Reference modification should extract 'CDEFG': got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_perform_thru() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PERF-THRU.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-RESULT PIC X(10) VALUE SPACES.
PROCEDURE DIVISION.
    PERFORM PARA-A THRU PARA-B.
    DISPLAY WS-RESULT.
    STOP RUN.
PARA-A.
    MOVE 'AB' TO WS-RESULT.
PARA-B.
    DISPLAY 'B-DONE'.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("B-DONE"),
        "PERFORM THRU should execute both paragraphs: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_divide_remainder() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DIVIDE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DIVIDEND  PIC 9(3) VALUE 17.
01 WS-DIVISOR   PIC 9(2) VALUE 5.
01 WS-QUOTIENT  PIC 9(3) VALUE 0.
01 WS-REMAINDER PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    DIVIDE WS-DIVIDEND BY WS-DIVISOR
        GIVING WS-QUOTIENT REMAINDER WS-REMAINDER.
    DISPLAY WS-QUOTIENT.
    DISPLAY WS-REMAINDER.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(
        lines.len() >= 2,
        "Should have 2 output lines, got: {:?}",
        lines
    );
    assert!(
        lines[0].trim().ends_with('3') || lines[0].contains("003"),
        "17/5 quotient should be 3: got '{}'",
        lines[0]
    );
    assert!(
        lines[1].trim().ends_with('2') || lines[1].contains("02"),
        "17%5 remainder should be 2: got '{}'",
        lines[1]
    );
}

#[test]
fn test_native_nested_perform_varying() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NESTED-VARY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-I PIC 9(2) VALUE 0.
01 WS-J PIC 9(2) VALUE 0.
01 WS-COUNT PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 1 BY 1 UNTIL WS-I > 3
        PERFORM VARYING WS-J FROM 1 BY 1 UNTIL WS-J > 4
            ADD 1 TO WS-COUNT
        END-PERFORM
    END-PERFORM.
    DISPLAY WS-COUNT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    assert!(
        trimmed.contains("012") || trimmed.ends_with("12"),
        "3*4=12 iterations: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_move_corresponding_run() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-RUN.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 FIELD-A PIC 9(3) VALUE 100.
   05 FIELD-B PIC X(5) VALUE 'HELLO'.
01 WS-DST.
   05 FIELD-A PIC 9(3) VALUE 0.
   05 FIELD-C PIC 9(3) VALUE 999.
PROCEDURE DIVISION.
    MOVE CORRESPONDING WS-SRC TO WS-DST.
    DISPLAY FIELD-A OF WS-DST.
    DISPLAY FIELD-C OF WS-DST.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(lines.len() >= 2, "Should have 2 lines: {:?}", lines);
    assert!(
        lines[0].contains("100"),
        "FIELD-A should be 100: got '{}'",
        lines[0]
    );
    assert!(
        lines[1].contains("999"),
        "FIELD-C should remain 999: got '{}'",
        lines[1]
    );
}

#[test]
fn test_qualified_display_lowers_to_data_ref() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. QUAL-HIR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 FIELD-A PIC 9(3) VALUE 111.
01 WS-DST.
   05 FIELD-A PIC 9(3) VALUE 222.
PROCEDURE DIVISION.
    DISPLAY FIELD-A OF WS-DST.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    match &hir.body[0] {
        HirStatement::Display { operands, .. } => match &operands[0] {
            cobol_hir::HirExpr::DataRef(data_ref) => {
                assert_eq!(data_ref.name.name.as_str(), "FIELD-A");
                assert_eq!(data_ref.name.qualifiers, vec!["WS-DST"]);
                assert!(data_ref.subscripts.is_empty());
                assert!(data_ref.refmod.is_none());
            }
            other => panic!("expected DataRef, got {:?}", other),
        },
        other => panic!("expected Display, got {:?}", other),
    }
}

#[test]
fn test_native_qualified_display_with_duplicate_member_names() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. QUAL-DISPLAY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 FIELD-A PIC 9(3) VALUE 111.
01 WS-DST.
   05 FIELD-A PIC 9(3) VALUE 222.
PROCEDURE DIVISION.
    DISPLAY FIELD-A OF WS-DST.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("WS_DST__FIELD_A"),
        "qualified display should use the resolved group member, got:\n{}",
        c_code
    );

    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.trim().contains("222"),
        "qualified display should resolve WS-DST.FIELD-A, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_qualified_subscripted_display_lowers_to_data_ref() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. QUAL-SUB-HIR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 ITEM-GRP OCCURS 2 TIMES.
      10 FIELD-A PIC 9(3) VALUE 111.
01 WS-DST.
   05 ITEM-GRP OCCURS 2 TIMES.
      10 FIELD-A PIC 9(3) VALUE 222.
PROCEDURE DIVISION.
    DISPLAY FIELD-A OF ITEM-GRP(2) OF WS-DST.
    STOP RUN.
";
    let hir = compile_to_hir(src);
    match &hir.body[0] {
        HirStatement::Display { operands, .. } => match &operands[0] {
            cobol_hir::HirExpr::DataRef(data_ref) => {
                assert_eq!(data_ref.name.name.as_str(), "FIELD-A");
                assert_eq!(data_ref.name.qualifiers, vec!["ITEM-GRP", "WS-DST"]);
                assert_eq!(data_ref.subscripts.len(), 1);
            }
            other => panic!("expected DataRef, got {:?}", other),
        },
        other => panic!("expected Display, got {:?}", other),
    }
}

#[test]
fn test_native_qualified_subscripted_display_with_duplicate_member_names() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. QUAL-SUB-DISPLAY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 ITEM-GRP OCCURS 2 TIMES.
      10 FIELD-A PIC 9(3).
01 WS-DST.
   05 ITEM-GRP OCCURS 2 TIMES.
      10 FIELD-A PIC 9(3).
PROCEDURE DIVISION.
    MOVE 111 TO FIELD-A OF ITEM-GRP(1) OF WS-SRC.
    MOVE 222 TO FIELD-A OF ITEM-GRP(2) OF WS-SRC.
    MOVE 333 TO FIELD-A OF ITEM-GRP(1) OF WS-DST.
    MOVE 444 TO FIELD-A OF ITEM-GRP(2) OF WS-DST.
    DISPLAY FIELD-A OF ITEM-GRP(2) OF WS-DST.
    DISPLAY FIELD-A OF ITEM-GRP(1) OF WS-SRC.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("WS_DST__ITEM_GRP__FIELD_A"),
        "fully qualified nested macro should be emitted, got:\n{}",
        c_code
    );
    assert!(
        !c_code.contains("#define ITEM_GRP__FIELD_A "),
        "ambiguous partial qualification should not be emitted, got:\n{}",
        c_code
    );

    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 output lines, got '{}'", stdout);
    assert!(
        lines[0].contains("444"),
        "qualified subscripted display should resolve WS-DST item 2, got '{}'",
        lines[0]
    );
    assert!(
        lines[1].contains("111"),
        "qualified subscripted display should resolve WS-SRC item 1, got '{}'",
        lines[1]
    );
}

#[test]
fn test_native_unstring_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. UNSTRING-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-INPUT  PIC X(20) VALUE 'HELLO,WORLD,BYE'.
01 WS-PART1  PIC X(10) VALUE SPACES.
01 WS-PART2  PIC X(10) VALUE SPACES.
01 WS-PART3  PIC X(10) VALUE SPACES.
PROCEDURE DIVISION.
    UNSTRING WS-INPUT DELIMITED BY ','
        INTO WS-PART1 WS-PART2 WS-PART3.
    DISPLAY WS-PART1.
    DISPLAY WS-PART2.
    DISPLAY WS-PART3.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(lines.len() >= 3, "Should have 3 lines: {:?}", lines);
    assert!(
        lines[0].contains("HELLO"),
        "Part1 should be HELLO: got '{}'",
        lines[0]
    );
    assert!(
        lines[1].contains("WORLD"),
        "Part2 should be WORLD: got '{}'",
        lines[1]
    );
    assert!(
        lines[2].contains("BYE"),
        "Part3 should be BYE: got '{}'",
        lines[2]
    );
}

#[test]
fn test_native_perform_thru_with_goto_inside_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRU-GOTO-RANGE.
PROCEDURE DIVISION.
    PERFORM PARA-A THRU PARA-C.
    DISPLAY 'DONE'.
    STOP RUN.
PARA-A.
    DISPLAY 'A'.
    GO TO PARA-C.
    DISPLAY 'BAD-A'.
PARA-B.
    DISPLAY 'BAD-B'.
PARA-C.
    DISPLAY 'C'.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 3, "expected A/C/DONE only, got '{stdout}'");
    assert_eq!(lines[0].trim(), "A");
    assert_eq!(lines[1].trim(), "C");
    assert_eq!(lines[2].trim(), "DONE");
}

#[test]
fn test_native_perform_thru_sections_do_not_duplicate_child_paragraphs() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRU-SECTION.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 P-COUNT PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    PERFORM B THRU D.
    ADD 1 TO P-COUNT.
    PERFORM A THRU C.
    ADD 1 TO P-COUNT.
    PERFORM A THRU D.
    ADD 1 TO P-COUNT.
    PERFORM B THRU C.
    ADD 1 TO P-COUNT.
    DISPLAY P-COUNT.
    STOP RUN.
A SECTION.
B.
    ADD 100 TO P-COUNT.
C SECTION.
D.
    ADD 10000 TO P-COUNT.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("40404"),
        "PERFORM THRU should not duplicate paragraphs owned by selected sections: got '{stdout}'"
    );
}

#[test]
fn test_native_reversed_perform_thru_keeps_goto_return() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRU-GOTO-RETURN.
PROCEDURE DIVISION.
MAIN.
    PERFORM G THRU B.
    DISPLAY 'OK'.
    STOP RUN.
A SECTION.
B.
    DISPLAY 'PASS'.
C.
    DISPLAY 'BAD-C'.
E.
    GO TO L.
F.
    DISPLAY 'BAD-F'.
G SECTION.
H.
    GO TO E.
I.
    DISPLAY 'BAD-I'.
J SECTION.
K.
    DISPLAY 'BAD-K'.
L.
    GO TO B.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().map(str::trim).collect();
    assert_eq!(lines, vec!["PASS", "OK"], "unexpected output: '{stdout}'");
}

#[test]
fn test_native_perform_times_evaluates_count_once() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TIMES-COUNT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 N PIC 9(3) VALUE 7.
PROCEDURE DIVISION.
    PERFORM STEP-PARA N TIMES.
    DISPLAY N.
    STOP RUN.
STEP-PARA.
    ADD 100 TO N.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("707"),
        "PERFORM TIMES should evaluate the count once before looping: got '{stdout}'"
    );
}

#[test]
fn test_native_numeric_edited_move_and_compare() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NUMERIC-EDITED.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 EDITED PIC $9,9B9.90+.
01 EXPECTED PIC X(10) VALUE '$1,2 3.40+'.
PROCEDURE DIVISION.
    MOVE +123.4 TO EDITED.
    IF EDITED = EXPECTED
        DISPLAY 'PASS'
    ELSE
        DISPLAY EDITED
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_alphanumeric_edited_move_and_compare() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ALPHA-EDITED.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 EDITED PIC ABABX0A.
01 EXPECTED PIC X(7) VALUE 'A C D0E'.
PROCEDURE DIVISION.
    MOVE 'ACDE' TO EDITED.
    IF EDITED = EXPECTED
        DISPLAY 'PASS'
    ELSE
        DISPLAY EDITED
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_numeric_to_alphanumeric_compare_uses_display_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NUM-ALPHA-CMP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 N PIC S9(18).
01 X PIC X(18).
PROCEDURE DIVISION.
    MOVE 111111111111111111 TO N.
    MOVE '111111111111111111' TO X.
    IF N = X
        DISPLAY 'PASS'
    ELSE
        DISPLAY 'FAIL'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_quote_move_fills_group_member_without_null_slot() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. QUOTE-GROUP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 G.
   02 A PIC X(2).
   02 B PIC X(2).
PROCEDURE DIVISION.
    MOVE QUOTES TO A.
    MOVE QUOTES TO B.
    IF G = '""""'
        DISPLAY 'PASS'
    ELSE
        DISPLAY 'FAIL'
    END-IF.
    STOP RUN.
"#;
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_complex_condition() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COND-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9(3) VALUE 50.
01 WS-B PIC 9(3) VALUE 100.
01 WS-C PIC 9(3) VALUE 75.
PROCEDURE DIVISION.
    IF WS-A < WS-B AND WS-C > WS-A
        DISPLAY 'BOTH-TRUE'
    END-IF.
    IF WS-A > WS-B OR WS-C < WS-A
        DISPLAY 'SHOULD-NOT-SHOW'
    ELSE
        DISPLAY 'OR-FALSE'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("BOTH-TRUE"),
        "AND condition should be true: got '{}'",
        stdout.trim()
    );
    assert!(
        stdout.contains("OR-FALSE"),
        "OR condition should be false: got '{}'",
        stdout.trim()
    );
    assert!(
        !stdout.contains("SHOULD-NOT-SHOW"),
        "OR-true branch should not execute"
    );
}

#[test]
fn test_native_accept_from_date() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DATE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATE PIC 9(8) VALUE 0.
PROCEDURE DIVISION.
    ACCEPT WS-DATE FROM DATE YYYYMMDD.
    DISPLAY WS-DATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    // Date should be 8 digits starting with 20xx
    assert!(
        trimmed.len() >= 8,
        "Date should be at least 8 digits: got '{}'",
        trimmed
    );
    assert!(
        trimmed.starts_with("20"),
        "Date should start with 20xx: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_evaluate_with_values() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVAL-VAL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-CODE PIC 9(2) VALUE 2.
PROCEDURE DIVISION.
    EVALUATE WS-CODE
        WHEN 1
            DISPLAY 'ONE'
        WHEN 2
            DISPLAY 'TWO'
        WHEN 3
            DISPLAY 'THREE'
        WHEN OTHER
            DISPLAY 'OTHER'
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("TWO"),
        "EVALUATE WS-CODE=2 should display TWO: got '{}'",
        stdout.trim()
    );
    assert!(
        !stdout.contains("ONE") && !stdout.contains("THREE") && !stdout.contains("OTHER"),
        "Should only display TWO"
    );
}

#[test]
fn test_native_inspect_replacing() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-REPL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(10) VALUE 'AABBAACCAA'.
PROCEDURE DIVISION.
    INSPECT WS-DATA REPLACING ALL 'A' BY 'X'.
    DISPLAY WS-DATA.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("XXBBXXCCXX"),
        "INSPECT REPLACING should replace A with X: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_perform_paragraph_with_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PARA-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-I PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    PERFORM SHOW-MSG 3 TIMES.
    STOP RUN.
SHOW-MSG.
    ADD 1 TO WS-I.
    DISPLAY WS-I.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 3, "Should display 3 times: got {:?}", lines);
}

#[test]
fn test_native_file_io_write_read() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FILE-IO-TEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO '/tmp/cobol_test_io.dat'
        ORGANIZATION IS LINE SEQUENTIAL
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD TEST-FILE.
01 TEST-RECORD PIC X(20).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX VALUE SPACES.
01 WS-READ-REC PIC X(20) VALUE SPACES.
PROCEDURE DIVISION.
    OPEN OUTPUT TEST-FILE.
    MOVE 'HELLO FROM COBOL' TO TEST-RECORD.
    WRITE TEST-RECORD.
    CLOSE TEST-FILE.
    OPEN INPUT TEST-FILE.
    READ TEST-FILE INTO WS-READ-REC.
    CLOSE TEST-FILE.
    DISPLAY WS-READ-REC.
    STOP RUN.
";
    // Clean up any leftover file
    let _ = std::fs::remove_file("/tmp/cobol_test_io.dat");
    let (stdout, _, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_test_io.dat");
    assert_eq!(code, 0, "File I/O test should exit 0");
    assert!(
        stdout.contains("HELLO FROM COBOL"),
        "Should read back written record: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_go_to_paragraph() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GOTO-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
PROCEDURE DIVISION.
    DISPLAY 'START'.
    GO TO SKIP-HERE.
    DISPLAY 'SHOULD-NOT-SHOW'.
SKIP-HERE.
    DISPLAY 'END'.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("START"), "Should show START");
    assert!(stdout.contains("END"), "Should show END");
    assert!(
        !stdout.contains("SHOULD-NOT-SHOW"),
        "Should skip middle paragraph"
    );
}

#[test]
fn test_native_compute_complex() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9(3) VALUE 10.
01 WS-B PIC 9(3) VALUE 3.
01 WS-RESULT PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    COMPUTE WS-RESULT = (WS-A + WS-B) * (WS-A - WS-B).
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    // (10+3)*(10-3) = 13*7 = 91
    assert!(
        trimmed.contains("91"),
        "(10+3)*(10-3) should be 91: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_88_level_condition() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LEVEL88-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-STATUS PIC 9(1) VALUE 1.
   88 IS-ACTIVE VALUE 1.
   88 IS-INACTIVE VALUE 0.
PROCEDURE DIVISION.
    IF IS-ACTIVE
        DISPLAY 'ACTIVE'
    END-IF.
    MOVE 0 TO WS-STATUS.
    IF IS-INACTIVE
        DISPLAY 'INACTIVE'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("ACTIVE"),
        "88 level IS-ACTIVE should be true: got '{}'",
        stdout.trim()
    );
    assert!(
        stdout.contains("INACTIVE"),
        "88 level IS-INACTIVE should be true after MOVE 0: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_initialize_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INIT-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(5) VALUE 12345.
01 WS-STR PIC X(10) VALUE 'ABCDEFGHIJ'.
PROCEDURE DIVISION.
    INITIALIZE WS-NUM.
    INITIALIZE WS-STR.
    DISPLAY WS-NUM.
    DISPLAY WS-STR.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        !lines.is_empty(),
        "Should have at least 1 line: {:?}",
        lines
    );
    // Numeric initialized to 0
    assert!(
        lines[0].contains('0') && !lines[0].contains("12345"),
        "Numeric should be initialized to 0: got '{}'",
        lines[0]
    );
    // Alphanumeric initialized to spaces — second line may be all spaces or empty
    if lines.len() >= 2 {
        assert!(
            lines[1].trim().is_empty() || lines[1].chars().all(|c| c == ' '),
            "Alpha should be initialized to spaces: got '{}'",
            lines[1]
        );
    }
}

#[test]
fn test_native_all_figurative_constant() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ALL-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-FIELD PIC X(5) VALUE SPACES.
PROCEDURE DIVISION.
    MOVE ALL '*' TO WS-FIELD.
    DISPLAY WS-FIELD.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("*****"),
        "ALL '*' should fill with asterisks: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_exit_paragraph() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EXIT-PARA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-FLAG PIC 9(1) VALUE 1.
PROCEDURE DIVISION.
    PERFORM CHECK-FLAG.
    DISPLAY 'DONE'.
    STOP RUN.
CHECK-FLAG.
    IF WS-FLAG = 1
        DISPLAY 'FLAG-IS-1'
        EXIT PARAGRAPH
    END-IF.
    DISPLAY 'SHOULD-NOT-SHOW'.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("FLAG-IS-1"),
        "Should display FLAG-IS-1: got '{}'",
        stdout.trim()
    );
    assert!(
        stdout.contains("DONE"),
        "Should display DONE: got '{}'",
        stdout.trim()
    );
    assert!(
        !stdout.contains("SHOULD-NOT-SHOW"),
        "EXIT PARAGRAPH should skip remaining statements"
    );
}

#[test]
fn test_native_evaluate_true_conditions() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVAL-TRUE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SCORE PIC 9(3) VALUE 85.
PROCEDURE DIVISION.
    EVALUATE TRUE
        WHEN WS-SCORE >= 90
            DISPLAY 'A'
        WHEN WS-SCORE >= 80
            DISPLAY 'B'
        WHEN WS-SCORE >= 70
            DISPLAY 'C'
        WHEN OTHER
            DISPLAY 'F'
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    assert_eq!(
        trimmed, "B",
        "Score 85 should get grade B: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_subtract_from() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SUB-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-TOTAL PIC 9(5) VALUE 1000.
01 WS-AMT   PIC 9(3) VALUE 250.
PROCEDURE DIVISION.
    SUBTRACT WS-AMT FROM WS-TOTAL.
    DISPLAY WS-TOTAL.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("750"),
        "1000-250 should be 750: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_add_giving() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-GIVING.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9(3) VALUE 100.
01 WS-B PIC 9(3) VALUE 200.
01 WS-C PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    ADD WS-A WS-B GIVING WS-C.
    DISPLAY WS-C.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("300"),
        "100+200 should be 300: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_multiply_giving() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MUL-GIVING.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9(3) VALUE 15.
01 WS-B PIC 9(3) VALUE 20.
01 WS-C PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    MULTIPLY WS-A BY WS-B GIVING WS-C.
    DISPLAY WS-C.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("300"),
        "15*20 should be 300: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_array_loop_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ARRAY-LOOP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-TABLE.
   05 WS-ITEM PIC 9(3) OCCURS 5 TIMES.
01 WS-I PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 1 BY 1 UNTIL WS-I > 5
        COMPUTE WS-ITEM(WS-I) = WS-I * 10
    END-PERFORM.
    PERFORM VARYING WS-I FROM 1 BY 1 UNTIL WS-I > 5
        DISPLAY WS-ITEM(WS-I)
    END-PERFORM.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 5, "Should have 5 lines: {:?}", lines);
    assert!(lines[0].contains("10"), "Item 1: got '{}'", lines[0]);
    assert!(lines[1].contains("20"), "Item 2: got '{}'", lines[1]);
    assert!(lines[2].contains("30"), "Item 3: got '{}'", lines[2]);
    assert!(lines[3].contains("40"), "Item 4: got '{}'", lines[3]);
    assert!(lines[4].contains("50"), "Item 5: got '{}'", lines[4]);
}

#[test]
fn test_native_move_literal_to_alphanumeric() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOVE-LIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(15) VALUE SPACES.
PROCEDURE DIVISION.
    MOVE 'COBOL ROCKS' TO WS-NAME.
    DISPLAY WS-NAME.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("COBOL ROCKS"),
        "Should display 'COBOL ROCKS': got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_not_condition() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NOT-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9(3) VALUE 5.
PROCEDURE DIVISION.
    IF NOT WS-A = 10
        DISPLAY 'NOT-EQUAL'
    END-IF.
    IF NOT WS-A > 10
        DISPLAY 'NOT-GREATER'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("NOT-EQUAL"),
        "NOT 5=10 should be true: got '{}'",
        stdout.trim()
    );
    assert!(
        stdout.contains("NOT-GREATER"),
        "NOT 5>10 should be true: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_perform_until_with_paragraph() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. UNTIL-PARA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-COUNT PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    PERFORM INCREMENT-COUNT UNTIL WS-COUNT >= 5.
    DISPLAY WS-COUNT.
    STOP RUN.
INCREMENT-COUNT.
    ADD 1 TO WS-COUNT.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let trimmed = stdout.trim();
    assert!(
        trimmed.ends_with('5') || trimmed.contains("005"),
        "Should count to 5: got '{}'",
        trimmed
    );
}

#[test]
fn test_native_sales_report() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SALES-RPT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-ITEMS.
   05 WS-PRICE PIC 9(5) OCCURS 3 TIMES.
01 WS-QTY.
   05 WS-QUANTITY PIC 9(3) OCCURS 3 TIMES.
01 WS-TOTAL PIC 9(8) VALUE 0.
01 WS-LINE-TOTAL PIC 9(8) VALUE 0.
01 WS-I PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    MOVE 100 TO WS-PRICE(1).
    MOVE 250 TO WS-PRICE(2).
    MOVE 50 TO WS-PRICE(3).
    MOVE 10 TO WS-QUANTITY(1).
    MOVE 5 TO WS-QUANTITY(2).
    MOVE 20 TO WS-QUANTITY(3).
    PERFORM VARYING WS-I FROM 1 BY 1 UNTIL WS-I > 3
        COMPUTE WS-LINE-TOTAL =
            WS-PRICE(WS-I) * WS-QUANTITY(WS-I)
        ADD WS-LINE-TOTAL TO WS-TOTAL
    END-PERFORM.
    DISPLAY WS-TOTAL.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    // 100*10 + 250*5 + 50*20 = 1000 + 1250 + 1000 = 3250
    assert!(
        stdout.contains("3250"),
        "Total should be 3250: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_string_padding() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PAD-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SHORT PIC X(3) VALUE 'AB'.
01 WS-LONG  PIC X(10) VALUE SPACES.
PROCEDURE DIVISION.
    MOVE WS-SHORT TO WS-LONG.
    DISPLAY '>' WS-LONG '<'.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    // COBOL pads with spaces on the right
    assert!(
        stdout.contains(">AB"),
        "Should start with >AB: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_fibonacci() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FIBONACCI.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-N PIC 9(2) VALUE 10.
01 WS-A PIC 9(8) VALUE 0.
01 WS-B PIC 9(8) VALUE 1.
01 WS-TEMP PIC 9(8) VALUE 0.
01 WS-I PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    PERFORM VARYING WS-I FROM 2 BY 1 UNTIL WS-I > WS-N
        COMPUTE WS-TEMP = WS-A + WS-B
        MOVE WS-B TO WS-A
        MOVE WS-TEMP TO WS-B
    END-PERFORM.
    DISPLAY WS-B.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    // fib(10) = 55
    assert!(
        stdout.contains("55"),
        "fib(10) should be 55: got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_native_bubble_sort() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. BUBBLE-SORT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-TABLE.
   05 WS-ITEM PIC 9(3) OCCURS 5 TIMES.
01 WS-I PIC 9(2) VALUE 0.
01 WS-J PIC 9(2) VALUE 0.
01 WS-TEMP PIC 9(3) VALUE 0.
01 WS-SWAPPED PIC 9(1) VALUE 0.
PROCEDURE DIVISION.
    MOVE 50 TO WS-ITEM(1).
    MOVE 20 TO WS-ITEM(2).
    MOVE 40 TO WS-ITEM(3).
    MOVE 10 TO WS-ITEM(4).
    MOVE 30 TO WS-ITEM(5).
    MOVE 1 TO WS-SWAPPED.
    PERFORM UNTIL WS-SWAPPED = 0
        MOVE 0 TO WS-SWAPPED
        PERFORM VARYING WS-I FROM 1 BY 1
            UNTIL WS-I > 4
            COMPUTE WS-J = WS-I + 1
            IF WS-ITEM(WS-I) > WS-ITEM(WS-J)
                MOVE WS-ITEM(WS-I) TO WS-TEMP
                COMPUTE WS-ITEM(WS-I) = WS-ITEM(WS-J)
                COMPUTE WS-ITEM(WS-J) = WS-TEMP
                MOVE 1 TO WS-SWAPPED
            END-IF
        END-PERFORM
    END-PERFORM.
    PERFORM VARYING WS-I FROM 1 BY 1 UNTIL WS-I > 5
        DISPLAY WS-ITEM(WS-I)
    END-PERFORM.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 5, "Should have 5 sorted items: {:?}", lines);
    // Should be sorted: 10, 20, 30, 40, 50
    assert!(lines[0].contains("10"), "First: got '{}'", lines[0]);
    assert!(lines[1].contains("20"), "Second: got '{}'", lines[1]);
    assert!(lines[2].contains("30"), "Third: got '{}'", lines[2]);
    assert!(lines[3].contains("40"), "Fourth: got '{}'", lines[3]);
    assert!(lines[4].contains("50"), "Fifth: got '{}'", lines[4]);
}

#[test]
fn test_validate_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(5) VALUE 12345.
PROCEDURE DIVISION.
    VALIDATE WS-NUM.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_validate"),
        "should generate validate call: {}",
        c_code
    );
}

#[test]
fn test_xml_generate_parse() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. XMLTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(100).
01 WS-XML PIC X(500).
PROCEDURE DIVISION.
    XML GENERATE WS-XML FROM WS-DATA
    END-XML.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_xml_generate"),
        "should generate xml_generate call: {}",
        c_code
    );
}

#[test]
fn test_xml_parse_statement() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. XMLPARSE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-XML PIC X(500).
PROCEDURE DIVISION.
MAIN-PARA.
    XML PARSE WS-XML PROCESSING PROCEDURE XML-HANDLER
    END-XML.
    STOP RUN.
XML-HANDLER.
    DISPLAY \"XML EVENT\".
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_xml_parse"),
        "should generate xml_parse call: {}",
        c_code
    );
}

#[test]
fn test_json_generate_parse() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. JSONTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(100).
01 WS-JSON PIC X(200).
PROCEDURE DIVISION.
    JSON GENERATE WS-JSON FROM WS-DATA
    END-JSON.
    JSON PARSE WS-JSON INTO WS-DATA
    END-JSON.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_json_generate"),
        "should generate json_generate call: {}",
        c_code
    );
    assert!(
        c_code.contains("cobol_json_parse"),
        "should generate json_parse call: {}",
        c_code
    );
}

#[test]
fn test_native_date_functions() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DATETEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATE PIC 9(8) VALUE 20260319.
01 WS-INT PIC 9(9).
01 WS-RESULT PIC 9(8).
01 WS-VALID PIC 9.
PROCEDURE DIVISION.
    COMPUTE WS-INT = FUNCTION INTEGER-OF-DATE(WS-DATE).
    COMPUTE WS-RESULT = FUNCTION DATE-OF-INTEGER(WS-INT).
    DISPLAY WS-RESULT.
    COMPUTE WS-VALID = FUNCTION TEST-DATE-YYYYMMDD(WS-DATE).
    DISPLAY WS-VALID.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(
        lines[0].contains("20260319"),
        "DATE-OF-INTEGER roundtrip should return 20260319, got: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("0"),
        "TEST-DATE should return 0 for valid date, got: {}",
        lines[1]
    );
}

#[test]
fn test_native_math_functions() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MATHTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC S9(9) VALUE 42.
01 WS-RESULT PIC S9(9).
PROCEDURE DIVISION.
    COMPUTE WS-RESULT = FUNCTION ABS(WS-NUM).
    DISPLAY WS-RESULT.
    COMPUTE WS-RESULT = FUNCTION FACTORIAL(5).
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert!(
        lines[0].contains("42"),
        "ABS(42) should be 42, got: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("120"),
        "FACTORIAL(5) should be 120, got: {}",
        lines[1]
    );
}

#[test]
fn test_nist_if101a_intrinsic_acos_zero_is_in_expected_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IF101A-SMALL.
PROCEDURE DIVISION.
    IF FUNCTION ACOS(0) > 1
       AND FUNCTION ACOS(0) < 2
        DISPLAY 'OK'
    ELSE
        DISPLAY 'BAD'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "OK",
        "ACOS(0) should fall between 1 and 2 radians, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_renames_clause() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RENTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-RECORD.
   05 WS-FIELD-A PIC X(10) VALUE \"HELLO\".
   05 WS-FIELD-B PIC X(10) VALUE \"WORLD\".
66 WS-ALIAS RENAMES WS-FIELD-A.
PROCEDURE DIVISION.
    DISPLAY WS-ALIAS.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("WS_ALIAS") || c_code.contains("ws_alias"),
        "should reference RENAMES alias: {}",
        c_code
    );
    // The RENAMES item should generate a #define, not a separate variable
    assert!(
        c_code.contains("#define WS_ALIAS"),
        "RENAMES should generate a #define macro: {}",
        c_code
    );
}

#[test]
fn test_native_file_status_sequential() {
    let _ = std::fs::remove_file("/tmp/cobol_fs_test.dat");
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FSTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO '/tmp/cobol_fs_test.dat'
        ORGANIZATION IS LINE SEQUENTIAL
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD TEST-FILE.
01 TEST-RECORD PIC X(20).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
01 WS-DATA PIC X(20).
PROCEDURE DIVISION.
    OPEN OUTPUT TEST-FILE.
    DISPLAY WS-STATUS.
    MOVE 'HELLO COBOL' TO TEST-RECORD.
    WRITE TEST-RECORD.
    DISPLAY WS-STATUS.
    CLOSE TEST-FILE.
    DISPLAY WS-STATUS.
    OPEN INPUT TEST-FILE.
    READ TEST-FILE INTO WS-DATA.
    DISPLAY WS-STATUS.
    DISPLAY WS-DATA.
    READ TEST-FILE INTO WS-DATA.
    DISPLAY WS-STATUS.
    CLOSE TEST-FILE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_fs_test.dat");
    assert_eq!(code, 0, "stderr: {}", stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    // Status 00 for successful operations
    assert!(
        lines[0].starts_with("00"),
        "OPEN status should be 00, got: {}",
        lines[0]
    );
    assert!(
        lines[1].starts_with("00"),
        "WRITE status should be 00, got: {}",
        lines[1]
    );
    assert!(
        lines[2].starts_with("00"),
        "CLOSE status should be 00, got: {}",
        lines[2]
    );
    assert!(
        lines[3].starts_with("00"),
        "READ status should be 00, got: {}",
        lines[3]
    );
    // Line 4 should contain "HELLO COBOL"
    assert!(
        lines[4].contains("HELLO COBOL"),
        "read data should contain HELLO COBOL, got: {}",
        lines[4]
    );
    // Status 10 for end-of-file on second read
    assert!(
        lines[5].starts_with("10"),
        "EOF status should be 10, got: {}",
        lines[5]
    );
}

#[test]
fn test_native_file_write_read() {
    let _ = std::fs::remove_file("/tmp/cobol_fwr_test.dat");
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FWRTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO '/tmp/cobol_fwr_test.dat'
        ORGANIZATION IS LINE SEQUENTIAL.
DATA DIVISION.
FILE SECTION.
FD TEST-FILE.
01 TEST-RECORD PIC X(20).
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(20).
PROCEDURE DIVISION.
    OPEN OUTPUT TEST-FILE.
    MOVE 'FILE IO WORKS' TO TEST-RECORD.
    WRITE TEST-RECORD.
    CLOSE TEST-FILE.
    OPEN INPUT TEST-FILE.
    READ TEST-FILE INTO WS-DATA.
    DISPLAY WS-DATA.
    CLOSE TEST-FILE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_fwr_test.dat");
    assert_eq!(code, 0);
    assert!(
        stdout.contains("FILE IO WORKS"),
        "should read back written data, got: {}",
        stdout
    );
}

#[test]
fn test_nist_ix111a_open_missing_indexed_file_sets_status_35() {
    let path = format!(
        "/tmp/cobol_ix_missing_{}_{}.dat",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    );
    let _ = std::fs::remove_file(&path);
    let src = format!(
        "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IX111A-SMALL.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-NOP ASSIGN TO '{path}'
        ORGANIZATION IS INDEXED
        ACCESS MODE IS SEQUENTIAL
        RECORD KEY IS IX-NOP-KEY
        FILE STATUS IS IX-NOP-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-NOP.
01 IX-NOP-REC.
   05 IX-NOP-KEY PIC X(10).
   05 IX-NOP-DATA PIC X(10).
WORKING-STORAGE SECTION.
01 IX-NOP-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN INPUT IX-NOP.
    DISPLAY IX-NOP-STATUS.
    STOP RUN.
"
    );
    let (stdout, _, code) = compile_and_run_no_sema(&src);
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0);
    assert!(
        stdout.trim().starts_with("35"),
        "missing indexed file should report status 35, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_rounded_preserves_fractional_result() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-SMALL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-RESULT PIC 9V99 VALUE 0.
PROCEDURE DIVISION.
    MOVE 1.25 TO WS-RESULT.
    MULTIPLY 2 BY WS-RESULT ROUNDED.
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("2.50") || stdout.contains("250"),
        "1.25 * 2 with ROUNDED should keep fractional result, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_by_decimal_target_compares_scaled_result() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-BY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 MULTIPLY-DATA.
   02 MULT1 PIC 999V99 VALUE 80.12.
   02 MULT5 PIC 9 VALUE 4.
PROCEDURE DIVISION.
    MOVE 80.12 TO MULT1.
    MOVE 4 TO MULT5.
    MULTIPLY MULT5 BY MULT1.
    IF MULT1 EQUAL TO 320.48
        DISPLAY \"PASS\"
    ELSE
        DISPLAY MULT1
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "80.12 * 4 should compare equal to 320.48, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_rounded_display_numeric_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-ROUNDED.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 MULTIPLY-DATA.
   02 MULT4 PIC S99 VALUE -56.
PROCEDURE DIVISION.
    MULTIPLY -1.3 BY MULT4 ROUNDED.
    DISPLAY MULT4.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("73"),
        "-56 * -1.3 ROUNDED should store 73, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_decimal_operand_preserves_integer_target_digits() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-WIDE.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 WRK-DS-18V00 PIC S9(18).
77 A06THREES-DS-03V03 PIC S999V999 VALUE 333.333.
PROCEDURE DIVISION.
    MOVE 222222222222 TO WRK-DS-18V00.
    MULTIPLY A06THREES-DS-03V03 BY WRK-DS-18V00.
    IF WRK-DS-18V00 EQUAL TO 000074073999999925
        DISPLAY \"PASS\"
    ELSE
        DISPLAY WRK-DS-18V00
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "222222222222 * 333.333 should keep the integer target width, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_rounded_decimal_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-ROUNDED-DECIMAL.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 WRK-DS-06V06 PIC S9(6)V9(6).
77 A08TWOS-DS-02V06 PIC S99V9(6) VALUE 22.222222.
PROCEDURE DIVISION.
    MOVE A08TWOS-DS-02V06 TO WRK-DS-06V06.
    MULTIPLY 0.4 BY WRK-DS-06V06 ROUNDED.
    IF WRK-DS-06V06 EQUAL TO 8.888889
        DISPLAY \"PASS\"
    ELSE
        DISPLAY WRK-DS-06V06
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "22.222222 * 0.4 ROUNDED should store 8.888889, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_scaled_p_target_by_comp_decimal() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-P.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 WRK-DS-0201P PIC S99P.
77 A01ONE-CS-00V01 PIC SV9 COMPUTATIONAL VALUE .1.
77 WRK-DS-05V00 PIC S9(5).
PROCEDURE DIVISION.
    MOVE -990 TO WRK-DS-0201P.
    MULTIPLY A01ONE-CS-00V01 BY WRK-DS-0201P.
    MOVE WRK-DS-0201P TO WRK-DS-05V00.
    IF WRK-DS-05V00 EQUAL TO -00090
        DISPLAY \"PASS\"
    ELSE
        DISPLAY WRK-DS-05V00
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "S99P target multiplied by .1 should move out as -90, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_leading_p_decimal_operand() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-LEADING-P.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 WRK-CS-18V00 PIC S9(18) COMPUTATIONAL.
77 WRK-DU-18V00 PIC 9(18).
77 A18ONES-DS-18V00 PIC S9(18) VALUE 111111111111111111.
77 A01ONE-DS-P0801 PIC SP(8)9 VALUE .000000001.
PROCEDURE DIVISION.
    MOVE A18ONES-DS-18V00 TO WRK-CS-18V00.
    MULTIPLY A01ONE-DS-P0801 BY WRK-CS-18V00.
    MOVE WRK-CS-18V00 TO WRK-DU-18V00.
    IF WRK-DU-18V00 EQUAL TO 000000000111111111
        DISPLAY \"PASS\"
    ELSE
        DISPLAY WRK-DU-18V00
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "S9(18) target multiplied by SP(8)9 should keep 111111111, got '{}'",
        stdout.trim()
    );
}

#[test]
fn test_nist_nc101a_multiply_leading_p_operand_multiple_targets() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NC101A-MULTIPLY-P-MULTI.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WRK-DU-0V12-1 PIC V9(12) VALUE .00001.
01 WRK-DU-2P4-1 PIC 99P(4) VALUE 990000.
01 WRK-DU-4P1-1 PIC P(4)9 VALUE .00001.
01 WRK-DU-5V1-1 PIC 9(5)V9 VALUE 12345.6.
01 WRK-DU-6V0-1 PIC 9(6) VALUE 99999.
01 WRK-DU-6V0-2 PIC 9(6) VALUE 99999.
PROCEDURE DIVISION.
    MOVE .00001 TO WRK-DU-4P1-1.
    MOVE 12345.6 TO WRK-DU-5V1-1.
    MULTIPLY WRK-DU-4P1-1 BY WRK-DU-5V1-1 ROUNDED
        WRK-DU-2P4-1 WRK-DU-6V0-1 ROUNDED
        WRK-DU-6V0-2 WRK-DU-0V12-1.
    IF WRK-DU-2P4-1 = 0
        DISPLAY \"PASS\"
    ELSE
        DISPLAY WRK-DU-2P4-1
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("PASS"),
        "P(4)9 multiplier should zero 99P(4) target, got '{}'",
        stdout.trim()
    );
}

/// Test ADD/SUBTRACT/MULTIPLY/DIVIDE with subscripted targets.
#[test]
fn test_native_subscripted_arithmetic() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SUBTEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-TABLE.
   05 WS-ITEM PIC 9(3) OCCURS 5 TIMES.
01 WS-IDX PIC 9 VALUE 3.
PROCEDURE DIVISION.
    MOVE 10 TO WS-ITEM(WS-IDX)
    ADD 5 TO WS-ITEM(WS-IDX)
    DISPLAY WS-ITEM(WS-IDX)
    SUBTRACT 3 FROM WS-ITEM(WS-IDX)
    DISPLAY WS-ITEM(WS-IDX)
    MULTIPLY 4 BY WS-ITEM(WS-IDX)
    DISPLAY WS-ITEM(WS-IDX)
    DIVIDE 6 INTO WS-ITEM(WS-IDX)
    DISPLAY WS-ITEM(WS-IDX)
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.len() >= 4, "expected 4 output lines, got: {}", stdout);
    // 10 + 5 = 15
    assert!(lines[0].contains("15"), "ADD: 10+5=15, got: {}", lines[0]);
    // 15 - 3 = 12
    assert!(
        lines[1].contains("12"),
        "SUBTRACT: 15-3=12, got: {}",
        lines[1]
    );
    // 12 * 4 = 48
    assert!(
        lines[2].contains("48"),
        "MULTIPLY: 12*4=48, got: {}",
        lines[2]
    );
    // 48 / 6 = 8
    assert!(lines[3].contains("8"), "DIVIDE: 48/6=8, got: {}", lines[3]);
}

// -----------------------------------------------------------------------
// Indexed and relative file operations – codegen tests
// -----------------------------------------------------------------------

#[test]
fn test_indexed_file_codegen() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. IXTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_ix_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(15).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN OUTPUT IX-FILE.
    MOVE 00001 TO IX-KEY.
    MOVE "FIRST RECORD" TO IX-DATA.
    WRITE IX-RECORD.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_open_indexed"),
        "indexed file should use cobol_file_open_indexed, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_file_write"),
        "should generate file write call, got:\n{}",
        c_code
    );
}

#[test]
fn test_relative_file_codegen() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. RLTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT RL-FILE ASSIGN TO "/tmp/cobol_rl_test.dat"
        ORGANIZATION IS RELATIVE
        ACCESS MODE IS RANDOM
        RELATIVE KEY IS WS-REL-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD RL-FILE.
01 RL-RECORD PIC X(20).
WORKING-STORAGE SECTION.
01 WS-REL-KEY PIC 9(5).
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN OUTPUT RL-FILE.
    MOVE 1 TO WS-REL-KEY.
    MOVE "RELATIVE RECORD 1" TO RL-RECORD.
    WRITE RL-RECORD.
    CLOSE RL-FILE.
    OPEN INPUT RL-FILE.
    MOVE 0 TO WS-REL-KEY.
    READ RL-FILE.
    CLOSE RL-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_open"),
        "should generate file open call, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_file_current_record"),
        "relative sequential read/update path should sync RELATIVE KEY, got:\n{}",
        c_code
    );
}

#[test]
fn test_start_statement_codegen() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. STARTTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_start_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(15).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN I-O IX-FILE.
    MOVE 00005 TO IX-KEY.
    START IX-FILE KEY IS EQUAL TO IX-KEY
        INVALID KEY DISPLAY "NOT FOUND"
        NOT INVALID KEY DISPLAY "FOUND"
    END-START.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_start") || c_code.contains("start"),
        "should generate START statement code, got:\n{}",
        c_code
    );
}

#[test]
fn test_read_with_key_codegen_uses_read_key_runtime() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. READKEY.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_read_key.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS RANDOM
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(15).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN INPUT IX-FILE.
    MOVE 00010 TO IX-KEY.
    READ IX-FILE KEY IS IX-KEY
        INVALID KEY DISPLAY "MISS"
        NOT INVALID KEY DISPLAY "HIT"
    END-READ.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_read_key("),
        "READ ... KEY should call cobol_file_read_key, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("MISS") && c_code.contains("HIT"),
        "READ INVALID KEY / NOT INVALID KEY branches should be preserved, got:\n{}",
        c_code
    );
}

#[test]
fn test_delete_statement_codegen() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DELTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_del_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(15).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN I-O IX-FILE.
    MOVE 00001 TO IX-KEY.
    DELETE IX-FILE
        INVALID KEY DISPLAY "DELETE FAILED"
    END-DELETE.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_delete") || c_code.contains("delete"),
        "should generate DELETE statement code, got:\n{}",
        c_code
    );
}

#[test]
fn test_rewrite_statement_codegen() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. RWTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_rw_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(15).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN I-O IX-FILE.
    MOVE 00001 TO IX-KEY.
    READ IX-FILE.
    MOVE "UPDATED DATA" TO IX-DATA.
    REWRITE IX-RECORD
        INVALID KEY DISPLAY "REWRITE FAILED"
    END-REWRITE.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_rewrite") || c_code.contains("rewrite"),
        "should generate REWRITE statement code, got:\n{}",
        c_code
    );
}

// ---------------------------------------------------------------------------
// SORT with USING/GIVING
// ---------------------------------------------------------------------------
#[test]
fn test_native_sort_using_giving() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. SORTTEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SORT-FILE ASSIGN TO "/tmp/cobol_sort_work.dat".
    SELECT INPUT-FILE ASSIGN TO "/tmp/cobol_sort_in.dat"
        ORGANIZATION IS LINE SEQUENTIAL.
    SELECT OUTPUT-FILE ASSIGN TO "/tmp/cobol_sort_out.dat"
        ORGANIZATION IS LINE SEQUENTIAL.
DATA DIVISION.
FILE SECTION.
SD SORT-FILE.
01 SORT-RECORD.
   05 SORT-KEY PIC 9(3).
   05 SORT-DATA PIC X(7).
FD INPUT-FILE.
01 INPUT-RECORD PIC X(10).
FD OUTPUT-FILE.
01 OUTPUT-RECORD PIC X(10).
WORKING-STORAGE SECTION.
01 WS-EOF PIC 9 VALUE 0.
PROCEDURE DIVISION.
    OPEN OUTPUT INPUT-FILE.
    MOVE "003CHERRY " TO INPUT-RECORD.
    WRITE INPUT-RECORD.
    MOVE "001APPLE  " TO INPUT-RECORD.
    WRITE INPUT-RECORD.
    MOVE "002BANANA " TO INPUT-RECORD.
    WRITE INPUT-RECORD.
    CLOSE INPUT-FILE.
    SORT SORT-FILE
        ON ASCENDING KEY SORT-KEY
        USING INPUT-FILE
        GIVING OUTPUT-FILE.
    OPEN INPUT OUTPUT-FILE.
    PERFORM UNTIL WS-EOF = 1
        READ OUTPUT-FILE INTO OUTPUT-RECORD
            AT END MOVE 1 TO WS-EOF
            NOT AT END DISPLAY OUTPUT-RECORD
        END-READ
    END-PERFORM.
    CLOSE OUTPUT-FILE.
    STOP RUN.
"#;
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("cobol_sort") || c_code.contains("sort"),
        "should generate sort-related code"
    );
}

// ---------------------------------------------------------------------------
// PERFORM THRU across multiple paragraphs (A through C)
// ---------------------------------------------------------------------------
#[test]
fn test_native_perform_thru_multiple_paragraphs() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRUTEST.
PROCEDURE DIVISION.
    PERFORM PARA-A THRU PARA-C.
    STOP RUN.
PARA-A.
    DISPLAY \"A\".
PARA-B.
    DISPLAY \"B\".
PARA-C.
    DISPLAY \"C\".
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("A"), "should execute PARA-A");
    assert!(stdout.contains("B"), "should execute PARA-B");
    assert!(stdout.contains("C"), "should execute PARA-C");
}

// ---------------------------------------------------------------------------
// GOBACK terminates execution (no AFTER output)
// ---------------------------------------------------------------------------
#[test]
fn test_native_goback_terminates() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GOBACKTEST.
PROCEDURE DIVISION.
    DISPLAY \"BEFORE\".
    GOBACK.
    DISPLAY \"AFTER\".
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(stdout.contains("BEFORE"), "should display BEFORE");
    assert!(!stdout.contains("AFTER"), "should not display AFTER");
}

// ---------------------------------------------------------------------------
// DECLARATIVES with USE AFTER (HIR-level, parser doesn't support syntax yet)
// ---------------------------------------------------------------------------
#[test]
fn test_declaratives_codegen() {
    // Build a basic program and inject a declarative handler manually,
    // since the parser doesn't yet support DECLARATIVES syntax.
    let mut hir = parse_and_lower(
        "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DECLTEST2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    DISPLAY \"MAIN START\".
    STOP RUN.
",
    );

    hir.declaratives.push(HirDeclarative {
        name: "ERR-SECTION".into(),
        use_kind: HirDeclarativeUse::AfterException,
        is_global: false,
        file_names: vec!["TEST-FILE".into()],
        debug_items: vec![],
        body: vec![HirStatement::Display {
            operands: vec![cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::String(
                "FILE ERROR".into(),
            ))],
            no_advancing: false,
            span: Span::dummy(),
        }],
    });

    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("FILE ERROR") || c_code.contains("DECL") || c_code.contains("decl"),
        "should generate declaratives code"
    );
    assert!(
        !c_code.contains("_suppress_debug_event"),
        "non-debug declaratives should not depend on debug helper globals:\n{c_code}"
    );
}

#[test]
fn test_use_for_debugging_is_lowered_into_hir_declaratives() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEBUG-DECL.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
GO-TO SECTION.
    USE FOR DEBUGGING ON GO-TO-TEST.
DBG-PARA.
    DISPLAY \"DBG\".
END DECLARATIVES.
GO-TO-TEST.
    DISPLAY \"MAIN\".
    STOP RUN.
";
    let hir = parse_and_lower(src);
    assert_eq!(
        hir.declaratives.len(),
        1,
        "expected one debugging declarative"
    );
    let decl = &hir.declaratives[0];
    assert_eq!(decl.use_kind, HirDeclarativeUse::ForDebugging);
    assert_eq!(
        decl.debug_items
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        vec!["GO-TO-TEST"]
    );
    assert!(
        decl.body
            .iter()
            .any(|stmt| matches!(stmt, HirStatement::Display { .. })),
        "debugging declarative body should be lowered"
    );
}

#[test]
fn test_debug_declaratives_codegen_uses_debug_suppression_helper() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEBUG-DECL.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON TARGET-PARA.
DBG-PARA.
    DISPLAY \"DBG\".
END DECLARATIVES.
MAIN-SECTION SECTION.
TARGET-PARA.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("static int _suppress_debug_event = 0;"),
        "debug declaratives should declare suppression helper:\n{c_code}"
    );
    assert!(
        c_code.contains("int _prev_suppress_debug_event = _suppress_debug_event;"),
        "debug declaratives should save suppression helper state:\n{c_code}"
    );
}

#[test]
fn test_native_use_for_debugging_start_program_sets_debug_registers() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-START.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON MAIN-SECTION.
DBG-PARA.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
MAIN-SECTION SECTION.
MAIN-PARA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.trim() == "MAIN-SECTION"),
        "stdout should include DEBUG-NAME, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| line.trim() == "START PROGRAM"),
        "stdout should include DEBUG-CONTENTS, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_perform_sets_debug_context() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-PERFORM.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON TARGET-PARA.
DBG-PARA.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
MAIN-SECTION SECTION.
START-PARA.
    PERFORM TARGET-PARA.
    STOP RUN.
TARGET-PARA.
    EXIT.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.trim() == "TARGET-PARA"),
        "stdout should include DEBUG-NAME, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| line.trim() == "PERFORM LOOP"),
        "stdout should include PERFORM debug contents, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_go_to_sets_debug_context() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-GOTO.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON TARGET-PARA.
DBG-PARA.
    DISPLAY DEBUG-NAME.
    DISPLAY \"[\".
    DISPLAY DEBUG-CONTENTS.
    DISPLAY \"]\".
END DECLARATIVES.
MAIN-SECTION SECTION.
START-PARA.
    GO TO TARGET-PARA.
TARGET-PARA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.trim() == "TARGET-PARA"),
        "stdout should include DEBUG-NAME, got:\n{stdout}"
    );
    let open_idx = lines.iter().position(|line| line.trim() == "[");
    let close_idx = lines.iter().position(|line| line.trim() == "]");
    assert!(
        open_idx.is_some() && close_idx.is_some() && close_idx.unwrap() > open_idx.unwrap(),
        "stdout should include bracketed DEBUG-CONTENTS markers, got:\n{stdout}"
    );
    assert!(
        lines[open_idx.unwrap() + 1].trim().is_empty(),
        "DEBUG-CONTENTS for GO TO should be blank, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_fallthrough_sets_debug_context() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-FALLTHROUGH.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON TARGET-PARA.
DBG-PARA.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
MAIN-SECTION SECTION.
START-PARA.
    DISPLAY \"START\".
TARGET-PARA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.trim() == "TARGET-PARA"),
        "stdout should include DEBUG-NAME, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| line.trim() == "FALL THROUGH"),
        "stdout should include FALL THROUGH debug contents, got:\n{stdout}"
    );
}

#[test]
fn test_native_alter_redirects_go_to_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ALTER.
PROCEDURE DIVISION.
    GO TO ALTER-INIT.
ALTERABLE-PARA.
    GO TO ORIGINAL-TARGET.
ORIGINAL-TARGET.
    DISPLAY \"ORIGINAL\".
    STOP RUN.
ALTER-INIT.
    ALTER ALTERABLE-PARA TO PROCEED TO ALTERED-TARGET.
    GO TO ALTERABLE-PARA.
ALTERED-TARGET.
    DISPLAY \"ALTERED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "ALTERED"),
        "stdout should include ALTERED target output, got:\n{stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.trim() == "ORIGINAL"),
        "stdout should not include original target output, got:\n{stdout}"
    );
}

#[test]
fn test_native_alter_multiple_pairs_redirect_each_go_to_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ALTER-MULTI.
PROCEDURE DIVISION.
    GO TO ALTER-INIT.
ALTER-A.
    GO TO ORIG-A.
ALTER-B.
    GO TO ORIG-B.
ORIG-A.
    DISPLAY \"ORIG-A\".
    STOP RUN.
ORIG-B.
    DISPLAY \"ORIG-B\".
    STOP RUN.
ALTER-INIT.
    ALTER ALTER-A TO PROCEED TO TARGET-A
          ALTER-B TO PROCEED TO TARGET-B.
    GO TO ALTER-A.
TARGET-A.
    DISPLAY \"TARGET-A\".
    GO TO ALTER-B.
TARGET-B.
    DISPLAY \"TARGET-B\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "TARGET-A")
            && stdout.lines().any(|line| line.trim() == "TARGET-B"),
        "stdout should include both altered targets, got:\n{stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.trim() == "ORIG-A")
            && !stdout.lines().any(|line| line.trim() == "ORIG-B"),
        "stdout should not include original targets, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_alter_sets_debug_context() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-ALTER.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
OBJECT-COMPUTER. TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 RESULT PIC 9 VALUE 0.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON ALTERABLE-PARA.
DBG-PARA.
    MOVE 7 TO RESULT.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
    ALTER ALTERABLE-PARA TO PROCEED TO ALTERED-TARGET.
    DISPLAY RESULT.
    STOP RUN.
ALTERABLE-PARA.
    GO TO ORIGINAL-TARGET.
ORIGINAL-TARGET.
    STOP RUN.
ALTERED-TARGET.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.trim() == "7"),
        "stdout should include declarative result marker, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| line.trim() == "ALTERABLE-PARA"),
        "stdout should include DEBUG-NAME for ALTER, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| line.trim() == "ALTERED-TARGET"),
        "stdout should include DEBUG-CONTENTS for ALTER, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_section_start_keeps_start_program_contents() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-START.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 DBG-CONTENTS PIC X(20).
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SEC SECTION.
    USE FOR DEBUGGING ON MAIN-SEC.
DBG-PARA.
    MOVE DEBUG-CONTENTS TO DBG-CONTENTS.
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    DISPLAY DBG-CONTENTS.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "START PROGRAM"),
        "section entry debug contents should preserve START PROGRAM, got:\n{stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.trim() == "FALL THROUGH"),
        "section entry should not overwrite START PROGRAM with FALL THROUGH, got:\n{stdout}"
    );
}

#[test]
fn test_use_for_debugging_ignored_without_source_debugging_mode() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-OFF.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 RESULT PIC 9 VALUE 0.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SEC SECTION.
    USE FOR DEBUGGING ON MAIN-SEC.
DBG-PARA.
    MOVE 7 TO RESULT.
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    DISPLAY RESULT.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    assert!(
        hir.declaratives.is_empty(),
        "debugging declaratives require SOURCE-COMPUTER WITH DEBUGGING MODE"
    );
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "0"),
        "debug declarative should not run without compile-time debugging mode, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_respects_object_time_switch_off() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-RUNTIME-OFF.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 RESULT PIC 9 VALUE 0.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SEC SECTION.
    USE FOR DEBUGGING ON MAIN-SEC.
DBG-PARA.
    MOVE 7 TO RESULT.
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    DISPLAY RESULT.
    STOP RUN.
";
    let (stdout, stderr, code) =
        compile_and_run_no_sema_with_env(src, &[("COBOL_DEBUGGING_MODE", "OFF")]);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "0"),
        "debug declarative should not run when object-time switch is OFF, got:\n{stdout}"
    );
}

#[test]
fn test_native_use_for_debugging_all_procedures_does_not_reenter_declarative() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-ALL.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 COUNT-VALUE PIC 99 VALUE 0.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SEC SECTION.
    USE FOR DEBUGGING ON ALL PROCEDURES.
DBG-PARA.
    ADD 1 TO COUNT-VALUE.
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    DISPLAY COUNT-VALUE.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "2"),
        "ALL PROCEDURES should dispatch for section and paragraph without reentering itself, got:\n{stdout}"
    );
}

#[test]
fn test_fixed_debug_lines_are_comments_without_source_debugging_mode() {
    let src = "\
000100 IDENTIFICATION DIVISION.
000200 PROGRAM-ID. DBG-LINE-OFF.
000300 ENVIRONMENT DIVISION.
000400 CONFIGURATION SECTION.
000500 SOURCE-COMPUTER. TEST.
000600 PROCEDURE DIVISION.
000700D    DISPLAY \"FAIL\".
000800     DISPLAY \"OK\".
000900     STOP RUN.
";
    let hir = parse_and_lower_fixed(src);
    let c_code = generate_c(&hir);
    assert!(
        !c_code.contains("FAIL"),
        "fixed-format D lines are comments without SOURCE-COMPUTER WITH DEBUGGING MODE:\n{c_code}"
    );
    assert!(
        c_code.contains("OK"),
        "non-debug line should remain in generated code:\n{c_code}"
    );
}

// ---------------------------------------------------------------------------
// EVALUATE ALSO with multiple subjects
// ---------------------------------------------------------------------------
#[test]
fn test_native_evaluate_also() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVALTEST2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9 VALUE 1.
01 WS-B PIC 9 VALUE 2.
PROCEDURE DIVISION.
    EVALUATE WS-A ALSO WS-B
        WHEN 1 ALSO 2
            DISPLAY \"MATCH-1-2\"
        WHEN 1 ALSO 3
            DISPLAY \"MATCH-1-3\"
        WHEN OTHER
            DISPLAY \"NO-MATCH\"
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("MATCH-1-2"),
        "should match 1 ALSO 2, got: {}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// TYPEDEF codegen verification
// ---------------------------------------------------------------------------
#[test]
fn test_typedef_codegen() {
    let c_code_has_typedef = {
        let hir = cobol_hir::HirProgram {
            name: "TDTEST".into(),
            data_items: vec![],
            communication_descriptions: vec![],
            paragraphs: vec![],
            body: vec![],
            using_params: vec![],
            classes: vec![],
            functions: vec![],
            typedefs: vec![cobol_hir::HirTypedef {
                name: "MY-TYPE".into(),
                base_type: cobol_hir::HirType::Numeric {
                    size: 9,
                    decimal_places: 0,
                    is_signed: true,
                },
                span: Span::new(0, 0, FileId(0)),
            }],
            interfaces: vec![],
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_relative_keys: std::collections::HashMap::new(),
            file_status_vars: vec![],
            declaratives: vec![],
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::new(0, 0, FileId(0)),
        };
        let c = cobol_codegen::generate_c(&hir);
        c.contains("typedef") && c.contains("MY_TYPE")
    };
    assert!(c_code_has_typedef, "TYPEDEF should generate C typedef");
}

#[test]
fn test_communication_error_key_codegen_uses_table_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMM-ERR-KEY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 DEST-COUNT PIC 9 VALUE 2.
01 DEST-TABLE.
   05 SYM-DEST PIC X(8) OCCURS 2 TIMES.
01 ERR-TABLE.
   05 ERR-KEY PIC X OCCURS 2 TIMES.
01 OUT-LEN PIC 9(4) VALUE 4.
01 MSG PIC X(4) VALUE \"PING\".
COMMUNICATION SECTION.
CD CM-OUTQUE-1 OUTPUT
   DESTINATION COUNT DEST-COUNT
   TEXT LENGTH OUT-LEN
   DESTINATION TABLE OCCURS 2 TIMES
   ERROR KEY ERR-KEY
   DESTINATION SYM-DEST.
PROCEDURE DIVISION.
    MOVE \"OUTQ0001\" TO SYM-DEST(1).
    MOVE \"BADDEST2\" TO SYM-DEST(2).
    SEND CM-OUTQUE-1 FROM MSG.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(
        c_code.contains("(uint8_t*)ERR_KEY, 2"),
        "ERROR KEY should pass the full OCCURS area length, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_move_string((const uint8_t*)\"0\", 1, (uint8_t*)ERR_KEY, 2)"),
        "ERROR KEY reset should clear the full OCCURS area, got:\n{}",
        c_code
    );
}

#[test]
fn test_report_statements_codegen() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RPTTEST.
PROCEDURE DIVISION.
    INITIATE SALES-REPORT.
    GENERATE DETAIL-LINE.
    TERMINATE SALES-REPORT.
    STOP RUN.
";
    let c_code = compile_to_c(src);
    assert!(c_code.contains("INITIATE"), "should have INITIATE comment");
    assert!(c_code.contains("GENERATE"), "should have GENERATE comment");
    assert!(
        c_code.contains("TERMINATE"),
        "should have TERMINATE comment"
    );
}

#[test]
fn test_screen_section_codegen() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRTEST.
DATA DIVISION.
SCREEN SECTION.
01 MAIN-SCREEN.
   05 LINE 1 COLUMN 1 VALUE \"HELLO SCREEN\".
   05 LINE 3 COLUMN 5 HIGHLIGHT VALUE \"BOLD TEXT\".
   05 LINE 5 COLUMN 1 BLANK SCREEN VALUE \"AFTER CLEAR\".
PROCEDURE DIVISION.
    DISPLAY MAIN-SCREEN.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        !c_code.is_empty(),
        "should generate C code for screen program"
    );
    // Should contain screen positioning calls
    assert!(
        c_code.contains("cobol_screen_position"),
        "should have screen position call, got:\n{}",
        c_code
    );
}

#[test]
fn test_screen_section_highlight() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRTEST2.
DATA DIVISION.
SCREEN SECTION.
01 MY-SCREEN.
   05 LINE 2 COLUMN 10 HIGHLIGHT VALUE \"HI\".
PROCEDURE DIVISION.
    DISPLAY MY-SCREEN.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_screen_highlight_on"),
        "should have highlight on call, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_screen_reset_attrs"),
        "should reset attrs after highlight, got:\n{}",
        c_code
    );
}

#[test]
fn test_screen_section_blank_screen() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRTEST3.
DATA DIVISION.
SCREEN SECTION.
01 CLR-SCREEN.
   05 BLANK SCREEN LINE 1 COLUMN 1 VALUE \"FRESH START\".
PROCEDURE DIVISION.
    DISPLAY CLR-SCREEN.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_screen_clear"),
        "should have screen clear call, got:\n{}",
        c_code
    );
}

#[test]
fn test_screen_section_reverse_video() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRTEST4.
DATA DIVISION.
SCREEN SECTION.
01 RV-SCREEN.
   05 LINE 1 COLUMN 1 REVERSE-VIDEO VALUE \"REVERSED\".
PROCEDURE DIVISION.
    DISPLAY RV-SCREEN.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_screen_reverse_on"),
        "should have reverse video on call, got:\n{}",
        c_code
    );
}

// ===========================================================================
// Phase 6 edge case tests (production-gaps.md section 4)
// ===========================================================================

// ---------------------------------------------------------------------------
// 4-1: EXIT statement semantics - bare EXIT acts as CONTINUE (no-op)
// ---------------------------------------------------------------------------
#[test]
fn test_native_exit_bare() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EXIT-BARE.
PROCEDURE DIVISION.
    PERFORM PARA-A.
    DISPLAY 'AFTER-PERFORM'.
    STOP RUN.
PARA-A.
    DISPLAY 'BEFORE-EXIT'.
    EXIT.
    DISPLAY 'AFTER-EXIT'.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("BEFORE-EXIT"),
        "should display BEFORE-EXIT, got: {}",
        stdout.trim()
    );
    assert!(
        stdout.contains("AFTER-EXIT"),
        "bare EXIT should be a no-op, AFTER-EXIT should appear, got: {}",
        stdout.trim()
    );
    assert!(
        stdout.contains("AFTER-PERFORM"),
        "should continue after PERFORM, got: {}",
        stdout.trim()
    );
}

// ---------------------------------------------------------------------------
// 4-1: EXIT PROGRAM should end the program
// ---------------------------------------------------------------------------
#[test]
fn test_native_exit_program() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EXIT-PROG.
PROCEDURE DIVISION.
    DISPLAY 'BEFORE-EXIT'.
    EXIT PROGRAM.
    DISPLAY 'SHOULD-NOT-SHOW'.
    STOP RUN.
";
    let (stdout, _, _code) = compile_and_run_no_sema(src);
    assert!(
        stdout.contains("BEFORE-EXIT"),
        "should display BEFORE-EXIT, got: {}",
        stdout.trim()
    );
    assert!(
        !stdout.contains("SHOULD-NOT-SHOW"),
        "EXIT PROGRAM should stop execution, got: {}",
        stdout.trim()
    );
}

#[test]
fn test_native_exit_program_in_subprogram_returns_to_caller() {
    // cspell:ignore MAINPROG SUBPROG
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MAINPROG.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(2) VALUE 0.
PROCEDURE DIVISION.
    CALL 'SUBPROG' USING WS-NUM.
    DISPLAY WS-NUM.
    STOP RUN.
END PROGRAM MAINPROG.
IDENTIFICATION DIVISION.
PROGRAM-ID. SUBPROG.
DATA DIVISION.
LINKAGE SECTION.
01 LK-NUM PIC 9(2).
PROCEDURE DIVISION USING LK-NUM.
    MOVE 7 TO LK-NUM.
    EXIT PROGRAM.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains('7'),
        "EXIT PROGRAM in subprogram should return to caller, got: {}",
        stdout.trim()
    );
}

#[test]
fn test_call_on_overflow_routes_to_exception_path_for_missing_program() {
    // cspell:ignore MAINPROG
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MAINPROG.
PROCEDURE DIVISION.
    CALL 'XXXXXXXX'
        ON OVERFLOW
            DISPLAY 'OVERFLOW'
    END-CALL.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("OVERFLOW"),
        "CALL ON OVERFLOW should execute the exception path, got: {}",
        stdout.trim()
    );
}

#[test]
fn test_exit_program_in_nested_subprogram_without_using_returns_to_caller() {
    // cspell:ignore MAINPROG SUBPROG
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MAINPROG.
PROCEDURE DIVISION.
    CALL 'SUBPROG'.
    DISPLAY 'AFTER-CALL'.
    STOP RUN.
END PROGRAM MAINPROG.
IDENTIFICATION DIVISION.
PROGRAM-ID. SUBPROG.
PROCEDURE DIVISION.
    DISPLAY 'INSIDE-SUB'.
    EXIT PROGRAM.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("INSIDE-SUB") && stdout.contains("AFTER-CALL"),
        "EXIT PROGRAM in a nested subprogram should return to caller, got: {}",
        stdout.trim()
    );
}

#[test]
fn test_external_working_storage_is_shared_across_nested_program_without_using() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MAINPROG.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 EXTERNAL-DATA IS EXTERNAL.
   03 EXT-A PIC X(2).
   03 EXT-B PIC 9(4).
PROCEDURE DIVISION.
    MOVE 'AA' TO EXT-A.
    MOVE 1 TO EXT-B.
    CALL 'SUBPROG'.
    DISPLAY EXT-A.
    DISPLAY EXT-B.
    STOP RUN.
END PROGRAM MAINPROG.
IDENTIFICATION DIVISION.
PROGRAM-ID. SUBPROG.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 EXTERNAL-DATA IS EXTERNAL.
   03 EXT-A PIC X(2).
   03 EXT-B PIC 9(4).
PROCEDURE DIVISION.
    MOVE 'ZZ' TO EXT-A.
    ADD 10 TO EXT-B.
    EXIT PROGRAM.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("ZZ") && stdout.contains("11"),
        "EXTERNAL working-storage should be shared across nested programs, got: {}",
        stdout.trim()
    );
}

// ---------------------------------------------------------------------------
// 4-3: Group-to-group MOVE with space-padding
// ---------------------------------------------------------------------------
#[test]
fn test_native_group_to_group_move() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GRP-MOVE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SRC.
   05 WS-SRC-A PIC X(5) VALUE 'HELLO'.
   05 WS-SRC-B PIC X(5) VALUE 'WORLD'.
01 WS-DST.
   05 WS-DST-A PIC X(5).
   05 WS-DST-B PIC X(5).
PROCEDURE DIVISION.
    MOVE WS-SRC TO WS-DST.
    DISPLAY WS-DST-A.
    DISPLAY WS-DST-B.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("HELLO"),
        "group MOVE should copy first field, got: {}",
        stdout.trim()
    );
    assert!(
        stdout.contains("WORLD"),
        "group MOVE should copy second field, got: {}",
        stdout.trim()
    );
}

// ---------------------------------------------------------------------------
// 4-3: Group-to-group MOVE with shorter source (padding test)
// ---------------------------------------------------------------------------
#[test]
fn test_native_group_move_padding() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GRP-PAD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SHORT.
   05 WS-S1 PIC X(3) VALUE 'ABC'.
01 WS-LONG.
   05 WS-L1 PIC X(3) VALUE 'XYZ'.
   05 WS-L2 PIC X(3) VALUE '123'.
PROCEDURE DIVISION.
    MOVE WS-SHORT TO WS-LONG.
    DISPLAY 'L1=[' WS-L1 ']'.
    DISPLAY 'L2=[' WS-L2 ']'.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("L1=[ABC]"),
        "first 3 bytes should be ABC, got: {}",
        stdout.trim()
    );
    assert!(
        stdout.contains("L2=[   ]"),
        "remaining 3 bytes should be space-padded, got: {}",
        stdout.trim()
    );
}

// ---------------------------------------------------------------------------
// 4-4: EVALUATE ALSO with 3 subjects
// ---------------------------------------------------------------------------
#[test]
fn test_native_evaluate_also_three_subjects() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVAL-3ALSO.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9 VALUE 1.
01 WS-B PIC 9 VALUE 2.
01 WS-C PIC 9 VALUE 3.
PROCEDURE DIVISION.
    EVALUATE WS-A ALSO WS-B ALSO WS-C
        WHEN 1 ALSO 2 ALSO 3
            DISPLAY 'MATCH-123'
        WHEN 1 ALSO 2 ALSO 4
            DISPLAY 'MATCH-124'
        WHEN 1 ALSO 3 ALSO 3
            DISPLAY 'MATCH-133'
        WHEN OTHER
            DISPLAY 'NO-MATCH'
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("MATCH-123"),
        "should match 1 ALSO 2 ALSO 3, got: {}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// 4-4: EVALUATE ALSO with 3 subjects - OTHER path
// ---------------------------------------------------------------------------
#[test]
fn test_native_evaluate_also_three_subjects_other() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. EVAL-3OTHER.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-A PIC 9 VALUE 9.
01 WS-B PIC 9 VALUE 8.
01 WS-C PIC 9 VALUE 7.
PROCEDURE DIVISION.
    EVALUATE WS-A ALSO WS-B ALSO WS-C
        WHEN 1 ALSO 2 ALSO 3
            DISPLAY 'MATCH-123'
        WHEN 1 ALSO 2 ALSO 4
            DISPLAY 'MATCH-124'
        WHEN OTHER
            DISPLAY 'NO-MATCH'
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("NO-MATCH"),
        "should fall through to OTHER, got: {}",
        stdout
    );
}

// ===========================================================================
// NATIONAL data type (PIC N) tests
// ===========================================================================

#[test]
fn test_national_pic_n_codegen() {
    // Test that PIC N generates uint16_t arrays and correct runtime calls
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NATTEST1.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NATIONAL PIC N(10).
01  WS-ALPHA   PIC X(10) VALUE \"HELLO\".
PROCEDURE DIVISION.
    MOVE WS-ALPHA TO WS-NATIONAL.
    DISPLAY WS-NATIONAL.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("uint16_t WS_NATIONAL[10]"),
        "PIC N should generate uint16_t array, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_move_to_national"),
        "MOVE to NATIONAL should use cobol_move_to_national, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_display_national"),
        "DISPLAY NATIONAL should use cobol_display_national, got:\n{}",
        c_code
    );
}

#[test]
fn test_native_national_move_and_display() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NATTEST2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAT     PIC N(10).
01  WS-ALPHA   PIC X(10) VALUE \"HELLO\".
01  WS-RESULT  PIC X(10).
PROCEDURE DIVISION.
    MOVE WS-ALPHA TO WS-NAT.
    DISPLAY WS-NAT.
    MOVE WS-NAT TO WS-RESULT.
    DISPLAY WS-RESULT.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "program should exit with code 0");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.iter().any(|l| l.contains("HELLO")),
        "DISPLAY of NATIONAL should show HELLO, got: {}",
        stdout
    );
    assert!(
        lines.len() >= 2,
        "should have at least 2 output lines, got: {}",
        stdout
    );
}

#[test]
fn test_nested_occurs_codegen() {
    // Verify that nested OCCURS generates multi-dimensional struct access,
    // not flat array subscripts that would fail with "subscripted value is not an array".
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NESTED-OCCURS.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-1.
    05  GRP-ENTRY OCCURS 3 TIMES.
        10  TABLE-ITEM PIC 9(3).
PROCEDURE DIVISION.
    MOVE 42 TO TABLE-ITEM(2).
    DISPLAY TABLE-ITEM(1).
    STOP RUN.
";
    let c_code = compile_to_c(src);
    // The generated C should contain struct-based access for the subscripted item,
    // e.g., TABLE_1.members._m_GRP_ENTRY[(idx)-1].members._m_TABLE_ITEM
    // It should NOT contain TABLE_ITEM[...][...] (flat multi-dimensional subscript)
    assert!(
        c_code.contains("_m_GRP_ENTRY["),
        "nested OCCURS should generate struct subscript at group level, got:\n{}",
        c_code
    );
    // The group struct member should have array dimension for OCCURS
    assert!(
        c_code.contains("_m_GRP_ENTRY[3]") || c_code.contains("_m_GRP_ENTRY[3];"),
        "group with OCCURS should have array dimension in struct, got:\n{}",
        c_code
    );
}

#[test]
fn test_nested_occurs_2d_codegen() {
    // Two levels of OCCURS: TABLE-ITEM(I, J) should produce 2D struct access
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NESTED-2D.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-1.
    05  GRP-ENTRY OCCURS 3 TIMES.
        10  SUB-ENTRY OCCURS 4 TIMES.
            15  TABLE-ITEM PIC 9(3).
PROCEDURE DIVISION.
    MOVE 99 TO TABLE-ITEM(2, 3).
    DISPLAY TABLE-ITEM(2, 3).
    STOP RUN.
";
    let c_code = compile_to_c(src);
    // Should have subscript at GRP-ENTRY level and SUB-ENTRY level
    assert!(
        c_code.contains("_m_GRP_ENTRY["),
        "should have GRP_ENTRY subscript, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("_m_SUB_ENTRY["),
        "should have SUB_ENTRY subscript, got:\n{}",
        c_code
    );
}

#[test]
fn test_native_nested_occurs() {
    // Nested OCCURS: compile, link, and execute with clang
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NESTED-OCCURS-EXEC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-1.
    05  GRP-ENTRY OCCURS 3 TIMES.
        10  TABLE-ITEM PIC 9(3).
01  WS-I PIC 9 VALUE 1.
PROCEDURE DIVISION.
    MOVE 42 TO TABLE-ITEM(1).
    MOVE 99 TO TABLE-ITEM(2).
    MOVE 77 TO TABLE-ITEM(3).
    DISPLAY TABLE-ITEM(1).
    DISPLAY TABLE-ITEM(2).
    DISPLAY TABLE-ITEM(3).
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "program should exit with code 0");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.iter().any(|l| l.contains("42")),
        "TABLE-ITEM(1) should be 42, got: {}",
        stdout
    );
    assert!(
        lines.iter().any(|l| l.contains("99")),
        "TABLE-ITEM(2) should be 99, got: {}",
        stdout
    );
    assert!(
        lines.iter().any(|l| l.contains("77")),
        "TABLE-ITEM(3) should be 77, got: {}",
        stdout
    );
}

#[test]
fn test_native_nested_occurs_2d() {
    // Two-level nested OCCURS: compile, link, and execute with clang
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NESTED-2D-EXEC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-1.
    05  GRP-ENTRY OCCURS 3 TIMES.
        10  SUB-ENTRY OCCURS 4 TIMES.
            15  TABLE-ITEM PIC 9(3).
PROCEDURE DIVISION.
    MOVE 11 TO TABLE-ITEM(1, 1).
    MOVE 12 TO TABLE-ITEM(1, 2).
    MOVE 23 TO TABLE-ITEM(2, 3).
    MOVE 34 TO TABLE-ITEM(3, 4).
    DISPLAY TABLE-ITEM(1, 1).
    DISPLAY TABLE-ITEM(1, 2).
    DISPLAY TABLE-ITEM(2, 3).
    DISPLAY TABLE-ITEM(3, 4).
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "program should exit with code 0");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "should have 4 output lines, got: {}",
        stdout
    );
    assert!(
        lines[0].contains("11"),
        "line 1 should be 11, got: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("12"),
        "line 2 should be 12, got: {}",
        lines[1]
    );
    assert!(
        lines[2].contains("23"),
        "line 3 should be 23, got: {}",
        lines[2]
    );
    assert!(
        lines[3].contains("34"),
        "line 4 should be 34, got: {}",
        lines[3]
    );
}
