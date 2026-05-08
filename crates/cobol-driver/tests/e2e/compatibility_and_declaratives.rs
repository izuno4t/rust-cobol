use super::*;

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
fn test_native_88_values_are_thru_range_with_qualification() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALUES-ARE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  WS-NUM PIC 9.
    88 DUP VALUE 1.
77  WS-SCORE PIC 9.
    88 DUP VALUES ARE 2 THRU 4.
PROCEDURE DIVISION.
    MOVE 3 TO WS-SCORE.
    IF DUP OF WS-SCORE
        DISPLAY \"PASS\"
    ELSE
        DISPLAY \"FAIL\"
    END-IF.
    STOP RUN.
";
    let (output, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={output:?}, stderr={stderr:?}"
    );
    assert_eq!(
        output.trim(),
        "PASS",
        "qualified 88 VALUES ARE THRU range should resolve to its parent item"
    );
}

#[test]
fn test_native_all_zeroes_numeric_comparison_uses_numeric_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ALL-ZERO-NUMERIC-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  ZERO-D PIC 9 VALUE 0.
PROCEDURE DIVISION.
    IF ALL ZEROES = ZERO-D
        DISPLAY \"PASS1\"
    ELSE
        DISPLAY \"FAIL1\"
    END-IF.
    IF ALL \"00\" NOT > ZERO-D
        DISPLAY \"PASS2\"
    ELSE
        DISPLAY \"FAIL2\"
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS1"),
        "ALL ZEROES should compare as numeric zero"
    );
    assert!(
        stdout.contains("PASS2"),
        "ALL \"00\" should compare as numeric zero"
    );
}

#[test]
fn test_native_abbreviated_relation_inherits_not_and_falls_back_left() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ABBREV-REL-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SMALL-VALU PIC 99 VALUE 7.
01 SMALLER-VALU PIC 99 VALUE 6.
01 SMALLEST-VALU PIC 99 VALUE 5.
01 EVEN-SMALLER PIC 99 VALUE 1.
PROCEDURE DIVISION.
    IF SMALLEST-VALU GREATER THAN SMALL-VALU
        AND IS NOT LESS THAN EVEN-SMALLER OR SMALLER-VALU
        DISPLAY \"FAIL1\"
    ELSE
        DISPLAY \"PASS1\"
    END-IF.
    IF SMALLEST-VALU LESS THAN SMALL-VALU
        AND NOT EVEN-SMALLER OR SMALLER-VALU
        DISPLAY \"PASS2\"
    ELSE
        DISPLAY \"FAIL2\"
    END-IF.
    MOVE 9 TO SMALL-VALU.
    MOVE 8 TO SMALLER-VALU.
    MOVE 7 TO SMALLEST-VALU.
    IF SMALL-VALU > SMALLER-VALU AND NOT < 10 OR 11
        DISPLAY \"PASS3\"
    ELSE
        DISPLAY \"FAIL3\"
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS1"),
        "NOT relation abbreviation should be inherited"
    );
    assert!(
        stdout.contains("PASS2"),
        "OR abbreviation should fall back to the left comparison"
    );
    assert!(
        stdout.contains("PASS3"),
        "OR abbreviation should stay in the AND branch"
    );
}

#[test]
fn test_native_add_size_error_to_numeric_renames_uses_display_storage() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RENAMES-ADD-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  W-RENAMES-DATA.
    02 WIDGET-4 PIC 9(4).
66  RENAME-12 RENAMES WIDGET-4.
PROCEDURE DIVISION.
    MOVE 8000 TO WIDGET-4.
    ADD 3500 TO RENAME-12 ON SIZE ERROR
        DISPLAY \"SIZE\"
    END-ADD.
    IF RENAME-12 = 8000
        DISPLAY \"UNCHANGED\"
    ELSE
        DISPLAY \"CHANGED\"
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("SIZE"),
        "overflow on renamed PIC 9(4) should set SIZE ERROR"
    );
    assert!(
        stdout.contains("UNCHANGED"),
        "ON SIZE ERROR should preserve renamed display numeric storage"
    );
}

