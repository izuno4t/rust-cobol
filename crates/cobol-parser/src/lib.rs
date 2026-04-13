// COBOL Compiler - COBOL source code parser
//
// This crate implements a recursive descent parser that transforms a token
// stream (from cobol-lexer) into an abstract syntax tree (from cobol-ast).
//
// Error details are accumulated in DiagnosticReporter, so Result<_, ()> is
// the conventional return type for parsing methods.
#![allow(clippy::result_unit_err)]

pub mod data_div;
pub mod env_div;
pub mod error;
pub mod expr;
pub mod ident_div;
pub mod parser;
pub mod proc_div;

pub use parser::Parser;

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_ast::*;
    use cobol_common::{FileId, SourceFormat};
    use cobol_lexer::Lexer;

    fn parse(source: &str) -> Result<CobolProgram, ()> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        parser.parse_program()
    }

    fn parse_free(source: &str) -> Result<CobolProgram, ()> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        parser.parse_program()
    }

    fn parse_fixed(source: &str) -> Result<CobolProgram, ()> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        parser.parse_program()
    }

    #[test]
    fn test_parse_minimal_program() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. HELLO.
       PROCEDURE DIVISION.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        assert_eq!(program.identification.program_id.as_str(), "HELLO");
        assert!(program.procedure.is_some());
    }

    #[test]
    fn test_parse_hello_world() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. HELLO-WORLD.
       PROCEDURE DIVISION.
           DISPLAY \"Hello, World!\".
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        assert_eq!(program.identification.program_id.as_str(), "HELLO-WORLD");
        let proc = program.procedure.unwrap();
        // Should have statements
        let stmts: Vec<_> = proc
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .chain(proc.paragraphs.iter())
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Display(_))));
        assert!(stmts.iter().any(|s| matches!(s, Statement::StopRun)));
    }

    #[test]
    fn test_parse_working_storage() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-DATA.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-NAME PIC X(20).
       01  WS-COUNT PIC 9(5).
       PROCEDURE DIVISION.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let data = program.data.unwrap();
        assert_eq!(
            data.working_storage.len(),
            2,
            "unexpected working_storage AST: {:#?}",
            data.working_storage
        );
    }

    #[test]
    fn test_parse_fixed_working_storage_with_integer_literal_level_after_header() {
        let src = "\
000100 IDENTIFICATION DIVISION.
000200 PROGRAM-ID. TEST-DATA.
000300 DATA DIVISION.
000400 WORKING-STORAGE SECTION.
000500 01  HEADER-GROUP.
000600                                                                02
000700     ITEM-A                    PIC X(4) VALUE \"ABCD\".
000800 01  TEST-RESULTS.
000900     02 P-OR-F                 PIC X(5).
001000 PROCEDURE DIVISION.
001100     MOVE SPACE TO TEST-RESULTS.
001200     MOVE \"PASS \" TO P-OR-F.
001300     STOP RUN.
";
        let program = parse_fixed(src).unwrap();
        let data = program.data.unwrap();
        assert_eq!(data.working_storage.len(), 2);
        assert_eq!(
            data.working_storage[0].children[0].name.as_deref(),
            Some("ITEM-A")
        );
        assert_eq!(
            data.working_storage[1].children[0].name.as_deref(),
            Some("P-OR-F")
        );
    }

    #[test]
    fn test_parse_if_statement() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-IF.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-A PIC 9(3).
       PROCEDURE DIVISION.
           IF WS-A > 100
               DISPLAY \"BIG\"
           ELSE
               DISPLAY \"SMALL\"
           END-IF.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = if let Some(proc) = program.procedure {
            proc
        } else {
            panic!("expected procedure division, got AST: {:#?}", program);
        };
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::If(_))));
    }

    #[test]
    fn test_parse_if_with_and_function_condition_continuation() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-IF-FUNC.
       PROCEDURE DIVISION.
           IF FUNCTION ACOS(0) > 1
              AND FUNCTION ACOS(0) < 2
               DISPLAY \"OK\"
           ELSE
               DISPLAY \"BAD\"
           END-IF.
           STOP RUN.";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        match &stmts[0] {
            Statement::If(if_stmt) => {
                assert!(
                    matches!(if_stmt.condition, cobol_ast::expr::Condition::And(_, _)),
                    "IF condition should parse as conjunction"
                );
                assert_eq!(if_stmt.then_body.len(), 1);
                assert_eq!(if_stmt.else_body.len(), 1);
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_communication_section_synthetic_items() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-COMM.
       DATA DIVISION.
       COMMUNICATION SECTION.
       CD  CM-IN FOR INPUT
           TEXT LENGTH IS MSG-LENGTH
           END KEY IS END-KEY
           STATUS KEY IS STATUS-KEY
           MESSAGE COUNT IS MSG-COUNT.
       PROCEDURE DIVISION.
           MOVE STATUS-KEY TO END-KEY.
           ACCEPT CM-IN MESSAGE COUNT.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let data = program.data.unwrap();
        let cd = &data.communication[0];
        let names: Vec<_> = cd
            .data_items
            .iter()
            .filter_map(|item| item.name.as_deref())
            .collect();
        assert!(names.contains(&"MSG-LENGTH"), "{names:?}");
        assert!(names.contains(&"END-KEY"), "{names:?}");
        assert!(names.contains(&"STATUS-KEY"), "{names:?}");
        assert!(names.contains(&"MSG-COUNT"), "{names:?}");
    }

    #[test]
    fn test_parse_communication_destination_table_synthetic_items() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-COMM-OUT.
       DATA DIVISION.
       COMMUNICATION SECTION.
       CD  CM-OUT OUTPUT
           DESTINATION COUNT DEST-COUNT
           DESTINATION TABLE OCCURS 2 TIMES INDEXED BY IDX-1
           ERROR KEY ERR-KEY
           DESTINATION SYM-DEST.
       PROCEDURE DIVISION.
           MOVE \"OUTQUEUE\" TO SYM-DEST (1).
           MOVE ERR-KEY (2) TO ERR-KEY (1).
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let data = program.data.unwrap();
        let cd = &data.communication[0];
        let sym_dest = cd
            .data_items
            .iter()
            .find(|item| item.name.as_deref() == Some("SYM-DEST"))
            .unwrap();
        assert_eq!(sym_dest.occurs.as_ref().map(|o| o.max), Some(2));
        assert_eq!(
            sym_dest
                .occurs
                .as_ref()
                .map(|o| o.indexed_by.clone())
                .unwrap_or_default(),
            vec!["IDX-1"]
        );
        let err_key = cd
            .data_items
            .iter()
            .find(|item| item.name.as_deref() == Some("ERR-KEY"))
            .unwrap();
        assert_eq!(err_key.occurs.as_ref().map(|o| o.max), Some(2));
    }

    #[test]
    fn test_parse_send_and_display_with_advancing_phrases() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-ADV.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  MSG PIC X(10).
       PROCEDURE DIVISION.
           DISPLAY MSG AFTER ADVANCING PAGE.
           SEND DEST FROM MSG WITH EMI BEFORE ADVANCING THREE LINES.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Display(_))));
        assert!(stmts.iter().any(|s| matches!(s, Statement::Send(_))));
    }

    #[test]
    fn test_parse_perform_varying() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-LOOP.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-I PIC 9(3).
       PROCEDURE DIVISION.
           PERFORM VARYING WS-I FROM 1 BY 1
               UNTIL WS-I > 10
               DISPLAY WS-I
           END-PERFORM.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Perform(_))));
    }

    #[test]
    fn test_parse_numeric_procedure_names() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-NUMPROC.
       PROCEDURE DIVISION.
       MAIN-SEC SECTION.
       ENTRY-PARA.
           PERFORM 00.
           GO TO 00.
       00 SECTION 00.
       PARA-00.
           STOP RUN.                                                    ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        assert!(proc.sections.iter().any(|s| s.name == "MAIN-SEC"));
        assert!(proc.sections.iter().any(|s| s.name == "00"));
        assert!(proc
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .any(|p| p.name == "ENTRY-PARA"));
        assert!(proc
            .sections
            .iter()
            .flat_map(|s| s.paragraphs.iter())
            .any(|p| p.name == "PARA-00"));
    }

    #[test]
    fn test_parse_move_statement() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-MOVE.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-A PIC X(10).
       01  WS-B PIC X(10).
       PROCEDURE DIVISION.
           MOVE \"HELLO\" TO WS-A WS-B.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let move_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::Move(_)))
            .unwrap();
        if let Statement::Move(m) = move_stmt {
            assert_eq!(m.to.len(), 2);
        }
    }

    #[test]
    fn test_parse_compute() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-COMPUTE.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-A PIC 9(5).
       01  WS-B PIC 9(5).
       01  WS-C PIC 9(5).
       PROCEDURE DIVISION.
           COMPUTE WS-A = WS-B + WS-C * 2.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Compute(_))));
    }

    #[test]
    fn test_parse_free_format() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. FREE-TEST.
