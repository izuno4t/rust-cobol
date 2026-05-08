use super::*;

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
fn test_native_inspect_tallying_before_after_initial_limits_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-RANGE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA  PIC X(83) VALUE
   'AH YES AH YES W.C. FRITOES HERE. ANYONE WHO HATES DOGS AND KIDS CAN NOT BE ALL BAD.'.
01 WS-AFTER PIC 9(3) VALUE 0.
01 WS-BEFORE PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-DATA TALLYING WS-AFTER FOR CHARACTERS AFTER ' W'.
    INSPECT WS-DATA TALLYING WS-BEFORE FOR ALL SPACE BEFORE INITIAL 'W.C.'.
    DISPLAY WS-AFTER.
    DISPLAY WS-BEFORE.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert!(
        lines.iter().any(|line| *line == "068" || *line == "68"),
        "INSPECT AFTER INITIAL should tally only after delimiter, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| *line == "004" || *line == "4"),
        "INSPECT BEFORE INITIAL should tally only before delimiter, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_replacing_before_after_initial_limits_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-REPLACE-RANGE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA  PIC X(83) VALUE
   'AH YES AH YES W.C. FRITOES HERE. ANYONE WHO HATES DOGS AND KIDS CAN NOT BE ALL BAD.'.
PROCEDURE DIVISION.
    INSPECT WS-DATA
        REPLACING LEADING 'AH' BY 'OH' BEFORE INITIAL ' AH YES'
                  FIRST 'I' BY 'O' AFTER INITIAL '.'
                  ALL '. ' BY ', ' AFTER INITIAL 'HE'.
    DISPLAY WS-DATA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.contains("OH YES AH YES W.C. FR"),
        "INSPECT REPLACING BEFORE/AFTER should limit each phrase range, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_converting_before_after_limits_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-CONVERT-RANGE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(13) VALUE 'GADQAUZTABAGA'.
PROCEDURE DIVISION.
    INSPECT WS-DATA CONVERTING 'AU' TO '23' BEFORE 'B' AFTER 'Q'.
    DISPLAY WS-DATA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.contains("GADQ23ZT2BAGA"),
        "INSPECT CONVERTING BEFORE/AFTER should limit the conversion range, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_converting_before_limits_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-CONVERT-BEFORE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(13) VALUE 'GA4Q23ZT2BAGA'.
PROCEDURE DIVISION.
    INSPECT WS-DATA CONVERTING 'GA' TO '67' BEFORE 'B'.
    DISPLAY WS-DATA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.contains("674Q23ZT2BAGA"),
        "INSPECT CONVERTING BEFORE should leave bytes after the delimiter unchanged, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_tallying_replacing_tally_phrases_share_scan_position() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-SERIES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA  PIC X(83) VALUE
   'AH YES AH YES W.C. FRITOES HERE. ANYONE WHO HATES DOGS AND KIDS CAN NOT BE ALL BAD.'.
01 C-A      PIC 9(3) VALUE 0.
01 C-LEAD   PIC 9(3) VALUE 0.
01 C-CHARS  PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-DATA
        TALLYING C-A FOR ALL 'A'
                 C-LEAD FOR LEADING 'AH'
                 C-CHARS FOR CHARACTERS BEFORE '.'
        REPLACING FIRST 'L ' BY 'ZZ' AFTER INITIAL 'AL'
                  FIRST 'BAD' BY 'ZZZ' AFTER 'L '
                  LEADING 'BAD' BY 'ZZZ' BEFORE INITIAL 'Q'
                  FIRST 'BAD' BY 'ZZZ' BEFORE INITIAL 'Z'
                  FIRST 'BAD' BY 'ZZZ' AFTER 'ALL '
                  ALL '.' BY 'Z' AFTER 'AL'.
    DISPLAY C-A.
    DISPLAY C-LEAD.
    DISPLAY C-CHARS.
    DISPLAY WS-DATA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert!(
        lines.iter().any(|line| *line == "008" || *line == "8"),
        "first tally phrase should count all A characters, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| *line == "000" || *line == "0"),
        "later LEADING phrase should not recount bytes consumed by prior phrase, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| *line == "013" || *line == "13"),
        "CHARACTERS BEFORE should count remaining inspected bytes before delimiter, got:\n{stdout}"
    );
    assert!(
        stdout.contains("IDS CAN NOT BE ALZZZZZZ"),
        "REPLACING phrases should use the original scan text while updating target, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_tallying_signed_display_numeric_uses_digits_without_sign() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-SIGNED.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM   PIC S9(5) VALUE -12345.
01 C-MINUS  PIC 9(3) VALUE 0.
01 C-FIVE   PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-NUM TALLYING C-MINUS FOR ALL '-'
                             C-FIVE FOR ALL '5'.
    DISPLAY C-MINUS.
    DISPLAY C-FIVE.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert!(
        lines.iter().any(|line| *line == "000" || *line == "0"),
        "signed DISPLAY numeric INSPECT should not expose a '-' byte, got:\n{stdout}"
    );
    assert!(
        lines.iter().any(|line| *line == "001" || *line == "1"),
        "signed DISPLAY numeric INSPECT should expose the trailing digit, got:\n{stdout}"
    );
}

#[test]
fn test_native_inspect_tallying_phrases_share_scan_position() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. INSPECT-TALLY-SERIES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-DATA PIC X(4) VALUE 'AABA'.
01 C-AA    PIC 9(3) VALUE 0.
01 C-A     PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
    INSPECT WS-DATA TALLYING C-AA FOR ALL 'AA'
                              C-A FOR ALL 'A'.
    DISPLAY C-AA.
    DISPLAY C-A.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    assert!(matches!(lines.first(), Some(&"001") | Some(&"1")));
    assert!(
        matches!(lines.get(1), Some(&"001") | Some(&"1")),
        "second phrase should count only the remaining A, got:\n{stdout}"
    );
}

