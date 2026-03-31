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

    let mut lexer = Lexer::new(&preprocessed.source, FileId(0), SourceFormat::Fixed);
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