PROCEDURE DIVISION.
    DISPLAY \"Free format!\".
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        assert_eq!(program.identification.program_id.as_str(), "FREE-TEST");
    }

    #[test]
    fn test_parse_evaluate() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-EVAL.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-GRADE PIC X.
       PROCEDURE DIVISION.
           EVALUATE WS-GRADE
               WHEN \"A\"
                   DISPLAY \"EXCELLENT\"
               WHEN \"B\"
                   DISPLAY \"GOOD\"
               WHEN OTHER
                   DISPLAY \"UNKNOWN\"
           END-EVALUATE.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Evaluate(_))));
    }

    #[test]
    fn test_parse_compute_on_size_error() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-SIZE-ERR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3).
01  WS-B PIC 9(3).
PROCEDURE DIVISION.
    COMPUTE WS-A = WS-B + 999
      ON SIZE ERROR
        DISPLAY \"overflow\"
      NOT ON SIZE ERROR
        DISPLAY \"ok\"
    END-COMPUTE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let compute = stmts
            .iter()
            .find(|s| matches!(s, Statement::Compute(_)))
            .unwrap();
        if let Statement::Compute(c) = compute {
            assert!(!c.on_size_error.is_empty());
            assert!(!c.not_on_size_error.is_empty());
        }
    }

    #[test]
    fn test_parse_add_on_size_error() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ADD-ERR.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3).