#[test]
fn test_native_add_corresponding_recurses_into_matching_groups() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-CORR-GROUP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC-GRP.
   02 NEST.
      03 A PIC 99 VALUE 11.
      03 B PIC 99 VALUE 22.
01 DST-GRP.
   02 NEST.
      03 A PIC 99 VALUE 01.
      03 B PIC 99 VALUE 02.
PROCEDURE DIVISION.
    ADD CORRESPONDING SRC-GRP TO DST-GRP.
    DISPLAY DST-GRP.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.contains("1224"),
        "ADD CORRESPONDING should recurse into matching groups, got:\n{stdout}"
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
fn test_native_perform_paragraph_thru_skips_intermediate_section_header() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PERF-THRU-SECT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  N PIC 9(4) VALUE 0.
PROCEDURE DIVISION.
    PERFORM A-PARA THRU B-PARA 2 TIMES.
    DISPLAY N.
    STOP RUN.
A-SECTION SECTION.
A-PARA.
    ADD 10 TO N.
B-SECTION SECTION.
B-PARA.
    ADD 100 TO N.
B-NEXT.
    ADD 1000 TO N.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "220"),
        "PERFORM paragraph THRU paragraph should not execute paragraph after through target, got:\n{stdout}"
    );
}

#[test]
fn test_native_numeric_edited_decimal_point_comma_picture() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DEC-COMMA.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SPECIAL-NAMES.
    DECIMAL-POINT IS COMMA.
DATA DIVISION.
WORKING-STORAGE SECTION.
77  DATA-K PIC 9999999V99 VALUE 1234567,89.
77  DATA-L PIC 9.999.999,99.
PROCEDURE DIVISION.
    MOVE DATA-K TO DATA-L.
    DISPLAY DATA-L.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "1.234.567,89"),
        "numeric edited decimal comma picture should use comma as decimal point, got:\n{stdout}"
    );
}

#[test]
fn test_native_group_display_pic_p_initial_value_scales_storage_digits() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. PIC-P-INIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 G.
   02 A PIC S9PP VALUE 100.
   02 B PIC S999 VALUE 100.
PROCEDURE DIVISION.
    SUBTRACT A -98 -1 -1 FROM B.
    DISPLAY B.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "100"),
        "PIC S9PP in group should initialize stored digit as 1 and read as 100, got:\n{stdout}"
    );
}

#[test]
fn test_native_group_display_decimal_initial_value_keeps_fractional_scale_for_size_error() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DISP-DEC-INIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 G.
   02 A PIC S9V99 VALUE -9.99.
PROCEDURE DIVISION.
    SUBTRACT .01 FROM A
        ON SIZE ERROR DISPLAY \"SIZE\".
    IF A = -9.99
        DISPLAY \"UNCHANGED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "SIZE")
            && stdout.lines().any(|line| line.trim() == "UNCHANGED"),
        "DISPLAY decimal group initial value should stay scaled and unchanged on size error, got:\n{stdout}"
    );
}