#[test]
fn test_native_move_decimal_to_numeric_edited_renames_uses_source_picture() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RENAMES-EDITED-MOVE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  W-RENAMES-DATA.
    02 WIDGET-2 PIC ***,***.**.
66  RENAME-11 RENAMES WIDGET-2.
PROCEDURE DIVISION.
    MOVE SPACES TO W-RENAMES-DATA.
    MOVE 234.5 TO RENAME-11.
    DISPLAY WIDGET-2.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("****234.50"),
        "MOVE to numeric-edited RENAMES should use the source item's PIC: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_initiate_sets_report_writer_counters() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-INIT-COUNTERS.
DATA DIVISION.
REPORT SECTION.
RD RPT.
01 TYPE DETAIL.
PROCEDURE DIVISION.
    INITIATE RPT.
    DISPLAY PAGE-COUNTER.
    DISPLAY LINE-COUNTER.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<_> = stdout.lines().map(str::trim).collect();
    assert_eq!(lines, vec!["1", "0"]);
}

#[test]
fn test_native_generate_advances_report_writer_line_counter() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-GENERATE-COUNTER.
DATA DIVISION.
REPORT SECTION.
RD RPT.
01 DETAIL-LINE TYPE DETAIL.
PROCEDURE DIVISION.
    INITIATE RPT.
    GENERATE DETAIL-LINE.
    GENERATE DETAIL-LINE.
    DISPLAY LINE-COUNTER.
    DISPLAY PAGE-COUNTER.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<_> = stdout.lines().map(str::trim).collect();
    assert_eq!(lines, vec!["DETAIL_LINE", "DETAIL_LINE", "2", "1"]);
}

#[test]
fn test_native_generate_uses_report_first_detail_line() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-FIRST-DETAIL.
DATA DIVISION.
REPORT SECTION.
RD RPT
    FIRST DETAIL 6.
01 DETAIL-LINE TYPE DETAIL.
PROCEDURE DIVISION.
    INITIATE RPT.
    DISPLAY LINE-COUNTER.
    GENERATE DETAIL-LINE.
    DISPLAY LINE-COUNTER.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<_> = stdout.lines().map(str::trim).collect();
    assert_eq!(lines, vec!["0", "DETAIL_LINE", "6"]);
}

#[test]
fn test_native_generate_resets_line_counter_after_last_detail() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-LAST-DETAIL.
DATA DIVISION.
REPORT SECTION.
RD RPT
    FIRST DETAIL 2
    LAST DETAIL 3.
01 DETAIL-LINE TYPE DETAIL.
PROCEDURE DIVISION.
    INITIATE RPT.
    GENERATE DETAIL-LINE.
    GENERATE DETAIL-LINE.
    GENERATE DETAIL-LINE.
    DISPLAY LINE-COUNTER.
    DISPLAY PAGE-COUNTER.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<_> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec!["DETAIL_LINE", "DETAIL_LINE", "DETAIL_LINE", "2", "2"]
    );
}

#[test]
fn test_native_generate_uses_report_page_limit_for_page_advance() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-PAGE-LIMIT.
DATA DIVISION.
REPORT SECTION.
RD RPT
    FIRST DETAIL 2
    PAGE LIMIT 3.
01 DETAIL-LINE TYPE DETAIL.
PROCEDURE DIVISION.
    INITIATE RPT.
    GENERATE DETAIL-LINE.
    GENERATE DETAIL-LINE.
    GENERATE DETAIL-LINE.
    DISPLAY LINE-COUNTER.
    DISPLAY PAGE-COUNTER.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<_> = stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec!["DETAIL_LINE", "DETAIL_LINE", "DETAIL_LINE", "2", "2"]
    );
}