01  WS-B PIC 9(3).
PROCEDURE DIVISION.
    ADD WS-A TO WS-B
      ON SIZE ERROR
        DISPLAY \"overflow\"
    END-ADD.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let add = stmts
            .iter()
            .find(|s| matches!(s, Statement::Add(_)))
            .unwrap();
        if let Statement::Add(a) = add {
            assert!(!a.on_size_error.is_empty());
        }
    }

    #[test]
    fn test_parse_read_at_end() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-READ.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-REC PIC X(80).
PROCEDURE DIVISION.
    READ INPUT-FILE INTO WS-REC
      AT END
        DISPLAY \"eof\"
      NOT AT END
        DISPLAY \"got record\"
    END-READ.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let read = stmts
            .iter()
            .find(|s| matches!(s, Statement::Read(_)))
            .unwrap();
        if let Statement::Read(r) = read {
            assert!(!r.at_end.is_empty());
            assert!(!r.not_at_end.is_empty());
        }
    }

    #[test]
    fn test_parse_write_invalid_key() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-WRITE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-REC PIC X(80).
PROCEDURE DIVISION.
    WRITE WS-REC
      INVALID KEY
        DISPLAY \"error\"
    END-WRITE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let write = stmts
            .iter()
            .find(|s| matches!(s, Statement::Write(_)))
            .unwrap();
        if let Statement::Write(w) = write {
            assert!(!w.invalid_key.is_empty());
        }
    }

    #[test]
    fn test_parse_string_on_overflow() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-STRING.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC X(10).
