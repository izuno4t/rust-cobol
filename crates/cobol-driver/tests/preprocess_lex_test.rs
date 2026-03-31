use std::fs;

use cobol_common::{FileId, SourceFormat};
use cobol_lexer::{token::TokenKind, Lexer};
use cobol_preprocessor::{preprocess, PreprocessorConfig};

#[test]
fn test_preprocess_then_lex_quote_heavy_replace_keeps_to_token() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("sm208a.cob");
    fs::write(&source_path, "").unwrap();

    let source = concat!(
        "036100 REPLACE   ==\"Z\"== BY                          ==\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036200-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036300-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036400-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036500-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036600-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "036700-    \"\"\"\"\"\"==.\n",
        "036800     MOVE \"Z\" TO WRK-XN-00322.\n",
    );
    let config = PreprocessorConfig {
        source_format: SourceFormat::Fixed,
        ..Default::default()
    };

    let preprocessed = preprocess(source, &source_path, &config);
    assert!(
        preprocessed.diagnostics.iter().all(|diag| !diag.is_error()),
        "{:?}",
        preprocessed.diagnostics
    );
    assert_eq!(preprocessed.effective_source_format, SourceFormat::Free);

    let mut lexer = Lexer::new(
        &preprocessed.source,
        FileId(0),
        preprocessed.effective_source_format,
    );
    let tokens = lexer.lex_all();

    let move_idx = tokens
        .iter()
        .position(|t| t.kind == TokenKind::Move)
        .expect("MOVE should remain after preprocessing");
    assert_eq!(
        tokens.get(move_idx + 1).map(|t| t.kind),
        Some(TokenKind::StringLiteral),
        "MOVE should be followed by a single string literal.\nsource:\n{}\ntokens:\n{:?}",
        preprocessed.source,
        tokens,
    );
    assert_eq!(
        tokens.get(move_idx + 2).map(|t| t.kind),
        Some(TokenKind::To),
        "quote-heavy replacement should still close before TO.\nsource:\n{}\ntokens:\n{:?}",
        preprocessed.source,
        tokens,
    );
}

#[test]
fn test_preprocess_then_lex_fixed_continued_string_closes_before_following_statement() {
    let dir = tempfile::tempdir().unwrap();
    let source_path = dir.path().join("sm208a_if.cob");
    fs::write(&source_path, "").unwrap();

    let source = concat!(
        "037600     IF      WRK-XN-00322 =                      \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "037700-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "037800-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "037900-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "038000-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "038100-    \"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\"\n",
        "038200-    \"\"\"\"                                                        \n",
        "038300             PERFORM PASS                                         \n",
        "038400             PERFORM PRINT-DETAIL                                 \n",
        "038500     ELSE                                                         \n",
        "038600             MOVE   \"REPLACING SINGLE CHARACTER BY 160 QUOTES\"    \n",
        "038700                  TO RE-MARK                                      \n",
    );
    let config = PreprocessorConfig {
        source_format: SourceFormat::Fixed,
        ..Default::default()
    };

    let preprocessed = preprocess(source, &source_path, &config);
    assert!(
        preprocessed.diagnostics.iter().all(|diag| !diag.is_error()),
        "{:?}",
        preprocessed.diagnostics
    );

    let mut lexer = Lexer::new(
        &preprocessed.source,
        FileId(0),
        preprocessed.effective_source_format,
    );
    let tokens = lexer.lex_all();

    assert!(
        !tokens.iter().any(|t| t.kind == TokenKind::Error),
        "continued IF literal should close before the following MOVE.\nsource:\n{}\ntokens:\n{:?}",
        preprocessed.source,
        tokens,
    );
}