#[test]
fn test_native_generate_report_detail_source_and_column() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-SOURCE-COLUMN.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(5) VALUE \"ALICE\".
01 WS-AMOUNT PIC 9(3) VALUE 123.
REPORT SECTION.
RD RPT.
01 DETAIL-LINE TYPE DETAIL.
   05 LINE 1 COLUMN 1 VALUE \"NAME:\".
   05 LINE 1 COLUMN 7 SOURCE WS-NAME.
   05 LINE 1 COLUMN 15 VALUE \"AMT:\".
   05 LINE 1 COLUMN 20 SOURCE WS-AMOUNT.
PROCEDURE DIVISION.
    INITIATE RPT.
    GENERATE DETAIL-LINE.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(
        code, 0,
        "native report generation failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("NAME: ALICE   AMT: 123"),
        "report detail should honor SOURCE and COLUMN clauses, got: {stdout:?}"
    );
}

#[test]
fn test_native_generate_report_nested_multi_line_group() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RW-NESTED-LINES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(5) VALUE \"ALICE\".
01 WS-AMOUNT PIC 9(3) VALUE 123.
REPORT SECTION.
RD RPT.
01 DETAIL-LINE TYPE DETAIL.
   05 FILLER LINE 1.
      10 FILLER COLUMN 1 VALUE \"NAME:\".
      10 FILLER COLUMN 7 SOURCE WS-NAME.
   05 FILLER LINE 2.
      10 FILLER COLUMN 1 VALUE \"AMT:\".
      10 FILLER COLUMN 7 SOURCE WS-AMOUNT.
PROCEDURE DIVISION.
    INITIATE RPT.
    GENERATE DETAIL-LINE.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(
        code, 0,
        "native report generation failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(
        stdout, "NAME: ALICE\nAMT:  123\n",
        "nested report group should inherit parent LINE and honor child COLUMN/SOURCE clauses"
    );
}

#[test]
fn test_native_invoke_null_object_returns_zero_without_crashing() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INVOKE-NULL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 MY-OBJ USAGE POINTER.
01 MY-RESULT PIC 9(5).
PROCEDURE DIVISION.
    INVOKE MY-OBJ \"DO-SOMETHING\" RETURNING MY-RESULT.
    DISPLAY MY-RESULT.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(
        code, 0,
        "INVOKE on a null object should return normally: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stderr.contains("COBOL INVOKE: null object reference"),
        "runtime should report the null object path: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout.trim(), "0");
}

#[test]
fn test_native_generated_class_method_dispatch_runs() {
    let mut hir = empty_hir_program("CLASS-NATIVE");
    hir.classes.push(cobol_hir::HirClass {
        name: "MY-CLASS".into(),
        parent: None,
        factory_methods: Vec::new(),
        instance_methods: vec![cobol_hir::HirMethod {
            name: "PING".into(),
            params: Vec::new(),
            returning: None,
            data_items: Vec::new(),
            body: vec![HirStatement::Display {
                operands: vec![cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::String(
                    "CLASS-PING".into(),
                ))],
                no_advancing: false,
                span: Span::dummy(),
            }],
            span: Span::dummy(),
        }],
        factory_data: Vec::new(),
        instance_data: Vec::new(),
        span: Span::dummy(),
    });

    let generated = generate_c(&hir);
    let harness = format!(
        "#define main cobol_generated_main\n{generated}\n#undef main\n\
         int main(void) {{ MY_CLASS* obj = MY_CLASS_new(); \
         cobol_invoke((void*)obj, \"PING\", (int64_t[]){{0}}, 0); return 0; }}\n"
    );

    let (stdout, stderr, code) = compile_c_and_run(&harness);

    assert_eq!(
        code, 0,
        "generated class harness failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "CLASS-PING\n");
}