01  WS-B PIC X(10).
01  WS-C PIC X(20).
PROCEDURE DIVISION.
    STRING WS-A DELIMITED BY SIZE
           WS-B DELIMITED BY SIZE
      INTO WS-C
      ON OVERFLOW
        DISPLAY \"overflow\"
    END-STRING.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let string_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::String(_)))
            .unwrap();
        if let Statement::String(s) = string_stmt {
            assert!(!s.on_overflow.is_empty());
        }
    }

    #[test]
    fn test_parse_sort_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-SORT.
PROCEDURE DIVISION.
    SORT SORT-FILE
      ON ASCENDING KEY SORT-KEY-1
      ON DESCENDING KEY SORT-KEY-2
      USING INPUT-FILE
      GIVING OUTPUT-FILE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let sort = stmts
            .iter()
            .find(|s| matches!(s, Statement::Sort(_)))
            .unwrap();
        if let Statement::Sort(s) = sort {
            assert_eq!(s.keys.len(), 2);
            assert_eq!(s.keys[0].order, statement::SortOrder::Ascending);
            assert_eq!(s.keys[1].order, statement::SortOrder::Descending);
        }
    }

    #[test]
    fn test_parse_inspect_tallying() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INSPECT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SRC PIC X(20).
01  WS-COUNT PIC 9(3).
PROCEDURE DIVISION.
    INSPECT WS-SRC TALLYING
      WS-COUNT FOR CHARACTERS.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let inspect = stmts
            .iter()
            .find(|s| matches!(s, Statement::Inspect(_)))
            .unwrap();
        if let Statement::Inspect(i) = inspect {
            if let statement::InspectKind::Tallying { tallying } = &i.kind {
                assert_eq!(tallying.len(), 1);
            } else {
                panic!("expected Tallying kind");
            }
        }
    }

    #[test]
    fn test_parse_inspect_replacing() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INSPECT-REPL.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SRC PIC X(20).
PROCEDURE DIVISION.
    INSPECT WS-SRC REPLACING
      ALL \"A\" BY \"B\"
      FIRST \"X\" BY \"Y\".
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let inspect = stmts
            .iter()
            .find(|s| matches!(s, Statement::Inspect(_)))
            .unwrap();
        if let Statement::Inspect(i) = inspect {
            if let statement::InspectKind::Replacing { replacing } = &i.kind {
                assert_eq!(replacing.len(), 2);
            } else {
                panic!("expected Replacing kind");
            }
        }
    }

    #[test]
    fn test_parse_unstring_delimited() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-UNSTRING.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-SRC PIC X(50).
01  WS-T1 PIC X(20).
01  WS-T2 PIC X(20).
PROCEDURE DIVISION.
    UNSTRING WS-SRC DELIMITED BY \",\" OR \";\"
      INTO WS-T1 WS-T2
    END-UNSTRING.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let unstring = stmts
            .iter()
            .find(|s| matches!(s, Statement::Unstring(_)))
            .unwrap();
        if let Statement::Unstring(u) = unstring {
            assert_eq!(u.delimiters.len(), 2);
            assert_eq!(u.into.len(), 2);
        }
    }

    #[test]
    fn test_parse_occurs_depending_on() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-OCCURS.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  TABLE-COUNT PIC 9(3).
01  TABLE-GROUP.
    05  TABLE-ENTRY OCCURS 1 TO 100 TIMES
        DEPENDING ON TABLE-COUNT PIC X(10).
