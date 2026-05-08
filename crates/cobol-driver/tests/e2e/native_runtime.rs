use super::*;

#[test]
fn test_redefines_occurs_table_survives_optimized_native_compile() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REDEF-ALIAS.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  I PIC 9(2) VALUE 0.
01  TOTAL PIC 9(5) VALUE 0.
01  RAW-DATA.
    05 FILLER PIC X(53) VALUE \"SSSSSTTTTT166WWWWWXXXXX060ALTKEY1FFFFFEEEEE135ALTKEY2\".
    05 FILLER PIC X(53) VALUE \"SSSSTTTTTT165WWWWXXXXXX061ALTKEY1FFFFEEEEEE136ALTKEY2\".
    05 FILLER PIC X(53) VALUE \"SSSTTTTTTT164WWWXXXXXXX062ALTKEY1FFFEEEEEEE137ALTKEY2\".
    05 FILLER PIC X(53) VALUE \"SSTTTTTTTT163WWXXXXXXXX063ALTKEY1FFEEEEEEEE138ALTKEY2\".
01  TABLE-DATA REDEFINES RAW-DATA.
    05 ROWS OCCURS 4 TIMES.
       10 RKEY.
          15 FILLER PIC X(10).
          15 RKEY-N PIC 9(3).
       10 AKEY1.
          15 FILLER PIC X(10).
          15 AKEY1-N PIC 9(3).
          15 FILLER PIC X(7).
       10 AKEY2.
          15 FILLER PIC X(10).
          15 AKEY2-N PIC 9(3).
          15 FILLER PIC X(7).
PROCEDURE DIVISION.
    PERFORM VARYING I FROM 1 BY 1 UNTIL I > 4
        ADD RKEY-N OF ROWS(I) TO TOTAL
        ADD AKEY1-N OF ROWS(I) TO TOTAL
        ADD AKEY2-N OF ROWS(I) TO TOTAL
    END-PERFORM.
    DISPLAY TOTAL.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("1450"),
        "REDEFINES table values should remain stable under optimized native compile; stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_redefines_occurs_alphanumeric_element_uses_byte_stride() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. REDEF-OCCURS-BYTES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  RAW-DATA.
    05 RAW-BYTES PIC X(10) VALUE \"ABCDEFGHIJ\".
    05 TABLE-BYTES REDEFINES RAW-BYTES PIC X(5) OCCURS 2 TIMES.
PROCEDURE DIVISION.
    DISPLAY TABLE-BYTES(2).
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("FGHIJ"),
        "REDEFINES OCCURS should address the second 5-byte element, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_filler_group_redefines_preserves_child_overlay() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FILLER-GROUP-REDEF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  BASE-AREA PIC X(5).
01  FILLER REDEFINES BASE-AREA.
    05 FIRST-CHAR PIC X.
    05 REST-CHARS PIC X(4).
PROCEDURE DIVISION.
    MOVE \"ABCDE\" TO BASE-AREA.
    DISPLAY FIRST-CHAR.
    MOVE \"Z\" TO FIRST-CHAR.
    DISPLAY BASE-AREA.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("A") && stdout.contains("ZBCDE"),
        "FILLER group REDEFINES children should share the target storage, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_currency_picture_redefines_occurs_and_blank_when_zero() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CURRENCY-REDEF-BLANK.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SPECIAL-NAMES.
    CURRENCY \"<\".
DATA DIVISION.
WORKING-STORAGE SECTION.
01  COMPLETE-01.
    02 COMPLETE-F.
       03 FILLER PIC X(90) VALUE SPACE.
       03 FL-LESS PIC <(3),<<<.99 VALUE \" <1,111.11\".
    02 COMPLETE-FORMAT REDEFINES COMPLETE-F PIC X(5) OCCURS 20 TIMES.
    02 MORE-COMPLETE-FORMAT BLANK WHEN ZERO PIC 9 VALUE \"5\".