#[test]
fn test_native_generated_function_typedef_and_interface_compile_and_run() {
    let mut hir = empty_hir_program("LATER-NATIVE");
    hir.functions.push(cobol_hir::HirFunction {
        name: "HELLO-FUNC".into(),
        params: Vec::new(),
        returning: HirType::Numeric {
            size: 1,
            decimal_places: 0,
            is_signed: false,
        },
        data_items: Vec::new(),
        body: vec![HirStatement::Display {
            operands: vec![cobol_hir::HirExpr::Literal(cobol_hir::HirLiteral::String(
                "FUNC-RUN".into(),
            ))],
            no_advancing: false,
            span: Span::dummy(),
        }],
        span: Span::dummy(),
    });
    hir.typedefs.push(cobol_hir::HirTypedef {
        name: "MONEY-TYPE".into(),
        base_type: HirType::Numeric {
            size: 9,
            decimal_places: 2,
            is_signed: true,
        },
        span: Span::dummy(),
    });
    hir.interfaces.push(cobol_hir::HirInterface {
        name: "I-RUNNABLE".into(),
        methods: vec![cobol_hir::HirMethod {
            name: "RUN".into(),
            params: Vec::new(),
            returning: None,
            data_items: Vec::new(),
            body: Vec::new(),
            span: Span::dummy(),
        }],
        span: Span::dummy(),
    });

    let generated = generate_c(&hir);
    let harness = format!(
        "#define main cobol_generated_main\n{generated}\n#undef main\n\
         int main(void) {{ I_RUNNABLE_vtable iface = {{0}}; MONEY_TYPE amount = 0; \
         (void)iface; (void)amount; cobol_func_hello_func(); return 0; }}\n"
    );

    let (stdout, stderr, code) = compile_c_and_run(&harness);

    assert_eq!(
        code, 0,
        "generated function/type/interface harness failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "FUNC-RUN\n");
}

#[test]
fn test_native_group_move_to_redefined_base_uses_logical_target_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. REDEF-MOVE-SIZE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  BASE-GROUP.
    02 BASE-A PIC X(4).
01  LARGE-REDEF REDEFINES BASE-GROUP.
    02 LARGE-A PIC X(8).
01  SOURCE-GROUP.
    02 SOURCE-A PIC X(8) VALUE \"AAAAAAAA\".
PROCEDURE DIVISION.
    MOVE SPACES TO LARGE-REDEF.
    MOVE SOURCE-GROUP TO BASE-GROUP.
    IF LARGE-A = \"AAAA    \"
        DISPLAY \"PASS\"
    ELSE
        DISPLAY LARGE-A
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(
        stdout.trim(),
        "PASS",
        "group MOVE to redefined base should affect only the base group's logical size"
    );
}

#[test]
fn test_native_single_and_range_renames_use_renamed_storage_length() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RENAMES-LENGTH-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  RENAMES-DATA.
    02 NAME1.
        03 NAME1A PIC XX VALUE SPACE.
        03 NAME1B PIC XXX VALUE SPACE.
    02 NAME2 PIC X(10) VALUE SPACE.
66  RENAME2 RENAMES NAME1A THRU NAME1B.
66  RENAME4 RENAMES NAME1.
PROCEDURE DIVISION.
    MOVE \"AB\" TO NAME1A.
    MOVE \"CD\" TO NAME1B.
    IF RENAME4 = \"ABCD \"
        DISPLAY \"PASS1\"
    ELSE
        DISPLAY RENAME4
    END-IF.
    MOVE ALL \"X\" TO RENAME2.
    IF NAME1 = \"XXXXX\"
        DISPLAY \"PASS2\"
    ELSE
        DISPLAY NAME1
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS1"),
        "single RENAMES should use source group length: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS2"),
        "range RENAMES should use full range length: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_qualified_duplicate_range_renames_use_own_group_length() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RENAMES-QUAL-RANGE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  T-RENAMES-DATA.
    02 TAG-1.
        03 TAG-1A PIC X(4).
        03 TAG-1B PIC X(6).
66  RENAME-5 RENAMES TAG-1A THRU TAG-1B.
01  U-RENAMES-DATA.
    02 UNIT-1.
        03 UNIT-1A PIC X(7).
        03 UNIT-1B PIC X(4).
    02 NAME-2 PIC X(5).
