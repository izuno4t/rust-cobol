// COBOL Compiler - Semantic analysis
//
// This crate performs semantic analysis on a parsed COBOL AST:
// - Symbol table construction with scope management
// - PICTURE clause analysis for data type determination
// - Name resolution (including COBOL qualified name lookup)
// - Basic type checking for statement operands

pub mod analyzer;
pub mod name_resolver;
pub mod picture_analyzer;
pub mod symbol_table;
pub mod type_checker;

pub use analyzer::{AnalysisResult, SemanticAnalyzer};
pub use picture_analyzer::PictureAnalyzer;
pub use symbol_table::{CobolType, Scope, ScopeKind, Symbol, SymbolKind, SymbolTable};

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_ast::PictureCategory;
    use cobol_common::{FileId, SourceFormat, Span};
    use cobol_lexer::Lexer;
    use cobol_parser::Parser;

    /// Helper: lex + parse + analyze a COBOL source string.
    fn analyze(source: &str) -> (AnalysisResult, cobol_diagnostics::DiagnosticReporter) {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        let program = parser.parse_program().unwrap();
        let mut analyzer = SemanticAnalyzer::new();
        let result = analyzer.analyze(&program);
        let diagnostics = analyzer.take_diagnostics();
        (result, diagnostics)
    }

    /// Helper: analyze a PICTURE string directly.
    fn analyze_picture(pic_str: &str) -> cobol_ast::PictureClause {
        PictureAnalyzer::analyze(pic_str, Span::dummy())
    }

    #[test]
    fn test_basic_analysis() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST1.
PROCEDURE DIVISION.
    STOP RUN.
";
        let (result, diag) = analyze(src);
        assert!(!result.has_errors);
        assert!(!diag.has_errors());
    }

    #[test]
    fn test_data_item_registration() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST2.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
01  WS-COUNT PIC 9(5).
PROCEDURE DIVISION.
    STOP RUN.
";
        let (result, _) = analyze(src);
        assert!(!result.has_errors);
        let sym = result.symbol_table.lookup(&"WS-NAME".into());
        assert!(sym.is_some());
        let sym = result.symbol_table.lookup(&"WS-COUNT".into());
        assert!(sym.is_some());
    }

    #[test]
    fn test_picture_analysis_numeric() {
        let pic = analyze_picture("9(5)");
        assert_eq!(pic.category, PictureCategory::Numeric);
        assert_eq!(pic.size, 5);
        assert!(!pic.is_signed);
        assert_eq!(pic.decimal_positions, 0);
    }

    #[test]
    fn test_picture_analysis_signed_decimal() {
        let pic = analyze_picture("S9(7)V99");
        assert_eq!(pic.category, PictureCategory::Numeric);
        assert_eq!(pic.size, 9); // 7 + 2 = 9 digit positions
        assert!(pic.is_signed);
        assert_eq!(pic.decimal_positions, 2);
    }

    #[test]
    fn test_picture_analysis_alphanumeric() {
        let pic = analyze_picture("X(20)");
        assert_eq!(pic.category, PictureCategory::Alphanumeric);
        assert_eq!(pic.size, 20);
    }

    #[test]
    fn test_picture_analysis_edited() {
        let pic = analyze_picture("Z,ZZZ,ZZ9.99");
        assert_eq!(pic.category, PictureCategory::NumericEdited);
        assert!(pic.is_edited);
    }

    #[test]
    fn test_program_id_registered() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. MY-PROG.
PROCEDURE DIVISION.
    STOP RUN.
";
        let (result, _) = analyze(src);
        let sym = result.symbol_table.lookup(&"MY-PROG".into());
        assert!(sym.is_some());
        assert!(matches!(sym.unwrap().kind, SymbolKind::Program));
    }

    #[test]
    fn test_data_type_numeric() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST3.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NUM PIC 9(5).
PROCEDURE DIVISION.
    STOP RUN.
";
        let (result, _) = analyze(src);
        let sym = result.symbol_table.lookup(&"WS-NUM".into()).unwrap();
        assert!(sym.data_type.as_ref().unwrap().is_numeric());
    }

    #[test]
    fn test_data_type_alphanumeric() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST4.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-STR PIC X(10).
PROCEDURE DIVISION.
    STOP RUN.
";
        let (result, _) = analyze(src);
        let sym = result.symbol_table.lookup(&"WS-STR".into()).unwrap();
        assert!(sym.data_type.as_ref().unwrap().is_alphanumeric());
    }

    #[test]
    fn test_communication_clause_names_are_registered() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-COMM.
DATA DIVISION.
COMMUNICATION SECTION.
CD CM-IN FOR INPUT
   STATUS KEY IS STATUS-KEY
   END KEY IS END-KEY
   MESSAGE COUNT IS MSG-COUNT.
PROCEDURE DIVISION.
    MOVE STATUS-KEY TO END-KEY.
    MOVE MSG-COUNT TO MSG-COUNT.
    STOP RUN.
";
        let (result, diag) = analyze(src);
        assert!(!result.has_errors, "{:?}", diag.diagnostics());
        assert!(!diag.has_errors(), "{:?}", diag.diagnostics());
        assert!(result.symbol_table.lookup(&"STATUS-KEY".into()).is_some());
        assert!(result.symbol_table.lookup(&"END-KEY".into()).is_some());
        assert!(result.symbol_table.lookup(&"MSG-COUNT".into()).is_some());
    }
}