01  DATA-P PIC 999 VALUE \"000\" BLANK WHEN ZERO.
01  DATA-P1 REDEFINES DATA-P PIC XXX.
PROCEDURE DIVISION.
    DISPLAY COMPLETE-FORMAT(19).
    DISPLAY MORE-COMPLETE-FORMAT.
    DISPLAY DATA-P1.
    MOVE ZERO TO MORE-COMPLETE-FORMAT.
    IF MORE-COMPLETE-FORMAT = SPACE
        DISPLAY \"BLANK\"
    ELSE
        DISPLAY MORE-COMPLETE-FORMAT
    END-IF.
    MOVE ZERO TO FL-LESS.
    DISPLAY FL-LESS.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    let c_code = generate_c(&parse_and_lower(src));

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        c_code.contains("#ifndef COMPLETE_FORMAT\n#define COMPLETE_FORMAT"),
        "duplicate REDEFINES aliases should be guarded in generated C:\n{c_code}"
    );
    assert!(
        !stderr.contains("macro redefined"),
        "group REDEFINES should not emit duplicate C macro definitions, stderr={stderr}"
    );
    assert!(
        stdout.contains(" <1,1"),
        "currency PICTURE length should preserve the redefined table byte layout, stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains('5'),
        "numeric DISPLAY VALUE string should initialize BLANK WHEN ZERO item, stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("000"),
        "BLANK WHEN ZERO should not blank VALUE storage observed through REDEFINES, stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("BLANK"),
        "MOVE ZERO to BLANK WHEN ZERO item should store spaces, stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("      <.00"),
        "currency floating insertion should format zero as the active currency symbol, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_numeric_edited_zero_star_fill_suppresses_currency() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ZERO-STAR-CURRENCY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  EDITED PIC $**.**CR VALUE ZERO.
01  VIEW-EDITED PIC X(8).
PROCEDURE DIVISION.
    MOVE ZERO TO EDITED.
    MOVE EDITED TO VIEW-EDITED.
    DISPLAY VIEW-EDITED.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("***.****"),
        "zero star fill should suppress the floating currency symbol, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_add_negative_to_unsigned_display_stores_unsigned_digits() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-NEG-UNSIGNED.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  A PIC S9(18) VALUE -555555555555555555 COMPUTATIONAL.
77  B PIC 9(18) VALUE ZERO.
PROCEDURE DIVISION.
    MOVE 000000777777777777 TO B.
    ADD A TO B.
    IF B = 555554777777777778
        DISPLAY \"PASS\"
    ELSE
        DISPLAY B
    END-IF.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "ADD into unsigned DISPLAY should store unsigned digits after arithmetic, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_add_giving_size_error_preserves_all_receiving_items() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-GIVING-SIZE.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  BIG PIC S9(17) VALUE 22222222222222222.
77  A PIC 9V9 VALUE 1.1.
77  B PIC 9V9 VALUE 2.3.
77  R1 PIC 99V9 VALUE ZERO.
77  R2 PIC 99 VALUE ZERO.
77  FLAG PIC X VALUE SPACE.
PROCEDURE DIVISION.
    ADD BIG A 6 B GIVING R1 R2 R1 ROUNDED R2 ROUNDED
        ON SIZE ERROR MOVE \"A\" TO FLAG.
    IF R1 = ZERO AND R2 = ZERO AND FLAG = \"A\"
        DISPLAY \"PASS\"
    ELSE
        DISPLAY R1
        DISPLAY R2
        DISPLAY FLAG
    END-IF.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "ADD GIVING with SIZE ERROR should preserve receiving items, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_divide_giving_decimal_target_preserves_fraction() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DIV-GIVING-FRAC.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  DIVISOR PIC S99 VALUE 16.
77  DIVIDEND PIC S999 VALUE 174.
77  QUOTIENT PIC S9(4)V9 VALUE ZERO.
77  REM PIC ***99 VALUE ZERO.
PROCEDURE DIVISION.
    DIVIDE DIVISOR INTO DIVIDEND GIVING QUOTIENT REMAINDER REM.
    IF QUOTIENT = 10.8
        DISPLAY \"PASS\"
    ELSE
        DISPLAY QUOTIENT
        DISPLAY REM
    END-IF.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "DIVIDE GIVING should preserve the quotient target scale, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_divide_giving_rounded_remainder_uses_truncated_quotient() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DIV-ROUNDED-REM.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  DIVISOR PIC 99V9 VALUE 10.0.
77  DIVIDEND PIC 9V9(17) VALUE 3.14159265358979323.
77  QUOTIENT PIC 9V9(5) VALUE ZERO.
77  REM PIC .9999/99999,99999,99 VALUE ZERO.
PROCEDURE DIVISION.
    DIVIDE DIVISOR INTO DIVIDEND GIVING QUOTIENT ROUNDED REMAINDER REM.
    IF QUOTIENT = 0.31416 AND REM = \".0000/92653,58979,32\"
        DISPLAY \"PASS\"
    ELSE
        DISPLAY QUOTIENT
        DISPLAY REM
    END-IF.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "DIVIDE ROUNDED REMAINDER should use the unrounded quotient for remainder, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_special_names_switch_conditions_are_complements() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SWITCH-COMPLEMENT.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SPECIAL-NAMES.
    \"DUMMY-SWITCH\" IS ABBREV-SWITCH
        ON ON-SWITCH
        OFF IS OFF-SWITCH.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  COUNT-X PIC 9 VALUE 0.
PROCEDURE DIVISION.
    IF ON-SWITCH ADD 1 TO COUNT-X.
    IF OFF-SWITCH ADD 1 TO COUNT-X.
    DISPLAY COUNT-X.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "1"),
        "ON/OFF switch condition names should be boolean complements, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_special_names_switch_conditions_follow_set_status() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SWITCH-SET-STATUS.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SPECIAL-NAMES.
    XXXXX051 IS SW-1
        ON STATUS IS ON-SWITCH-1
        OFF STATUS IS OFF-SWITCH-1
    XXXXX052 IS SW-2
        ON IS ON-SWITCH-2
        OFF IS OFF-SWITCH-2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  COUNT-X PIC 9 VALUE 0.