66  RENAME-5 RENAMES UNIT-1A THRU UNIT-1B.
PROCEDURE DIVISION.
    MOVE SPACES TO U-RENAMES-DATA.
    MOVE \"CHICAGO ILLINOIS\" TO RENAME-5 OF U-RENAMES-DATA.
    IF U-RENAMES-DATA = \"CHICAGO ILL     \"
        DISPLAY \"PASS\"
    ELSE
        DISPLAY U-RENAMES-DATA
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS"),
        "qualified range RENAMES should not reuse an earlier duplicate RENAMES size: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_compute_decimal_expression_to_display_numeric_rescales_to_picture() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-DISPLAY-SCALE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  COMPUTE-5  PIC 9999V99 VALUE ZERO.
01  COMPUTE-5A PIC 999V9 VALUE 11.1.
PROCEDURE DIVISION.
    COMPUTE COMPUTE-5 = COMPUTE-5A * 36.1.
    DISPLAY COMPUTE-5.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("0400.71"),
        "COMPUTE into PIC 9999V99 should store the value scaled to two decimal places: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_compute_rounded_to_display_integer_uses_decimal_scale() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-ROUNDED-DISPLAY-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  COMPUTE-DATA.
    02 COMPUTE-9  PIC 9999 VALUE ZERO.
    02 COMPUTE-6A PIC 999V9 VALUE 374.4.
PROCEDURE DIVISION.
    COMPUTE COMPUTE-9 ROUNDED = COMPUTE-6A * 7.0.
    DISPLAY COMPUTE-9.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("2621"),
        "ROUNDED COMPUTE into PIC 9999 should use decimal source scale before rounding: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_compute_rounded_integer_division_to_decimal_target_preserves_fraction() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-ROUNDED-DIV-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  W-11 PIC S99V9 VALUE ZERO.
PROCEDURE DIVISION.
    COMPUTE W-11 ROUNDED = 25 / 10.
    DISPLAY W-11.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("2.5"),
        "COMPUTE division into a decimal target should preserve the fractional quotient: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_compute_rounded_decimal_expression_to_integer_target_rounds() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-ROUNDED-INT-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  WRK-DS-02V00 PIC S99 VALUE ZERO.
77  A99-DS-02V00 PIC S99 VALUE 99.
77  AZERO-DS-05V05 PIC S9(5)V9(5) VALUE ZERO.
PROCEDURE DIVISION.
    COMPUTE WRK-DS-02V00 ROUNDED = A99-DS-02V00 + AZERO-DS-05V05 - 2.5.
    DISPLAY WRK-DS-02V00.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("97"),
        "ROUNDED COMPUTE into an integer target should round 96.5 to 97: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_compute_integer_division_to_subscripted_display_decimal_target_keeps_fraction() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMPUTE-SUB-DISPLAY-DIV-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  GRP-0010.
    02 WRK-O005F-0012 OCCURS 2 TIMES.
       03 WRK-O003F-0013 OCCURS 2 TIMES.
          05 WRK-DS-03V04-0003F-0014 PIC S9(3)V9999 OCCURS 2 TIMES.
PROCEDURE DIVISION.
    COMPUTE WRK-DS-03V04-0003F-0014 (2, 2, 2) = 174 / 16.
    IF WRK-DS-03V04-0003F-0014 (2, 2, 2) > 10.8749
        AND WRK-DS-03V04-0003F-0014 (2, 2, 2) < 10.8751
        DISPLAY \"PASS\"
    ELSE
        DISPLAY \"FAIL\"
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS"),
        "COMPUTE integer division into subscripted PIC S9(3)V9999 should preserve four fractional digits: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_subtract_corresponding_unsigned_display_stores_abs_digits() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SUB-CORR-UNSIGNED-DISPLAY-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC-GROUP.
    05 A PIC S99 VALUE 11.
    05 B PIC S99 VALUE 22.
