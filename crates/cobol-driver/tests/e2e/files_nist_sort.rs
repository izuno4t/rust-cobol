use super::*;

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
fn test_native_close_with_lock_sets_reopen_status_38() {
    let _ = std::fs::remove_file("/tmp/cobol_close_lock_test.dat");
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LOCKTST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO '/tmp/cobol_close_lock_test.dat'
        ORGANIZATION IS SEQUENTIAL
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD TEST-FILE.
01 TEST-RECORD PIC X(5).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
    OPEN OUTPUT TEST-FILE.
    MOVE 'HELLO' TO TEST-RECORD.
    WRITE TEST-RECORD.
    CLOSE TEST-FILE WITH LOCK.
    MOVE '**' TO WS-STATUS.
    OPEN INPUT TEST-FILE.
    DISPLAY WS-STATUS.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_close_lock_test.dat");
    assert_eq!(code, 0, "stderr: {stderr}\nstdout: {stdout}");
    assert!(
        stdout.trim().starts_with("38"),
        "OPEN after CLOSE WITH LOCK should set status 38, got: {stdout:?}"
    );
}

#[test]
fn test_native_variable_record_write_bounds_set_status_44() {
    let _ = std::fs::remove_file("/tmp/cobol_varrec_bounds_test.dat");
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VRBND.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT TEST-FILE ASSIGN TO '/tmp/cobol_varrec_bounds_test.dat'
        ORGANIZATION IS SEQUENTIAL
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD TEST-FILE
    RECORD IS VARYING IN SIZE FROM 5 TO 10 CHARACTERS
    DEPENDING ON WS-LEN.
01 TEST-RECORD PIC X(10).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
01 WS-LEN PIC 9(2).
PROCEDURE DIVISION.
    OPEN OUTPUT TEST-FILE.
    MOVE 4 TO WS-LEN.
    WRITE TEST-RECORD.
    DISPLAY WS-STATUS.
    MOVE 5 TO WS-LEN.
    WRITE TEST-RECORD.
    DISPLAY WS-STATUS.
    MOVE 11 TO WS-LEN.
    WRITE TEST-RECORD.
    DISPLAY WS-STATUS.
    CLOSE TEST-FILE.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_varrec_bounds_test.dat");
    assert_eq!(code, 0, "stderr: {stderr}\nstdout: {stdout}");
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["44", "00", "44"]);
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
    let c_code = generate_c(&parse_and_lower(src));
    assert!(
        !c_code.contains("cobol_decimal_to_display(&MULTIPLY_DATA__MULT1"),
        "group-stored DISPLAY numeric must not be passed directly to CobolDecimal API:\n{c_code}"
    );
    assert!(
        c_code.contains("CobolDecimal _display_dec"),
        "group-stored DISPLAY numeric should be converted through a decimal temporary:\n{c_code}"
    );

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
        NOT INVALID KEY DISPLAY "DELETE OK"
    END-DELETE.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_file_delete_record(FILE_ID_IX_FILE"),
        "should generate DELETE statement code, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("DELETE FAILED") && c_code.contains("DELETE OK"),
        "DELETE INVALID KEY / NOT INVALID KEY branches should be preserved, got:\n{}",
        c_code
    );
}

#[test]
fn test_write_from_subscripted_group_moves_to_record_area() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. WRFROM.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_write_from_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS OUT-KEY.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 OUT-REC.
   05 OUT-KEY PIC 9(5).
   05 OUT-TEXT PIC X(10).
WORKING-STORAGE SECTION.
01 SRC-TABLE OCCURS 2.
   05 SRC-KEY PIC 9(5).
   05 SRC-TEXT PIC X(10).
PROCEDURE DIVISION.
    OPEN OUTPUT IX-FILE.
    WRITE OUT-REC FROM SRC-TABLE (1)
        INVALID KEY DISPLAY "BAD"
    END-WRITE.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("memset(&OUT_REC, ' ', 15);")
            && c_code.contains("memcpy(&OUT_REC, &SRC_TABLE[(((int64_t)1)) - 1]"),
        "WRITE FROM should move the subscripted source into the FD record area, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_file_write(FILE_ID_IX_FILE, (const uint8_t*)&OUT_REC, 15)"),
        "WRITE should pass the FD record area to the runtime, got:\n{}",
        c_code
    );
}