#[test]
fn test_native_add_giving_decimal_to_integer_truncates_fraction() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-TRUNC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 N-43 PIC S9V9 VALUE +1.6.
01 N-45 PIC S9 VALUE 0.
PROCEDURE DIVISION.
    ADD N-43 1.4 GIVING N-45.
    DISPLAY N-45.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "3"),
        "ADD GIVING decimal result into integer target should truncate to 3, got:\n{stdout}"
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
fn test_native_perform_varying_after_reinitializes_after_outer_increment() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VARY-AFTER-ORDER.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  A PIC 9(5) VALUE 0.
01  B PIC 9(5) VALUE 0.
01  CNT PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    PERFORM
        VARYING A FROM 1 BY 1 UNTIL A > 3
        AFTER B FROM A BY 1 UNTIL B > 3
        ADD 1 TO CNT
    END-PERFORM.
    DISPLAY CNT.
    STOP RUN.
";

    let (stdout, stderr, code) = compile_and_run_no_sema(src);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.trim().ends_with("00006") || stdout.trim().ends_with('6'),
        "AFTER varying should use the incremented outer value for reinitialization, stdout={stdout:?}, stderr={stderr:?}"
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
fn test_native_move_corresponding_skips_unmatched_group_children() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-GROUP-SKIP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC.
   05 GRP.
      10 A PIC X(2) VALUE 'AA'.
01 DST.
   05 GRP.
      10 B PIC X(2) VALUE 'BB'.
PROCEDURE DIVISION.
    MOVE CORRESPONDING SRC TO DST.
    DISPLAY B OF DST.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "BB");
}

#[test]
fn test_native_move_corresponding_moves_elementary_to_same_named_group_storage() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-ELEM-GROUP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC.
   05 D-LEVEL.
      10 DICK PIC X(4) VALUE 'DICK'.
01 DST.
   05 D-LEVEL.
      10 DICK.
         15 RICHARD OCCURS 2 TIMES PIC X(2).
PROCEDURE DIVISION.
    MOVE 'TTTT' TO DICK OF DST.
    MOVE CORRESPONDING D-LEVEL OF SRC TO D-LEVEL OF DST.
    DISPLAY DICK OF DST.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "DICK");
}

#[test]
fn test_native_move_corresponding_skips_redefines_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-REDEF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC.
   05 DD-LEVEL.
      10 HARRY PIC X(5) VALUE 'HARRY'.
01 DST.
   05 DD-LEVEL-FALSE PIC X(5) VALUE 'TTTTT'.
   05 DD-LEVEL REDEFINES DD-LEVEL-FALSE.
      10 HARRY PIC X(5).
PROCEDURE DIVISION.
    MOVE CORRESPONDING SRC TO DST.
    DISPLAY HARRY OF DD-LEVEL OF DST.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "TTTTT");
}

#[test]
fn test_native_move_corresponding_preserves_target_subscript() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. CORR-SUB.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 SRC.
   05 GRP.
      10 A PIC X(3) VALUE 'TOM'.
01 VIEW.
   05 TARGET-A PIC X(3) VALUE 'OLD'.
01 TABLE-AREA REDEFINES VIEW.
   05 ROW OCCURS 1 TIMES.
      10 GRP.
         15 A PIC X(3).
PROCEDURE DIVISION.
    MOVE CORRESPONDING GRP OF SRC TO GRP OF ROW (1).
    DISPLAY TARGET-A.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "TOM");
}

#[test]
fn test_native_add_giving_signed_separate_decimal_uses_numeric_digits_for_size_error() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ADD-SIZE-DIGITS.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 R PIC S.9(18) SIGN IS LEADING SEPARATE VALUE ZERO.
77 S PIC X VALUE SPACE.
PROCEDURE DIVISION.
    ADD -.999999999999999999 -.999999999999999999 -.34 -.01
        +.999999999999999999 +.999999999999999999 +.1 .35
        GIVING R
        ON SIZE ERROR MOVE '1' TO S.
    DISPLAY R.
    DISPLAY S.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines
            .first()
            .is_some_and(|line| line.contains(".100000000000000000")),
        "expected .100000000000000000, got stdout={stdout:?}"
    );
    assert_eq!(lines.get(1).copied().unwrap_or_default(), " ");
}

#[test]
fn test_native_value_all_literal_and_figurative_constants_initialize_full_field() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ALL-VALUE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 G.
   05 A PIC X(6) VALUE ALL 'ABC'.
   05 B PIC X(3) VALUE ALL QUOTES.
   05 C PIC X(3) VALUE ALL HIGH-VALUES.
   05 D PIC X(3) VALUE ALL LOW-VALUES.
