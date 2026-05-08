use super::*;

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
fn test_validate_valid_numeric_runs() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALOK.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(5) VALUE 12345.
PROCEDURE DIVISION.
    VALIDATE WS-NUM.
    DISPLAY \"OK\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_validate_rejects_invalid_display_numeric_storage() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALBAD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(3) VALUE 123.
01 WS-RAW REDEFINES WS-NUM PIC X(3).
PROCEDURE DIVISION.
    MOVE \"12A\" TO WS-RAW.
    VALIDATE WS-NUM.
    DISPLAY \"UNREACHED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_ne!(code, 0, "VALIDATE should reject invalid storage");
    assert!(
        stderr.contains("EC-DATA-INCOMPATIBLE") || stderr.contains("COBOL EXCEPTION"),
        "stderr should report validation exception, got: {stderr}"
    );
    assert!(
        !stdout.contains("UNREACHED"),
        "execution should not continue after failed VALIDATE"
    );
}

#[test]
fn test_validate_rejects_value_clause_mismatch() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALVALUE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-CODE PIC X VALUE \"A\".
PROCEDURE DIVISION.
    MOVE \"B\" TO WS-CODE.
    VALIDATE WS-CODE.
    DISPLAY \"UNREACHED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_ne!(code, 0, "VALIDATE should reject VALUE mismatch");
    assert!(
        stderr.contains("EC-DATA-INCOMPATIBLE") || stderr.contains("COBOL EXCEPTION"),
        "stderr should report validation exception, got: {stderr}"
    );
    assert!(
        !stdout.contains("UNREACHED"),
        "execution should not continue after failed VALIDATE"
    );
}

#[test]
fn test_validate_accepts_88_condition_value() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VAL88OK.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-FLAG PIC X VALUE \"Y\".
   88 VALID-FLAG VALUE \"Y\" \"N\".
PROCEDURE DIVISION.
    MOVE \"N\" TO WS-FLAG.
    VALIDATE WS-FLAG.
    DISPLAY \"OK\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "OK");
}

#[test]
fn test_validate_rejects_outside_88_condition_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VAL88BAD.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-CODE PIC 9 VALUE 1.
   88 VALID-CODE VALUE 1 THRU 3.
PROCEDURE DIVISION.
    MOVE 9 TO WS-CODE.
    VALIDATE WS-CODE.
    DISPLAY \"UNREACHED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_ne!(code, 0, "VALIDATE should reject value outside 88 range");
    assert!(
        stderr.contains("EC-DATA-INCOMPATIBLE") || stderr.contains("COBOL EXCEPTION"),
        "stderr should report validation exception, got: {stderr}"
    );
    assert!(
        !stdout.contains("UNREACHED"),
        "execution should not continue after failed VALIDATE"
    );
}

#[test]
fn test_validate_group_rejects_child_value_mismatch() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALGROUP.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-GROUP.
   05 WS-CODE PIC X VALUE \"A\".
   05 WS-NUM PIC 9 VALUE 1.
PROCEDURE DIVISION.
    MOVE \"B\" TO WS-CODE.
    VALIDATE WS-GROUP.
    DISPLAY \"UNREACHED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_ne!(
        code, 0,
        "VALIDATE on a group should reject child VALUE mismatch"
    );
    assert!(
        stderr.contains("EC-DATA-INCOMPATIBLE") || stderr.contains("COBOL EXCEPTION"),
        "stderr should report validation exception, got: {stderr}"
    );
    assert!(
        !stdout.contains("UNREACHED"),
        "execution should not continue after failed group VALIDATE"
    );
}

#[test]
fn test_validate_rejects_non_alphabetic_picture_a_storage() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. VALALPHA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC A(3).
01 WS-RAW REDEFINES WS-NAME PIC X(3).
PROCEDURE DIVISION.
    MOVE \"A1C\" TO WS-RAW.
    VALIDATE WS-NAME.
    DISPLAY \"UNREACHED\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_ne!(
        code, 0,
        "VALIDATE should reject non-alphabetic PIC A storage"
    );
    assert!(
        stderr.contains("EC-DATA-INCOMPATIBLE") || stderr.contains("COBOL EXCEPTION"),
        "stderr should report validation exception, got: {stderr}"
    );
    assert!(
        !stdout.contains("UNREACHED"),
        "execution should not continue after failed alphabetic VALIDATE"
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
fn test_nist_if101a_evaluate_acos_zero_decimal_range() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. IF101A-EVAL.
PROCEDURE DIVISION.
    EVALUATE FUNCTION ACOS(0)
    WHEN 1.57076 THRU 1.57082
        DISPLAY 'OK'
    WHEN OTHER
        DISPLAY 'BAD'
    END-EVALUATE.
    STOP RUN.
";
    let (stdout, _, code) = compile_and_run(src);
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        "OK",
        "ACOS(0) should match the decimal range used by IF101A, got '{}'",
        stdout.trim()
    );
}
