// COBOL Parser - Core parser framework

use cobol_ast::*;
use cobol_common::{FileId, Span};
use cobol_diagnostics::DiagnosticReporter;
use cobol_lexer::token::{Token, TokenKind};
use smol_str::SmolStr;

use crate::error::{report_error, report_expected};

/// Recursive descent parser for COBOL source code.
///
/// Transforms a token stream (produced by the lexer) into a COBOL AST.
/// Supports error recovery by skipping to the next period (sentence terminator).
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    reporter: DiagnosticReporter,
    #[allow(dead_code)]
    file_id: FileId,
}

impl Parser {
    /// Creates a new parser from a token stream.
    ///
    /// Tokens produced by the lexer should include a trailing `Eof` token.
    /// Newline and error tokens are filtered out since they are not significant
    /// to parsing.
    pub fn new(tokens: Vec<Token>, file_id: FileId) -> Self {
        let mut tokens: Vec<Token> = tokens
            .into_iter()
            .filter(|t| t.kind != TokenKind::Newline && t.kind != TokenKind::Error)
            .collect();
        if tokens.is_empty() {
            tokens.push(Token {
                kind: TokenKind::Eof,
                text: SmolStr::default(),
                span: Span::dummy(),
            });
        }
        Self {
            tokens,
            pos: 0,
            reporter: DiagnosticReporter::new(),
            file_id,
        }
    }

    /// Parse a complete COBOL program (possibly containing nested programs).
    pub fn parse_program(&mut self) -> Result<CobolProgram, ()> {
        let start_span = self.span();

        let identification = self.parse_identification_division()?;

        let environment = if self.check(TokenKind::Environment) {
            Some(self.parse_environment_division()?)
        } else {
            None
        };

        let data = if self.check(TokenKind::Data) {
            Some(self.parse_data_division()?)
        } else {
            None
        };

        let procedure = if self.check(TokenKind::Procedure) {
            Some(self.parse_procedure_division()?)
        } else {
            None
        };

        // Parse nested programs (before END PROGRAM of the outer program)
        let mut nested_programs = Vec::new();
        while !self.at_eof() && self.check(TokenKind::Identification) && !self.at_end_program() {
            let nested = self.parse_program()?;
            nested_programs.push(nested);
        }

        // Consume optional END PROGRAM program-id.
        if self.at_end_program() {
            self.advance(); // END
            self.advance(); // PROGRAM
                            // Consume the program-id (may be an identifier or keyword)
            if !self.check(TokenKind::Period) && !self.at_eof() {
                self.advance(); // program-id
            }
            self.eat(TokenKind::Period);
        }

        let end_span = self.span();

        Ok(CobolProgram {
            identification,
            environment,
            data,
            procedure,
            nested_programs,
            span: start_span.merge(&end_span),
        })
    }

    /// Parse a compilation unit that may contain multiple programs.
    ///
    /// Returns the first program. Subsequent programs in the same source
    /// file are stored in `nested_programs` of the returned program for now,
    /// preserving them in the AST.
    pub fn parse_compilation_unit(&mut self) -> Result<Vec<CobolProgram>, ()> {
        let mut programs = Vec::new();
        while !self.at_eof() {
            programs.push(self.parse_program()?);
        }
        Ok(programs)
    }

    /// Get diagnostics reporter reference.
    pub fn diagnostics(&self) -> &DiagnosticReporter {
        &self.reporter
    }

    pub(crate) fn checkpoint(&self) -> usize {
        self.pos
    }

    pub(crate) fn restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Take ownership of diagnostics reporter.
    pub fn take_diagnostics(&mut self) -> DiagnosticReporter {
        std::mem::take(&mut self.reporter)
    }

    // --- Token access helpers ---

    /// Returns a reference to the current token.
    pub(crate) fn current(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    /// Peek at the token `n` positions ahead of the current position.
    pub(crate) fn peek(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    /// Advance the position by one and return the consumed token.
    pub(crate) fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        // Skip semicolons — they are noise separators in COBOL
        while self.current().kind == TokenKind::Semicolon && self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    /// Check if the current token matches the given kind.
    pub(crate) fn check(&self, kind: TokenKind) -> bool {
        self.current().kind == kind
    }

    /// Check if the current token is an identifier with the given name
    /// (case-insensitive).
    pub(crate) fn check_identifier(&self, name: &str) -> bool {
        (self.current().kind == TokenKind::Identifier || self.current().kind.is_keyword())
            && self.current().text.eq_ignore_ascii_case(name)
    }

    /// Consume a token of the expected kind, or report an error.
    pub(crate) fn expect(&mut self, kind: TokenKind) -> Result<Token, ()> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let span = self.span();
            let found = format!("{:?}", self.current().kind);
            let expected = format!("{:?}", kind);
            report_expected(&mut self.reporter, span, &expected, &found);
            Err(())
        }
    }