PROCEDURE DIVISION.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let data = program.data.unwrap();
        // Find the TABLE-ENTRY item with OCCURS
        fn find_occurs(items: &[cobol_ast::data_div::DataItem]) -> bool {
            for item in items {
                if let Some(ref oc) = item.occurs {
                    if oc.min == Some(1) && oc.max == 100 && oc.depending_on.is_some() {
                        return true;
                    }
                }
                if find_occurs(&item.children) {
                    return true;
                }
            }
            false
        }
        assert!(find_occurs(&data.working_storage));
    }

    #[test]
    fn test_parse_rewrite_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-REWRITE.
PROCEDURE DIVISION.
    REWRITE MY-RECORD FROM WS-REC
      INVALID KEY
        DISPLAY \"error\"
    END-REWRITE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let rewrite = stmts
            .iter()
            .find(|s| matches!(s, Statement::Rewrite(_)))
            .unwrap();
        if let Statement::Rewrite(r) = rewrite {
            assert!(!r.invalid_key.is_empty());
            assert!(r.from.is_some());
        }
    }

    #[test]
    fn test_parse_delete_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DELETE.
PROCEDURE DIVISION.
    DELETE MY-FILE RECORD
      INVALID KEY
        DISPLAY \"not found\"
    END-DELETE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Delete(_))));
    }

    #[test]
    fn test_parse_start_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-START.
PROCEDURE DIVISION.
    START MY-FILE KEY EQUAL MY-KEY
      INVALID KEY
        DISPLAY \"not found\"
    END-START.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let start = stmts
            .iter()
            .find(|s| matches!(s, Statement::Start(_)))
            .unwrap();
        if let Statement::Start(s) = start {
            assert!(s.key_condition.is_some());
            assert!(!s.invalid_key.is_empty());
        }
    }

    #[test]
    fn test_parse_return_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RETURN.
PROCEDURE DIVISION.
    RETURN SORT-FILE INTO WS-REC
      AT END
        DISPLAY \"end\"
    END-RETURN.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let ret = stmts
            .iter()
            .find(|s| matches!(s, Statement::Return(_)))
            .unwrap();
        if let Statement::Return(r) = ret {
            assert!(r.into.is_some());
            assert!(!r.at_end.is_empty());
        }
    }

    #[test]
    fn test_parse_call_on_exception() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-CALL-EX.
PROCEDURE DIVISION.
    CALL \"SUBPROG\"
      ON EXCEPTION
        DISPLAY \"call failed\"
      NOT ON EXCEPTION
        DISPLAY \"call ok\"
    END-CALL.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let call = stmts
            .iter()
            .find(|s| matches!(s, Statement::Call(_)))
            .unwrap();
        if let Statement::Call(c) = call {
            assert!(!c.on_exception.is_empty());
            assert!(!c.not_on_exception.is_empty());
        }
    }

    #[test]
    fn test_parse_cancel_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-CANCEL.
PROCEDURE DIVISION.
    CANCEL \"SUBPROG\".
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::Cancel(_))));
    }

    #[test]
    fn test_parse_release_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RELEASE.
PROCEDURE DIVISION.
    RELEASE SORT-REC FROM WS-REC.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let release = stmts
            .iter()
            .find(|s| matches!(s, Statement::Release(_)))
            .unwrap();
        if let Statement::Release(r) = release {
            assert!(r.from.is_some());
        }
    }

    #[test]
    fn test_parse_merge_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-MERGE.
PROCEDURE DIVISION.
    MERGE MERGE-FILE
      ON ASCENDING KEY MERGE-KEY
      USING FILE-1 FILE-2
      GIVING OUT-FILE.
    STOP RUN.