PROCEDURE DIVISION.
    IF A = 'ABCABC' AND B = QUOTES AND C = HIGH-VALUES AND D = LOW-VALUES
       DISPLAY 'OK'
    ELSE
       DISPLAY 'NG'
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_decimal_literal_comparison_preserves_fractional_precision() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DECLITCMP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SIX PIC 9 VALUE 6.
PROCEDURE DIVISION.
    IF 6.00000000000000001 NOT EQUAL TO SIX
        DISPLAY \"OK\"
    ELSE
        DISPLAY \"NG\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line.contains("OK")),
        "decimal literal should compare with fractional precision, got: {stdout}"
    );
}

#[test]
fn test_native_alphanumeric_numeric_class_rejects_signed_text() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NUMCLASS.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  CLASS-1 PIC X(5) VALUE \"+1234\".
PROCEDURE DIVISION.
    IF CLASS-1 NOT NUMERIC
        DISPLAY \"OK\"
    ELSE
        DISPLAY \"NG\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_program_collating_sequence_alphabet_literal_and_also() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COLLSEQ.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
OBJECT-COMPUTER.
    COMPUTER PROGRAM COLLATING SEQUENCE IS WILD.
SPECIAL-NAMES.
    ALPHABET WILD IS \"A\" THRU \"H\" \"I\" ALSO \"J\" ALSO \"K\"
    ALSO \"L\" ALSO \"M\" ALSO \"N\" \"O\" THRU \"Z\" \"0\" THRU \"9\".
DATA DIVISION.
WORKING-STORAGE SECTION.
01  A PIC X VALUE \"A\".
01  I PIC X VALUE \"I\".
01  N PIC X VALUE \"N\".
01  NINE PIC 9 VALUE 9.
PROCEDURE DIVISION.
    IF A = LOW-VALUE AND I = N AND NINE < SPACE
        DISPLAY \"OK\"
    ELSE
        DISPLAY \"NG\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_program_collating_sequence_treats_figuratives_as_values() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COLLFIG.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
OBJECT-COMPUTER.
    COMPUTER PROGRAM COLLATING SEQUENCE IS WILD.
SPECIAL-NAMES.
    ALPHABET WILD IS \"F\" \"U\" \"N\" ALSO HIGH-VALUE ALSO LOW-VALUE \"Y\".
DATA DIVISION.
WORKING-STORAGE SECTION.
01  F PIC X VALUE \"F\".
01  U PIC X VALUE \"U\".
01  N PIC X VALUE \"N\".
01  Q PIC X VALUE \"Q\".
PROCEDURE DIVISION.
    IF F < U AND U < N AND F = LOW-VALUE
        AND N NOT = HIGH-VALUE AND Q NOT = LOW-VALUE
        DISPLAY \"OK\"
    ELSE
        DISPLAY \"NG\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_numeric_edited_deediting_preserves_floating_picture_digits() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. NEDEDIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  A PIC $(4)9.99CR.
01  B PIC S9(4)V99.
01  C PIC --9B.99B99/99.
01  D PIC S99V9(6).
PROCEDURE DIVISION.
    MOVE -123.45 TO A.
    MOVE A TO B.
    MOVE -42.9876 TO C.
    MOVE C TO D.
    IF A = \" $123.45CR\" AND B = -123.45
        AND C = \"-42 .98 76/00\" AND D = -42.987600
        DISPLAY \"OK\"
    ELSE
        DISPLAY A
        DISPLAY B
        DISPLAY C
        DISPLAY D.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_string_updates_pointer_after_delimited_by_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. STRPTR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  OUT PIC X(5) VALUE \"*****\".
01  PTR PIC 99 VALUE 1.
PROCEDURE DIVISION.
    STRING \"ABCDEF\" DELIMITED BY SIZE INTO OUT WITH POINTER PTR.
    IF OUT = \"ABCDE\" AND PTR = 6
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT
        DISPLAY PTR.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_string_uses_subscripted_identifier_delimiter() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. STRDELIM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  DELIM-TABLE PIC X(5) VALUE \"CDEFF\".
01  DELIM-REDEF REDEFINES DELIM-TABLE.
    05  DELIM-CHAR PIC X OCCURS 5 TIMES.
01  SRC PIC X(7) VALUE \"ABCDEFG\".
01  OUT PIC X(5) VALUE \"*****\".
01  PTR PIC 99 VALUE 1.
01  IDX PIC 99 VALUE 5.
PROCEDURE DIVISION.
    STRING SRC DELIMITED BY DELIM-CHAR(IDX) INTO OUT POINTER PTR.
    IF OUT = \"ABCDE\" AND PTR = 6
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT
        DISPLAY PTR.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_string_treats_figurative_constants_as_single_characters() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. STRFIG.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  OUT PIC X(5) VALUE \"*****\".