PROCEDURE DIVISION.
    IF ON-SWITCH-1 ADD 1 TO COUNT-X.
    IF OFF-SWITCH-2 ADD 1 TO COUNT-X.
    SET SW-1 SW-2 TO OFF.
    IF OFF-SWITCH-1 ADD 1 TO COUNT-X.
    IF OFF-SWITCH-2 ADD 1 TO COUNT-X.
    SET SW-1 TO ON
        SW-2 TO OFF.
    IF ON-SWITCH-1 ADD 1 TO COUNT-X.
    IF OFF-SWITCH-2 ADD 1 TO COUNT-X.
    DISPLAY COUNT-X.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "6"),
        "SPECIAL-NAMES switch conditions should follow the switch storage and SET status, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_special_names_custom_class_condition() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CUSTOM-CLASS.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SPECIAL-NAMES.
    CLASS ORDINAL-A-THROUGH-D IS \"A\" THROUGH \"D\"
    CLASS ACTUAL-ABCD IS \"ABCD\".
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC X VALUE \"C\".
01  WS-B PIC X(5) VALUE \"ADCBA\".
01  WS-C PIC X(5) VALUE \"VWXYZ\".
01  COUNT-X PIC 9 VALUE 0.
PROCEDURE DIVISION.
    IF WS-A ORDINAL-A-THROUGH-D ADD 1 TO COUNT-X.
    IF WS-B ACTUAL-ABCD ADD 1 TO COUNT-X.
    IF WS-C NOT ACTUAL-ABCD ADD 1 TO COUNT-X.
    DISPLAY COUNT-X.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "3"),
        "SPECIAL-NAMES custom CLASS clauses should define character sets, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_move_signed_display_pic_p_to_alphanumeric_expands_scale() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MOVE-P-SCALE-ALPHA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC S9P(17) SIGN LEADING SEPARATE VALUE -100000000000000000.
01  DST PIC X(18) VALUE SPACES.
PROCEDURE DIVISION.
    MOVE SRC TO DST.
    DISPLAY DST.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "100000000000000000"),
        "MOVE from signed DISPLAY PIC P to alphanumeric should strip sign after restoring scale, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_leading_p_display_initial_value_preserves_stored_digit() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LEADING-P-DISPLAY.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC SP(8)9 SIGN TRAILING SEPARATE VALUE .000000001.
01  DST REDEFINES SRC PIC X(2).
PROCEDURE DIVISION.
    DISPLAY DST.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("1+"),
        "leading P DISPLAY initialization should store the significant digit and sign, stdout={stdout:?}, stderr={stderr:?}"
    );
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
