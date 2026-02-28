// COBOL Parser - ENVIRONMENT DIVISION parsing

use cobol_ast::env_div::*;
use cobol_lexer::token::TokenKind;
use smol_str::SmolStr;

use crate::parser::Parser;

impl Parser {
    /// Parse the ENVIRONMENT DIVISION.
    pub fn parse_environment_division(&mut self) -> Result<EnvironmentDivision, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Environment)?;
        self.expect(TokenKind::Division)?;
        self.expect(TokenKind::Period)?;

        let mut configuration = None;
        let mut input_output = None;

        if self.check(TokenKind::Configuration) {
            configuration = Some(self.parse_configuration_section()?);
        }

        if self.check(TokenKind::InputOutput) {
            input_output = Some(self.parse_input_output_section()?);
        }

        let end_span = self.span();

        Ok(EnvironmentDivision {
            configuration,
            input_output,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_configuration_section(&mut self) -> Result<ConfigurationSection, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Configuration)?;
        self.expect(TokenKind::Section)?;
        self.expect(TokenKind::Period)?;

        let mut source_computer = None;
        let mut object_computer = None;
        let special_names = Vec::new();
        let repository = Vec::new();

        loop {
            if self.at_division_header() || self.at_eof() || self.check(TokenKind::InputOutput) {
                break;
            }

            if self.check(TokenKind::SourceComputer) {
                self.advance();
                self.expect(TokenKind::Period)?;
                source_computer = Some(self.parse_paragraph_text());
            } else if self.check(TokenKind::ObjectComputer) {
                self.advance();
                self.expect(TokenKind::Period)?;
                object_computer = Some(self.parse_paragraph_text());
            } else if self.check(TokenKind::SpecialNames) || self.check(TokenKind::Repository) {
                self.advance();
                self.expect(TokenKind::Period)?;
                self.skip_to_next_paragraph();
            } else {
                self.advance();
            }
        }

        let end_span = self.span();

        Ok(ConfigurationSection {
            source_computer,
            object_computer,
            special_names,
            repository,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_input_output_section(&mut self) -> Result<InputOutputSection, ()> {
        let start_span = self.span();

        self.expect(TokenKind::InputOutput)?;
        self.expect(TokenKind::Section)?;
        self.expect(TokenKind::Period)?;

        let mut file_controls = Vec::new();

        if self.check(TokenKind::FileControl) {
            self.advance();
            self.expect(TokenKind::Period)?;

            while self.check(TokenKind::Select) && !self.at_eof() {
                if let Ok(entry) = self.parse_file_control_entry() {
                    file_controls.push(entry);
                } else {
                    self.recover_to_period();
                }
            }
        }

        if self.check(TokenKind::IoControl) {
            self.advance();
            self.expect(TokenKind::Period)?;
            self.skip_to_next_paragraph();
        }

        let end_span = self.span();

        Ok(InputOutputSection {
            file_controls,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_file_control_entry(&mut self) -> Result<FileControlEntry, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Select)?;

        if self.check_identifier("OPTIONAL") {
            self.advance();
        }

        let file_name = self.expect_identifier()?;

        self.expect(TokenKind::Assign)?;
        self.eat(TokenKind::To);

        let assign_to = self.expect_identifier_or_literal()?;

        let mut organization = None;
        let mut access_mode = None;
        let mut record_key = None;
        let mut alternate_keys = Vec::new();
        let mut file_status = None;

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Organization) {
                self.advance();
                self.eat_is();
                organization = Some(self.parse_file_organization()?);
            } else if self.check(TokenKind::AccessMode) {
                self.advance();
                self.eat(TokenKind::Mode);
                self.eat_is();
                access_mode = Some(self.parse_access_mode()?);
            } else if self.check(TokenKind::RecordKey) {
                self.advance();
                self.eat_is();
                record_key = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::AlternateRecordKey) {
                self.advance();
                self.eat(TokenKind::Record);
                self.eat(TokenKind::Key);
                self.eat_is();
                alternate_keys.push(self.parse_qualified_name()?);
            } else if self.check(TokenKind::FileStatus) {
                self.advance();
                self.eat_is();
                file_status = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Mode) {
                self.advance();
                self.eat_is();
                access_mode = Some(self.parse_access_mode()?);
            } else {
                self.advance();
            }
        }

        self.expect(TokenKind::Period)?;
        let end_span = self.span();

        Ok(FileControlEntry {
            file_name,
            assign_to,
            organization,
            access_mode,
            record_key,
            alternate_keys,
            file_status,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_file_organization(&mut self) -> Result<FileOrganization, ()> {
        if self.check(TokenKind::Sequential) {
            self.advance();
            Ok(FileOrganization::Sequential)
        } else if self.check(TokenKind::Indexed) {
            self.advance();
            Ok(FileOrganization::Indexed)
        } else if self.check(TokenKind::Relative) {
            self.advance();
            Ok(FileOrganization::Relative)
        } else if self.check(TokenKind::Line) {
            self.advance();
            self.eat(TokenKind::Sequential);
            Ok(FileOrganization::LineSequential)
        } else {
            self.error("expected file organization type");
            Err(())
        }
    }

    fn parse_access_mode(&mut self) -> Result<AccessMode, ()> {
        if self.check(TokenKind::Sequential) {
            self.advance();
            Ok(AccessMode::Sequential)
        } else if self.check(TokenKind::Random) {
            self.advance();
            Ok(AccessMode::Random)
        } else if self.check(TokenKind::Dynamic) {
            self.advance();
            Ok(AccessMode::Dynamic)
        } else {
            self.error("expected access mode");
            Err(())
        }
    }

    fn parse_paragraph_text(&mut self) -> SmolStr {
        let mut parts = Vec::new();
        while !self.at_eof() && !self.check(TokenKind::Period) {
            parts.push(self.current_text().to_string());
            self.advance();
        }
        self.eat(TokenKind::Period);
        SmolStr::from(parts.join(" "))
    }

    fn skip_to_next_paragraph(&mut self) {
        while !self.at_eof() {
            if self.at_division_header() {
                break;
            }
            if self.check(TokenKind::SourceComputer)
                || self.check(TokenKind::ObjectComputer)
                || self.check(TokenKind::SpecialNames)
                || self.check(TokenKind::Repository)
                || self.check(TokenKind::InputOutput)
                || self.check(TokenKind::FileControl)
                || self.check(TokenKind::IoControl)
                || self.at_data_section_header()
            {
                break;
            }
            self.advance();
        }
    }

    /// Expect and consume an identifier, returning its text.
    pub(crate) fn expect_identifier(&mut self) -> Result<SmolStr, ()> {
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            Ok(self.advance().text)
        } else {
            self.error("expected identifier");
            Err(())
        }
    }

    /// Expect an identifier or a string literal, returning its text.
    pub(crate) fn expect_identifier_or_literal(&mut self) -> Result<SmolStr, ()> {
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            Ok(self.advance().text)
        } else if self.check(TokenKind::StringLiteral) {
            let tok = self.advance();
            let s = tok.text.as_str();
            let stripped = if s.len() >= 2 { &s[1..s.len() - 1] } else { s };
            Ok(SmolStr::from(stripped))
        } else {
            self.error("expected identifier or literal");
            Err(())
        }
    }
}