#[test]
fn test_read_duplicate_key_status_does_not_dispatch_declarative() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. RDDUP02.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_read_dup_status_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        ALTERNATE RECORD KEY IS IX-ALT WITH DUPLICATES
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-ALT PIC X(3).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON IX-FILE.
ERR-PARA.
    DISPLAY "ERR".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    OPEN INPUT IX-FILE.
    READ IX-FILE.
    CLOSE IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("if (!((_fs == 0 || _fs == 2) || 0 || 0))"),
        "READ status 02 is a successful duplicate-key condition and must not dispatch declaratives, got:\n{}",
        c_code
    );
}

#[test]
fn test_start_without_key_uses_indexed_record_key() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. STARTKEY.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_start_key_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS SEQUENTIAL
        RECORD KEY IS IX-KEY.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(10).
PROCEDURE DIVISION.
    START IX-FILE.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains(
            "cobol_file_start(FILE_ID_IX_FILE, (const uint8_t*)IX_RECORD__IX_KEY, 5, 0, 0)"
        ),
        "START without KEY should use the indexed file's record key, got:\n{}",
        c_code
    );
}

#[test]
fn test_start_invalid_key_phrase_does_not_dispatch_declarative() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. STARTDECL.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT IX-FILE ASSIGN TO "/tmp/cobol_start_decl_test.dat"
        ORGANIZATION IS INDEXED
        ACCESS MODE IS DYNAMIC
        RECORD KEY IS IX-KEY
        FILE STATUS IS WS-STATUS.
DATA DIVISION.
FILE SECTION.
FD IX-FILE.
01 IX-RECORD.
   05 IX-KEY PIC 9(5).
   05 IX-DATA PIC X(10).
WORKING-STORAGE SECTION.
01 WS-STATUS PIC XX.
PROCEDURE DIVISION.
DECLARATIVES.
ERR-SEC SECTION.
    USE AFTER STANDARD EXCEPTION PROCEDURE ON IX-FILE.
ERR-PARA.
    DISPLAY "ERR".
END DECLARATIVES.
MAIN-SEC SECTION.
MAIN-PARA.
    START IX-FILE
        INVALID KEY DISPLAY "HANDLED"
    END-START.
    STOP RUN.
"#;
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        !c_code.contains("_check_file_declarative(\"IX_FILE\""),
        "START INVALID KEY phrase should handle the condition without declarative dispatch, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("HANDLED"),
        "START INVALID KEY body should be preserved, got:\n{}",
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

#[test]
fn test_native_sort_using_variable_file_giving_variable_file() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. SORTVAR.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SORT-FILE ASSIGN TO "/tmp/cobol_sort_var_work.dat".
    SELECT INPUT-FILE ASSIGN TO "/tmp/cobol_sort_var_in.dat".
    SELECT OUTPUT-FILE ASSIGN TO "/tmp/cobol_sort_var_out.dat".
DATA DIVISION.
FILE SECTION.
SD SORT-FILE.
01 SORT-RECORD PIC X(9).
FD INPUT-FILE
   RECORD CONTAINS 5 TO 9 CHARACTERS.
01 INPUT-SHORT PIC X(5).
01 INPUT-LONG PIC X(9).
FD OUTPUT-FILE
   RECORD CONTAINS 5 TO 9 CHARACTERS.
01 OUTPUT-SHORT PIC X(5).
01 OUTPUT-RECORD PIC X(9).
WORKING-STORAGE SECTION.
01 WS-EOF PIC 9 VALUE 0.
01 WS-OUT PIC X(9).
PROCEDURE DIVISION.
    OPEN OUTPUT INPUT-FILE.
    MOVE "B2222" TO INPUT-SHORT.
    WRITE INPUT-SHORT.
    MOVE "A11111111" TO INPUT-LONG.
    WRITE INPUT-LONG.
    CLOSE INPUT-FILE.
    SORT SORT-FILE
        ON ASCENDING KEY SORT-RECORD
        USING INPUT-FILE
        GIVING OUTPUT-FILE.
    OPEN INPUT OUTPUT-FILE.
    PERFORM UNTIL WS-EOF = 1
        READ OUTPUT-FILE INTO WS-OUT
            AT END MOVE 1 TO WS-EOF
            NOT AT END DISPLAY WS-OUT
        END-READ
    END-PERFORM.
    CLOSE OUTPUT-FILE.
    STOP RUN.
