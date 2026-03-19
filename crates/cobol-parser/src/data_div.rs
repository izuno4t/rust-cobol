// COBOL Parser - DATA DIVISION parsing

use cobol_ast::data_div::*;
use cobol_ast::picture::{PictureCategory, PictureClause};
use cobol_lexer::token::TokenKind;

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
        let communication = Vec::new();
        let report = Vec::new();

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
            } else if self.check(TokenKind::Communication) || self.check(TokenKind::Report) {
                self.advance();
                self.expect(TokenKind::Section)?;
                self.expect(TokenKind::Period)?;
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
        let mut label_records = None;
        let data_records = Vec::new();
        let mut recording_mode = None;

        while !self.check(TokenKind::Period) && !self.at_eof() {
            if self.check(TokenKind::Block) {
                self.advance();
                self.eat(TokenKind::Contains);
                let size = self.parse_integer()?;
                let unit = if self.check(TokenKind::Records) {
                    self.advance();
                    BlockUnit::Records
                } else {
                    self.eat(TokenKind::Characters);
                    BlockUnit::Characters
                };
                block_contains = Some(BlockContains {
                    min: None,
                    max: size,
                    unit,
                });
            } else if self.check(TokenKind::Record) {
                self.advance();
                self.eat(TokenKind::Contains);
                let size = self.parse_integer()?;
                self.eat(TokenKind::Characters);
                record_contains = Some(RecordContains {
                    min: None,
                    max: size,
                });
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
            label_records,
            data_records,
            recording_mode,
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
                ascending_keys.push(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Descending) {
                self.advance();
                self.eat(TokenKind::Key);
                self.eat_is();
                descending_keys.push(self.parse_qualified_name()?);
            } else if self.check(TokenKind::Indexed) {
                self.advance();
                self.eat(TokenKind::By);
                loop {
                    let idx = self.expect_identifier()?;
                    indexed_by.push(idx);
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

            if level == 88 {
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(item);
                } else if let Some(parent) = result.last_mut() {
                    parent.children.push(item);
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