01  PTR PIC 99 VALUE 1.
PROCEDURE DIVISION.
    STRING SPACE \"ABCDE\" DELIMITED BY \" ABCDE\" INTO OUT
        POINTER PTR
        ON OVERFLOW DISPLAY \"OV\".
    IF OUT = \" ABCD\" AND PTR = 6
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT
        DISPLAY PTR.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line == "OK"),
        "expected figurative SPACE to be a one-character source, got {stdout:?}"
    );
}

#[test]
fn test_native_unstring_updates_pointer_tally_count_and_overflow() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTRBASIC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(7) VALUE \"1200000\".
01  OUT PIC X VALUE ZERO.
01  DELIM PIC X(4) VALUE \"****\".
01  CNT PIC 99 VALUE 0.
01  PTR PIC 99 VALUE 1.
01  TALLY PIC 99 VALUE 0.
PROCEDURE DIVISION.
    UNSTRING SRC DELIMITED BY ZERO
        INTO OUT DELIMITER IN DELIM COUNT IN CNT
        WITH POINTER PTR
        TALLYING TALLY
        ON OVERFLOW DISPLAY \"OV\".
    IF OUT = \"1\" AND DELIM = \"0   \" AND CNT = 2 AND PTR = 4 AND TALLY = 1
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT
        DISPLAY DELIM
        DISPLAY CNT
        DISPLAY PTR
        DISPLAY TALLY.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.lines().any(|line| line == "OV") && stdout.lines().any(|line| line == "OK"),
        "expected UNSTRING overflow side effects, got {stdout:?}"
    );
}

#[test]
fn test_native_unstring_respects_justified_and_numeric_targets() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTRKINDS.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(7) VALUE \"1200000\".
01  OUT-J PIC X JUSTIFIED RIGHT VALUE SPACE.
01  OUT-N PIC 9 VALUE ZERO.
01  PTR PIC 99 VALUE 1.
PROCEDURE DIVISION.
    UNSTRING SRC DELIMITED BY ZERO INTO OUT-J POINTER PTR ON OVERFLOW CONTINUE.
    MOVE 1 TO PTR.
    UNSTRING SRC DELIMITED BY ZERO INTO OUT-N POINTER PTR ON OVERFLOW CONTINUE.
    IF OUT-J = \"2\" AND OUT-N = 2
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT-J
        DISPLAY OUT-N.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_unstring_delimited_by_all_collapses_delimiter_run() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTRALL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(7) VALUE \"1200000\".
01  OUT-N PIC S9 VALUE ZERO.
01  DELIM PIC X(4) VALUE \"****\".
01  CNT PIC 99 VALUE 0.
01  PTR PIC 99 VALUE 1.
01  TALLY PIC 99 VALUE 0.
PROCEDURE DIVISION.
    UNSTRING SRC DELIMITED BY ALL ZERO
        INTO OUT-N DELIMITER DELIM COUNT CNT
        POINTER PTR
        TALLYING TALLY.
    IF OUT-N = +2 AND DELIM = \"0   \" AND CNT = 2 AND PTR = 8 AND TALLY = 1
        DISPLAY \"OK\"
    ELSE
        DISPLAY OUT-N
        DISPLAY DELIM
        DISPLAY CNT
        DISPLAY PTR
        DISPLAY TALLY.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_unstring_without_delimiter_splits_by_receiving_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTRSIZES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(10) VALUE \"ABCDEFGHIJ\".
01  GRP.
    05  A PIC X.
    05  B PIC XX.
    05  C PIC XXX.
    05  D PIC XXXX.
PROCEDURE DIVISION.
    MOVE SPACES TO GRP.
    UNSTRING SRC INTO D C B A.
    IF GRP = \"JHIEFGABCD\"
        DISPLAY \"OK\"
    ELSE
        DISPLAY GRP.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_unstring_uses_earliest_or_delimiter() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTROR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(12) VALUE \"ABCDEFGHIJKL\".
01  A PIC X VALUE SPACE.
01  B PIC XX VALUE SPACES.
01  C PIC XXX VALUE SPACES.
01  TALLY PIC 99 VALUE 1.
PROCEDURE DIVISION.
    UNSTRING SRC DELIMITED BY \"E\" OR \"H\" OR \"K\" OR \"L\"
        INTO C B A
        TALLYING TALLY.
    IF C = \"ABC\" AND B = \"FG\" AND A = \"I\" AND TALLY = 4
        DISPLAY \"OK\"
    ELSE
        DISPLAY C
        DISPLAY B
        DISPLAY A
        DISPLAY TALLY.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_native_unstring_delimited_identifier_exhausts_remaining_field() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. USTRREM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  SRC PIC X(7) VALUE \"ABCDEFG\".