"#;
    let _ = std::fs::remove_file("/tmp/cobol_sort_var_in.dat");
    let _ = std::fs::remove_file("/tmp/cobol_sort_var_out.dat");
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_sort_var_in.dat");
    let _ = std::fs::remove_file("/tmp/cobol_sort_var_out.dat");
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected sorted variable records:\n{stdout}"
    );
    assert_eq!(lines[0], "A11111111");
    assert!(
        lines[1].starts_with("B2222"),
        "short record drifted: {stdout}"
    );
}

#[test]
fn test_native_merge_using_giving_variable_files() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. MERGEVAR.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT MERGE-FILE ASSIGN TO "/tmp/cobol_merge_var_work.dat".
    SELECT INPUT-A ASSIGN TO "/tmp/cobol_merge_var_a.dat".
    SELECT INPUT-B ASSIGN TO "/tmp/cobol_merge_var_b.dat".
    SELECT OUTPUT-FILE ASSIGN TO "/tmp/cobol_merge_var_out.dat".
DATA DIVISION.
FILE SECTION.
SD MERGE-FILE.
01 MERGE-RECORD.
   05 MERGE-KEY PIC 9(3).
   05 MERGE-TEXT PIC X(2).
FD INPUT-A
   RECORD CONTAINS 5 CHARACTERS.
01 INPUT-A-RECORD.
   05 INPUT-A-KEY PIC 9(3).
   05 INPUT-A-TEXT PIC X(2).
FD INPUT-B
   RECORD CONTAINS 5 CHARACTERS.
01 INPUT-B-RECORD.
   05 INPUT-B-KEY PIC 9(3).
   05 INPUT-B-TEXT PIC X(2).
FD OUTPUT-FILE
   RECORD CONTAINS 5 CHARACTERS.
01 OUTPUT-RECORD PIC X(5).
WORKING-STORAGE SECTION.
01 WS-EOF PIC 9 VALUE 0.
PROCEDURE DIVISION.
    OPEN OUTPUT INPUT-A.
    MOVE "001A1" TO INPUT-A-RECORD.
    WRITE INPUT-A-RECORD.
    MOVE "003A3" TO INPUT-A-RECORD.
    WRITE INPUT-A-RECORD.
    CLOSE INPUT-A.
    OPEN OUTPUT INPUT-B.
    MOVE "002B2" TO INPUT-B-RECORD.
    WRITE INPUT-B-RECORD.
    MOVE "004B4" TO INPUT-B-RECORD.
    WRITE INPUT-B-RECORD.
    CLOSE INPUT-B.
    MERGE MERGE-FILE
        ON ASCENDING KEY MERGE-KEY
        USING INPUT-A INPUT-B
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
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_b.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_out.dat");
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_b.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_var_out.dat");
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "001A1\n002B2\n003A3\n004B4\n");
}

#[test]
fn test_native_merge_writes_all_giving_files() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. MERGEMULTI.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT MERGE-FILE ASSIGN TO "/tmp/cobol_merge_multi_work.dat".
    SELECT INPUT-A ASSIGN TO "/tmp/cobol_merge_multi_a.dat".
    SELECT INPUT-B ASSIGN TO "/tmp/cobol_merge_multi_b.dat".
    SELECT OUTPUT-A ASSIGN TO "/tmp/cobol_merge_multi_out_a.dat".
    SELECT OUTPUT-B ASSIGN TO "/tmp/cobol_merge_multi_out_b.dat".
DATA DIVISION.
FILE SECTION.
SD MERGE-FILE.
01 MERGE-RECORD.
   05 MERGE-KEY PIC 9(3).
   05 MERGE-TEXT PIC X(2).
FD INPUT-A.
01 INPUT-A-RECORD PIC X(5).
FD INPUT-B.
01 INPUT-B-RECORD PIC X(5).
FD OUTPUT-A.
01 OUTPUT-A-RECORD PIC X(5).
FD OUTPUT-B.
01 OUTPUT-B-RECORD PIC X(5).
WORKING-STORAGE SECTION.
01 WS-EOF PIC 9 VALUE 0.
PROCEDURE DIVISION.
    OPEN OUTPUT INPUT-A.
    MOVE "001A1" TO INPUT-A-RECORD.
    WRITE INPUT-A-RECORD.
    CLOSE INPUT-A.
    OPEN OUTPUT INPUT-B.
    MOVE "002B2" TO INPUT-B-RECORD.
    WRITE INPUT-B-RECORD.
    CLOSE INPUT-B.
    MERGE MERGE-FILE
        ON ASCENDING KEY MERGE-KEY
        USING INPUT-A INPUT-B
        GIVING OUTPUT-A OUTPUT-B.
    OPEN INPUT OUTPUT-B.
    PERFORM UNTIL WS-EOF = 1
        READ OUTPUT-B
            AT END MOVE 1 TO WS-EOF
            NOT AT END DISPLAY OUTPUT-B-RECORD
        END-READ
    END-PERFORM.
    CLOSE OUTPUT-B.
    STOP RUN.
