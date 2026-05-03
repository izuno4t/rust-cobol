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
        let mut special_names = Vec::new();
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
            } else if self.check(TokenKind::SpecialNames) {
                self.advance();
                self.expect(TokenKind::Period)?;
                special_names = self.parse_special_names_paragraph();
            } else if self.check(TokenKind::Repository) {
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
        let mut same_record_areas = Vec::new();

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
            same_record_areas = self.parse_io_control_paragraph();
        }

        let end_span = self.span();

        Ok(InputOutputSection {
            file_controls,
            same_record_areas,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_io_control_paragraph(&mut self) -> Vec<Vec<SmolStr>> {
        let mut same_record_areas = Vec::new();

        while !self.at_eof() {
            if self.at_division_header()
                || self.check(TokenKind::FileControl)
                || self.check(TokenKind::IoControl)
                || self.at_data_section_header()
            {
                break;
            }

            if self.check_identifier("SAME") {
                self.advance();
                self.eat(TokenKind::Record);
                if self.check_identifier("AREA") || self.check_identifier("AREAS") {
                    self.advance();
                }

                let mut files = Vec::new();
                while !self.check(TokenKind::Period)
                    && !self.at_eof()
                    && !self.at_division_header()
                    && !self.at_data_section_header()
                {
                    if self.check(TokenKind::Comma) {
                        self.advance();
                        continue;
                    }
                    if let Ok(name) = self.expect_identifier() {
                        files.push(name);
                    } else {
                        self.advance();
                    }
                }
                if files.len() > 1 {
                    same_record_areas.push(files);
                }
                self.eat(TokenKind::Period);
                continue;
            }

            while !self.check(TokenKind::Period) && !self.at_eof() {
                self.advance();
            }
            self.eat(TokenKind::Period);
        }

        same_record_areas
    }

    fn parse_file_control_entry(&mut self) -> Result<FileControlEntry, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Select)?;

        let optional = if self.check_identifier("OPTIONAL") {
            let warning_span = self.span();
            self.advance();
            self.warning_at(
                warning_span,
                "SELECT OPTIONAL is a non-conforming file-control feature",
            );
            true
        } else {
            false
        };

        let file_name = self.expect_identifier()?;

        let mut assign_to = None;
        let mut organization = None;
        let mut access_mode = None;
        let mut record_key = None;
        let mut relative_key = None;
        let mut alternate_keys = Vec::new();
        let mut file_status = None;

        // COBOL SELECT clauses are order-independent; loop until period or EOF.
        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Assign) {
                self.advance();
                self.eat(TokenKind::To);
                assign_to = Some(self.expect_identifier_or_literal()?);
            } else if self.check(TokenKind::Organization) {
                self.advance();
                self.eat_is();
                organization = Some(self.parse_file_organization()?);
            } else if self.check(TokenKind::AccessMode) {
                self.advance();
                self.eat(TokenKind::Mode);
                self.eat_is();
                access_mode = Some(self.parse_access_mode()?);
            } else if self.check(TokenKind::RecordKey) {
                let warning_span = self.span();
                self.advance();
                self.eat_is();
                self.warning_at(
                    warning_span,
                    "RECORD KEY is a non-conforming indexed file feature",
                );
                record_key = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Record) && self.peek(1).kind == TokenKind::Key {
                let warning_span = self.span();
                self.advance();
                self.advance();
                self.eat_is();
                self.warning_at(
                    warning_span,
                    "RECORD KEY is a non-conforming indexed file feature",
                );
                record_key = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Record) && self.peek(1).kind == TokenKind::Identifier {
                // NIST fixed-format sources sometimes use an abbreviated
                // FILE-CONTROL form like `RECORD FOO-KEY.` for RECORD KEY.
                let warning_span = self.span();
                self.advance();
                self.warning_at(
                    warning_span,
                    "RECORD KEY is a non-conforming indexed file feature",
                );
                record_key = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Relative) && self.peek(1).kind == TokenKind::Key {
                self.advance();
                self.advance();
                self.eat_is();
                relative_key = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::AlternateRecordKey) {
                let warning_span = self.span();
                self.advance();
                self.eat(TokenKind::Record);
                self.eat(TokenKind::Key);
                self.eat_is();
                self.warning_at(
                    warning_span,
                    "ALTERNATE RECORD KEY is a non-conforming indexed file feature",
                );
                let name = self.parse_qualified_name()?;
                let duplicates = if self.check(TokenKind::With) || self.check(TokenKind::Duplicates)
                {
                    self.eat(TokenKind::With);
                    self.eat(TokenKind::Duplicates);
                    true
                } else {
                    false
                };
                alternate_keys.push(cobol_ast::env_div::AlternateKey { name, duplicates });
            } else if self.check(TokenKind::FileStatus) {
                self.advance();
                self.eat_is();
                file_status = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::File)
                && self.peek(1).text.eq_ignore_ascii_case("STATUS")
            {
                // Standard COBOL syntax: FILE STATUS IS variable-name
                self.advance(); // FILE
                self.advance(); // STATUS
                self.eat_is();
                file_status = Some(self.parse_qualified_name()?);
            } else if self.check_identifier("STATUS") {
                // COBOL permits the abbreviated form: STATUS IS variable-name.
                self.advance(); // STATUS
                self.eat_is();
                file_status = Some(self.parse_qualified_name()?);
            } else if self.check_identifier("RESERVE") {
                let warning_span = self.span();
                self.advance();
                if self.check(TokenKind::IntegerLiteral) {
                    self.advance();
                }
                if self.check_identifier("AREA") || self.check_identifier("AREAS") {
                    self.advance();
                }
                self.warning_at(
                    warning_span,
                    "RESERVE AREAS is a non-conforming file-control feature",
                );
            } else if self.check(TokenKind::Mode) {
                self.advance();
                self.eat_is();
                access_mode = Some(self.parse_access_mode()?);
            } else {
                self.advance();
            }
        }

        // ASSIGN TO is required; default to file name if not specified.
        let assign_to = assign_to.unwrap_or_else(|| file_name.clone());

        self.expect(TokenKind::Period)?;
        let end_span = self.span();

        Ok(FileControlEntry {
            file_name,
            optional,
            assign_to,
            organization,
            access_mode,
            record_key,
            relative_key,
            alternate_keys,
            file_status,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_file_organization(&mut self) -> Result<FileOrganization, ()> {
        let start_span = self.span();
        if self.check(TokenKind::Sequential) {
            self.advance();
            Ok(FileOrganization::Sequential)
        } else if self.check(TokenKind::Indexed) {
            self.advance();
            self.warning_at(
                start_span,
                "ORGANIZATION IS INDEXED is a non-conforming indexed feature",
            );
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
        let start_span = self.span();
        if self.check(TokenKind::Sequential) {
            self.advance();
            Ok(AccessMode::Sequential)
        } else if self.check(TokenKind::Random) {
            self.advance();
            self.warning_at(
                start_span,
                "ACCESS MODE IS RANDOM is a non-conforming indexed feature",
            );
            Ok(AccessMode::Random)
        } else if self.check(TokenKind::Dynamic) {
            self.advance();
            self.warning_at(
                start_span,
                "ACCESS MODE IS DYNAMIC is a non-conforming indexed feature",
            );
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

    /// Parse the SPECIAL-NAMES paragraph, extracting switch condition
    /// names and skipping other entries (CLASS, CURRENCY, DECIMAL-POINT).
    fn parse_special_names_paragraph(&mut self) -> Vec<SpecialNameEntry> {
        let mut entries = Vec::new();

        while !self.at_eof() {
            // Check for end of paragraph.
            if self.at_division_header()
                || self.check(TokenKind::SourceComputer)
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

            // CLASS clause — skip to next period.
            if self.check(TokenKind::Class) || self.check_identifier("CLASS") {
                self.advance();
                while !self.check(TokenKind::Period) && !self.at_eof() {
                    self.advance();
                }
                self.eat(TokenKind::Period);
                continue;
            }

            // DECIMAL-POINT IS COMMA — set flag and skip to period.
            if self.check_identifier("DECIMAL-POINT") {
                self.advance(); // DECIMAL-POINT
                                // Look for IS COMMA
                if self.check_identifier("IS") {
                    self.advance();
                }
                if self.check(TokenKind::Comma) || self.current().text.eq_ignore_ascii_case("COMMA")
                {
                    self.decimal_point_is_comma = true;
                    self.advance();
                }
                while !self.check(TokenKind::Period) && !self.at_eof() {
                    self.advance();
                }
                self.eat(TokenKind::Period);
                continue;
            }

            // CURRENCY SIGN / other clauses — skip to next period or next
            // SPECIAL-NAMES clause keyword (multiple clauses may share a
            // single terminating period).
            if self.check_identifier("CURRENCY")
                || self.check_identifier("CURSOR")
                || self.check_identifier("CRT")
                || self.check_identifier("SYMBOLIC")
                || self.check_identifier("ALPHABET")
            {
                self.advance();
                while !self.check(TokenKind::Period)
                    && !self.at_eof()
                    && !self.check_identifier("DECIMAL-POINT")
                    && !self.check_identifier("CURRENCY")
                    && !self.check_identifier("CURSOR")
                    && !self.check_identifier("CRT")
                    && !self.check_identifier("SYMBOLIC")
                    && !self.check_identifier("ALPHABET")
                    && !self.check(TokenKind::Class)
                    && !self.check_identifier("CLASS")
                {
                    self.advance();
                }
                self.eat(TokenKind::Period);
                continue;
            }

            // Try to parse an implementor-name entry:
            //   system-name IS user-name
            //       [ON [STATUS] IS on-condition]
            //       [OFF [STATUS] IS off-condition].
            if self.check(TokenKind::Identifier)
                || self.current().kind.is_keyword()
                || self.check(TokenKind::StringLiteral)
                || self.check(TokenKind::IntegerLiteral)
            {
                let start_span = self.span();
                let system_name = self.advance().text;

                let mut user_name = None;
                let mut on_condition = None;
                let mut off_condition = None;

                // IS user-name
                if self.check_identifier("IS") {
                    self.advance(); // IS
                    if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
                        user_name = Some(self.advance().text);
                    }
                }

                // ON [STATUS] IS condition-name  /  OFF [STATUS] IS condition-name
                // These can appear in any order, and may repeat.
                // Note: ON is TokenKind::OnKw, OFF is TokenKind::Off.
                loop {
                    if self.check(TokenKind::OnKw) || self.check_identifier("ON") {
                        self.advance(); // ON
                        self.eat_identifier("STATUS");
                        self.eat_is();
                        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
                            on_condition = Some(self.advance().text);
                        }
                    } else if self.check(TokenKind::Off) || self.check_identifier("OFF") {
                        self.advance(); // OFF
                        self.eat_identifier("STATUS");
                        self.eat_is();
                        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
                            off_condition = Some(self.advance().text);
                        }
                    } else {
                        break;
                    }
                }

                let end_span = self.span();

                // Only record entries that actually defined something useful.
                if user_name.is_some() || on_condition.is_some() || off_condition.is_some() {
                    entries.push(SpecialNameEntry {
                        system_name,
                        user_name,
                        on_condition,
                        off_condition,
                        span: start_span.merge(&end_span),
                    });
                }

                // Consume the trailing period if present.
                self.eat(TokenKind::Period);
                continue;
            }

            // Skip unknown tokens.
            self.advance();
        }

        entries
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