01  DELIMS PIC X(2) VALUE \"CE\".
01  DELIMS-R REDEFINES DELIMS.
    05  D PIC X OCCURS 2 TIMES.
01  GRP.
    05  A PIC X(5).
    05  B PIC X.
01  DEL1 PIC X(4) VALUE \"****\".
01  DEL2 PIC X(4) VALUE \"****\".
01  C1 PIC 99 VALUE 0.
01  C2 PIC 99 VALUE 0.
01  IDX PIC 99 VALUE 1.
01  TALLY PIC 99 VALUE 1.
PROCEDURE DIVISION.
    MOVE SPACES TO GRP.
    UNSTRING SRC DELIMITED BY D(IDX)
        INTO A DELIMITER IN DEL1 COUNT IN C1
             B DELIMITER IN DEL2 COUNT IN C2
        TALLYING IN TALLY.
    IF GRP = \"AB   D\" AND DEL1 = \"C   \" AND C1 = 2 AND C2 = 4
        AND TALLY = 3
        DISPLAY \"OK\"
    ELSE
        DISPLAY GRP
        DISPLAY DEL1
        DISPLAY C1
        DISPLAY C2
        DISPLAY TALLY.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
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
fn test_native_section_perform_thru_honors_goto_to_end_paragraph() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. THRU-SECTION-GOTO-END.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-COUNT PIC 9(3) VALUE 0.
PROCEDURE DIVISION.
MAIN SECTION.
START-PARA.
    PERFORM CHECK-PARA THRU CHECK-EXIT.
    DISPLAY WS-COUNT.
    STOP RUN.
CHECK-PARA.
    GO TO CHECK-EXIT.
CHECK-FAIL.
    ADD 1 TO WS-COUNT.
CHECK-EXIT.
    EXIT.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert!(
        stdout.trim() == "0" || stdout.contains("000"),
        "GO TO the THRU end paragraph inside a section should skip intervening paragraphs: got '{stdout}'"
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
fn test_native_accept_console_reads_full_fixed_width_field() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ACCEPT-FIXED.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 ACCEPT-D1.
   05 ACCEPT-D1-A PIC X(20).
   05 ACCEPT-D1-B PIC X(7).
01 ACCEPT-D2 PIC X(27) VALUE \"ABCDEFGHIJKLMNOPQRSTUVWXY Z\".
PROCEDURE DIVISION.
    ACCEPT ACCEPT-D1.
    IF ACCEPT-D1 = ACCEPT-D2
        DISPLAY \"PASS\"
    ELSE
        DISPLAY ACCEPT-D1-A
        DISPLAY ACCEPT-D1-B
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) =
        compile_and_run_no_sema_with_stdin(src, "ABCDEFGHIJKLMNOPQRSTUVWXY Z\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "ACCEPT should read the full 27-byte field, stdout={stdout:?}, stderr={stderr:?}"
    );
}

#[test]
fn test_native_accept_console_preserves_subscripted_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ACCEPT-SUB.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 ACCEPT-VALUE PIC X(12) VALUE \"............\".
01 ACCEPT-D21 REDEFINES ACCEPT-VALUE.
   05 TAB-ACCEPT OCCURS 3 TIMES.
      10 TAB-A PIC X(4).
