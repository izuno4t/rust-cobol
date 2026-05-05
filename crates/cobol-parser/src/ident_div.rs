// COBOL Parser - IDENTIFICATION DIVISION parsing

use cobol_ast::IdentificationDivision;
use cobol_lexer::token::TokenKind;
use smol_str::SmolStr;

use crate::parser::Parser;

impl Parser {
    /// Parse the IDENTIFICATION DIVISION.
    ///
    /// Grammar:
    ///   IDENTIFICATION DIVISION.
    ///   PROGRAM-ID. <name> [IS INITIAL|RECURSIVE|COMMON] .
    ///   [AUTHOR. <text> .]
    ///   [INSTALLATION. <text> .]
    ///   [DATE-WRITTEN. <text> .]
    ///   [DATE-COMPILED. <text> .]
    ///   [SECURITY. <text> .]
    pub fn parse_identification_division(&mut self) -> Result<IdentificationDivision, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Identification)?;
        self.expect(TokenKind::Division)?;
        self.expect(TokenKind::Period)?;

        self.expect(TokenKind::ProgramId)?;
        self.expect(TokenKind::Period)?;

        let program_id = self.parse_program_name()?;

        let mut is_initial = false;
        let mut is_recursive = false;
        let mut is_common = false;

        // Optional IS keyword
        self.eat_is();

        // Optional attributes
        loop {
            if self.check(TokenKind::Initial) {
                self.advance();
                is_initial = true;
            } else if self.check_identifier("RECURSIVE") {
                self.advance();
                is_recursive = true;
            } else if self.check_identifier("COMMON") {
                self.advance();
                is_common = true;
            } else {
                break;
            }
        }

        self.expect(TokenKind::Period)?;

        // Optional paragraphs
        let mut author = None;
        let mut installation = None;
        let mut date_written = None;
        let mut date_compiled = None;
        let mut security = None;

        loop {
            if self.at_division_header() || self.at_eof() {
                break;
            }
            if self.check_identifier("AUTHOR") {
                let warning_span = self.span();
                self.advance();
                self.expect(TokenKind::Period)?;
                self.warning_at(warning_span, "AUTHOR is an obsolete feature");
                author = Some(self.parse_comment_text());
            } else if self.check_identifier("INSTALLATION") {
                let warning_span = self.span();
                self.advance();
                self.expect(TokenKind::Period)?;
                self.warning_at(warning_span, "INSTALLATION is an obsolete feature");
                installation = Some(self.parse_comment_text());
            } else if self.check_identifier("DATE-WRITTEN") {
                let warning_span = self.span();
                self.advance();
                self.expect(TokenKind::Period)?;
                self.warning_at(warning_span, "DATE-WRITTEN is an obsolete feature");
                date_written = Some(self.parse_comment_text());
            } else if self.check_identifier("DATE-COMPILED") {
                let warning_span = self.span();
                self.advance();
                self.expect(TokenKind::Period)?;
                self.warning_at(warning_span, "DATE-COMPILED is an obsolete feature");
                date_compiled = Some(self.parse_comment_text());
            } else if self.check_identifier("SECURITY") {
                let warning_span = self.span();
                self.advance();
                self.expect(TokenKind::Period)?;
                self.warning_at(warning_span, "SECURITY is an obsolete feature");
                security = Some(self.parse_comment_text());
            } else {
                self.advance();
            }
        }

        let end_span = self.span();

        Ok(IdentificationDivision {
            program_id,
            is_initial,
            is_recursive,
            is_common,
            author,
            installation,
            date_written,
            date_compiled,
            security,
            span: start_span.merge(&end_span),
        })
    }

    /// Parse a program name (identifier or string literal).
    fn parse_program_name(&mut self) -> Result<SmolStr, ()> {
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            let tok = self.advance();
            Ok(tok.text)
        } else if self.check(TokenKind::StringLiteral) {
            let tok = self.advance();
            let s = tok.text.as_str();
            let stripped = if s.len() >= 2 { &s[1..s.len() - 1] } else { s };
            Ok(SmolStr::from(stripped))
        } else {
            self.error("expected program name");
            Err(())
        }
    }

    /// Parse comment text until the next period.
    fn parse_comment_text(&mut self) -> SmolStr {
        let mut parts = Vec::new();
        while !self.at_eof() && !self.check(TokenKind::Period) {
            if self.at_division_header() {
                break;
            }
            parts.push(self.current_text().to_string());
            self.advance();
        }
        self.eat(TokenKind::Period);
        SmolStr::from(parts.join(" "))
    }
}
