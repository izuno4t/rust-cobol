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
}