01  DST-GROUP.
    05 A PIC 99.
    05 B PIC 99.
PROCEDURE DIVISION.
    MOVE ZERO TO DST-GROUP.
    SUBTRACT CORRESPONDING SRC-GROUP FROM DST-GROUP.
    IF DST-GROUP = \"1122\"
        DISPLAY \"PASS\"
    ELSE
        DISPLAY DST-GROUP
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS"),
        "SUBTRACT CORRESPONDING into unsigned DISPLAY PIC should store unsigned digit bytes, not negative overpunch: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_deep_qualified_duplicate_name_uses_qualified_picture_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEEP-QUAL-SIZE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-LEVEL-5A.
    02 TABLE-LEVEL-4A.
       03 TABLE-LEVEL-3A.
          04 TABLE-LEVEL-2A.
             05 TABLE-LEVEL-1A.
                06 TBL-LEVEL-0A PIC X(12) VALUE \"5A4A3A2A1A0A\".
01  TABLE-LEVEL-5B.
    02 TABLE-LEVEL-4A.
       03 TABLE-LEVEL-3A.
          04 TABLE-LEVEL-2A.
             05 TABLE-LEVEL-1A.
                06 TBL-LEVEL-0A PIC X VALUE \"Z\".
01  OUT-FIELD PIC X(12).
PROCEDURE DIVISION.
    IF TBL-LEVEL-0A OF TABLE-LEVEL-1A IN TABLE-LEVEL-2A OF
        TABLE-LEVEL-3A IN TABLE-LEVEL-4A OF TABLE-LEVEL-5A =
        \"5A4A3A2A1A0A\"
        MOVE TBL-LEVEL-0A OF TABLE-LEVEL-1A IN TABLE-LEVEL-2A OF
            TABLE-LEVEL-3A IN TABLE-LEVEL-4A OF TABLE-LEVEL-5A TO OUT-FIELD
    END-IF.
    DISPLAY OUT-FIELD.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("5A4A3A2A1A0A"),
        "deep qualified duplicate data-name should use the fully qualified PIC X(12) size: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_group_move_with_inherited_computational_usage_copies_all_members() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. GROUP-COMP-MOVE-TEST.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC-GROUP USAGE COMPUTATIONAL.
    05 A PIC 9 VALUE 5.
    05 B PIC 9 VALUE 6.
01  DST-GROUP USAGE COMPUTATIONAL.
    05 A PIC 9.
    05 B PIC 9.
PROCEDURE DIVISION.
    MOVE SRC-GROUP TO DST-GROUP.
    IF B OF DST-GROUP = 6
        DISPLAY \"PASS\"
    ELSE
        DISPLAY B OF DST-GROUP
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("PASS"),
        "group MOVE between USAGE COMPUTATIONAL groups should copy the full internal storage for all members: stdout={stdout:?}, stderr={stderr:?}"
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

#[test]
fn test_native_nested_program_inherits_global_file_metadata_and_use_declarative() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let input_path = tmp.path().join("global-input.dat");
    let missing_path = tmp.path().join("global-missing.dat");
    std::fs::write(&input_path, "ABCDE\n").expect("write global input file");
    let input_path = input_path.to_string_lossy();
    let missing_path = missing_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. OUTER.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO "{input_path}".
    SELECT MISSING-FILE ASSIGN TO "{missing_path}".
DATA DIVISION.
FILE SECTION.
FD TEST-FILE GLOBAL.
01 TEST-REC PIC X(5).
FD MISSING-FILE GLOBAL.
01 MISSING-REC PIC X.
WORKING-STORAGE SECTION.
01 USE-FLAG PIC X VALUE "N".
PROCEDURE DIVISION.
DECLARATIVES.
USE-SECT SECTION.
    USE GLOBAL AFTER ERROR PROCEDURE ON INPUT.
USE-PARA.
    MOVE "Y" TO USE-FLAG.