"#;
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_b.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_out_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_out_b.dat");
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_b.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_out_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_multi_out_b.dat");
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "001A1\n002B2\n");
}

#[test]
fn test_native_merge_using_output_procedure_returns_records() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. MERGEOUT.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT MERGE-FILE ASSIGN TO "/tmp/cobol_merge_out_work.dat".
    SELECT INPUT-A ASSIGN TO "/tmp/cobol_merge_out_a.dat".
    SELECT INPUT-B ASSIGN TO "/tmp/cobol_merge_out_b.dat".
DATA DIVISION.
FILE SECTION.
SD MERGE-FILE.
01 MERGE-RECORD.
   05 MERGE-KEY PIC 9(3).
   05 MERGE-TEXT PIC X(2).
FD INPUT-A.
01 INPUT-A-RECORD PIC X(5).
FD INPUT-B.
01 INPUT-B-RECORD PIC X(5).
PROCEDURE DIVISION.
    OPEN OUTPUT INPUT-A.
    MOVE "001A1" TO INPUT-A-RECORD.
    WRITE INPUT-A-RECORD.
    MOVE "003A3" TO INPUT-A-RECORD.
    WRITE INPUT-A-RECORD.
    CLOSE INPUT-A.
    OPEN OUTPUT INPUT-B.
    MOVE "002B2" TO INPUT-B-RECORD.
    WRITE INPUT-B-RECORD.
    MOVE "004B4" TO INPUT-B-RECORD.
    WRITE INPUT-B-RECORD.
    CLOSE INPUT-B.
    MERGE MERGE-FILE
        ON ASCENDING KEY MERGE-KEY
        USING INPUT-A INPUT-B
        OUTPUT PROCEDURE IS DRAIN-MERGE THRU DRAIN-END.
    STOP RUN.
DRAIN-MERGE.
    RETURN MERGE-FILE
        AT END GO TO DRAIN-END
    END-RETURN.
    DISPLAY MERGE-RECORD.
    GO TO DRAIN-MERGE.
DRAIN-END.
    EXIT.
"#;
    let _ = std::fs::remove_file("/tmp/cobol_merge_out_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_out_b.dat");
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_merge_out_a.dat");
    let _ = std::fs::remove_file("/tmp/cobol_merge_out_b.dat");
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "001A1\n002B2\n003A3\n004B4\n");
}

#[test]
fn test_native_sort_input_output_procedure_returns_sorted_records() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. SORTPROC.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SORT-FILE ASSIGN TO "SORTWORK".
DATA DIVISION.
FILE SECTION.
SD SORT-FILE.
01 S-REC.
   05 S-KEY PIC 9.
PROCEDURE DIVISION.
MAIN.
    SORT SORT-FILE
        ON ASCENDING KEY S-KEY
        INPUT PROCEDURE IS MAKE-INPUT THRU MAKE-END
        OUTPUT PROCEDURE IS READ-OUTPUT THRU READ-END.
    STOP RUN.
MAKE-INPUT.
    MOVE 3 TO S-KEY.
    RELEASE S-REC.
    MOVE 1 TO S-KEY.
    RELEASE S-REC.
MAKE-END.
    EXIT.
READ-OUTPUT.
    RETURN SORT-FILE AT END GO TO READ-END.
    DISPLAY S-KEY.
    GO TO READ-OUTPUT.
READ-END.
    EXIT.
"#;
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "1\n3\n");
}

#[test]
fn test_native_sort_input_procedure_sorts_redefined_display_numeric_keys() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. SORTKEYS.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SORT-FILE ASSIGN TO "SORTWORK".
DATA DIVISION.
FILE SECTION.
SD SORT-FILE.
01 S-REC.
   05 KEYS-GROUP.
      10 KEY-1 PIC 9.
      10 KEY-2 PIC 99.
      10 KEY-3 PIC 999.
      10 KEY-4 PIC 9999.
      10 KEY-5 PIC 9(5).
   05 RDF-KEYS REDEFINES KEYS-GROUP PIC 9(15).
