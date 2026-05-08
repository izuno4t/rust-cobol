use super::*;

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
fn test_standalone_program_with_using_group_has_default_linkage_storage() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SUBONLY.
DATA DIVISION.
LINKAGE SECTION.
01 LK-GROUP.
   05 LK-CHAR PIC X.
01 LK-TEXT PIC X(3).
PROCEDURE DIVISION USING LK-GROUP LK-TEXT.
    MOVE 'Z' TO LK-CHAR.
    MOVE 'ABC' TO LK-TEXT.
    DISPLAY LK-CHAR.
    DISPLAY LK-TEXT.
    EXIT PROGRAM.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "standalone USING program should exit 0: {stderr}");
    assert!(
        stdout.contains('Z') && stdout.contains("ABC"),
        "default linkage storage should be readable and writable, got: {}",
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

#[test]
fn test_relative_random_read_without_key_uses_relative_key() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. RLRAND.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT RL-FD ASSIGN TO \"rl.dat\"
        ORGANIZATION IS RELATIVE
        ACCESS MODE IS RANDOM
        RELATIVE RL-KEY.
DATA DIVISION.
FILE SECTION.
FD RL-FD.
01 RL-REC PIC X(10).
WORKING-STORAGE SECTION.
01 RL-KEY PIC 9(4).
PROCEDURE DIVISION.
    OPEN I-O RL-FD.
    READ RL-FD RECORD.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    assert_eq!(hir.file_access_modes.get("RL-FD").copied(), Some(1));
    assert_eq!(hir.file_organizations.get("RL-FD").copied(), Some(2));
    assert_eq!(
        hir.file_relative_keys.get("RL-FD").map(|s| s.as_str()),
        Some("RL-KEY")
    );

    let HirStatement::Read { key, is_next, .. } = &hir.body[1] else {
        panic!("Expected READ statement");
    };
    assert!(!is_next);
    assert_eq!(key.as_deref(), Some("RL-KEY"));

    let c_code = generate_c(&hir);
    assert!(c_code.contains("cobol_file_read_relative(FILE_ID_RL_FD"));
    assert!(!c_code.contains("cobol_file_read_next(FILE_ID_RL_FD"));
}