END DECLARATIVES.
MAIN-PARA.
    CALL "INNER".
    DISPLAY TEST-REC.
    DISPLAY USE-FLAG.
    STOP RUN.

IDENTIFICATION DIVISION.
PROGRAM-ID. INNER.
DATA DIVISION.
PROCEDURE DIVISION.
INNER-PARA.
    OPEN INPUT TEST-FILE.
    READ TEST-FILE.
    CLOSE TEST-FILE.
    OPEN INPUT MISSING-FILE.
    EXIT PROGRAM.
END PROGRAM INNER.
END PROGRAM OUTER.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(code, 0, "program failed: stderr={stderr}");
    assert!(
        stdout.contains("ABCDE\nY\n"),
        "nested program should inherit GLOBAL FD assignment and global USE declarative: stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_declarative_runs_on_missing_input_open() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let missing_path = tmp.path().join("missing-input.dat");
    let missing_path = missing_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-OPEN.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IN-FILE ASSIGN TO "{missing_path}".
DATA DIVISION.
FILE SECTION.
FD IN-FILE.
01 IN-REC PIC X.
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON IN-FILE.
ERR-PARA.
    DISPLAY "OPEN-DECL".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    OPEN INPUT IN-FILE.
    DISPLAY "DONE".
    STOP RUN.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(
        code, 0,
        "program failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("OPEN-DECL\nDONE\n"),
        "missing input OPEN should dispatch USE AFTER EXCEPTION, got stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_declarative_runs_on_write_without_open() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let output_path = tmp.path().join("not-open-output.dat");
    let output_path = output_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-WRITE.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT OUT-FILE ASSIGN TO "{output_path}".
DATA DIVISION.
FILE SECTION.
FD OUT-FILE.
01 OUT-REC PIC X(3).
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON OUT-FILE.
ERR-PARA.
    DISPLAY "WRITE-DECL".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    MOVE "ABC" TO OUT-REC.
    WRITE OUT-REC.
    DISPLAY "DONE".
    STOP RUN.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(
        code, 0,
        "program failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("WRITE-DECL\nDONE\n"),
        "WRITE without OPEN should dispatch USE AFTER EXCEPTION, got stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_declarative_runs_on_rewrite_without_open() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let output_path = tmp.path().join("rewrite-output.dat");
    let output_path = output_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-REWRITE.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT OUT-FILE ASSIGN TO "{output_path}".
DATA DIVISION.
FILE SECTION.
FD OUT-FILE.
01 OUT-REC PIC X(3).
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON OUT-FILE.
ERR-PARA.
    DISPLAY "REWRITE-DECL".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    MOVE "ABC" TO OUT-REC.
    REWRITE OUT-REC.
    DISPLAY "DONE".
    STOP RUN.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(
        code, 0,
        "program failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("REWRITE-DECL\nDONE\n"),
        "REWRITE without OPEN should dispatch USE AFTER EXCEPTION, got stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_declarative_runs_on_delete_without_open() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let output_path = tmp.path().join("delete-output.dat");
    let output_path = output_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-DELETE.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT OUT-FILE ASSIGN TO "{output_path}"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS OUT-KEY.
DATA DIVISION.
FILE SECTION.
FD OUT-FILE.
01 OUT-REC.
   05 OUT-KEY PIC X(3).
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON OUT-FILE.
ERR-PARA.
    DISPLAY "DELETE-DECL".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    MOVE "ABC" TO OUT-KEY.
    DELETE OUT-FILE.
    DISPLAY "DONE".
    STOP RUN.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(
        code, 0,
        "program failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("DELETE-DECL\nDONE\n"),
        "DELETE without OPEN should dispatch USE AFTER EXCEPTION, got stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_declarative_runs_on_start_without_open() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let output_path = tmp.path().join("start-output.dat");
    let output_path = output_path.to_string_lossy();
    let src = format!(
        r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. DECL-START.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT OUT-FILE ASSIGN TO "{output_path}"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS OUT-KEY.
DATA DIVISION.
FILE SECTION.
FD OUT-FILE.
01 OUT-REC.
   05 OUT-KEY PIC X(3).
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON OUT-FILE.
ERR-PARA.
    DISPLAY "START-DECL".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    MOVE "ABC" TO OUT-KEY.
    START OUT-FILE KEY IS EQUAL TO OUT-KEY.
    DISPLAY "DONE".
    STOP RUN.
"#
    );

    let (stdout, stderr, code) = compile_and_run_no_sema(&src);
    assert_eq!(
        code, 0,
        "program failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("START-DECL\nDONE\n"),
        "START without OPEN should dispatch USE AFTER EXCEPTION, got stdout={stdout:?}, stderr={stderr:?}"
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
                            sign: None,
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            validation_values: Vec::new(),
                            occurs: None,
                            occurs_depending_on: None,
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
                            sign: None,
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            validation_values: Vec::new(),
                            occurs: None,
                            occurs_depending_on: None,
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
                sign: None,
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                validation_values: Vec::new(),
                occurs: None,
                occurs_depending_on: None,
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
                            sign: None,
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            validation_values: Vec::new(),
                            occurs: None,
                            occurs_depending_on: None,
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
                            sign: None,
                            picture: None,
                            is_numeric_edited: false,
                            blank_when_zero: false,
                            scale_adjustment: 0,
                            is_external: false,
                            initial_value: None,
                            validation_values: Vec::new(),
                            occurs: None,
                            occurs_depending_on: None,
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
                sign: None,
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                validation_values: Vec::new(),
                occurs: None,
                occurs_depending_on: None,
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
            from_subscripts: Vec::new(),
            to: cobol_hir::HirDataName::simple("WS-DST"),
            to_subscripts: Vec::new(),
            span: Span::dummy(),
        }],
        classes: Vec::new(),
        functions: Vec::new(),
        typedefs: Vec::new(),
        interfaces: Vec::new(),
        using_params: Vec::new(),
        file_organizations: std::collections::HashMap::new(),
        file_access_modes: std::collections::HashMap::new(),
        file_assignments: std::collections::HashMap::new(),
        file_optionals: std::collections::HashSet::new(),
        file_relative_keys: std::collections::HashMap::new(),
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
                        sign: None,
                        picture: None,
                        is_numeric_edited: false,
                        blank_when_zero: false,
                        scale_adjustment: 0,
                        is_external: false,
                        initial_value: None,
                        validation_values: Vec::new(),
                        occurs: None,
                        occurs_depending_on: None,
                        indexed_by: Vec::new(),
                        redefines: None,
                        renames: None,
                        screen_info: None,
                        justified: false,
                        span: Span::dummy(),
                    }],
                    size: 9,
                },
                sign: None,
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                validation_values: Vec::new(),
                occurs: None,
                occurs_depending_on: None,
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
                        sign: None,
                        picture: None,
                        is_numeric_edited: false,
                        blank_when_zero: false,
                        scale_adjustment: 0,
                        is_external: false,
                        initial_value: None,
                        validation_values: Vec::new(),
                        occurs: None,
                        occurs_depending_on: None,
                        indexed_by: Vec::new(),
                        redefines: None,
                        renames: None,
                        screen_info: None,
                        justified: false,
                        span: Span::dummy(),
                    }],
                    size: 9,
                },
                sign: None,
                picture: None,
                is_numeric_edited: false,
                blank_when_zero: false,
                scale_adjustment: 0,
                is_external: false,
                initial_value: None,
                validation_values: Vec::new(),
                occurs: None,
                occurs_depending_on: None,
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
            rounded: false,
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
        file_access_modes: std::collections::HashMap::new(),
        file_assignments: std::collections::HashMap::new(),
        file_optionals: std::collections::HashSet::new(),
        file_relative_keys: std::collections::HashMap::new(),
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
