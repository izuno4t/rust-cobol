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
            .paragraphs
            .iter()
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
        assert_eq!(data.working_storage.len(), 2);
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
        let proc = program.procedure.unwrap();
        let stmts: Vec<_> = proc
            .paragraphs
            .iter()
            .flat_map(|p| p.sentences.iter())
            .flat_map(|s| s.statements.iter())
            .collect();
        assert!(stmts.iter().any(|s| matches!(s, Statement::If(_))));
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
            cobol_ast::proc_div::UseStatement::AfterException { file_names } => {
                assert_eq!(file_names.len(), 1);
            }
            other => panic!("expected AfterException, got {:?}", other),
        }
        assert!(!proc.declaratives[0].paragraphs.is_empty());
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
}