";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let merge = stmts
            .iter()
            .find(|s| matches!(s, Statement::Merge(_)))
            .unwrap();
        if let Statement::Merge(m) = merge {
            assert_eq!(m.keys.len(), 1);
            assert_eq!(m.using.len(), 2);
        }
    }

    #[test]
    fn test_parse_reference_modification_start_and_length() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-REFMOD.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-NAME PIC X(20).
       PROCEDURE DIVISION.
           DISPLAY WS-NAME(1:5).
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let display_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::Display(_)))
            .unwrap();
        if let Statement::Display(d) = display_stmt {
            assert_eq!(d.operands.len(), 1);
            match &d.operands[0] {
                Expr::ReferenceModification {
                    variable,
                    start,
                    length,
                    ..
                } => {
                    assert_eq!(variable.name.as_str(), "WS-NAME");
                    assert!(matches!(**start, Expr::Literal(Literal::Integer(1))));
                    assert!(length.is_some());
                    let len = length.as_ref().unwrap();
                    assert!(matches!(**len, Expr::Literal(Literal::Integer(5))));
                }
                other => panic!(
                    "expected ReferenceModification, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        } else {
            panic!("expected DISPLAY statement");
        }
    }

    #[test]
    fn test_parse_reference_modification_start_only() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-REFMOD2.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-NAME PIC X(20).
       PROCEDURE DIVISION.
           DISPLAY WS-NAME(3:).
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let display_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::Display(_)))
            .unwrap();
        if let Statement::Display(d) = display_stmt {
            match &d.operands[0] {
                Expr::ReferenceModification {
                    variable,
                    start,
                    length,
                    ..
                } => {
                    assert_eq!(variable.name.as_str(), "WS-NAME");
                    assert!(matches!(**start, Expr::Literal(Literal::Integer(3))));
                    assert!(length.is_none());
                }
                other => panic!(
                    "expected ReferenceModification, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        } else {
            panic!("expected DISPLAY statement");
        }
    }

    #[test]
    fn test_parse_reference_modification_with_variables() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-REFMOD3.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-NAME PIC X(20).
       01  WS-START PIC 9(2).
       01  WS-LEN PIC 9(2).
       PROCEDURE DIVISION.
           DISPLAY WS-NAME(WS-START:WS-LEN).
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let display_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::Display(_)))
            .unwrap();
        if let Statement::Display(d) = display_stmt {
            match &d.operands[0] {
                Expr::ReferenceModification {
                    variable,
                    start,
                    length,
                    ..
                } => {
                    assert_eq!(variable.name.as_str(), "WS-NAME");
                    assert!(matches!(**start, Expr::Identifier(_)));
                    assert!(length.is_some());
                    let len = length.as_ref().unwrap();
                    assert!(matches!(**len, Expr::Identifier(_)));
                }
                other => panic!(
                    "expected ReferenceModification, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        } else {
            panic!("expected DISPLAY statement");
        }
    }

    #[test]
    fn test_parse_subscript_not_reference_modification() {
        // Ensure TABLE(1) is parsed as subscript, not reference modification
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-SUB.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  WS-TABLE.
           05  WS-ITEM PIC X(10) OCCURS 5 TIMES.
       PROCEDURE DIVISION.
           DISPLAY WS-ITEM(1).
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        let display_stmt = stmts
            .iter()
            .find(|s| matches!(s, Statement::Display(_)))
            .unwrap();
        if let Statement::Display(d) = display_stmt {
            // Should be a plain identifier with subscripts, NOT ReferenceModification
            match &d.operands[0] {
                Expr::Identifier(qn) => {
                    assert_eq!(qn.name.as_str(), "WS-ITEM");
                    assert_eq!(qn.subscripts.len(), 1);
                }
                other => panic!(
                    "expected Identifier with subscript, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        } else {
            panic!("expected DISPLAY statement");
        }
    }

    #[test]
    fn test_parse_if_with_qualified_name_and_qualified_subscript() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-SM206.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  GRP-001.
           05  GRP-002 OCCURS 2 TIMES.
               10 WRK-DS-05V00-O005-001 PIC S9(5).
       PROCEDURE DIVISION.
           IF WRK-DS-05V00-O005-001 OF GRP-002 (1) EQUAL TO +6
               PERFORM PASS
           ELSE
               PERFORM FAIL
           END-IF.
           STOP RUN.                                                     ";
        let program = parse(src).unwrap();
        assert!(program.procedure.is_some());
    }

    #[test]
    fn test_parse_declaratives() {
        let source = r#"
            IDENTIFICATION DIVISION.
            PROGRAM-ID. DECLTEST.
            DATA DIVISION.
            FILE SECTION.
            FD INPUT-FILE.
            01 INPUT-REC PIC X(80).
            PROCEDURE DIVISION.
            DECLARATIVES.
            INPUT-ERR SECTION.
                USE AFTER EXCEPTION ON INPUT-FILE.
            INPUT-ERR-PARA.
                DISPLAY "FILE ERROR".
            END DECLARATIVES.
            MAIN-PARA.
                DISPLAY "HELLO".
                STOP RUN.
        "#;
        let program = parse_free(source).expect("parse failed");
        let proc = program.procedure.expect("no procedure division");
        assert_eq!(proc.declaratives.len(), 1);
        assert_eq!(proc.declaratives[0].name.to_ascii_uppercase(), "INPUT-ERR");
        match &proc.declaratives[0].use_statement {
            cobol_ast::proc_div::UseStatement::AfterException {
                file_names,
                is_global,
            } => {
                assert_eq!(file_names.len(), 1);
                assert!(!is_global);
            }
            other => panic!("expected AfterException, got {:?}", other),
        }
        assert!(!proc.declaratives[0].paragraphs.is_empty());
    }

    #[test]
    fn test_parse_declaratives_with_keyword_section_name() {
        let source = r#"
            IDENTIFICATION DIVISION.
            PROGRAM-ID. DECLDBG.
            PROCEDURE DIVISION.
            DECLARATIVES.
            GO-TO SECTION.
                USE FOR DEBUGGING ON GO-TO-TEST.
            DBG-PARA.
                DISPLAY "TRACE".
            END DECLARATIVES.
            MAIN-PARA.
                STOP RUN.
        "#;
        let program = parse_free(source).expect("parse failed");
        let proc = program.procedure.expect("no procedure division");
        assert_eq!(proc.declaratives.len(), 1);
        assert_eq!(proc.declaratives[0].name.to_ascii_uppercase(), "GO-TO");
        match &proc.declaratives[0].use_statement {
            cobol_ast::proc_div::UseStatement::ForDebugging { debug_items } => {
                assert_eq!(debug_items, &vec![smol_str::SmolStr::new("GO-TO-TEST")]);
            }
            other => panic!("expected ForDebugging, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_evaluate_also() {
        let source = r#"
            IDENTIFICATION DIVISION.
            PROGRAM-ID. EVALTEST.
            DATA DIVISION.
            WORKING-STORAGE SECTION.
            01 WS-A PIC 9 VALUE 1.
            01 WS-B PIC 9 VALUE 2.
            PROCEDURE DIVISION.
            MAIN-PARA.
                EVALUATE WS-A ALSO WS-B
                    WHEN 1 ALSO 2
                        DISPLAY "ONE-TWO"
                    WHEN 1 ALSO 3
                        DISPLAY "ONE-THREE"
                    WHEN OTHER
                        DISPLAY "OTHER"
                END-EVALUATE.
                STOP RUN.
        "#;
        let program = parse_free(source).expect("parse failed");
        let proc = program.procedure.expect("no procedure division");
        let stmts = &proc.paragraphs[0].sentences[0].statements;
        match &stmts[0] {
            statement::Statement::Evaluate(eval) => {
                assert_eq!(eval.subjects.len(), 2, "should have 2 subjects");
                assert_eq!(eval.when_clauses.len(), 2);
                // Each WHEN clause should have 2 ALSO-separated object groups
                assert_eq!(eval.when_clauses[0].objects.len(), 2);
                assert_eq!(eval.when_clauses[1].objects.len(), 2);
            }
            other => panic!("expected Evaluate, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_move_then_perform_same_sentence() {
        // MOVE X TO Y PERFORM Z — two statements in one sentence (no period between).
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST1.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01 X PIC X.
       01 Y PIC X.
       PROCEDURE DIVISION.
       PARA-1.
           MOVE X TO Y PERFORM WRITE-LINE.
       WRITE-LINE.
           DISPLAY \"DONE\".
           STOP RUN.";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        // PARA-1 should have one sentence with two statements: MOVE and PERFORM
        let para = &proc.paragraphs[0];
        let stmts: Vec<_> = para
            .sentences
            .iter()
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(
            stmts.len() >= 2,
            "expected at least 2 statements in sentence, got {}",
            stmts.len()
        );
        assert!(
            matches!(stmts[0], statement::Statement::Move(_)),
            "first statement should be MOVE"
        );
        assert!(
            matches!(stmts[1], statement::Statement::Perform(_)),
            "second statement should be PERFORM"
        );
    }

    #[test]
    fn test_if_with_perform_then_perform() {
        // IF cond PERFORM proc1 PERFORM proc2 — two PERFORMs in IF then-body.
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST2.
       PROCEDURE DIVISION.
       MAIN-PARA.
           IF X EQUAL TO \"Y\"
               PERFORM WRITE-LINE
               PERFORM FAIL-ROUTINE
           END-IF.
           STOP RUN.
       WRITE-LINE.
           DISPLAY \"W\".
       FAIL-ROUTINE.
           DISPLAY \"F\".";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc.paragraphs[0]
            .sentences
            .iter()
            .flat_map(|s| s.statements.iter())
            .collect();
        // First statement should be IF
        match &stmts[0] {
            statement::Statement::If(if_stmt) => {
                assert_eq!(
                    if_stmt.then_body.len(),
                    2,
                    "IF then-body should have 2 PERFORM statements"
                );
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_if_without_end_if() {
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST-NESTED-IF.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01 A PIC X.
       01 B PIC X.
       PROCEDURE DIVISION.
       MAIN-PARA.
           IF A = \"Y\"
               IF B = \"Y\"
                   DISPLAY \"BOTH\"
               ELSE
                   NEXT SENTENCE
           ELSE
               DISPLAY \"OUTER\"
           .
           STOP RUN.";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc.paragraphs[0]
            .sentences
            .iter()
            .flat_map(|s| s.statements.iter())
            .collect();
        match &stmts[0] {
            statement::Statement::If(if_stmt) => {
                assert_eq!(if_stmt.then_body.len(), 1);
                assert_eq!(if_stmt.else_body.len(), 1);
                assert!(matches!(if_stmt.then_body[0], statement::Statement::If(_)));
            }
            other => panic!("expected If, got {:?}", other),
        }
    }

    #[test]
    fn test_perform_proc_followed_by_move() {
        // PERFORM proc-name MOVE X TO Y — PERFORM followed by MOVE on same line.
        let src = "\
       IDENTIFICATION DIVISION.
       PROGRAM-ID. TEST3.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01 X PIC X.
       01 Y PIC X.
       PROCEDURE DIVISION.
       MAIN-PARA.
           PERFORM WRITE-LINE MOVE X TO Y.
           STOP RUN.
       WRITE-LINE.
           DISPLAY \"W\".";
        let program = parse_free(src).unwrap();
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc.paragraphs[0]
            .sentences
            .iter()
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(
            stmts.len() >= 2,
            "expected at least 2 statements, got {}",
            stmts.len()
        );
        assert!(
            matches!(stmts[0], statement::Statement::Perform(_)),
            "first statement should be PERFORM"
        );
        assert!(
            matches!(stmts[1], statement::Statement::Move(_)),
            "second statement should be MOVE"
        );
    }
}
