// COBOL Parser - DATA DIVISION parsing

use cobol_ast::data_div::*;
use cobol_ast::picture::{PictureCategory, PictureClause};
use cobol_lexer::token::TokenKind;
use smol_str::SmolStr;

use crate::parser::Parser;

impl Parser {
    /// Parse the DATA DIVISION.
    pub fn parse_data_division(&mut self) -> Result<DataDivision, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Data)?;
        self.expect(TokenKind::Division)?;
        self.expect(TokenKind::Period)?;

        let mut file_section = Vec::new();
        let mut working_storage = Vec::new();
        let mut local_storage = Vec::new();
        let mut linkage = Vec::new();
        let mut screen = Vec::new();
        let mut communication = Vec::new();
        let mut report = Vec::new();

        loop {
            if self.at_division_header() || self.at_eof() {
                break;
            }

            if self.check(TokenKind::File) && self.peek(1).kind == TokenKind::Section {
                self.advance(); // FILE
                self.advance(); // SECTION
                self.expect(TokenKind::Period)?;
                file_section = self.parse_file_section()?;
            } else if self.check(TokenKind::WorkingStorage) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                working_storage = self.parse_data_items()?;
            } else if self.check(TokenKind::LocalStorage) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                local_storage = self.parse_data_items()?;
            } else if self.check(TokenKind::Linkage) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                linkage = self.parse_data_items()?;
            } else if self.check(TokenKind::Screen) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                screen = self.parse_data_items()?;
            } else if self.check(TokenKind::Communication) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                communication = self.parse_communication_section()?;
            } else if self.check(TokenKind::Report) {
                let rpt_span = self.span();
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
                report = vec![DataItem {
                    level: 1,
                    name: Some(smol_str::SmolStr::new("RW-DUMMY-MARKER")),
                    picture: None,
                    usage: None,
                    value: None,
                    occurs: None,
                    redefines: None,
                    renames: None,
                    sign_clause: None,
                    justified: false,
                    blank_when_zero: false,
                    is_external: false,
                    is_global: false,
                    condition_values: Vec::new(),
                    line_clause: None,
                    column_clause: None,
                    blank_screen: false,
                    blank_line: false,
                    highlight: false,
                    reverse_video: false,
                    source_field: None,
                    using_field: None,
                    children: Vec::new(),
                    span: rpt_span,
                }];
                self.skip_section_content();
            } else {
                self.advance();
            }
        }

        let end_span = self.span();

        Ok(DataDivision {
            file_section,
            working_storage,
            local_storage,
            linkage,
            screen,
            communication,
            report,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_file_section(&mut self) -> Result<Vec<FileDescription>, ()> {
        let mut fds = Vec::new();

        while (self.check(TokenKind::Fd) || self.check(TokenKind::Sd))
            && !self.at_division_header()
            && !self.at_eof()
        {
            let fd = self.parse_file_description()?;
            fds.push(fd);
        }

        Ok(fds)
    }

    fn parse_communication_section(&mut self) -> Result<Vec<CommunicationDescription>, ()> {
        let mut entries = Vec::new();

        while self.check_identifier("CD") && !self.at_division_header() && !self.at_eof() {
            entries.push(self.parse_communication_description()?);
        }

        Ok(entries)
    }

    fn parse_communication_description(&mut self) -> Result<CommunicationDescription, ()> {
        let start_span = self.span();
        self.eat_identifier("CD");
        let name = self.expect_identifier()?;
        self.eat(TokenKind::ForKw);
        let direction = if self.check(TokenKind::Initial) {
            self.advance();
            self.expect(TokenKind::Input)?;
            CommunicationDirection::InitialInput
        } else if self.check(TokenKind::Input) {
            self.advance();
            CommunicationDirection::Input
        } else if self.check(TokenKind::Output) {
            self.advance();
            CommunicationDirection::Output
        } else if self.check(TokenKind::IoMode) {
            self.advance();
            CommunicationDirection::InputOutput
        } else {
            self.error("expected communication direction");
            return Err(());
        };

        let mut symbolic_queue = None;
        let mut symbolic_sub_queue_1 = None;
        let mut symbolic_sub_queue_2 = None;
        let mut symbolic_sub_queue_3 = None;
        let mut message_date = None;
        let mut message_time = None;
        let mut symbolic_source = None;
        let mut text_length = None;
        let mut end_key = None;
        let mut status_key = None;
        let mut message_count = None;
        let mut destination_count = None;
        let mut destination_table_count = None;
        let mut destination_table_indexed_by = Vec::new();
        let mut error_key = None;
        let mut destination = None;
        let mut positional_fields = Vec::new();

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Symbolic) {
                self.advance();
                if self.check(TokenKind::Queue) {
                    self.advance();
                    self.eat_is();
                    symbolic_queue = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::SubQueue1) {
                    self.advance();
                    self.eat_is();
                    symbolic_sub_queue_1 = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::SubQueue2) {
                    self.advance();
                    self.eat_is();
                    symbolic_sub_queue_2 = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::SubQueue3) {
                    self.advance();
                    self.eat_is();
                    symbolic_sub_queue_3 = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::SourceField) {
                    self.advance();
                    self.eat_is();
                    symbolic_source = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::Destination) {
                    self.advance();
                    self.eat_is();
                    destination = Some(self.expect_identifier()?);
                } else {
                    self.advance();
                }
            } else if self.check(TokenKind::Message) {
                self.advance();
                if self.check(TokenKind::DateKw) {
                    self.advance();
                    self.eat_is();
                    message_date = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::TimeKw) {
                    self.advance();
                    self.eat_is();
                    message_time = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::Count) {
                    self.advance();
                    self.eat_is();
                    message_count = Some(self.expect_identifier()?);
                } else {
                    self.advance();
                }
            } else if self.check(TokenKind::Text) {
                self.advance();
                self.eat(TokenKind::Length);
                self.eat_is();
                text_length = Some(self.expect_identifier()?);
            } else if self.check_identifier("TEXT") {
                self.advance();
                self.eat(TokenKind::Length);
                self.eat_identifier("LENGTH");
                self.eat_is();
                text_length = Some(self.expect_identifier()?);
            } else if self.check(TokenKind::EndKey) {
                self.advance();
                self.eat_is();
                end_key = Some(self.expect_identifier()?);
            } else if self.check_identifier("END") {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_identifier("KEY");
                self.eat_is();
                end_key = Some(self.expect_identifier()?);
            } else if self.check(TokenKind::StatusKey) {
                self.advance();
                self.eat_is();
                status_key = Some(self.expect_identifier()?);
            } else if self.check_identifier("STATUS") {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_identifier("KEY");
                self.eat_is();
                status_key = Some(self.expect_identifier()?);
            } else if self.check(TokenKind::Destination) {
                self.advance();
                if self.check(TokenKind::Count) {
                    self.advance();
                    self.eat_is();
                    destination_count = Some(self.expect_identifier()?);
                } else if self.check(TokenKind::Table) {
                    self.advance();
                    self.expect(TokenKind::Occurs)?;
                    destination_table_count = Some(self.parse_integer()?);
                    self.eat(TokenKind::Times);
                    if self.check(TokenKind::Index) || self.check_identifier("INDEXED") {
                        self.eat(TokenKind::Index);
                        self.eat_identifier("BY");
                        destination_table_indexed_by.push(self.expect_identifier()?);
                    } else if self.check_identifier("INDEXED") {
                        self.advance();
                        self.eat_identifier("BY");
                        destination_table_indexed_by.push(self.expect_identifier()?);
                    }
                } else {
                    destination = Some(self.expect_identifier()?);
                }
            } else if self.check(TokenKind::ErrorKey) {
                self.advance();
                self.eat_is();
                error_key = Some(self.expect_identifier()?);
            } else if self.check_identifier("ERROR") {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_identifier("KEY");
                self.eat_is();
                error_key = Some(self.expect_identifier()?);
            } else if self.check(TokenKind::Identifier) {
                positional_fields.push(self.advance().text);
            } else {
                self.advance();
            }
        }

        apply_positional_communication_fields(
            direction,
            &positional_fields,
            &mut symbolic_queue,
            &mut symbolic_sub_queue_1,
            &mut symbolic_sub_queue_2,
            &mut symbolic_sub_queue_3,
            &mut message_date,
            &mut message_time,
            &mut symbolic_source,
            &mut text_length,
            &mut end_key,
            &mut status_key,
            &mut message_count,
        );

        self.expect(TokenKind::Period)?;
        let mut data_items = self.parse_data_items()?;
        let synthetic_items = build_communication_data_items(
            symbolic_queue.as_ref(),
            symbolic_sub_queue_1.as_ref(),
            symbolic_sub_queue_2.as_ref(),
            symbolic_sub_queue_3.as_ref(),
            message_date.as_ref(),
            message_time.as_ref(),
            symbolic_source.as_ref(),
            text_length.as_ref(),
            end_key.as_ref(),
            status_key.as_ref(),
            message_count.as_ref(),
            destination_count.as_ref(),
            error_key.as_ref(),
            destination.as_ref(),
            start_span,
        );
        data_items.splice(0..0, synthetic_items);
        let end_span = self.span();

        Ok(CommunicationDescription {
            name,
            direction,
            symbolic_queue,
            symbolic_sub_queue_1,
            symbolic_sub_queue_2,
            symbolic_sub_queue_3,
            message_date,
            message_time,
            symbolic_source,
            text_length,
            end_key,
            status_key,
            message_count,
            destination_count,
            destination_table_count,
            destination_table_indexed_by,
            error_key,
            destination,
            data_items,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_file_description(&mut self) -> Result<FileDescription, ()> {
        let start_span = self.span();

        let fd_or_sd = if self.check(TokenKind::Fd) {
            self.advance();
            FdType::Fd
        } else {
            self.expect(TokenKind::Sd)?;
            FdType::Sd
        };

        let file_name = self.expect_identifier()?;

        let mut block_contains = None;
        let mut record_contains = None;
        let mut record_varying = None;
        let mut label_records = None;
        let data_records = Vec::new();
        let mut recording_mode = None;
        let mut linage = None;

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Block) {
                self.advance();
                self.eat(TokenKind::Contains);
                let first = self.parse_integer()?;
                // BLOCK CONTAINS n TO m RECORDS/CHARACTERS
                let (min, max) = if self.check(TokenKind::To) {
                    self.advance();
                    let second = self.parse_integer()?;
                    (Some(first), second)
                } else {
                    (None, first)
                };
                let unit = if self.check(TokenKind::Records) {
                    self.advance();
                    BlockUnit::Records
                } else {
                    self.eat(TokenKind::Characters);
                    BlockUnit::Characters
                };
                block_contains = Some(BlockContains { min, max, unit });
            } else if self.check(TokenKind::Record) {
                self.advance();
                if self.check(TokenKind::Contains) {
                    // RECORD CONTAINS n [TO m] CHARACTERS
                    self.advance();
                    let first = self.parse_integer()?;
                    let (min, max) = if self.check(TokenKind::To) {
                        self.advance();
                        let second = self.parse_integer()?;
                        (Some(first), second)
                    } else {
                        (None, first)
                    };
                    self.eat(TokenKind::Characters);
                    record_contains = Some(RecordContains { min, max });
                } else {
                    // RECORD [IS] VARYING [IN SIZE] [FROM n] [TO m]
                    //   [CHARACTERS] [DEPENDING [ON] data-name]
                    // Also: RECORD VARYING n TO m [DEPENDING [ON] data-name]
                    self.eat_is();
                    self.eat(TokenKind::Varying);
                    self.eat_identifier("IN");
                    self.eat(TokenKind::SizeKw);
                    let min = if self.check(TokenKind::From) {
                        self.advance();
                        Some(self.parse_integer()?)
                    } else if self.check(TokenKind::IntegerLiteral) {
                        Some(self.parse_integer()?)
                    } else {
                        None
                    };
                    let max = if self.check(TokenKind::To) {
                        self.advance();
                        Some(self.parse_integer()?)
                    } else {
                        None
                    };
                    self.eat(TokenKind::Characters);
                    let depending_on = if self.check(TokenKind::Depending) {
                        self.advance();
                        self.eat(TokenKind::OnKw);
                        Some(self.expect_identifier()?)
                    } else {
                        None
                    };
                    record_varying = Some(RecordVarying {
                        min,
                        max,
                        depending_on,
                    });
                }
            } else if self.check(TokenKind::Label) {
                self.advance();
                self.eat(TokenKind::Records);
                self.eat(TokenKind::Record);
                self.eat_is();
                if self.check_identifier("STANDARD") {
                    self.advance();
                    label_records = Some(LabelRecords::Standard);
                } else if self.check(TokenKind::Omitted) {
                    self.advance();
                    label_records = Some(LabelRecords::Omitted);
                }
            } else if self.check(TokenKind::Recording) {
                self.advance();
                self.eat(TokenKind::Mode);
                self.eat_is();
                let mode = self.expect_identifier()?;
                recording_mode = Some(mode);
            } else if self.check(TokenKind::Linage) {
                // LINAGE IS n LINES [WITH FOOTING AT n]
                //   [LINES AT TOP n] [LINES AT BOTTOM n]
                self.advance();
                self.eat_is();
                let lines = self.parse_linage_value()?;
                self.eat(TokenKind::Lines);
                let mut footing_val = None;
                let mut top_val = None;
                let mut bottom_val = None;
                // Parse optional sub-clauses
                loop {
                    if self.check(TokenKind::With) || self.check(TokenKind::Footing) {
                        // WITH FOOTING AT n
                        self.eat(TokenKind::With);
                        self.eat(TokenKind::Footing);
                        self.eat(TokenKind::At);
                        footing_val = Some(self.parse_linage_value()?);
                    } else if self.check(TokenKind::Lines) {
                        self.advance();
                        self.eat(TokenKind::At);
                        if self.check(TokenKind::Top) {
                            self.advance();
                            top_val = Some(self.parse_linage_value()?);
                        } else if self.check(TokenKind::Bottom) {
                            self.advance();
                            bottom_val = Some(self.parse_linage_value()?);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                linage = Some(LinageClause {
                    lines,
                    footing: footing_val,
                    top: top_val,
                    bottom: bottom_val,
                });
            } else if self.check(TokenKind::Data) {
                // DATA RECORD IS / DATA RECORDS ARE — obsolete clause, skip
                self.advance();
                self.eat(TokenKind::Record);
                self.eat(TokenKind::Records);
                self.eat_is();
                // ARE is not a keyword, skip it if present as identifier
                if self.check_identifier("ARE") {
                    self.advance();
                }
                while self.check(TokenKind::Identifier) {
                    self.advance();
                    // skip comma separators between record names
                    self.eat(TokenKind::Comma);
                }
            } else if self.check(TokenKind::Value) {
                // VALUE OF clause — obsolete, skip until next keyword or period
                self.advance();
                self.eat(TokenKind::Of);
                while !self.check(TokenKind::Period)
                    && !self.check(TokenKind::Data)
                    && !self.check(TokenKind::Block)
                    && !self.check(TokenKind::Record)
                    && !self.check(TokenKind::Label)
                    && !self.check(TokenKind::Recording)
                    && !self.check(TokenKind::Linage)
                    && !self.at_eof()
                {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        self.expect(TokenKind::Period)?;

        let items = self.parse_data_items()?;

        let end_span = self.span();

        Ok(FileDescription {
            fd_or_sd,
            file_name,
            block_contains,
            record_contains,
            record_varying,
            label_records,
            data_records,
            recording_mode,
            linage,
            items,
            span: start_span.merge(&end_span),
        })
    }

    /// Parse a sequence of data item entries.
    pub(crate) fn parse_data_items(&mut self) -> Result<Vec<DataItem>, ()> {
        let mut items = Vec::new();

        while self.check(TokenKind::LevelNumber) && !self.at_division_header() && !self.at_eof() {
            if self.at_data_section_header() {
                break;
            }
            if self.check(TokenKind::Fd) || self.check(TokenKind::Sd) {
                break;
            }

            let item = self.parse_data_item()?;
            items.push(item);
        }

        Ok(self.build_data_hierarchy(items))
    }

    fn parse_data_item(&mut self) -> Result<DataItem, ()> {
        let start_span = self.span();

        let level_tok = self.expect(TokenKind::LevelNumber)?;
        let level: u8 = level_tok.text.parse().unwrap_or(1);

        let name = if self.check(TokenKind::Filler) {
            self.advance();
            None
        } else if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            if self.check(TokenKind::Period) {
                None
            } else {
                Some(self.advance().text)
            }
        } else {
            None
        };

        let mut picture = None;
        let mut usage = None;
        let mut value = None;
        let mut occurs = None;
        let mut redefines = None;
        let mut renames = None;
        let mut sign_clause = None;
        let mut justified = false;
        let mut blank_when_zero = false;
        let mut is_external = false;
        let mut is_global = false;
        let mut condition_values = Vec::new();
        let mut line_clause = None;
        let mut column_clause = None;
        let mut blank_screen = false;
        let mut blank_line = false;
        let mut highlight = false;
        let mut reverse_video = false;
        let mut source_field = None;
        let mut using_field = None;

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Pic) {
                self.advance();
                picture = Some(self.parse_picture_clause()?);
            } else if self.check(TokenKind::Usage) {
                self.advance();
                self.eat_is();
                usage = Some(self.parse_usage()?);
            } else if self.is_usage_keyword() {
                usage = Some(self.parse_usage()?);
            } else if self.check(TokenKind::Value) || self.check(TokenKind::Values) {
                self.advance();
                self.eat_is();
                if level == 88 {
                    condition_values.push(self.parse_condition_value()?);
                } else {
                    value = Some(self.parse_value_clause()?);
                }
            } else if self.check(TokenKind::Occurs) {
                self.advance();
                occurs = Some(self.parse_occurs_clause()?);
            } else if self.check(TokenKind::Redefines) {
                self.advance();
                redefines = Some(self.expect_identifier()?);
            } else if self.check(TokenKind::Renames) {
                self.advance();
                renames = Some(self.parse_renames_clause()?);
            } else if self.check(TokenKind::SignKw) {
                self.advance();
                sign_clause = Some(self.parse_sign_clause()?);
            } else if self.check(TokenKind::Justified) {
                self.advance();
                self.eat_identifier("RIGHT");
                justified = true;
            } else if self.check(TokenKind::Blank) {
                self.advance();
                if self.check(TokenKind::Screen) {
                    self.advance();
                    blank_screen = true;
                } else if self.check(TokenKind::Line) {
                    self.advance();
                    blank_line = true;
                } else {
                    self.eat(TokenKind::When);
                    self.eat(TokenKind::Zero);
                    blank_when_zero = true;
                }
            } else if self.check(TokenKind::Line) {
                self.advance();
                self.eat_identifier("NUMBER");
                self.eat_is();
                if self.check(TokenKind::IntegerLiteral) {
                    line_clause = Some(self.parse_integer()?);
                }
            } else if self.check(TokenKind::Column) {
                self.advance();
                self.eat_identifier("NUMBER");
                self.eat_is();
                if self.check(TokenKind::IntegerLiteral) {
                    column_clause = Some(self.parse_integer()?);
                }
            } else if self.check(TokenKind::Highlight) {
                self.advance();
                highlight = true;
            } else if self.check(TokenKind::ReverseVideo) {
                self.advance();
                reverse_video = true;
            } else if self.check(TokenKind::SourceField) {
                self.advance();
                self.eat_is();
                source_field = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Using) && usage.is_none() {
                // USING in screen section context (not USAGE)
                // Only treat as screen USING if it looks like a qualified name follows
                let next = self.peek(1).kind;
                if next == TokenKind::Identifier || next.is_keyword() {
                    self.advance();
                    self.eat_is();
                    using_field = Some(self.parse_qualified_name()?);
                } else {
                    self.advance();
                }
            } else if self.check(TokenKind::External) {
                self.advance();
                is_external = true;
            } else if self.check(TokenKind::Global) {
                self.advance();
                is_global = true;
            } else {
                self.advance();
            }
        }

        self.expect(TokenKind::Period)?;

        let end_span = self.span();

        Ok(DataItem {
            level,
            name,
            picture,
            usage,
            value,
            occurs,
            redefines,
            renames,
            sign_clause,
            justified,
            blank_when_zero,
            is_external,
            is_global,
            condition_values,
            line_clause,
            column_clause,
            blank_screen,
            blank_line,
            highlight,
            reverse_video,
            source_field,
            using_field,
            children: Vec::new(),
            span: start_span.merge(&end_span),
        })
    }

    fn parse_picture_clause(&mut self) -> Result<PictureClause, ()> {
        let start_span = self.span();

        if self.check(TokenKind::PictureString) {
            let tok = self.advance();
            let raw = tok.text.clone();
            let pic = analyze_picture(&raw);
            let end_span = self.span();
            Ok(PictureClause {
                raw_string: raw,
                category: pic.category,
                size: pic.size,
                decimal_positions: pic.decimal_positions,
                is_signed: pic.is_signed,
                is_edited: pic.is_edited,
                span: start_span.merge(&end_span),
            })
        } else {
            self.error("expected PICTURE string");
            Err(())
        }
    }

    fn parse_usage(&mut self) -> Result<Usage, ()> {
        match self.current().kind {
            TokenKind::Display => {
                self.advance();
                Ok(Usage::Display)
            }
            TokenKind::Computational => {
                self.advance();
                Ok(Usage::Computational)
            }
            TokenKind::Comp => {
                self.advance();
                Ok(Usage::Comp)
            }
            TokenKind::Comp1 => {
                self.advance();
                Ok(Usage::Comp1)
            }
            TokenKind::Comp2 => {
                self.advance();
                Ok(Usage::Comp2)
            }
            TokenKind::Comp3 => {
                self.advance();
                Ok(Usage::Comp3)
            }
            TokenKind::Comp4 => {
                self.advance();
                Ok(Usage::Comp4)
            }
            TokenKind::Comp5 => {
                self.advance();
                Ok(Usage::Comp5)
            }
            TokenKind::Binary => {
                self.advance();
                Ok(Usage::Binary)
            }
            TokenKind::PackedDecimal => {
                self.advance();
                Ok(Usage::PackedDecimal)
            }
            TokenKind::Index => {
                self.advance();
                Ok(Usage::Index)
            }
            TokenKind::Pointer => {
                self.advance();
                Ok(Usage::Pointer)
            }
            TokenKind::FunctionPointer => {
                self.advance();
                Ok(Usage::FunctionPointer)
            }
            TokenKind::National => {
                self.advance();
                Ok(Usage::National)
            }
            TokenKind::FloatShort => {
                self.advance();
                Ok(Usage::FloatShort)
            }
            TokenKind::FloatLong => {
                self.advance();
                Ok(Usage::FloatLong)
            }
            TokenKind::FloatExtended => {
                self.advance();
                Ok(Usage::FloatExtended)
            }
            _ => {
                self.error("expected USAGE type");
                Err(())
            }
        }
    }

    fn is_usage_keyword(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Computational
                | TokenKind::Comp
                | TokenKind::Comp1
                | TokenKind::Comp2
                | TokenKind::Comp3
                | TokenKind::Comp4
                | TokenKind::Comp5
                | TokenKind::Binary
                | TokenKind::PackedDecimal
                | TokenKind::Index
                | TokenKind::Pointer
                | TokenKind::FunctionPointer
        )
    }

    fn parse_value_clause(&mut self) -> Result<ValueClause, ()> {
        let start_span = self.span();
        if let Some(lit) = self.try_parse_literal() {
            let end_span = self.span();
            Ok(ValueClause {
                value: lit,
                span: start_span.merge(&end_span),
            })
        } else {
            self.error("expected literal value");
            Err(())
        }
    }

    fn parse_condition_value(&mut self) -> Result<ConditionValue, ()> {
        let start_span = self.span();
        let mut values = Vec::new();

        while let Some(lit) = self.try_parse_literal() {
            if self.check(TokenKind::Thru) {
                self.advance();
                if let Some(to_lit) = self.try_parse_literal() {
                    values.push(ConditionValueItem::Range {
                        from: lit,
                        to: to_lit,
                    });
                } else {
                    self.error("expected literal after THRU");
                    return Err(());
                }
            } else {
                values.push(ConditionValueItem::Single(lit));
            }

            if self.check(TokenKind::Period) || self.at_eof() {
                break;
            }
        }

        let end_span = self.span();
        Ok(ConditionValue {
            values,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_occurs_clause(&mut self) -> Result<OccursClause, ()> {
        let start_span = self.span();

        let first_value = self.parse_integer()?;

        // Check for OCCURS n TO m TIMES DEPENDING ON
        let (min, max) = if self.check(TokenKind::To) {
            self.advance();
            let max_val = self.parse_integer()?;
            (Some(first_value), max_val)
        } else {
            (None, first_value)
        };

        self.eat(TokenKind::Times);

        let mut depending_on = None;
        let mut ascending_keys = Vec::new();
        let mut descending_keys = Vec::new();
        let mut indexed_by = Vec::new();

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Depending) {
                self.advance();
                self.eat(TokenKind::OnKw);
                depending_on = Some(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Ascending) {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_is();
                loop {
                    ascending_keys.push(self.parse_qualified_name()?);
                    if !self.check(TokenKind::Identifier) {
                        break;
                    }
                }
            } else if self.check(TokenKind::Descending) {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_is();
                loop {
                    descending_keys.push(self.parse_qualified_name()?);
                    if !self.check(TokenKind::Identifier) {
                        break;
                    }
                }
            } else if self.check(TokenKind::Indexed) {
                self.advance();
                self.eat(TokenKind::By);
                loop {
                    let idx = self.expect_identifier()?;
                    indexed_by.push(idx);
                    let _ = self.eat(TokenKind::Comma);
                    if !self.check(TokenKind::Identifier) {
                        break;
                    }
                }
            } else {
                break;
            }
        }

        let end_span = self.span();

        Ok(OccursClause {
            min,
            max,
            depending_on,
            ascending_keys,
            descending_keys,
            indexed_by,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_renames_clause(&mut self) -> Result<RenamesClause, ()> {
        let start_span = self.span();

        let from = self.parse_qualified_name()?;
        let thru = if self.check(TokenKind::Thru) {
            self.advance();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let end_span = self.span();

        Ok(RenamesClause {
            from,
            thru,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_sign_clause(&mut self) -> Result<SignClause, ()> {
        self.eat_is();

        let position = if self.check(TokenKind::Leading) {
            self.advance();
            SignPosition::Leading
        } else if self.check(TokenKind::Trailing) {
            self.advance();
            SignPosition::Trailing
        } else {
            self.error("expected LEADING or TRAILING");
            return Err(());
        };

        let separate = if self.check(TokenKind::Separate) {
            self.advance();
            self.eat_identifier("CHARACTER");
            true
        } else {
            false
        };

        Ok(SignClause { position, separate })
    }

    /// Parse an integer literal, returning its value.
    pub(crate) fn parse_integer(&mut self) -> Result<u32, ()> {
        if self.check(TokenKind::IntegerLiteral) {
            let tok = self.advance();
            tok.text.parse().map_err(|_| {
                self.error("invalid integer");
            })
        } else {
            self.error("expected integer");
            Err(())
        }
    }

    /// Parse a LINAGE value: either an integer literal or a data-name.
    fn parse_linage_value(&mut self) -> Result<LinageValue, ()> {
        if self.check(TokenKind::IntegerLiteral) {
            let v = self.parse_integer()?;
            Ok(LinageValue::Integer(v))
        } else if self.check(TokenKind::Identifier) {
            let name = self.expect_identifier()?;
            Ok(LinageValue::DataName(name))
        } else {
            self.error("expected integer or data-name for LINAGE value");
            Err(())
        }
    }

    /// Eat an identifier if it matches (case-insensitive).
    pub(crate) fn eat_identifier(&mut self, name: &str) -> bool {
        if self.check_identifier(name) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Build the hierarchical structure of data items based on level numbers.
    fn build_data_hierarchy(&self, flat_items: Vec<DataItem>) -> Vec<DataItem> {
        if flat_items.is_empty() {
            return Vec::new();
        }

        let mut result: Vec<DataItem> = Vec::new();
        let mut stack: Vec<DataItem> = Vec::new();

        for item in flat_items {
            let level = item.level;

            if level == 77 {
                while let Some(completed) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(completed);
                    } else {
                        result.push(completed);
                    }
                }
                result.push(item);
                continue;
            }

            if level == 88 || level == 66 {
                // Level 88 (condition names) and level 66 (RENAMES) attach
                // to the nearest enclosing record (01-level).
                if level == 66 {
                    // Flush the stack back to the 01-level parent, then
                    // attach the level 66 item there.
                    while stack.len() > 1 {
                        let completed = stack.pop().unwrap();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(completed);
                        }
                    }
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(item);
                    } else if let Some(parent) = result.last_mut() {
                        parent.children.push(item);
                    }
                } else {
                    // Level 88 — attach to the immediately preceding item.
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(item);
                    } else if let Some(parent) = result.last_mut() {
                        parent.children.push(item);
                    }
                }
                continue;
            }

            while let Some(top) = stack.last() {
                if top.level >= level {
                    let completed = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(completed);
                    } else {
                        result.push(completed);
                    }
                } else {
                    break;
                }
            }

            stack.push(item);
        }

        while let Some(completed) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(completed);
            } else {
                result.push(completed);
            }
        }

        result
    }

    fn skip_section_content(&mut self) {
        while !self.at_eof() {
            if self.at_division_header() || self.at_data_section_header() {
                break;
            }
            self.advance();
        }
    }
}

fn take_positional_field(fields: &[SmolStr], index: usize) -> Option<SmolStr> {
    let value = fields.get(index)?.clone();
    if value.eq_ignore_ascii_case("FILLER") {
        None
    } else {
        Some(value)
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_positional_communication_fields(
    direction: CommunicationDirection,
    fields: &[SmolStr],
    symbolic_queue: &mut Option<SmolStr>,
    symbolic_sub_queue_1: &mut Option<SmolStr>,
    symbolic_sub_queue_2: &mut Option<SmolStr>,
    symbolic_sub_queue_3: &mut Option<SmolStr>,
    message_date: &mut Option<SmolStr>,
    message_time: &mut Option<SmolStr>,
    symbolic_source: &mut Option<SmolStr>,
    text_length: &mut Option<SmolStr>,
    end_key: &mut Option<SmolStr>,
    status_key: &mut Option<SmolStr>,
    message_count: &mut Option<SmolStr>,
) {
    match direction {
        CommunicationDirection::Input | CommunicationDirection::InitialInput => {
            if symbolic_queue.is_none() {
                *symbolic_queue = take_positional_field(fields, 0);
            }
            if symbolic_sub_queue_1.is_none() {
                *symbolic_sub_queue_1 = take_positional_field(fields, 1);
            }
            if symbolic_sub_queue_2.is_none() {
                *symbolic_sub_queue_2 = take_positional_field(fields, 2);
            }
            if symbolic_sub_queue_3.is_none() {
                *symbolic_sub_queue_3 = take_positional_field(fields, 3);
            }
            if message_date.is_none() {
                *message_date = take_positional_field(fields, 4);
            }
            if message_time.is_none() {
                *message_time = take_positional_field(fields, 5);
            }
            if symbolic_source.is_none() {
                *symbolic_source = take_positional_field(fields, 6);
            }
            if text_length.is_none() {
                *text_length = take_positional_field(fields, 7);
            }
            if end_key.is_none() {
                *end_key = take_positional_field(fields, 8);
            }
            if status_key.is_none() {
                *status_key = take_positional_field(fields, 9);
            }
            if message_count.is_none() {
                *message_count = take_positional_field(fields, 10);
            }
        }
        CommunicationDirection::Output | CommunicationDirection::InputOutput => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn build_communication_data_items(
    symbolic_queue: Option<&SmolStr>,
    symbolic_sub_queue_1: Option<&SmolStr>,
    symbolic_sub_queue_2: Option<&SmolStr>,
    symbolic_sub_queue_3: Option<&SmolStr>,
    message_date: Option<&SmolStr>,
    message_time: Option<&SmolStr>,
    symbolic_source: Option<&SmolStr>,
    text_length: Option<&SmolStr>,
    end_key: Option<&SmolStr>,
    status_key: Option<&SmolStr>,
    message_count: Option<&SmolStr>,
    destination_count: Option<&SmolStr>,
    error_key: Option<&SmolStr>,
    destination: Option<&SmolStr>,
    span: cobol_common::Span,
) -> Vec<DataItem> {
    let mut items = Vec::new();

    for name in [
        symbolic_queue,
        symbolic_sub_queue_1,
        symbolic_sub_queue_2,
        symbolic_sub_queue_3,
        symbolic_source,
        destination,
    ]
    .into_iter()
    .flatten()
    {
        items.push(make_alpha_item(name.clone(), 12, span));
    }

    if let Some(name) = message_date {
        items.push(make_numeric_item(name.clone(), "9(8)", 8, span));
    }
    if let Some(name) = message_time {
        items.push(make_time_group_item(name.clone(), span));
    }
    if let Some(name) = text_length {
        items.push(make_numeric_item(name.clone(), "9(4)", 4, span));
    }
    if let Some(name) = end_key {
        items.push(make_alpha_item(name.clone(), 1, span));
    }
    if let Some(name) = status_key {
        items.push(make_alpha_item(name.clone(), 2, span));
    }
    if let Some(name) = message_count {
        items.push(make_numeric_item(name.clone(), "9(6)", 6, span));
    }
    if let Some(name) = destination_count {
        items.push(make_numeric_item(name.clone(), "9(4)", 4, span));
    }
    if let Some(name) = error_key {
        items.push(make_alpha_item(name.clone(), 1, span));
    }

    items
}

fn make_picture(raw: &str, category: PictureCategory, size: u32, span: cobol_common::Span) -> PictureClause {
    PictureClause {
        raw_string: raw.into(),
        category,
        size,
        decimal_positions: 0,
        is_signed: false,
        is_edited: false,
        span,
    }
}

fn make_alpha_item(name: SmolStr, size: u32, span: cobol_common::Span) -> DataItem {
    make_item_with_picture(77, name, make_picture(&format!("X({size})"), PictureCategory::Alphanumeric, size, span), span)
}

fn make_numeric_item(name: SmolStr, raw: &str, size: u32, span: cobol_common::Span) -> DataItem {
    make_item_with_picture(77, name, make_picture(raw, PictureCategory::Numeric, size, span), span)
}

fn make_group_numeric_item(level: u8, name: SmolStr, raw: &str, size: u32, span: cobol_common::Span) -> DataItem {
    make_item_with_picture(level, name, make_picture(raw, PictureCategory::Numeric, size, span), span)
}

fn make_item_with_picture(level: u8, name: SmolStr, picture: PictureClause, span: cobol_common::Span) -> DataItem {
    DataItem {
        level,
        name: Some(name),
        picture: Some(picture),
        usage: None,
        value: None,
        occurs: None,
        redefines: None,
        renames: None,
        sign_clause: None,
        justified: false,
        blank_when_zero: false,
        is_external: false,
        is_global: false,
        condition_values: Vec::new(),
        line_clause: None,
        column_clause: None,
        blank_screen: false,
        blank_line: false,
        highlight: false,
        reverse_video: false,
        source_field: None,
        using_field: None,
        children: Vec::new(),
        span,
    }
}

fn make_time_group_item(name: SmolStr, span: cobol_common::Span) -> DataItem {
    DataItem {
        level: 1,
        name: Some(name),
        picture: None,
        usage: None,
        value: None,
        occurs: None,
        redefines: None,
        renames: None,
        sign_clause: None,
        justified: false,
        blank_when_zero: false,
        is_external: false,
        is_global: false,
        condition_values: Vec::new(),
        line_clause: None,
        column_clause: None,
        blank_screen: false,
        blank_line: false,
        highlight: false,
        reverse_video: false,
        source_field: None,
        using_field: None,
        children: vec![
            make_group_numeric_item(2, "HRS".into(), "99", 2, span),
            make_group_numeric_item(2, "MINS".into(), "99", 2, span),
            DataItem {
                level: 2,
                name: Some("SECS".into()),
                picture: Some(PictureClause {
                    raw_string: "99V99".into(),
                    category: PictureCategory::Numeric,
                    size: 4,
                    decimal_positions: 2,
                    is_signed: false,
                    is_edited: false,
                    span,
                }),
                usage: None,
                value: None,
                occurs: None,
                redefines: None,
                renames: None,
                sign_clause: None,
                justified: false,
                blank_when_zero: false,
                is_external: false,
                is_global: false,
                condition_values: Vec::new(),
                line_clause: None,
                column_clause: None,
                blank_screen: false,
                blank_line: false,
                highlight: false,
                reverse_video: false,
                source_field: None,
                using_field: None,
                children: Vec::new(),
                span,
            },
        ],
        span,
    }
}

struct PictureAnalysis {
    category: PictureCategory,
    size: u32,
    decimal_positions: u32,
    is_signed: bool,
    is_edited: bool,
}

fn analyze_picture(raw: &str) -> PictureAnalysis {
    let upper = raw.to_uppercase();
    let mut size: u32 = 0;
    let mut decimal_positions: u32 = 0;
    let mut is_signed = false;
    let mut is_edited = false;
    let mut has_nine = false;
    let mut has_x = false;
    let mut has_a = false;
    let mut has_n = false;
    let mut after_v = false;

    let bytes = upper.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        match ch {
            b'S' => {
                is_signed = true;
            }
            b'9' => {
                has_nine = true;
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
                if after_v {
                    decimal_positions += count;
                }
            }
            b'X' => {
                has_x = true;
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
            }
            b'A' => {
                has_a = true;
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
            }
            b'V' => {
                after_v = true;
            }
            b'P' => {
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
                if after_v {
                    decimal_positions += count;
                }
            }
            b'N' => {
                has_n = true;
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
            }
            b'Z' | b'*' | b'+' | b'-' | b'$' | b'B' | b'0' | b'/' | b',' | b'.' => {
                is_edited = true;
                let count = parse_repeat_count(bytes, &mut i);
                size += count;
            }
            _ => {}
        }
        i += 1;
    }

    let category = if has_n {
        if is_edited {
            PictureCategory::NationalEdited
        } else {
            PictureCategory::National
        }
    } else if has_x {
        if is_edited {
            PictureCategory::AlphanumericEdited
        } else {
            PictureCategory::Alphanumeric
        }
    } else if has_a && !has_nine {
        PictureCategory::Alphabetic
    } else if has_nine {
        if is_edited {
            PictureCategory::NumericEdited
        } else {
            PictureCategory::Numeric
        }
    } else if is_edited {
        PictureCategory::NumericEdited
    } else {
        PictureCategory::Alphanumeric
    };

    PictureAnalysis {
        category,
        size,
        decimal_positions,
        is_signed,
        is_edited,
    }
}

fn parse_repeat_count(bytes: &[u8], i: &mut usize) -> u32 {
    if *i + 1 < bytes.len() && bytes[*i + 1] == b'(' {
        let start = *i + 2;
        let mut end = start;
        while end < bytes.len() && bytes[end] != b')' {
            end += 1;
        }
        if end < bytes.len() {
            let num_str = std::str::from_utf8(&bytes[start..end]).unwrap_or("1");
            let count = num_str.parse().unwrap_or(1);
            *i = end;
            count
        } else {
            1
        }
    } else {
        1
    }
}