01 ACCEPT-D22 PIC X(12) VALUE \"....ABCD....\".
PROCEDURE DIVISION.
    ACCEPT TAB-ACCEPT(2).
    IF ACCEPT-D21 = ACCEPT-D22
        DISPLAY \"PASS\"
    ELSE
        DISPLAY ACCEPT-D21
    END-IF.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema_with_stdin(src, "ABCD\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("PASS"),
        "ACCEPT should keep the target subscript, stdout={stdout:?}, stderr={stderr:?}"
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
fn test_native_fd_multiple_record_lengths_are_variable_records() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FD-VARREC-TEST.
ENVIRONMENT DIVISION.
INPUT-OUTPUT SECTION.
FILE-CONTROL.
    SELECT MIX-FILE ASSIGN TO '/tmp/cobol_fd_varrec_test.dat'.
DATA DIVISION.
FILE SECTION.
FD MIX-FILE.
01 SHORT-REC.
   05 SHORT-KIND PIC X.
   05 SHORT-DATA PIC X(4).
01 LONG-REC.
   05 LONG-KIND PIC X.
   05 LONG-DATA PIC X(8).
WORKING-STORAGE SECTION.
01 WS-OUT PIC X(9) VALUE SPACES.
PROCEDURE DIVISION.
    OPEN OUTPUT MIX-FILE.
    MOVE 'SABCD' TO SHORT-REC.
    WRITE SHORT-REC.
    MOVE 'L12345678' TO LONG-REC.
    WRITE LONG-REC.
    CLOSE MIX-FILE.
    OPEN INPUT MIX-FILE.
    READ MIX-FILE INTO WS-OUT.
    DISPLAY WS-OUT.
    READ MIX-FILE INTO WS-OUT.
    DISPLAY WS-OUT.
    CLOSE MIX-FILE.
    STOP RUN.
";
    let _ = std::fs::remove_file("/tmp/cobol_fd_varrec_test.dat");
    let (stdout, _, code) = compile_and_run_no_sema(src);
    let _ = std::fs::remove_file("/tmp/cobol_fd_varrec_test.dat");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected two variable records, got:\n{stdout}"
    );
    assert!(
        lines[0].starts_with("SABCD"),
        "short record drifted: {stdout}"
    );
    assert_eq!(lines[1], "L12345678", "long record drifted: {stdout}");
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
fn test_native_88_level_multiple_alphanumeric_values() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. LEVEL88-MULTI-ALNUM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-SIGN PIC X VALUE '+'.
   88 VALID-SIGN VALUE '-', '+', '0'.
PROCEDURE DIVISION.
    IF VALID-SIGN
        DISPLAY 'OK'
    ELSE
        DISPLAY 'BAD'
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_nist_if107a_current_date_88_ranges() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IF107A-CURRENT-DATE-RANGES.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 TEMP1 PIC X(21).
01 WS-DATE.
   02 WS-YEAR PIC 9999.
      88 CON-YEAR VALUE 1990 THRU 9999.
   02 WS-MONTH PIC 99.
      88 CON-MONTH VALUE 01 THRU 12.
   02 WS-DAY PIC 99.
      88 CON-DAY VALUE 01 THRU 31.
   02 WS-HOUR PIC 99.
      88 CON-HOUR VALUE 00 THRU 23.
   02 WS-MIN PIC 99.
      88 CON-MIN VALUE 00 THRU 59.
   02 WS-SECOND PIC 99.
      88 CON-SEC VALUE 00 THRU 59.
   02 WS-HUNDSEC PIC 99.
      88 CON-HUNDSEC VALUE 00 THRU 99.
   02 WS-GREENW PIC X.
      88 CON-GREENW VALUE '-', '+', '0'.
   02 WS-OFFSET PIC 99.
      88 CON-OFFSET VALUE 00 THRU 13.
   02 WS-OFFSET2 PIC 99.
      88 CON-OFFSET2 VALUE 00 THRU 59.
PROCEDURE DIVISION.
    MOVE FUNCTION CURRENT-DATE TO TEMP1.
    MOVE TEMP1 TO WS-DATE.
    IF CON-YEAR AND CON-MONTH AND CON-DAY AND CON-HOUR
       AND CON-MIN AND CON-SEC AND CON-HUNDSEC AND CON-GREENW
       AND CON-OFFSET AND CON-OFFSET2
        DISPLAY 'ALL-OK'
    ELSE
        DISPLAY TEMP1
        IF NOT CON-YEAR DISPLAY 'BAD-YEAR' END-IF
        IF NOT CON-MONTH DISPLAY 'BAD-MONTH' END-IF
        IF NOT CON-DAY DISPLAY 'BAD-DAY' END-IF
        IF NOT CON-HOUR DISPLAY 'BAD-HOUR' END-IF
        IF NOT CON-MIN DISPLAY 'BAD-MIN' END-IF
        IF NOT CON-SEC DISPLAY 'BAD-SEC' END-IF
        IF NOT CON-HUNDSEC DISPLAY 'BAD-HUNDSEC' END-IF
        IF NOT CON-GREENW DISPLAY 'BAD-GREENW' END-IF
        IF NOT CON-OFFSET DISPLAY 'BAD-OFFSET' END-IF
        IF NOT CON-OFFSET2 DISPLAY 'BAD-OFFSET2' END-IF
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "ALL-OK", "stdout={stdout:?}");
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
fn test_native_move_all_literal_repeats_into_group() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. ALL-GROUP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-GROUP.
   05 WS-A PIC XX.
   05 WS-B PIC XX.
   05 WS-C PIC XX.