PROCEDURE DIVISION.
MAIN.
    SORT SORT-FILE
        ON ASCENDING KEY KEY-1
        ON DESCENDING KEY KEY-2
        ON ASCENDING KEY KEY-3
        DESCENDING KEY-4 KEY-5
        INPUT PROCEDURE IS MAKE-INPUT THRU MAKE-END
        OUTPUT PROCEDURE IS READ-OUTPUT THRU READ-END.
    STOP RUN.
MAKE-INPUT.
    MOVE 900009000000000 TO RDF-KEYS.
    RELEASE S-REC.
    MOVE 009000000900009 TO RDF-KEYS.
    RELEASE S-REC.
    MOVE 900008000000000 TO RDF-KEYS.
    RELEASE S-REC.
    MOVE 009000000900008 TO RDF-KEYS.
    RELEASE S-REC.
MAKE-END.
    EXIT.
READ-OUTPUT.
    RETURN SORT-FILE AT END GO TO READ-END.
    IF RDF-KEYS = 009000000900009
        DISPLAY "LOW9"
        GO TO READ-OUTPUT
    END-IF.
    IF RDF-KEYS = 009000000900008
        DISPLAY "LOW8"
        GO TO READ-OUTPUT
    END-IF.
    IF RDF-KEYS = 900008000000000
        DISPLAY "HIGH8"
        GO TO READ-OUTPUT
    END-IF.
    IF RDF-KEYS = 900009000000000
        DISPLAY "HIGH9"
        GO TO READ-OUTPUT
    END-IF.
    DISPLAY "BAD".
    GO TO READ-OUTPUT.
READ-END.
    EXIT.
"#;
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "LOW9\nLOW8\nHIGH8\nHIGH9\n");
}

#[test]
fn test_native_sort_preserves_separate_sign_display_numeric_record_layout() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. SORTSEPSIGN.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT SORT-FILE ASSIGN TO "SORTWORK".
DATA DIVISION.
FILE SECTION.
SD SORT-FILE.
01 S-REC.
   05 S-KEY-1 PIC S9 SIGN IS LEADING SEPARATE.
   05 S-KEY-2 PIC SV9 SIGN IS TRAILING SEPARATE.
   05 S-TAG   PIC X.
PROCEDURE DIVISION.
MAIN.
    SORT SORT-FILE
        ON ASCENDING KEY S-KEY-1
        ON ASCENDING KEY S-KEY-2
        INPUT PROCEDURE IS MAKE-INPUT THRU MAKE-END
        OUTPUT PROCEDURE IS READ-OUTPUT THRU READ-END.
    STOP RUN.
MAKE-INPUT.
    MOVE 1 TO S-KEY-1.
    MOVE .6 TO S-KEY-2.
    MOVE "P" TO S-TAG.
    RELEASE S-REC.
    MOVE 1 TO S-KEY-1.
    MOVE -.6 TO S-KEY-2.
    MOVE "N" TO S-TAG.
    RELEASE S-REC.
MAKE-END.
    EXIT.
READ-OUTPUT.
    RETURN SORT-FILE AT END GO TO READ-END.
    DISPLAY S-TAG.
    GO TO READ-OUTPUT.
READ-END.
    EXIT.
"#;
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "N\nP\n");
}

#[test]
fn test_native_group_move_from_nested_group_uses_cobol_layout_for_binary_members() {
    let src = r#"
IDENTIFICATION DIVISION.
PROGRAM-ID. GMOVEBIN.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC.
   05 SRC-GROUP.
      10 SRC-BIN PIC S9(4) COMP.
      10 SRC-DIS PIC 999.
      10 SRC-TAG PIC X.
01 DST.
   05 DST-BIN PIC S9(4) COMP.
   05 DST-DIS PIC 999.
   05 DST-TAG PIC X.
PROCEDURE DIVISION.
    MOVE -12 TO SRC-BIN.
    MOVE 345 TO SRC-DIS.
    MOVE "Z" TO SRC-TAG.
    MOVE SRC-GROUP TO DST.
    IF DST-BIN = -12
        DISPLAY "BIN"
    ELSE
        DISPLAY "BAD-BIN"
    END-IF.
    IF DST-DIS = 345
        DISPLAY "DIS"
    ELSE
        DISPLAY "BAD-DIS"
    END-IF.
    DISPLAY DST-TAG.
    STOP RUN.
"#;
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native execution failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "BIN\nDIS\nZ\n");
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
