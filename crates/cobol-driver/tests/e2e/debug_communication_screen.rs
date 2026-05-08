use super::*;

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
fn test_native_use_for_debugging_registers_pass_semantic_pipeline() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-SEMA.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST WITH DEBUGGING MODE.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON MAIN-SECTION.
DBG-PARA.
    DISPLAY DEBUG-LINE.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
MAIN-SECTION SECTION.
MAIN-PARA.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "MAIN-SECTION"),
        "stdout should include DEBUG-NAME, got:\n{stdout}"
    );
}

#[test]
fn test_use_for_debugging_without_source_debugging_mode_passes_semantic_pipeline() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DEBUG-OFF-SEMA.
ENVIRONMENT DIVISION.
CONFIGURATION SECTION.
SOURCE-COMPUTER. TEST.
PROCEDURE DIVISION.
DECLARATIVES.
DBG-SECTION SECTION.
    USE FOR DEBUGGING ON MAIN-SECTION.
DBG-PARA.
    DISPLAY DEBUG-LINE.
    DISPLAY DEBUG-NAME.
    DISPLAY DEBUG-CONTENTS.
END DECLARATIVES.
MAIN-SECTION SECTION.
MAIN-PARA.
    DISPLAY \"MAIN\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "MAIN");
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
fn test_native_alter_qualified_target_redirects_duplicate_paragraph() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ALTER-QUALIFIED.
PROCEDURE DIVISION.
QUAL-SECTION-1 SECTION.
START-PARA.
    ALTER PARA-5A IN QUAL-SECTION-1 TO PROCEED TO PARA-5C OF QUAL-SECTION-2.
PARA-5A.
    GO TO PARA-5C OF QUAL-SECTION-1.
PARA-5C.
    DISPLAY \"WRONG\".
    STOP RUN.
QUAL-SECTION-2 SECTION.
PARA-5C.
    DISPLAY \"RIGHT\".
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stdout.lines().any(|line| line.trim() == "RIGHT"),
        "stdout should include the qualified altered target, got:\n{stdout}"
    );
    assert!(
        !stdout.lines().any(|line| line.trim() == "WRONG"),
        "stdout should not include the unqualified duplicate target, got:\n{stdout}"
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
            file_access_modes: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_optionals: std::collections::HashSet::new(),
            file_relative_keys: std::collections::HashMap::new(),
            file_status_vars: vec![],
            declaratives: vec![],
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
fn test_native_communication_send_receive_message_round_trip() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. COMM-E2E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 MSG PIC X(5) VALUE \"HELLO\".
01 OUT-MSG PIC X(5).
01 COMM-KEY PIC X VALUE \"K\".
COMMUNICATION SECTION.
CD CM-QUEUE I-O.
PROCEDURE DIVISION.
    ENABLE I-O CM-QUEUE WITH KEY COMM-KEY.
    SEND CM-QUEUE FROM MSG.
    RECEIVE CM-QUEUE MESSAGE INTO OUT-MSG
        NO DATA DISPLAY \"NO-DATA\".
    DISPLAY OUT-MSG.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema(src);
    assert_eq!(
        code, 0,
        "native communication round trip failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert_eq!(stdout, "HELLO\n");
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
        c_code.contains("printf(\"DETAIL_LINE\\n\")"),
        "GENERATE should emit a visible report line, got:\n{c_code}"
    );
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

#[test]
fn test_screen_section_accept_using_codegen() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRACPT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(5) VALUE SPACES.
SCREEN SECTION.
01 ENTRY-SCREEN.
   05 LINE 1 COLUMN 1 VALUE \"NAME:\".
   05 LINE 1 COLUMN 7 USING WS-NAME.
PROCEDURE DIVISION.
    ACCEPT ENTRY-SCREEN.
    STOP RUN.
";
    let hir = parse_and_lower(src);
    let c_code = generate_c(&hir);
    assert!(
        c_code.contains("cobol_screen_accept"),
        "screen ACCEPT should read USING field, got:\n{}",
        c_code
    );
    assert!(
        c_code.contains("cobol_screen_position(1, 7)"),
        "screen ACCEPT should position cursor at USING field, got:\n{}",
        c_code
    );
}

#[test]
fn test_native_screen_section_accept_using_reads_stdin() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRACPTN.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NAME PIC X(5) VALUE SPACES.
SCREEN SECTION.
01 ENTRY-SCREEN.
   05 LINE 1 COLUMN 1 VALUE \"NAME:\".
   05 LINE 1 COLUMN 7 USING WS-NAME.
PROCEDURE DIVISION.
    ACCEPT ENTRY-SCREEN.
    DISPLAY WS-NAME.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema_with_stdin(src, "ALICE\n");
    assert_eq!(
        code, 0,
        "native screen ACCEPT failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("NAME:"),
        "screen prompt should be displayed, got: {stdout:?}"
    );
    assert!(
        stdout.contains("ALICE"),
        "USING field should receive stdin, got: {stdout:?}"
    );
}

#[test]
fn test_native_screen_section_accept_numeric_picture_filters_input() {
    let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. SCRNUM.
DATA DIVISION.
WORKING-STORAGE SECTION.
01 WS-NUM PIC 9(3) VALUE 0.
SCREEN SECTION.
01 ENTRY-SCREEN.
   05 LINE 1 COLUMN 1 VALUE \"NUM:\".
   05 LINE 1 COLUMN 6 PIC 9(3) USING WS-NUM.
PROCEDURE DIVISION.
    ACCEPT ENTRY-SCREEN.
    DISPLAY WS-NUM.
    STOP RUN.
";
    let (stdout, stderr, code) = compile_and_run_no_sema_with_stdin(src, "A1B2C3\n");
    assert_eq!(
        code, 0,
        "native screen numeric ACCEPT failed: stdout={stdout:?}, stderr={stderr:?}"
    );
    assert!(
        stdout.contains("123"),
        "numeric screen ACCEPT should apply a PIC 9 input mask, got: {stdout:?}"
    );
}

// ===========================================================================
// Phase 6 edge case tests (production-gaps.md section 4)
// ===========================================================================

// ---------------------------------------------------------------------------
// 4-1: EXIT statement semantics - bare EXIT acts as CONTINUE (no-op)
// ---------------------------------------------------------------------------