PROCEDURE DIVISION.
    MOVE ALL \"ABC\" TO WS-GROUP.
    DISPLAY WS-A.
    DISPLAY WS-B.
    DISPLAY WS-C.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines, vec!["AB", "CA", "BC"]);
}

#[test]
fn test_native_search_respects_occurs_depending_on_bound() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SEARCH-ODO.
DATA DIVISION.
WORKING-STORAGE SECTION.
77 TBL-LEN PIC 9 VALUE 2.
01 TBL.
   05 ENT OCCURS 1 TO 3 DEPENDING ON TBL-LEN INDEXED BY IDX.
      10 KEY-FLD PIC X.
PROCEDURE DIVISION.
    MOVE \"ABC\" TO TBL.
    SET IDX TO 1.
    SEARCH ENT AT END DISPLAY \"NOT-FOUND\"
        WHEN KEY-FLD (IDX) = \"C\" DISPLAY \"FOUND\".
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), vec!["NOT-FOUND"]);
}

#[test]
fn test_native_qualified_subscript_uses_exact_display_numeric_size() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. QUAL-SUB.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 GROUP-4-TABLE.
   05 UNQUAL-ITEM PIC X OCCURS 15 TIMES.
01 SUBSCRIPTS-PART1.
   05 SUBSCRIPTS.
      10 SUB1 PIC 9 VALUE 5.
      10 SUB2 PIC 99 VALUE 12.
01 SUBSCRIPTS-PART2.
   05 SUBSCRIPTS.
      10 SUB1 PIC 999 VALUE 5.
01 TEMP-VALUE PIC X.
PROCEDURE DIVISION.
    MOVE \"ABCDEFGHIJKLMNO\" TO GROUP-4-TABLE.
    MOVE UNQUAL-ITEM (SUB1 OF SUBSCRIPTS OF SUBSCRIPTS-PART1) TO TEMP-VALUE.
    DISPLAY TEMP-VALUE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), vec!["E"]);
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
fn test_native_rounded_multiply_by_subscripted_display_decimal_keeps_operand_scale() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MUL-IDX.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 TABLE1.
   05 TABLE1-NUM PIC S9V99 OCCURS 2 INDEXED BY INDEX1.
01 NUM-9V9 PIC 9V9.
PROCEDURE DIVISION.
    MOVE 1.34 TO TABLE1-NUM(2).
    MOVE 4.0 TO NUM-9V9.
    SET INDEX1 TO 2.
    MULTIPLY TABLE1-NUM(INDEX1) BY NUM-9V9 ROUNDED.
    IF NUM-9V9 = 5.4
        DISPLAY \"PASS\"
    ELSE
        DISPLAY NUM-9V9
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_decimal_multiply_size_error_preserves_target() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MUL-SIZE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 TABLE1.
   05 TABLE1-NUM PIC S9V99 OCCURS 3 INDEXED BY INDEX1.
01 NUM-9V9 PIC 9V9.
PROCEDURE DIVISION.
    MOVE 7.00 TO TABLE1-NUM(3).
    MOVE 6.0 TO NUM-9V9.
    SET INDEX1 TO 3.
    MULTIPLY TABLE1-NUM(INDEX1) BY NUM-9V9
        ON SIZE ERROR
            IF NUM-9V9 = 6.0
                DISPLAY \"PASS\"
            ELSE
                DISPLAY \"CHANGED\"
            END-IF
        NOT ON SIZE ERROR
            DISPLAY \"NOERR\"
    END-MULTIPLY.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
}

#[test]
fn test_native_divide_multiple_giving_uses_original_operands() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. DIV-GIVING-SNAPSHOT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 DIVISOR PIC 9V9.
01 DIVIDEND PIC 99.
01 OUT-A PIC 99V9.
01 OUT-B PIC 99.
01 OUT-C PIC 99V9.
PROCEDURE DIVISION.
    MOVE 3.9 TO DIVISOR.
    MOVE 10 TO DIVIDEND.
    DIVIDE DIVISOR INTO DIVIDEND
        GIVING OUT-A
               DIVIDEND ROUNDED
               OUT-C.
    IF OUT-A = 2.5 AND DIVIDEND = 3 AND OUT-C = 2.5
        DISPLAY \"PASS\"
    ELSE
        DISPLAY OUT-A
        DISPLAY DIVIDEND
        DISPLAY OUT-C
    END-IF.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "PASS", "unexpected output: '{stdout}'");
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