    /// Consume the current token if it matches the given kind. Returns the
    /// token if consumed, `None` otherwise.
    pub(crate) fn eat(&mut self, kind: TokenKind) -> Option<Token> {
        if self.check(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    /// Check if we have reached the end of the token stream.
    pub(crate) fn at_eof(&self) -> bool {
        self.current().kind == TokenKind::Eof
    }

    /// Skip tokens until we reach a period (COBOL sentence terminator) or EOF.
    /// Consumes the period if found.
    pub(crate) fn recover_to_period(&mut self) {
        while !self.at_eof() && !self.check(TokenKind::Period) {
            self.advance();
        }
        self.eat(TokenKind::Period);
    }

    /// Returns the span of the current token.
    pub(crate) fn span(&self) -> Span {
        self.current().span
    }

    /// Report an error at the current position.
    pub(crate) fn error(&mut self, msg: &str) {
        let span = self.span();
        report_error(&mut self.reporter, span, msg);
    }

    /// Returns the text of the current token as a SmolStr.
    pub(crate) fn current_text(&self) -> SmolStr {
        self.current().text.clone()
    }

    /// Skip optional keyword IS.
    pub(crate) fn eat_is(&mut self) {
        if self.check_identifier("IS") {
            self.advance();
        }
    }

    /// Check if the current token is a keyword that starts a new statement.
    pub(crate) fn at_statement_start(&self) -> bool {
        Self::is_statement_start_keyword(self.current().kind)
    }

    /// Check if the given `TokenKind` is a statement-starting keyword.
    pub(crate) fn is_statement_start_keyword(kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Move
                | TokenKind::Compute
                | TokenKind::Add
                | TokenKind::Subtract
                | TokenKind::Multiply
                | TokenKind::Divide
                | TokenKind::Display
                | TokenKind::Accept
                | TokenKind::Enable
                | TokenKind::Disable
                | TokenKind::Send
                | TokenKind::Receive
                | TokenKind::Purge
                | TokenKind::If
                | TokenKind::Evaluate
                | TokenKind::Perform
                | TokenKind::Go
                | TokenKind::GoTo
                | TokenKind::Call
                | TokenKind::Stop
                | TokenKind::Goback
                | TokenKind::Continue
                | TokenKind::Exit
                | TokenKind::Open
                | TokenKind::Close
                | TokenKind::Read
                | TokenKind::Write
                | TokenKind::Rewrite
                | TokenKind::Delete
                | TokenKind::Start
                | TokenKind::Return
                | TokenKind::String
                | TokenKind::Unstring
                | TokenKind::Inspect
                | TokenKind::Initialize
                | TokenKind::Set
                | TokenKind::Sort
                | TokenKind::Merge
                | TokenKind::Release
                | TokenKind::Cancel
                | TokenKind::Alter
                | TokenKind::Raise
                | TokenKind::Resume
                | TokenKind::Allocate
                | TokenKind::Free
                | TokenKind::Invoke
        )
    }

    /// Check if current token begins a division header.
    pub(crate) fn at_division_header(&self) -> bool {
        let k = self.current().kind;
        (k == TokenKind::Identification
            || k == TokenKind::Environment
            || k == TokenKind::Data
            || k == TokenKind::Procedure)
            && self.peek(1).kind == TokenKind::Division
    }

    /// Check if current token is a data-section header (in DATA DIVISION).
    pub(crate) fn at_data_section_header(&self) -> bool {
        let k = self.current().kind;
        matches!(
            k,
            TokenKind::WorkingStorage
                | TokenKind::LocalStorage
                | TokenKind::Linkage
                | TokenKind::Screen
                | TokenKind::Communication
                | TokenKind::Report
                | TokenKind::File
        ) && self.peek(1).kind == TokenKind::Section
    }
}
