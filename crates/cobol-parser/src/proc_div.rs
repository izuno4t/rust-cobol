// COBOL Parser - PROCEDURE DIVISION parsing

use cobol_ast::expr::{Condition, Expr};
use cobol_ast::proc_div::*;
use cobol_ast::statement::*;
use cobol_common::Span;
use cobol_lexer::token::TokenKind;
use smol_str::SmolStr;

use crate::parser::Parser;

impl Parser {
    /// Parse the PROCEDURE DIVISION.
    pub fn parse_procedure_division(&mut self) -> Result<ProcedureDivision, ()> {
        let start_span = self.span();

        self.expect(TokenKind::Procedure)?;
        self.expect(TokenKind::Division)?;

        let mut using_params = Vec::new();
        if self.check(TokenKind::Using) {
            self.advance();
            using_params = self.parse_proc_params()?;
        }

        let mut returning = None;
        if self.check(TokenKind::Returning) {
            self.advance();
            returning = Some(self.expect_identifier()?);
        }

        self.expect(TokenKind::Period)?;

        let declaratives = if self.check_identifier("DECLARATIVES") {
            self.parse_declaratives()?
        } else {
            Vec::new()
        };
        let mut sections = Vec::new();
        let mut paragraphs = Vec::new();

        self.parse_procedure_body(&mut sections, &mut paragraphs)?;

        let end_span = self.span();

        Ok(ProcedureDivision {
            using_params,
            returning,
            declaratives,
            sections,
            paragraphs,
            span: start_span.merge(&end_span),
        })
    }

    fn parse_proc_params(&mut self) -> Result<Vec<ProcParam>, ()> {
        let mut params = Vec::new();
        let mut current_mode = ParamMode::ByReference;

        while !self.check(TokenKind::Period) && !self.check(TokenKind::Returning) && !self.at_eof()
        {
            if self.check(TokenKind::By) {
                self.advance();
                if self.check(TokenKind::Reference) {
                    self.advance();
                    current_mode = ParamMode::ByReference;
                } else if self.check(TokenKind::Content) {
                    self.advance();
                    current_mode = ParamMode::ByContent;
                } else if self.check(TokenKind::Value) {
                    self.advance();
                    current_mode = ParamMode::ByValue;
                }
                continue;
            }

            let span = self.span();
            let name = self.expect_identifier()?;
            params.push(ProcParam {
                mode: current_mode,
                name,
                span,
            });
        }

        Ok(params)
    }

    /// Parse the DECLARATIVES section.
    ///
    /// ```text
    /// DECLARATIVES.
    ///   section-name SECTION.
    ///     USE AFTER EXCEPTION ON file-name-1 ...
    ///   paragraph-name.
    ///     statements ...
    /// END DECLARATIVES.
    /// ```
    fn parse_declaratives(&mut self) -> Result<Vec<DeclarativeSection>, ()> {
        let mut sections = Vec::new();

        // Consume "DECLARATIVES"
        self.advance();
        self.expect(TokenKind::Period)?;

        // Parse sections until END DECLARATIVES
        while !self.at_eof() {
            // Check for END DECLARATIVES
            if self.check_identifier("END")
                && self.peek(1).text.eq_ignore_ascii_case("DECLARATIVES")
            {
                self.advance(); // END
                self.advance(); // DECLARATIVES
                self.expect(TokenKind::Period)?;
                break;
            }

            // Parse section-name SECTION.
            let section_start = self.span();
            let section_name = self.expect_identifier()?;
            self.expect(TokenKind::Section)?;
            self.expect(TokenKind::Period)?;

            // Parse USE statement
            let use_stmt = self.parse_use_statement()?;

            // Parse paragraphs within this declarative section
            let mut paragraphs = Vec::new();
            let mut current_para_name: Option<SmolStr> = None;
            let mut current_para_span: Option<Span> = None;
            let mut current_sentences: Vec<Sentence> = Vec::new();

            while !self.at_eof() {
                // Check for END DECLARATIVES or next section header
                if self.check_identifier("END")
                    && self.peek(1).text.eq_ignore_ascii_case("DECLARATIVES")
                {
                    break;
                }

                // Check for next section (section-name SECTION)
                if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                    && !self.at_statement_start()
                    && self.peek(1).kind == TokenKind::Section
                {
                    break;
                }

                // Check for paragraph header
                if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                    && !self.at_statement_start()
                    && self.peek(1).kind == TokenKind::Period
                {
                    // Flush current paragraph
                    if current_para_name.is_some() || !current_sentences.is_empty() {
                        let para = self.make_paragraph(
                            current_para_name.take(),
                            current_para_span.take(),
                            std::mem::take(&mut current_sentences),
                        );
                        paragraphs.push(para);
                    }
                    current_para_span = Some(self.span());
                    current_para_name = Some(self.advance().text);
                    self.advance(); // period
                    continue;
                }

                // Parse a sentence
                if let Some(sentence) = self.parse_sentence()? {
                    current_sentences.push(sentence);
                }
            }

            // Flush last paragraph
            if current_para_name.is_some() || !current_sentences.is_empty() {
                let para = self.make_paragraph(
                    current_para_name.take(),
                    current_para_span.take(),
                    std::mem::take(&mut current_sentences),
                );
                paragraphs.push(para);
            }

            let section_end = self.span();
            sections.push(DeclarativeSection {
                name: section_name,
                use_statement: use_stmt,
                paragraphs,
                span: section_start.merge(&section_end),
            });
        }

        Ok(sections)
    }

    /// Parse a USE statement in a declarative section.
    fn parse_use_statement(&mut self) -> Result<UseStatement, ()> {
        // Expect "USE"
        if !self.check_identifier("USE") {
            self.error("expected USE statement in declarative section");
            return Err(());
        }
        self.advance(); // USE

        let use_stmt = if self.check(TokenKind::After) || self.check_identifier("GLOBAL") {
            // USE [GLOBAL] AFTER [STANDARD] EXCEPTION/ERROR ON ...
            if self.check_identifier("GLOBAL") {
                self.advance();
            }
            self.expect(TokenKind::After)?;
            // optional STANDARD
            if self.check_identifier("STANDARD") {
                self.advance();
            }
            // EXCEPTION or ERROR (synonyms)
            if self.check(TokenKind::ExceptionKw) || self.check(TokenKind::ErrorKw) {
                self.advance();
            }
            // optional ON
            if self.check(TokenKind::OnKw) {
                self.advance();
            }
            // Parse file names (or INPUT/OUTPUT/I-O/EXTEND)
            let mut file_names = Vec::new();
            while !self.check(TokenKind::Period) && !self.at_eof() {
                let name = self.expect_identifier()?;
                file_names.push(name);
            }
            UseStatement::AfterException { file_names }
        } else if self.check(TokenKind::Before) {
            // USE BEFORE REPORTING report-group
            self.advance(); // BEFORE
            if self.check(TokenKind::Report) {
                self.advance(); // REPORTING
            }
            let report_group = self.expect_identifier()?;
            UseStatement::BeforeReporting { report_group }
        } else if self.check(TokenKind::ForKw) {
            // USE FOR DEBUGGING ON debug-items
            self.advance(); // FOR
            if self.check_identifier("DEBUGGING") {
                self.advance();
            }
            if self.check(TokenKind::OnKw) {
                self.advance();
            }
            let mut debug_items = Vec::new();
            while !self.check(TokenKind::Period) && !self.at_eof() {
                let name = self.expect_identifier()?;
                debug_items.push(name);
            }
            UseStatement::ForDebugging { debug_items }
        } else {
            // Fallback: try AFTER EXCEPTION
            self.error("expected AFTER, BEFORE, or FOR in USE statement");
            return Err(());
        };

        self.expect(TokenKind::Period)?;
        Ok(use_stmt)
    }

    fn parse_procedure_body(
        &mut self,
        sections: &mut Vec<ProcSection>,
        paragraphs: &mut Vec<Paragraph>,
    ) -> Result<(), ()> {
        let mut current_para_name: Option<SmolStr> = None;
        let mut current_para_span: Option<Span> = None;
        let mut current_sentences: Vec<Sentence> = Vec::new();

        while !self.at_eof() {
            // Check for paragraph or section header
            if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_start()
            {
                if self.peek(1).kind == TokenKind::Section {
                    // Section header: flush current paragraph
                    if current_para_name.is_some() || !current_sentences.is_empty() {
                        let para = self.make_paragraph(
                            current_para_name.take(),
                            current_para_span.take(),
                            std::mem::take(&mut current_sentences),
                        );
                        paragraphs.push(para);
                    }

                    let section_name = self.advance().text;
                    self.advance(); // SECTION
                    self.expect(TokenKind::Period)?;

                    let section_start = self.span();
                    let mut section_paragraphs = Vec::new();
                    let mut sec_para_name: Option<SmolStr> = None;
                    let mut sec_para_span: Option<Span> = None;
                    let mut sec_sentences: Vec<Sentence> = Vec::new();

                    while !self.at_eof() {
                        if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                            && !self.at_statement_start()
                            && self.peek(1).kind == TokenKind::Section
                        {
                            break;
                        }

                        if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                            && !self.at_statement_start()
                            && self.peek(1).kind == TokenKind::Period
                        {
                            if sec_para_name.is_some() || !sec_sentences.is_empty() {
                                let para = self.make_paragraph(
                                    sec_para_name.take(),
                                    sec_para_span.take(),
                                    std::mem::take(&mut sec_sentences),
                                );
                                section_paragraphs.push(para);
                            }
                            sec_para_span = Some(self.span());
                            sec_para_name = Some(self.advance().text);
                            self.advance(); // period
                            continue;
                        }

                        if let Some(sentence) = self.parse_sentence()? {
                            sec_sentences.push(sentence);
                        } else {
                            break;
                        }
                    }

                    if sec_para_name.is_some() || !sec_sentences.is_empty() {
                        let para = self.make_paragraph(
                            sec_para_name.take(),
                            sec_para_span.take(),
                            std::mem::take(&mut sec_sentences),
                        );
                        section_paragraphs.push(para);
                    }

                    sections.push(ProcSection {
                        name: section_name,
                        paragraphs: section_paragraphs,
                        span: section_start,
                    });
                    continue;
                } else if self.peek(1).kind == TokenKind::Period {
                    // Paragraph header
                    if current_para_name.is_some() || !current_sentences.is_empty() {
                        let para = self.make_paragraph(
                            current_para_name.take(),
                            current_para_span.take(),
                            std::mem::take(&mut current_sentences),
                        );
                        paragraphs.push(para);
                    }

                    current_para_span = Some(self.span());
                    current_para_name = Some(self.advance().text);
                    self.advance(); // period
                    continue;
                }
            }

            if let Some(sentence) = self.parse_sentence()? {
                current_sentences.push(sentence);
            } else {
                break;
            }
        }

        if current_para_name.is_some() || !current_sentences.is_empty() {
            let para = self.make_paragraph(current_para_name, current_para_span, current_sentences);
            paragraphs.push(para);
        }

        Ok(())
    }

    fn make_paragraph(
        &self,
        name: Option<SmolStr>,
        span: Option<Span>,
        sentences: Vec<Sentence>,
    ) -> Paragraph {
        let name = name.unwrap_or_else(|| SmolStr::from(""));
        let span = span.unwrap_or_else(Span::dummy);
        Paragraph {
            name,
            sentences,
            span,
        }
    }

    fn parse_sentence(&mut self) -> Result<Option<Sentence>, ()> {
        if self.at_eof() {
            return Ok(None);
        }

        if self.check(TokenKind::Period) {
            self.advance();
            return Ok(Some(Sentence {
                statements: Vec::new(),
                span: self.span(),
            }));
        }

        let start_span = self.span();
        let mut statements = Vec::new();

        loop {
            if self.at_eof() || self.check(TokenKind::Period) {
                break;
            }

            if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_start()
                && (self.peek(1).kind == TokenKind::Period
                    || self.peek(1).kind == TokenKind::Section)
            {
                break;
            }

            match self.parse_statement() {
                Ok(stmt) => statements.push(stmt),
                Err(()) => {
                    self.recover_to_period();
                    return Ok(Some(Sentence {
                        statements,
                        span: start_span.merge(&self.span()),
                    }));
                }
            }
        }

        if statements.is_empty() && !self.check(TokenKind::Period) {
            return Ok(None);
        }

        self.eat(TokenKind::Period);

        let end_span = self.span();

        Ok(Some(Sentence {
            statements,
            span: start_span.merge(&end_span),
        }))
    }

    // =========================================================================
    // Statement parsing
    // =========================================================================

    pub(crate) fn parse_statement(&mut self) -> Result<Statement, ()> {
        match self.current().kind {
            TokenKind::Move => self.parse_move_statement(),
            TokenKind::Compute => self.parse_compute_statement(),
            TokenKind::Add => self.parse_add_statement(),
            TokenKind::Subtract => self.parse_subtract_statement(),
            TokenKind::Multiply => self.parse_multiply_statement(),
            TokenKind::Divide => self.parse_divide_statement(),
            TokenKind::Display => self.parse_display_statement(),
            TokenKind::Accept => self.parse_accept_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Evaluate => self.parse_evaluate_statement(),
            TokenKind::Perform => self.parse_perform_statement(),
            TokenKind::Go => self.parse_goto_statement(),
            TokenKind::GoTo => self.parse_goto_statement(),
            TokenKind::Call => self.parse_call_statement(),
            TokenKind::Stop => self.parse_stop_statement(),
            TokenKind::Goback => {
                self.advance();
                Ok(Statement::Goback)
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Statement::Continue)
            }
            TokenKind::Exit => self.parse_exit_statement(),
            TokenKind::Open => self.parse_open_statement(),
            TokenKind::Close => self.parse_close_statement(),
            TokenKind::Read => self.parse_read_statement(),
            TokenKind::Write => self.parse_write_statement(),
            TokenKind::Initialize => self.parse_initialize_statement(),
            TokenKind::Set => self.parse_set_statement(),
            TokenKind::String => self.parse_string_statement(),
            TokenKind::Unstring => self.parse_unstring_statement(),
            TokenKind::Inspect => self.parse_inspect_statement(),
            TokenKind::Sort => self.parse_sort_statement(),
            TokenKind::Merge => self.parse_merge_statement(),
            TokenKind::Release => self.parse_release_statement(),
            TokenKind::Cancel => self.parse_cancel_statement(),
            TokenKind::Rewrite => self.parse_rewrite_statement(),
            TokenKind::Delete => self.parse_delete_statement(),
            TokenKind::Start => self.parse_start_statement(),
            TokenKind::Return => self.parse_return_statement(),
            // --- COBOL 2002+ statements ---
            TokenKind::Raise => self.parse_raise_statement(),
            TokenKind::Resume => self.parse_resume_statement(),
            TokenKind::Invoke => self.parse_invoke_statement(),
            TokenKind::Allocate => self.parse_allocate_statement(),
            TokenKind::Free => self.parse_free_statement(),
            TokenKind::Validate => self.parse_validate_statement(),
            TokenKind::Xml => self.parse_xml_statement(),
            TokenKind::Json => self.parse_json_statement(),
            // --- Report writer statements ---
            TokenKind::Initiate => self.parse_initiate_statement(),
            TokenKind::Generate => self.parse_generate_statement(),
            TokenKind::Terminate => self.parse_terminate_statement(),
            _ => {
                let msg = format!("unexpected token: {:?}", self.current().kind);
                self.error(&msg);
                Err(())
            }
        }
    }

    // --- MOVE ---
    fn parse_move_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Move)?;

        let corresponding = self.eat(TokenKind::Corresponding).is_some();
        let from = self.parse_expr()?;
        self.expect(TokenKind::To)?;

        let mut to = Vec::new();
        loop {
            let target_start = self.span();
            let qn = self.parse_qualified_name()?;
            // Check for reference modification on the target
            let target_expr =
                if self.check(TokenKind::LeftParen) && self.is_reference_modification_ahead() {
                    let (ref_start, ref_length) = self.parse_reference_modification()?;
                    let target_end = self.span();
                    Expr::ReferenceModification {
                        variable: qn,
                        start: Box::new(ref_start),
                        length: ref_length.map(Box::new),
                        span: target_start.merge(&target_end),
                    }
                } else {
                    Expr::Identifier(qn)
                };
            to.push(target_expr);
            if !self.check(TokenKind::Identifier) || self.at_statement_terminator() {
                break;
            }
        }

        let end_span = self.span();
        Ok(Statement::Move(MoveStatement {
            corresponding,
            from,
            to,
            span: start_span.merge(&end_span),
        }))
    }

    // --- COMPUTE ---
    fn parse_compute_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Compute)?;

        let mut targets = Vec::new();
        loop {
            let target = self.parse_qualified_name()?;
            let rounded = self.eat(TokenKind::Rounded).is_some();
            targets.push(RoundedTarget { target, rounded });
            if self.check(TokenKind::Equals) {
                break;
            }
        }

        self.expect(TokenKind::Equals)?;
        let expr = self.parse_expr()?;

        let (on_size_error, not_on_size_error) =
            self.parse_size_error_phrases(TokenKind::EndCompute)?;

        let end_span = self.span();
        Ok(Statement::Compute(Box::new(ComputeStatement {
            targets,
            expr,
            on_size_error,
            not_on_size_error,
            span: start_span.merge(&end_span),
        })))
    }

    // --- ADD ---
    fn parse_add_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Add)?;

        let corresponding = self.eat(TokenKind::Corresponding).is_some();

        let mut operands = Vec::new();
        while !self.check(TokenKind::To) && !self.check(TokenKind::Giving) && !self.at_eof() {
            operands.push(self.parse_expr()?);
        }

        let mut to = Vec::new();
        let mut giving = Vec::new();

        if self.eat(TokenKind::To).is_some() {
            loop {
                let target = self.parse_qualified_name()?;
                let rounded = self.eat(TokenKind::Rounded).is_some();
                to.push(RoundedTarget { target, rounded });
                if self.at_statement_terminator()
                    || self.at_statement_start()
                    || self.check(TokenKind::Giving)
                    || self.check(TokenKind::OnKw)
                    || self.check(TokenKind::SizeKw)
                    || self.check(TokenKind::Not)
                    || self.check(TokenKind::EndAdd)
                {
                    break;
                }
            }
        }

        if self.eat(TokenKind::Giving).is_some() {
            loop {
                let target = self.parse_qualified_name()?;
                let rounded = self.eat(TokenKind::Rounded).is_some();
                giving.push(RoundedTarget { target, rounded });
                if self.at_statement_terminator()
                    || self.at_statement_start()
                    || self.check(TokenKind::OnKw)
                    || self.check(TokenKind::SizeKw)
                    || self.check(TokenKind::Not)
                    || self.check(TokenKind::EndAdd)
                {
                    break;
                }
            }
        }

        let (on_size_error, not_on_size_error) =
            self.parse_size_error_phrases(TokenKind::EndAdd)?;

        let end_span = self.span();
        Ok(Statement::Add(Box::new(AddStatement {
            operands,
            to,
            giving,
            corresponding,
            on_size_error,
            not_on_size_error,
            span: start_span.merge(&end_span),
        })))
    }

    // --- SUBTRACT ---
    fn parse_subtract_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Subtract)?;

        let corresponding = self.eat(TokenKind::Corresponding).is_some();

        let mut operands = Vec::new();
        while !self.check(TokenKind::From) && !self.at_eof() {
            operands.push(self.parse_expr()?);
        }

        self.expect(TokenKind::From)?;

        let mut from = Vec::new();
        let mut giving = Vec::new();

        loop {
            let target = self.parse_qualified_name()?;
            let rounded = self.eat(TokenKind::Rounded).is_some();
            from.push(RoundedTarget { target, rounded });
            if self.at_statement_terminator()
                || self.at_statement_start()
                || self.check(TokenKind::Giving)
                || self.check(TokenKind::OnKw)
                || self.check(TokenKind::SizeKw)
                || self.check(TokenKind::Not)
                || self.check(TokenKind::EndSubtract)
            {
                break;
            }
        }

        if self.eat(TokenKind::Giving).is_some() {
            loop {
                let target = self.parse_qualified_name()?;
                let rounded = self.eat(TokenKind::Rounded).is_some();
                giving.push(RoundedTarget { target, rounded });
                if self.at_statement_terminator()
                    || self.at_statement_start()
                    || self.check(TokenKind::OnKw)
                    || self.check(TokenKind::SizeKw)
                    || self.check(TokenKind::Not)
                    || self.check(TokenKind::EndSubtract)
                {
                    break;
                }
            }
        }

        let (on_size_error, not_on_size_error) =
            self.parse_size_error_phrases(TokenKind::EndSubtract)?;

        let end_span = self.span();
        Ok(Statement::Subtract(Box::new(SubtractStatement {
            operands,
            from,
            giving,
            corresponding,
            on_size_error,
            not_on_size_error,
            span: start_span.merge(&end_span),
        })))
    }

    // --- MULTIPLY ---
    fn parse_multiply_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Multiply)?;

        let operand = self.parse_expr()?;
        self.expect(TokenKind::By)?;

        let mut by = Vec::new();
        let mut giving = Vec::new();

        loop {
            let target = self.parse_qualified_name()?;
            let rounded = self.eat(TokenKind::Rounded).is_some();
            by.push(RoundedTarget { target, rounded });
            if self.at_statement_terminator()
                || self.at_statement_start()
                || self.check(TokenKind::Giving)
                || self.check(TokenKind::OnKw)
                || self.check(TokenKind::SizeKw)
                || self.check(TokenKind::Not)
                || self.check(TokenKind::EndMultiply)
            {
                break;
            }
        }

        if self.eat(TokenKind::Giving).is_some() {
            loop {
                let target = self.parse_qualified_name()?;
                let rounded = self.eat(TokenKind::Rounded).is_some();
                giving.push(RoundedTarget { target, rounded });
                if self.at_statement_terminator()
                    || self.at_statement_start()
                    || self.check(TokenKind::OnKw)
                    || self.check(TokenKind::SizeKw)
                    || self.check(TokenKind::Not)
                    || self.check(TokenKind::EndMultiply)
                {
                    break;
                }
            }
        }

        let (on_size_error, not_on_size_error) =
            self.parse_size_error_phrases(TokenKind::EndMultiply)?;

        let end_span = self.span();
        Ok(Statement::Multiply(Box::new(MultiplyStatement {
            operand,
            by,
            giving,
            on_size_error,
            not_on_size_error,
            span: start_span.merge(&end_span),
        })))
    }

    // --- DIVIDE ---
    fn parse_divide_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Divide)?;

        let mut operand = self.parse_expr()?;

        // DIVIDE A BY B → swap so operand=B(divisor), into=[A](dividend)
        // This matches the INTO form's codegen: result = into / operand
        if self.eat(TokenKind::By).is_some() {
            let by_value = self.parse_expr()?;
            let dividend = operand;
            operand = by_value;
            // Put dividend into the 'into' list; parser continues to GIVING
            let into = vec![RoundedTarget {
                target: match dividend {
                    Expr::Identifier(ref qname) => qname.clone(),
                    _ => {
                        self.error("DIVIDE BY requires identifier operand");
                        return Err(());
                    }
                },
                rounded: false,
            }];
            let mut giving = Vec::new();
            let mut remainder = None;

            if self.eat(TokenKind::Giving).is_some() {
                loop {
                    let target = self.parse_qualified_name()?;
                    let rounded = self.eat(TokenKind::Rounded).is_some();
                    giving.push(RoundedTarget { target, rounded });
                    if self.at_statement_terminator()
                        || self.at_statement_start()
                        || self.check(TokenKind::Remainder)
                        || self.check(TokenKind::OnKw)
                        || self.check(TokenKind::SizeKw)
                        || self.check(TokenKind::Not)
                        || self.check(TokenKind::EndDivide)
                    {
                        break;
                    }
                }
            }
            if self.eat(TokenKind::Remainder).is_some() {
                remainder = Some(self.parse_qualified_name()?);
            }
            let (on_size_error, not_on_size_error) =
                self.parse_size_error_phrases(TokenKind::EndDivide)?;
            let end_span = self.span();
            return Ok(Statement::Divide(Box::new(DivideStatement {
                operand,
                into,
                giving,
                remainder,
                on_size_error,
                not_on_size_error,
                span: start_span.merge(&end_span),
            })));
        }

        self.expect(TokenKind::Into)?;

        let mut into = Vec::new();
        let mut giving = Vec::new();
        let mut remainder = None;

        loop {
            let target = self.parse_qualified_name()?;
            let rounded = self.eat(TokenKind::Rounded).is_some();
            into.push(RoundedTarget { target, rounded });
            if self.at_statement_terminator()
                || self.at_statement_start()
                || self.check(TokenKind::Giving)
                || self.check(TokenKind::Remainder)
                || self.check(TokenKind::OnKw)
                || self.check(TokenKind::SizeKw)
                || self.check(TokenKind::Not)
                || self.check(TokenKind::EndDivide)
            {
                break;
            }
        }

        if self.eat(TokenKind::Giving).is_some() {
            loop {
                let target = self.parse_qualified_name()?;
                let rounded = self.eat(TokenKind::Rounded).is_some();
                giving.push(RoundedTarget { target, rounded });
                if self.at_statement_terminator()
                    || self.at_statement_start()
                    || self.check(TokenKind::Remainder)
                    || self.check(TokenKind::OnKw)
                    || self.check(TokenKind::SizeKw)
                    || self.check(TokenKind::Not)
                    || self.check(TokenKind::EndDivide)
                {
                    break;
                }
            }
        }

        if self.eat(TokenKind::Remainder).is_some() {
            remainder = Some(self.parse_qualified_name()?);
        }

        let (on_size_error, not_on_size_error) =
            self.parse_size_error_phrases(TokenKind::EndDivide)?;

        let end_span = self.span();
        Ok(Statement::Divide(Box::new(DivideStatement {
            operand,
            into,
            giving,
            remainder,
            on_size_error,
            not_on_size_error,
            span: start_span.merge(&end_span),
        })))
    }

    // --- DISPLAY ---
    fn parse_display_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Display)?;

        let mut operands = Vec::new();
        let mut upon = None;
        let mut with_no_advancing = false;

        while !self.at_statement_terminator() && !self.at_statement_start() && !self.at_eof() {
            if self.check_identifier("UPON") {
                self.advance();
                upon = Some(self.expect_identifier()?);
                continue;
            }
            if self.check(TokenKind::With) {
                self.advance();
                if self.check_identifier("NO") {
                    self.advance();
                    self.eat(TokenKind::Advancing);
                    with_no_advancing = true;
                }
                continue;
            }
            if self.check(TokenKind::EndDisplay) {
                self.advance();
                break;
            }
            // Stop at WHEN keyword (used inside EVALUATE blocks)
            if self.check(TokenKind::When) || self.check(TokenKind::Other) {
                break;
            }

            operands.push(self.parse_expr()?);
        }

        let end_span = self.span();
        Ok(Statement::Display(DisplayStatement {
            operands,
            upon,
            with_no_advancing,
            span: start_span.merge(&end_span),
        }))
    }

    // --- ACCEPT ---
    fn parse_accept_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Accept)?;

        let target = self.parse_qualified_name()?;

        let from = if self.check(TokenKind::From) {
            self.advance();
            if self.check_identifier("DATE") {
                self.advance();
                if self.check_identifier("YYYYMMDD") {
                    self.advance();
                    Some(AcceptSource::DateYyyymmdd)
                } else {
                    Some(AcceptSource::Date)
                }
            } else if self.check_identifier("DAY-OF-WEEK") {
                self.advance();
                Some(AcceptSource::DayOfWeek)
            } else if self.check_identifier("DAY") {
                self.advance();
                Some(AcceptSource::Day)
            } else if self.check_identifier("TIME") {
                self.advance();
                Some(AcceptSource::Time)
            } else if self.check_identifier("CONSOLE") {
                self.advance();
                Some(AcceptSource::Console)
            } else {
                None
            }
        } else {
            None
        };

        let end_span = self.span();
        Ok(Statement::Accept(AcceptStatement {
            target,
            from,
            span: start_span.merge(&end_span),
        }))
    }

    // --- IF ---
    fn parse_if_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::If)?;

        let condition = self.parse_condition()?;
        self.eat(TokenKind::Then);

        let mut then_body = Vec::new();
        let mut else_body = Vec::new();

        while !self.at_eof()
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::EndIf)
            && !self.check(TokenKind::Period)
        {
            then_body.push(self.parse_statement()?);
        }

        if self.eat(TokenKind::Else).is_some() {
            while !self.at_eof() && !self.check(TokenKind::EndIf) && !self.check(TokenKind::Period)
            {
                else_body.push(self.parse_statement()?);
            }
        }

        self.eat(TokenKind::EndIf);

        let end_span = self.span();
        Ok(Statement::If(Box::new(IfStatement {
            condition,
            then_body,
            else_body,
            span: start_span.merge(&end_span),
        })))
    }

    // --- EVALUATE ---
    fn parse_evaluate_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Evaluate)?;

        let mut subjects = Vec::new();
        let subject = self.parse_evaluate_subject()?;
        subjects.push(subject);

        while self.check(TokenKind::Also) {
            self.advance();
            subjects.push(self.parse_evaluate_subject()?);
        }

        let mut when_clauses = Vec::new();
        let mut when_other = Vec::new();

        while self.check(TokenKind::When) {
            self.advance();

            if self.check(TokenKind::Other) {
                self.advance();
                while !self.at_eof()
                    && !self.check(TokenKind::EndEvaluate)
                    && !self.check(TokenKind::Period)
                {
                    when_other.push(self.parse_statement()?);
                }
                break;
            }

            let when_span = self.span();
            let obj = self.parse_when_object()?;
            let mut objects = vec![vec![obj]];

            // Parse ALSO-separated objects for multi-subject EVALUATE
            while self.check(TokenKind::Also) {
                self.advance();
                let also_obj = self.parse_when_object()?;
                objects.push(vec![also_obj]);
            }

            let mut body = Vec::new();
            while !self.at_eof()
                && !self.check(TokenKind::When)
                && !self.check(TokenKind::EndEvaluate)
                && !self.check(TokenKind::Period)
            {
                body.push(self.parse_statement()?);
            }

            when_clauses.push(WhenClause {
                objects,
                body,
                span: when_span,
            });
        }

        self.eat(TokenKind::EndEvaluate);

        let end_span = self.span();
        Ok(Statement::Evaluate(Box::new(EvaluateStatement {
            subjects,
            when_clauses,
            when_other,
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_evaluate_subject(&mut self) -> Result<EvaluateSubject, ()> {
        if self.check(TokenKind::TrueKw) {
            self.advance();
            Ok(EvaluateSubject::True)
        } else if self.check(TokenKind::FalseKw) {
            self.advance();
            Ok(EvaluateSubject::False)
        } else {
            let expr = self.parse_expr()?;
            Ok(EvaluateSubject::Expr(expr))
        }
    }

    fn parse_when_object(&mut self) -> Result<WhenObject, ()> {
        if self.check_identifier("ANY") {
            self.advance();
            Ok(WhenObject::Any)
        } else if self.check(TokenKind::TrueKw) {
            self.advance();
            Ok(WhenObject::True)
        } else if self.check(TokenKind::FalseKw) {
            self.advance();
            Ok(WhenObject::False)
        } else if self.check(TokenKind::Not) {
            self.advance();
            let inner = self.parse_when_object()?;
            Ok(WhenObject::Not(Box::new(inner)))
        } else {
            let expr = self.parse_expr()?;
            // If followed by a comparison operator, this is a condition
            // (used with EVALUATE TRUE / EVALUATE FALSE)
            if self.is_comparison_op() {
                let op = self.parse_comparison_op()?;
                let right = self.parse_expr()?;
                let span = self.span();
                Ok(WhenObject::Condition(Condition::Comparison {
                    left: expr,
                    op,
                    right,
                    span,
                }))
            } else if self.check(TokenKind::Thru) {
                self.advance();
                let to = self.parse_expr()?;
                Ok(WhenObject::Range { from: expr, to })
            } else {
                Ok(WhenObject::Expr(expr))
            }
        }
    }

    // --- PERFORM ---
    fn parse_perform_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Perform)?;

        // Parse optional WITH TEST BEFORE/AFTER before VARYING/UNTIL
        let test = self.parse_perform_test();

        // PERFORM VARYING
        if self.check(TokenKind::Varying) {
            return self.parse_perform_varying(start_span, test);
        }

        // PERFORM UNTIL
        if self.check(TokenKind::Until) {
            return self.parse_perform_until(start_span, test);
        }

        // PERFORM n TIMES
        if self.check(TokenKind::IntegerLiteral)
            || (self.check(TokenKind::Identifier) && self.peek(1).kind == TokenKind::Times)
        {
            let times = self.parse_expr()?;
            self.expect(TokenKind::Times)?;

            let mut body = Vec::new();
            while !self.at_eof()
                && !self.check(TokenKind::EndPerform)
                && !self.check(TokenKind::Period)
            {
                body.push(self.parse_statement()?);
            }
            self.eat(TokenKind::EndPerform);

            let end_span = self.span();
            return Ok(Statement::Perform(Box::new(PerformStatement {
                kind: PerformKind::Times { times, body },
                span: start_span.merge(&end_span),
            })));
        }

        // Out-of-line PERFORM (procedure name) with optional TIMES/UNTIL/VARYING
        if (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
            && !self.at_statement_start()
            && (self.peek(1).kind == TokenKind::Thru
                || self.peek(1).kind == TokenKind::Period
                || self.peek(1).kind == TokenKind::Varying
                || self.peek(1).kind == TokenKind::Until
                || self.peek(1).kind == TokenKind::Times
                || self.peek(1).kind == TokenKind::IntegerLiteral
                || self.peek(1).kind == TokenKind::With
                || self.is_end_keyword(self.peek(1).kind)
                || Self::is_statement_start_keyword(self.peek(1).kind)
                || self.peek(1).kind == TokenKind::Eof)
        {
            let procedure = self.advance().text;
            let through = if self.eat(TokenKind::Thru).is_some() {
                Some(self.expect_identifier()?)
            } else {
                None
            };

            // Build a procedure call as the body for looping forms
            let proc_call = Statement::Perform(Box::new(PerformStatement {
                kind: PerformKind::ProcedureName {
                    procedure: procedure.clone(),
                    through: through.clone(),
                },
                span: start_span.merge(&self.span()),
            }));

            // Check for optional WITH TEST BEFORE/AFTER
            let test2 = self.parse_perform_test();

            // PERFORM proc-name n TIMES
            if self.check(TokenKind::IntegerLiteral)
                || (self.check(TokenKind::Identifier) && self.peek(1).kind == TokenKind::Times)
            {
                let times = self.parse_expr()?;
                self.expect(TokenKind::Times)?;
                let end_span = self.span();
                return Ok(Statement::Perform(Box::new(PerformStatement {
                    kind: PerformKind::Times {
                        times,
                        body: vec![proc_call],
                    },
                    span: start_span.merge(&end_span),
                })));
            }

            // PERFORM proc-name UNTIL condition
            if self.check(TokenKind::Until) {
                self.advance();
                let condition = self.parse_condition()?;
                let end_span = self.span();
                return Ok(Statement::Perform(Box::new(PerformStatement {
                    kind: PerformKind::Until {
                        test: test2,
                        condition,
                        body: vec![proc_call],
                    },
                    span: start_span.merge(&end_span),
                })));
            }

            // PERFORM proc-name VARYING ... (treat as inline varying
            // with proc call as body; existing parse_perform_varying parses
            // its own body, so we skip this rare form for now)
            if self.check(TokenKind::Varying) {
                // Fall through to parse_perform_varying which parses body
                // The procedure call will be lost, but this is a rare form.
                return self.parse_perform_varying(start_span, test2);
            }

            // Simple out-of-line PERFORM (no modifier)
            let end_span = self.span();
            return Ok(Statement::Perform(Box::new(PerformStatement {
                kind: PerformKind::ProcedureName { procedure, through },
                span: start_span.merge(&end_span),
            })));
        }

        // Inline PERFORM (simple)
        let mut body = Vec::new();
        while !self.at_eof() && !self.check(TokenKind::EndPerform) && !self.check(TokenKind::Period)
        {
            body.push(self.parse_statement()?);
        }
        self.eat(TokenKind::EndPerform);

        let end_span = self.span();
        Ok(Statement::Perform(Box::new(PerformStatement {
            kind: PerformKind::Simple { body },
            span: start_span.merge(&end_span),
        })))
    }

    /// Parse optional WITH TEST BEFORE/AFTER clause, returning the test type.
    fn parse_perform_test(&mut self) -> PerformTest {
        if self.check(TokenKind::With) || self.check_identifier("WITH") {
            self.advance();
            if self.check_identifier("TEST") {
                self.advance();
                if self.check_identifier("AFTER") {
                    self.advance();
                    return PerformTest::After;
                }
                // BEFORE is the default; consume it if present
                if self.check_identifier("BEFORE") {
                    self.advance();
                }
            }
        }
        PerformTest::Before
    }

    fn parse_perform_varying(
        &mut self,
        start_span: Span,
        test: PerformTest,
    ) -> Result<Statement, ()> {
        self.expect(TokenKind::Varying)?;

        let mut varying = Vec::new();
        let ident = self.parse_qualified_name()?;
        self.expect(TokenKind::From)?;
        let from = self.parse_expr()?;
        self.expect(TokenKind::By)?;
        let by = self.parse_expr()?;
        self.expect(TokenKind::Until)?;
        let until = self.parse_condition()?;

        varying.push(VaryingClause {
            identifier: ident,
            from,
            by,
            until,
        });

        while self.check(TokenKind::After) {
            self.advance();
            let ident = self.parse_qualified_name()?;
            self.expect(TokenKind::From)?;
            let from = self.parse_expr()?;
            self.expect(TokenKind::By)?;
            let by = self.parse_expr()?;
            self.expect(TokenKind::Until)?;
            let until = self.parse_condition()?;
            varying.push(VaryingClause {
                identifier: ident,
                from,
                by,
                until,
            });
        }

        let mut body = Vec::new();
        while !self.at_eof() && !self.check(TokenKind::EndPerform) && !self.check(TokenKind::Period)
        {
            body.push(self.parse_statement()?);
        }
        self.eat(TokenKind::EndPerform);

        let end_span = self.span();
        Ok(Statement::Perform(Box::new(PerformStatement {
            kind: PerformKind::Varying {
                test,
                varying,
                body,
            },
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_perform_until(
        &mut self,
        start_span: Span,
        test: PerformTest,
    ) -> Result<Statement, ()> {
        self.expect(TokenKind::Until)?;

        let condition = self.parse_condition()?;

        let mut body = Vec::new();
        while !self.at_eof() && !self.check(TokenKind::EndPerform) && !self.check(TokenKind::Period)
        {
            body.push(self.parse_statement()?);
        }
        self.eat(TokenKind::EndPerform);

        let end_span = self.span();
        Ok(Statement::Perform(Box::new(PerformStatement {
            kind: PerformKind::Until {
                test,
                condition,
                body,
            },
            span: start_span.merge(&end_span),
        })))
    }

    // --- GO TO ---
    fn parse_goto_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();

        if self.check(TokenKind::Go) {
            self.advance();
            self.eat(TokenKind::To);
        } else {
            self.advance(); // GoTo
        }

        let mut targets = Vec::new();
        let mut depending_on = None;

        while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
            && !self.check(TokenKind::Depending)
            && !self.at_statement_terminator()
        {
            targets.push(self.advance().text);
        }

        if self.check(TokenKind::Depending) {
            self.advance();
            self.eat(TokenKind::OnKw);
            depending_on = Some(self.parse_qualified_name()?);
        }

        let end_span = self.span();
        Ok(Statement::GoTo(GoToStatement {
            targets,
            depending_on,
            span: start_span.merge(&end_span),
        }))
    }

    // --- CALL ---
    fn parse_call_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Call)?;

        let program = self.parse_expr()?;

        let mut using = Vec::new();
        let mut returning = None;

        if self.check(TokenKind::Using) {
            self.advance();
            let mut current_mode = ParamMode::ByReference;

            while !self.at_statement_terminator()
                && !self.check(TokenKind::Returning)
                && !self.check(TokenKind::EndCall)
                && !self.at_eof()
            {
                if self.check(TokenKind::By) {
                    self.advance();
                    if self.check(TokenKind::Reference) {
                        self.advance();
                        current_mode = ParamMode::ByReference;
                    } else if self.check(TokenKind::Content) {
                        self.advance();
                        current_mode = ParamMode::ByContent;
                    } else if self.check(TokenKind::Value) {
                        self.advance();
                        current_mode = ParamMode::ByValue;
                    }
                    continue;
                }

                let value = self.parse_expr()?;
                using.push(CallParam {
                    mode: current_mode,
                    value,
                });
            }
        }

        if self.check(TokenKind::Returning) {
            self.advance();
            returning = Some(self.parse_qualified_name()?);
        }

        let mut on_overflow = Vec::new();
        let mut on_exception = Vec::new();
        let mut not_on_exception = Vec::new();

        // ON OVERFLOW
        if self.check(TokenKind::OnKw) && self.peek(1).kind == TokenKind::Overflow {
            self.advance(); // ON
            self.advance(); // OVERFLOW
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndCall)
                && !self.check(TokenKind::Period)
            {
                on_overflow.push(self.parse_statement()?);
            }
        }
        // ON EXCEPTION
        else if self.check(TokenKind::OnKw) && self.peek(1).kind == TokenKind::ExceptionKw {
            self.advance(); // ON
            self.advance(); // EXCEPTION
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndCall)
                && !self.check(TokenKind::Period)
            {
                on_exception.push(self.parse_statement()?);
            }
        } else if self.check(TokenKind::ExceptionKw) {
            self.advance();
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndCall)
                && !self.check(TokenKind::Period)
            {
                on_exception.push(self.parse_statement()?);
            }
        } else if self.check(TokenKind::Overflow) {
            self.advance();
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndCall)
                && !self.check(TokenKind::Period)
            {
                on_overflow.push(self.parse_statement()?);
            }
        }

        // NOT ON EXCEPTION
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::OnKw);
            self.eat(TokenKind::ExceptionKw);
            while !self.at_eof()
                && !self.check(TokenKind::EndCall)
                && !self.check(TokenKind::Period)
            {
                not_on_exception.push(self.parse_statement()?);
            }
        }

        self.eat(TokenKind::EndCall);

        let end_span = self.span();
        Ok(Statement::Call(Box::new(CallStatement {
            program,
            using,
            returning,
            on_overflow,
            on_exception,
            not_on_exception,
            span: start_span.merge(&end_span),
        })))
    }

    // --- STOP ---
    fn parse_stop_statement(&mut self) -> Result<Statement, ()> {
        self.expect(TokenKind::Stop)?;
        self.expect(TokenKind::Run)?;
        Ok(Statement::StopRun)
    }

    // --- EXIT ---
    fn parse_exit_statement(&mut self) -> Result<Statement, ()> {
        self.expect(TokenKind::Exit)?;

        if self.check(TokenKind::Program) {
            self.advance();
            Ok(Statement::ExitProgram)
        } else if self.check_identifier("PARAGRAPH") {
            self.advance();
            Ok(Statement::ExitParagraph)
        } else if self.check(TokenKind::Section) {
            self.advance();
            Ok(Statement::ExitSection)
        } else {
            // TODO: Add Statement::Exit variant for proper EXIT semantics
            Ok(Statement::Continue)
        }
    }

    // --- OPEN ---
    fn parse_open_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Open)?;

        let mut entries = Vec::new();

        while !self.at_statement_terminator() && !self.at_eof() {
            let mode = if self.check(TokenKind::Input) {
                self.advance();
                OpenMode::Input
            } else if self.check(TokenKind::Output) {
                self.advance();
                OpenMode::Output
            } else if self.check(TokenKind::IoMode) {
                self.advance();
                OpenMode::IoMode
            } else if self.check(TokenKind::Extend) {
                self.advance();
                OpenMode::Extend
            } else {
                break;
            };

            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
            {
                let file_name = self.advance().text;
                entries.push(OpenEntry { mode, file_name });
            }
        }

        let end_span = self.span();
        Ok(Statement::Open(OpenStatement {
            entries,
            span: start_span.merge(&end_span),
        }))
    }

    // --- CLOSE ---
    fn parse_close_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Close)?;

        let mut files = Vec::new();
        while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
            && !self.at_statement_terminator()
            && !self.at_eof()
        {
            let file_name = self.advance().text;
            files.push(CloseEntry {
                file_name,
                close_option: None,
            });
        }

        let end_span = self.span();
        Ok(Statement::Close(CloseStatement {
            files,
            span: start_span.merge(&end_span),
        }))
    }

    // --- READ ---
    fn parse_read_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Read)?;

        let file_name = self.expect_identifier()?;

        let into = if self.check(TokenKind::Into) {
            self.advance();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let key = if self.check(TokenKind::Key) {
            self.advance();
            self.eat_is();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let mut at_end = Vec::new();
        let mut not_at_end = Vec::new();
        let mut invalid_key = Vec::new();
        let mut not_invalid_key = Vec::new();

        // AT END
        if self.check(TokenKind::At) {
            self.advance();
            self.eat(TokenKind::End);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::InvalidKey)
                && !self.check(TokenKind::EndRead)
                && !self.check(TokenKind::Period)
            {
                at_end.push(self.parse_statement()?);
            }
        }

        // NOT AT END
        if self.check(TokenKind::Not) && self.peek(1).kind == TokenKind::At {
            self.advance(); // NOT
            self.eat(TokenKind::At);
            self.eat(TokenKind::End);
            while !self.at_eof()
                && !self.check(TokenKind::InvalidKey)
                && !self.check(TokenKind::EndRead)
                && !self.check(TokenKind::Period)
            {
                not_at_end.push(self.parse_statement()?);
            }
        }

        // INVALID KEY
        if self.check(TokenKind::InvalidKey) {
            self.advance();
            self.eat(TokenKind::Key);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndRead)
                && !self.check(TokenKind::Period)
            {
                invalid_key.push(self.parse_statement()?);
            }
        }

        // NOT INVALID KEY
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::InvalidKey);
            self.eat(TokenKind::Key);
            while !self.at_eof()
                && !self.check(TokenKind::EndRead)
                && !self.check(TokenKind::Period)
            {
                not_invalid_key.push(self.parse_statement()?);
            }
        }

        self.eat(TokenKind::EndRead);

        let end_span = self.span();
        Ok(Statement::Read(Box::new(ReadStatement {
            file_name,
            into,
            key,
            at_end,
            not_at_end,
            invalid_key,
            not_invalid_key,
            span: start_span.merge(&end_span),
        })))
    }

    // --- WRITE ---
    fn parse_write_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Write)?;

        let record_name = self.parse_qualified_name()?;

        let from = if self.check(TokenKind::From) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        // BEFORE/AFTER ADVANCING
        let advancing = if self.check(TokenKind::Before) || self.check(TokenKind::After) {
            self.advance();
            self.eat(TokenKind::Advancing);
            if self.check(TokenKind::Page) {
                self.advance();
                Some(WriteAdvancing::Page)
            } else {
                let expr = self.parse_expr()?;
                self.eat(TokenKind::Line);
                self.eat(TokenKind::Lines);
                Some(WriteAdvancing::Lines(expr))
            }
        } else if self.check(TokenKind::Advancing) {
            self.advance();
            if self.check(TokenKind::Page) {
                self.advance();
                Some(WriteAdvancing::Page)
            } else {
                let expr = self.parse_expr()?;
                self.eat(TokenKind::Line);
                self.eat(TokenKind::Lines);
                Some(WriteAdvancing::Lines(expr))
            }
        } else {
            None
        };

        let mut invalid_key = Vec::new();
        let mut not_invalid_key = Vec::new();
        let mut at_eop = Vec::new();
        let mut not_at_eop = Vec::new();

        // AT END-OF-PAGE / EOP
        if (self.check(TokenKind::At) && self.peek(1).kind == TokenKind::Eop)
            || self.check(TokenKind::Eop)
        {
            self.eat(TokenKind::At);
            self.eat(TokenKind::Eop);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::InvalidKey)
                && !self.check(TokenKind::EndWrite)
                && !self.check(TokenKind::Period)
            {
                at_eop.push(self.parse_statement()?);
            }
        }

        // NOT AT END-OF-PAGE / EOP
        if self.check(TokenKind::Not)
            && (self.peek(1).kind == TokenKind::At || self.peek(1).kind == TokenKind::Eop)
        {
            self.advance(); // NOT
            self.eat(TokenKind::At);
            self.eat(TokenKind::Eop);
            while !self.at_eof()
                && !self.check(TokenKind::InvalidKey)
                && !self.check(TokenKind::EndWrite)
                && !self.check(TokenKind::Period)
            {
                not_at_eop.push(self.parse_statement()?);
            }
        }

        // INVALID KEY
        if self.check(TokenKind::InvalidKey) {
            self.advance();
            self.eat(TokenKind::Key);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(TokenKind::EndWrite)
                && !self.check(TokenKind::Period)
            {
                invalid_key.push(self.parse_statement()?);
            }
        }

        // NOT INVALID KEY
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::InvalidKey);
            self.eat(TokenKind::Key);
            while !self.at_eof()
                && !self.check(TokenKind::EndWrite)
                && !self.check(TokenKind::Period)
            {
                not_invalid_key.push(self.parse_statement()?);
            }
        }

        self.eat(TokenKind::EndWrite);

        let end_span = self.span();
        Ok(Statement::Write(Box::new(WriteStatement {
            record_name,
            from,
            advancing,
            invalid_key,
            not_invalid_key,
            at_eop,
            not_at_eop,
            span: start_span.merge(&end_span),
        })))
    }

    // --- INITIALIZE ---
    fn parse_initialize_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Initialize)?;

        let mut targets = Vec::new();
        while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
            && !self.at_statement_terminator()
            && !self.check(TokenKind::Replacing)
            && !self.at_eof()
        {
            targets.push(self.parse_qualified_name()?);
        }

        let end_span = self.span();
        Ok(Statement::Initialize(Box::new(InitializeStatement {
            targets,
            replacing: Vec::new(),
            with_filler: false,
            span: start_span.merge(&end_span),
        })))
    }

    // --- SET ---
    fn parse_set_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Set)?;

        let mut targets = Vec::new();
        while !self.check(TokenKind::To)
            && !self.check(TokenKind::Up)
            && !self.check(TokenKind::Down)
            && !self.at_statement_terminator()
            && !self.at_eof()
        {
            targets.push(self.parse_qualified_name()?);
        }

        let kind = if self.check(TokenKind::To) {
            self.advance();
            let value = self.parse_expr()?;
            SetKind::To { targets, value }
        } else if self.check(TokenKind::Up) {
            self.advance();
            self.expect(TokenKind::By)?;
            let value = self.parse_expr()?;
            SetKind::UpDown {
                targets,
                direction: SetDirection::Up,
                value,
            }
        } else if self.check(TokenKind::Down) {
            self.advance();
            self.expect(TokenKind::By)?;
            let value = self.parse_expr()?;
            SetKind::UpDown {
                targets,
                direction: SetDirection::Down,
                value,
            }
        } else {
            self.error("expected TO, UP, or DOWN after SET targets");
            return Err(());
        };

        let end_span = self.span();
        Ok(Statement::Set(Box::new(SetStatement {
            kind,
            span: start_span.merge(&end_span),
        })))
    }

    // --- STRING ---
    fn parse_string_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::String)?;

        let mut sources = Vec::new();
        let mut items = Vec::new();
        let mut delimited_by = StringDelimiter::Size;

        while !self.check(TokenKind::Into)
            && !self.check(TokenKind::EndString)
            && !self.at_statement_terminator()
            && !self.at_eof()
        {
            if self.check(TokenKind::Delimited) {
                self.advance();
                self.eat(TokenKind::By);
                if self.check(TokenKind::SizeKw) || self.check_identifier("SIZE") {
                    self.advance();
                    delimited_by = StringDelimiter::Size;
                } else {
                    let val = self.parse_expr()?;
                    delimited_by = StringDelimiter::Value(val);
                }
                sources.push(StringSource {
                    items: std::mem::take(&mut items),
                    delimited_by: delimited_by.clone(),
                });
            } else {
                items.push(self.parse_expr()?);
            }
        }

        if !items.is_empty() {
            sources.push(StringSource {
                items,
                delimited_by,
            });
        }

        self.expect(TokenKind::Into)?;
        let into = self.parse_qualified_name()?;

        let pointer = if self.check(TokenKind::Pointer) || self.check_identifier("POINTER") {
            self.advance();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let (on_overflow, not_on_overflow) = self.parse_overflow_phrases(TokenKind::EndString)?;

        let end_span = self.span();
        Ok(Statement::String(Box::new(StringStatement {
            sources,
            into,
            pointer,
            on_overflow,
            not_on_overflow,
            span: start_span.merge(&end_span),
        })))
    }

    // --- UNSTRING ---
    fn parse_unstring_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Unstring)?;

        let source = self.parse_qualified_name()?;

        // DELIMITED BY
        let mut delimiters = Vec::new();
        if self.check(TokenKind::Delimited) {
            self.advance();
            self.eat(TokenKind::By);
            // Parse first delimiter
            let all = self.eat(TokenKind::All).is_some();
            let value = self.parse_expr()?;
            delimiters.push(UnstringDelimiter { all, value });
            // OR delimiter2 OR delimiter3 ...
            while self.check(TokenKind::Or) {
                self.advance();
                let all = self.eat(TokenKind::All).is_some();
                let value = self.parse_expr()?;
                delimiters.push(UnstringDelimiter { all, value });
            }
        }

        let mut into_targets = Vec::new();
        if self.eat(TokenKind::Into).is_some() {
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.check(TokenKind::EndUnstring)
                && !self.check(TokenKind::Pointer)
                && !self.check(TokenKind::Tallying)
                && !self.check(TokenKind::OnKw)
                && !self.check(TokenKind::Overflow)
                && !self.check(TokenKind::Not)
                && !self.at_eof()
            {
                if self.check_identifier("POINTER") || self.check_identifier("TALLYING") {
                    break;
                }
                let target = self.parse_qualified_name()?;
                let delimiter_in = if self.check(TokenKind::Delimiter) {
                    self.advance();
                    self.eat(TokenKind::In);
                    Some(self.parse_qualified_name()?)
                } else {
                    None
                };
                let count_in = if self.check(TokenKind::Count) {
                    self.advance();
                    self.eat(TokenKind::In);
                    Some(self.parse_qualified_name()?)
                } else {
                    None
                };
                into_targets.push(UnstringTarget {
                    target,
                    delimiter_in,
                    count_in,
                });
            }
        }

        let pointer = if self.check(TokenKind::Pointer) || self.check_identifier("POINTER") {
            self.advance();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let tallying = if self.check(TokenKind::Tallying) || self.check_identifier("TALLYING") {
            self.eat(TokenKind::Tallying);
            self.eat(TokenKind::In);
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let (on_overflow, not_on_overflow) = self.parse_overflow_phrases(TokenKind::EndUnstring)?;

        let end_span = self.span();
        Ok(Statement::Unstring(Box::new(UnstringStatement {
            source,
            delimiters,
            into: into_targets,
            pointer,
            tallying,
            on_overflow,
            not_on_overflow,
            span: start_span.merge(&end_span),
        })))
    }

    // --- INSPECT ---
    fn parse_inspect_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Inspect)?;

        let target = self.parse_qualified_name()?;

        let kind = if self.check(TokenKind::Tallying) {
            self.advance();
            let tallying = self.parse_inspect_tallying_items()?;
            // Check for TALLYING ... REPLACING
            if self.check(TokenKind::Replacing) {
                self.advance();
                let replacing = self.parse_inspect_replacing_items()?;
                InspectKind::TallyingReplacing {
                    tallying,
                    replacing,
                }
            } else {
                InspectKind::Tallying { tallying }
            }
        } else if self.check(TokenKind::Replacing) {
            self.advance();
            let replacing = self.parse_inspect_replacing_items()?;
            InspectKind::Replacing { replacing }
        } else if self.check(TokenKind::Converting) {
            self.advance();
            let from = self.parse_expr()?;
            self.expect(TokenKind::To)?;
            let to = self.parse_expr()?;
            let before_after = self.parse_before_after_clauses()?;
            InspectKind::Converting {
                from: Box::new(from),
                to: Box::new(to),
                before_after,
            }
        } else {
            self.error("expected TALLYING, REPLACING, or CONVERTING");
            return Err(());
        };

        let end_span = self.span();
        Ok(Statement::Inspect(Box::new(InspectStatement {
            target,
            kind,
            span: start_span.merge(&end_span),
        })))
    }

    /// Parse tallying items: counter FOR (CHARACTERS | ALL/LEADING literal)
    fn parse_inspect_tallying_items(&mut self) -> Result<Vec<InspectTallying>, ()> {
        let mut items = Vec::new();
        while !self.at_statement_terminator()
            && !self.at_eof()
            && !self.check(TokenKind::Replacing)
            && !self.at_statement_start()
        {
            // counter identifier
            let counter = self.parse_qualified_name()?;
            self.expect(TokenKind::ForKw)?;

            if self.check(TokenKind::Characters) {
                self.advance();
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectTallying {
                    counter,
                    kind: TallyingKind::Characters,
                    before_after,
                });
            } else if self.check(TokenKind::All) {
                self.advance();
                let value = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectTallying {
                    counter,
                    kind: TallyingKind::All(value),
                    before_after,
                });
            } else if self.check(TokenKind::Leading) {
                self.advance();
                let value = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectTallying {
                    counter,
                    kind: TallyingKind::Leading(value),
                    before_after,
                });
            } else {
                // Unexpected token, break out
                break;
            }
        }
        Ok(items)
    }

    /// Parse replacing items: (ALL|LEADING|TRAILING|FIRST|CHARACTERS) source BY target
    fn parse_inspect_replacing_items(&mut self) -> Result<Vec<InspectReplacing>, ()> {
        let mut items = Vec::new();
        while !self.at_statement_terminator() && !self.at_eof() && !self.at_statement_start() {
            if self.check(TokenKind::Characters) {
                self.advance();
                self.expect(TokenKind::By)?;
                let to = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectReplacing {
                    kind: ReplacingKind::Characters(to),
                    before_after,
                });
            } else if self.check(TokenKind::All) {
                self.advance();
                let from = self.parse_expr()?;
                self.expect(TokenKind::By)?;
                let to = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectReplacing {
                    kind: ReplacingKind::All { from, to },
                    before_after,
                });
            } else if self.check(TokenKind::Leading) {
                self.advance();
                let from = self.parse_expr()?;
                self.expect(TokenKind::By)?;
                let to = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectReplacing {
                    kind: ReplacingKind::Leading { from, to },
                    before_after,
                });
            } else if self.check(TokenKind::FirstKw) {
                self.advance();
                let from = self.parse_expr()?;
                self.expect(TokenKind::By)?;
                let to = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectReplacing {
                    kind: ReplacingKind::First { from, to },
                    before_after,
                });
            } else if self.check(TokenKind::Trailing) {
                // Trailing is handled in INSPECT REPLACING but not in the
                // ReplacingKind enum. Use Leading variant as closest match.
                // NOTE: Trailing should ideally have its own enum variant.
                self.advance();
                let from = self.parse_expr()?;
                self.expect(TokenKind::By)?;
                let to = self.parse_expr()?;
                let before_after = self.parse_before_after_clauses()?;
                items.push(InspectReplacing {
                    kind: ReplacingKind::Leading { from, to },
                    before_after,
                });
            } else {
                break;
            }
        }
        Ok(items)
    }

    // --- REWRITE ---
    fn parse_rewrite_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Rewrite)?;

        let record_name = self.parse_qualified_name()?;

        let from = if self.check(TokenKind::From) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let (invalid_key, not_invalid_key) =
            self.parse_invalid_key_phrases(TokenKind::EndRewrite)?;

        let end_span = self.span();
        Ok(Statement::Rewrite(Box::new(RewriteStatement {
            record_name,
            from,
            invalid_key,
            not_invalid_key,
            span: start_span.merge(&end_span),
        })))
    }

    // --- DELETE ---
    fn parse_delete_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Delete)?;

        let file_name = self.expect_identifier()?;
        self.eat(TokenKind::Record);

        let (invalid_key, not_invalid_key) =
            self.parse_invalid_key_phrases(TokenKind::EndDelete)?;

        let end_span = self.span();
        Ok(Statement::Delete(Box::new(DeleteStatement {
            file_name,
            invalid_key,
            not_invalid_key,
            span: start_span.merge(&end_span),
        })))
    }

    // --- START ---
    fn parse_start_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Start)?;

        let file_name = self.expect_identifier()?;

        let key_condition = if self.check(TokenKind::Key) {
            self.advance();
            self.eat_is();
            let op = if self.check(TokenKind::Equals) || self.check(TokenKind::Equal) {
                self.advance();
                self.eat(TokenKind::To);
                StartRelation::Equal
            } else if self.check(TokenKind::GreaterThan) {
                self.advance();
                StartRelation::GreaterThan
            } else if self.check(TokenKind::GreaterEqual) {
                self.advance();
                StartRelation::GreaterEqual
            } else if self.check(TokenKind::Greater) {
                self.advance();
                self.eat(TokenKind::Than);
                if self.check(TokenKind::Or) {
                    self.advance();
                    self.eat(TokenKind::Equal);
                    self.eat(TokenKind::To);
                    StartRelation::GreaterEqual
                } else {
                    StartRelation::GreaterThan
                }
            } else if self.check(TokenKind::Not) {
                self.advance();
                self.eat(TokenKind::Less);
                self.eat(TokenKind::Than);
                StartRelation::NotLessThan
            } else {
                StartRelation::Equal
            };
            let key = self.parse_qualified_name()?;
            Some(StartKeyCondition { key, op })
        } else {
            None
        };

        let (invalid_key, not_invalid_key) = self.parse_invalid_key_phrases(TokenKind::EndStart)?;

        let end_span = self.span();
        Ok(Statement::Start(Box::new(StartStatement {
            file_name,
            key_condition,
            invalid_key,
            not_invalid_key,
            span: start_span.merge(&end_span),
        })))
    }

    // --- RETURN ---
    fn parse_return_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Return)?;

        let file_name = self.expect_identifier()?;
        self.eat(TokenKind::Record);

        let into = if self.check(TokenKind::Into) {
            self.advance();
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let (at_end, not_at_end) = self.parse_at_end_phrases(TokenKind::EndReturn)?;

        let end_span = self.span();
        Ok(Statement::Return(Box::new(ReturnStatement {
            file_name,
            into,
            at_end,
            not_at_end,
            span: start_span.merge(&end_span),
        })))
    }

    // --- SORT ---
    fn parse_sort_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Sort)?;

        let file_name = self.expect_identifier()?;

        // Parse key clauses
        let mut keys = Vec::new();
        while self.check(TokenKind::OnKw)
            || self.check(TokenKind::Ascending)
            || self.check(TokenKind::Descending)
        {
            self.eat(TokenKind::OnKw);
            let order = if self.check(TokenKind::Ascending) {
                self.advance();
                SortOrder::Ascending
            } else if self.check(TokenKind::Descending) {
                self.advance();
                SortOrder::Descending
            } else {
                break;
            };
            self.eat(TokenKind::Key);
            let mut fields = Vec::new();
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.check(TokenKind::OnKw)
                && !self.check(TokenKind::Ascending)
                && !self.check(TokenKind::Descending)
                && !self.check(TokenKind::Using)
                && !self.check(TokenKind::Input)
                && !self.check(TokenKind::Giving)
                && !self.check(TokenKind::Output)
                && !self.check(TokenKind::Duplicates)
                && !self.check(TokenKind::With)
                && !self.at_eof()
            {
                fields.push(self.parse_qualified_name()?);
            }
            keys.push(SortKey { order, fields });
        }

        // DUPLICATES (WITH DUPLICATES IN ORDER)
        let duplicates = if self.check(TokenKind::With) || self.check(TokenKind::Duplicates) {
            self.eat(TokenKind::With);
            self.eat(TokenKind::Duplicates);
            self.eat(TokenKind::In);
            self.eat_identifier("ORDER");
            true
        } else {
            false
        };

        // Input: USING file-names or INPUT PROCEDURE procedure
        let input = if self.check(TokenKind::Using) {
            self.advance();
            let mut files = Vec::new();
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.check(TokenKind::Giving)
                && !self.check(TokenKind::Output)
                && !self.at_eof()
            {
                files.push(self.advance().text);
            }
            SortInput::Using(files)
        } else if self.check(TokenKind::Input) {
            self.advance();
            self.eat(TokenKind::Procedure);
            self.eat_is();
            let procedure = self.expect_identifier()?;
            let through = if self.eat(TokenKind::Thru).is_some() {
                Some(self.expect_identifier()?)
            } else {
                None
            };
            SortInput::InputProcedure { procedure, through }
        } else {
            SortInput::Using(Vec::new())
        };

        // Output: GIVING file-names or OUTPUT PROCEDURE procedure
        let output = if self.check(TokenKind::Giving) {
            self.advance();
            let mut files = Vec::new();
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.at_eof()
            {
                files.push(self.advance().text);
            }
            SortOutput::Giving(files)
        } else if self.check(TokenKind::Output) {
            self.advance();
            self.eat(TokenKind::Procedure);
            self.eat_is();
            let procedure = self.expect_identifier()?;
            let through = if self.eat(TokenKind::Thru).is_some() {
                Some(self.expect_identifier()?)
            } else {
                None
            };
            SortOutput::OutputProcedure { procedure, through }
        } else {
            SortOutput::Giving(Vec::new())
        };

        let end_span = self.span();
        Ok(Statement::Sort(Box::new(SortStatement {
            file_name,
            keys,
            duplicates,
            input,
            output,
            span: start_span.merge(&end_span),
        })))
    }

    // --- MERGE ---
    fn parse_merge_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Merge)?;

        let file_name = self.expect_identifier()?;

        // Parse key clauses
        let mut keys = Vec::new();
        while self.check(TokenKind::OnKw)
            || self.check(TokenKind::Ascending)
            || self.check(TokenKind::Descending)
        {
            self.eat(TokenKind::OnKw);
            let order = if self.check(TokenKind::Ascending) {
                self.advance();
                SortOrder::Ascending
            } else if self.check(TokenKind::Descending) {
                self.advance();
                SortOrder::Descending
            } else {
                break;
            };
            self.eat(TokenKind::Key);
            let mut fields = Vec::new();
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.check(TokenKind::OnKw)
                && !self.check(TokenKind::Ascending)
                && !self.check(TokenKind::Descending)
                && !self.check(TokenKind::Using)
                && !self.check(TokenKind::Giving)
                && !self.check(TokenKind::Output)
                && !self.at_eof()
            {
                fields.push(self.parse_qualified_name()?);
            }
            keys.push(SortKey { order, fields });
        }

        // USING file-names
        let mut using = Vec::new();
        if self.eat(TokenKind::Using).is_some() {
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.check(TokenKind::Giving)
                && !self.check(TokenKind::Output)
                && !self.at_eof()
            {
                using.push(self.advance().text);
            }
        }

        // Output: GIVING file-names or OUTPUT PROCEDURE procedure
        let output = if self.check(TokenKind::Giving) {
            self.advance();
            let mut files = Vec::new();
            while (self.check(TokenKind::Identifier) || self.current().kind.is_keyword())
                && !self.at_statement_terminator()
                && !self.at_eof()
            {
                files.push(self.advance().text);
            }
            SortOutput::Giving(files)
        } else if self.check(TokenKind::Output) {
            self.advance();
            self.eat(TokenKind::Procedure);
            self.eat_is();
            let procedure = self.expect_identifier()?;
            let through = if self.eat(TokenKind::Thru).is_some() {
                Some(self.expect_identifier()?)
            } else {
                None
            };
            SortOutput::OutputProcedure { procedure, through }
        } else {
            SortOutput::Giving(Vec::new())
        };

        let end_span = self.span();
        Ok(Statement::Merge(Box::new(MergeStatement {
            file_name,
            keys,
            using,
            output,
            span: start_span.merge(&end_span),
        })))
    }

    // --- RELEASE ---
    fn parse_release_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Release)?;

        let record_name = self.parse_qualified_name()?;

        let from = if self.check(TokenKind::From) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let end_span = self.span();
        Ok(Statement::Release(ReleaseStatement {
            record_name,
            from,
            span: start_span.merge(&end_span),
        }))
    }

    // --- CANCEL ---
    fn parse_cancel_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Cancel)?;

        let mut programs = Vec::new();
        while !self.at_statement_terminator() && !self.at_eof() {
            programs.push(self.parse_expr()?);
        }

        let end_span = self.span();
        Ok(Statement::Cancel(CancelStatement {
            programs,
            span: start_span.merge(&end_span),
        }))
    }

    // =========================================================================
    // COBOL 2002+ statements
    // =========================================================================

    fn parse_raise_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::{RaiseStatement, RaiseTarget};
        let start_span = self.span();
        self.expect(TokenKind::Raise)?;

        // RAISE EXCEPTION "name" or RAISE identifier
        let exception = if self.check_identifier("EXCEPTION") {
            self.advance(); // consume EXCEPTION
                            // RAISE EXCEPTION exception-name
            if self.check(TokenKind::StringLiteral) {
                let name = self.current().text.clone();
                self.advance();
                RaiseTarget::Exception(name)
            } else {
                let qn = self.parse_qualified_name()?;
                RaiseTarget::Exception(qn.name)
            }
        } else {
            // RAISE identifier
            let qn = self.parse_qualified_name()?;
            RaiseTarget::Identifier(qn)
        };

        let end_span = self.span();
        Ok(Statement::Raise(RaiseStatement {
            exception,
            span: start_span.merge(&end_span),
        }))
    }

    fn parse_resume_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::ResumeStatement;
        let start_span = self.span();
        self.expect(TokenKind::Resume)?;

        // RESUME [AT label]
        let target = if !self.at_statement_terminator() {
            self.eat(TokenKind::At); // optional AT keyword
            if self.check(TokenKind::Identifier) {
                let name = self.current().text.clone();
                self.advance();
                Some(name)
            } else {
                None
            }
        } else {
            None
        };

        let end_span = self.span();
        Ok(Statement::Resume(ResumeStatement {
            target,
            span: start_span.merge(&end_span),
        }))
    }

    fn parse_invoke_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::{CallParam, InvokeStatement};
        let start_span = self.span();
        self.expect(TokenKind::Invoke)?;

        // INVOKE object "method" [USING params] [RETURNING result]
        let object = self.parse_expr()?;

        let method = self.parse_expr()?;

        let mut using = Vec::new();
        if self.eat(TokenKind::Using).is_some() {
            let mut current_mode = ParamMode::ByReference;
            while !self.at_statement_terminator()
                && !self.check(TokenKind::Returning)
                && !self.at_eof()
            {
                if self.check(TokenKind::By) {
                    self.advance();
                    if self.check(TokenKind::Reference) {
                        self.advance();
                        current_mode = ParamMode::ByReference;
                    } else if self.check(TokenKind::Content) {
                        self.advance();
                        current_mode = ParamMode::ByContent;
                    } else if self.check(TokenKind::Value) {
                        self.advance();
                        current_mode = ParamMode::ByValue;
                    }
                    continue;
                }
                let value = self.parse_expr()?;
                using.push(CallParam {
                    mode: current_mode,
                    value,
                });
            }
        }

        let returning = if self.eat(TokenKind::Returning).is_some() {
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let end_span = self.span();
        Ok(Statement::Invoke(Box::new(InvokeStatement {
            object,
            method,
            using,
            returning,
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_allocate_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::{AllocateStatement, AllocateTarget};
        let start_span = self.span();
        self.expect(TokenKind::Allocate)?;

        // ALLOCATE data-name [RETURNING pointer] [INITIALIZED]
        // or ALLOCATE n CHARACTERS [RETURNING pointer] [INITIALIZED]
        let target = if self.check(TokenKind::IntegerLiteral) {
            let expr = self.parse_expr()?;
            self.eat(TokenKind::Characters);
            AllocateTarget::Characters(expr)
        } else {
            let qn = self.parse_qualified_name()?;
            AllocateTarget::DataName(qn)
        };

        let returning = if self.eat(TokenKind::Returning).is_some() {
            Some(self.parse_qualified_name()?)
        } else {
            None
        };

        let initialized = if self.check_identifier("INITIALIZED") {
            self.advance();
            true
        } else {
            false
        };

        let end_span = self.span();
        Ok(Statement::Allocate(Box::new(AllocateStatement {
            target,
            returning,
            initialized,
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_free_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::FreeStatement;
        let start_span = self.span();
        self.expect(TokenKind::Free)?;

        let mut targets = Vec::new();
        while !self.at_statement_terminator() {
            let qn = self.parse_qualified_name()?;
            targets.push(qn);
        }

        let end_span = self.span();
        Ok(Statement::Free(FreeStatement {
            targets,
            span: start_span.merge(&end_span),
        }))
    }

    fn parse_validate_statement(&mut self) -> Result<Statement, ()> {
        use cobol_ast::statement::ValidateStatement;
        let start_span = self.span();
        self.expect(TokenKind::Validate)?;
        let target = self.parse_qualified_name()?;
        let end_span = self.span();
        Ok(Statement::Validate(ValidateStatement {
            target,
            span: start_span.merge(&end_span),
        }))
    }

    // =========================================================================
    // Error-phrase helpers (ON SIZE ERROR, AT END, INVALID KEY, ON OVERFLOW)
    // =========================================================================

    /// Parse ON SIZE ERROR / NOT ON SIZE ERROR phrases.
    /// Returns (on_size_error_stmts, not_on_size_error_stmts).
    fn parse_size_error_phrases(
        &mut self,
        end_token: TokenKind,
    ) -> Result<(Vec<Statement>, Vec<Statement>), ()> {
        let mut on_size_error = Vec::new();
        let mut not_on_size_error = Vec::new();

        // ON SIZE ERROR
        if self.check(TokenKind::OnKw) || self.check(TokenKind::SizeKw) {
            self.eat(TokenKind::OnKw);
            self.eat(TokenKind::SizeKw);
            self.eat(TokenKind::ErrorKw);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(end_token)
                && !self.check(TokenKind::Period)
            {
                on_size_error.push(self.parse_statement()?);
            }
        }

        // NOT ON SIZE ERROR
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::OnKw);
            self.eat(TokenKind::SizeKw);
            self.eat(TokenKind::ErrorKw);
            while !self.at_eof() && !self.check(end_token) && !self.check(TokenKind::Period) {
                not_on_size_error.push(self.parse_statement()?);
            }
        }

        self.eat(end_token);

        Ok((on_size_error, not_on_size_error))
    }

    /// Parse AT END / NOT AT END phrases.
    /// Returns (at_end_stmts, not_at_end_stmts).
    fn parse_at_end_phrases(
        &mut self,
        end_token: TokenKind,
    ) -> Result<(Vec<Statement>, Vec<Statement>), ()> {
        let mut at_end = Vec::new();
        let mut not_at_end = Vec::new();

        // AT END
        if self.check(TokenKind::At) {
            self.advance();
            self.eat(TokenKind::End);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(end_token)
                && !self.check(TokenKind::Period)
            {
                at_end.push(self.parse_statement()?);
            }
        }

        // NOT AT END
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::At);
            self.eat(TokenKind::End);
            while !self.at_eof() && !self.check(end_token) && !self.check(TokenKind::Period) {
                not_at_end.push(self.parse_statement()?);
            }
        }

        self.eat(end_token);

        Ok((at_end, not_at_end))
    }

    /// Parse INVALID KEY / NOT INVALID KEY phrases.
    /// Returns (invalid_key_stmts, not_invalid_key_stmts).
    fn parse_invalid_key_phrases(
        &mut self,
        end_token: TokenKind,
    ) -> Result<(Vec<Statement>, Vec<Statement>), ()> {
        let mut invalid_key = Vec::new();
        let mut not_invalid_key = Vec::new();

        // INVALID KEY
        if self.check(TokenKind::InvalidKey) {
            self.advance();
            self.eat(TokenKind::Key);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(end_token)
                && !self.check(TokenKind::Period)
            {
                invalid_key.push(self.parse_statement()?);
            }
        }

        // NOT INVALID KEY
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::InvalidKey);
            self.eat(TokenKind::Key);
            while !self.at_eof() && !self.check(end_token) && !self.check(TokenKind::Period) {
                not_invalid_key.push(self.parse_statement()?);
            }
        }

        self.eat(end_token);

        Ok((invalid_key, not_invalid_key))
    }

    /// Parse ON OVERFLOW / NOT ON OVERFLOW phrases.
    /// Returns (on_overflow_stmts, not_on_overflow_stmts).
    fn parse_overflow_phrases(
        &mut self,
        end_token: TokenKind,
    ) -> Result<(Vec<Statement>, Vec<Statement>), ()> {
        let mut on_overflow = Vec::new();
        let mut not_on_overflow = Vec::new();

        // ON OVERFLOW
        if self.check(TokenKind::OnKw) || self.check(TokenKind::Overflow) {
            self.eat(TokenKind::OnKw);
            self.eat(TokenKind::Overflow);
            while !self.at_eof()
                && !self.check(TokenKind::Not)
                && !self.check(end_token)
                && !self.check(TokenKind::Period)
            {
                on_overflow.push(self.parse_statement()?);
            }
        }

        // NOT ON OVERFLOW
        if self.check(TokenKind::Not) {
            self.advance();
            self.eat(TokenKind::OnKw);
            self.eat(TokenKind::Overflow);
            while !self.at_eof() && !self.check(end_token) && !self.check(TokenKind::Period) {
                not_on_overflow.push(self.parse_statement()?);
            }
        }

        self.eat(end_token);

        Ok((on_overflow, not_on_overflow))
    }

    /// Parse BEFORE/AFTER INITIAL phrases for INSPECT.
    fn parse_before_after_clauses(&mut self) -> Result<Vec<BeforeAfter>, ()> {
        let mut clauses = Vec::new();
        while self.check(TokenKind::Before) || self.check(TokenKind::After) {
            let kind = if self.check(TokenKind::Before) {
                self.advance();
                BeforeAfterKind::Before
            } else {
                self.advance();
                BeforeAfterKind::After
            };
            self.eat(TokenKind::Initial);
            let value = self.parse_expr()?;
            clauses.push(BeforeAfter { kind, value });
        }
        Ok(clauses)
    }

    // =========================================================================
    // XML GENERATE / XML PARSE
    // =========================================================================

    fn parse_xml_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Xml)?;

        if self.check(TokenKind::Generate) {
            self.advance();
            self.parse_xml_generate(start_span)
        } else if self.check(TokenKind::Parse) {
            self.advance();
            self.parse_xml_parse(start_span)
        } else {
            self.error("expected GENERATE or PARSE after XML");
            Err(())
        }
    }

    fn parse_xml_generate(&mut self, start_span: Span) -> Result<Statement, ()> {
        let target = self.parse_qualified_name()?;

        // FROM source
        self.eat_identifier("FROM");
        let source = self.parse_qualified_name()?;

        let mut count = None;
        let mut encoding = None;
        let mut xml_declaration = false;
        let mut attributes = false;
        let mut namespace = None;
        let mut namespace_prefix = None;
        let mut name_mapping = Vec::new();
        let mut suppress = Vec::new();
        let mut on_exception = Vec::new();
        let mut not_on_exception = Vec::new();

        loop {
            if self.at_statement_terminator() || self.at_eof() {
                break;
            }
            if self.check_identifier("COUNT") {
                self.advance();
                self.eat(TokenKind::In);
                count = Some(self.parse_qualified_name()?);
            } else if self.check_identifier("ENCODING") {
                self.advance();
                encoding = Some(self.parse_expr()?);
            } else if self.check_identifier("XML-DECLARATION") {
                self.advance();
                xml_declaration = true;
            } else if self.check_identifier("ATTRIBUTES") {
                self.advance();
                attributes = true;
            } else if self.check_identifier("NAMESPACE") {
                self.advance();
                self.eat_identifier("IS");
                namespace = Some(self.parse_expr()?);
                if self.check_identifier("NAMESPACE-PREFIX") {
                    self.advance();
                    self.eat_identifier("IS");
                    namespace_prefix = Some(self.parse_expr()?);
                }
            } else if self.check_identifier("NAME") {
                self.advance();
                self.eat(TokenKind::Of);
                let data_name = self.parse_qualified_name()?;
                self.eat_identifier("IS");
                if self.check(TokenKind::StringLiteral) {
                    let xml_name = self.current().text.clone();
                    self.advance();
                    name_mapping.push(XmlNameMapping {
                        data_name,
                        xml_name,
                    });
                }
            } else if self.check_identifier("SUPPRESS") {
                self.advance();
                while !self.at_statement_terminator()
                    && !self.at_eof()
                    && !self.check_identifier("NAME")
                    && !self.check_identifier("END-XML")
                    && !self.check(TokenKind::OnKw)
                    && !self.check(TokenKind::Not)
                    && !self.check_identifier("ENCODING")
                    && !self.check_identifier("XML-DECLARATION")
                    && !self.check_identifier("ATTRIBUTES")
                    && !self.check_identifier("NAMESPACE")
                {
                    suppress.push(self.parse_qualified_name()?);
                }
            } else if self.check(TokenKind::OnKw) {
                self.advance();
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_eof()
                        && !self.check(TokenKind::Not)
                        && !self.check_identifier("END-XML")
                        && !self.check(TokenKind::Period)
                    {
                        on_exception.push(self.parse_statement()?);
                    }
                } else {
                    break;
                }
            } else if self.check(TokenKind::ExceptionKw) {
                self.advance();
                while !self.at_eof()
                    && !self.check(TokenKind::Not)
                    && !self.check_identifier("END-XML")
                    && !self.check(TokenKind::Period)
                {
                    on_exception.push(self.parse_statement()?);
                }
            } else if self.check(TokenKind::Not) {
                self.advance();
                self.eat(TokenKind::OnKw);
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                }
                while !self.at_eof()
                    && !self.check_identifier("END-XML")
                    && !self.check(TokenKind::Period)
                {
                    not_on_exception.push(self.parse_statement()?);
                }
            } else if self.check_identifier("END-XML") {
                self.advance();
                break;
            } else {
                break;
            }
        }

        let end_span = self.span();
        Ok(Statement::XmlGenerate(Box::new(XmlGenerateStatement {
            target,
            source,
            count,
            encoding,
            xml_declaration,
            attributes,
            namespace,
            namespace_prefix,
            name_mapping,
            suppress,
            on_exception,
            not_on_exception,
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_xml_parse(&mut self, start_span: Span) -> Result<Statement, ()> {
        let source = self.parse_qualified_name()?;

        // PROCESSING PROCEDURE IS procedure-name [THRU procedure-name]
        self.eat_identifier("PROCESSING");
        self.eat_identifier("PROCEDURE");
        self.eat_identifier("IS");

        let proc_name = if self.check(TokenKind::Identifier) {
            let name = self.current().text.clone();
            self.advance();
            name
        } else {
            self.error("expected processing procedure name");
            return Err(());
        };

        let through = if self.check(TokenKind::Thru) {
            self.advance();
            if self.check(TokenKind::Identifier) {
                let name = self.current().text.clone();
                self.advance();
                Some(name)
            } else {
                None
            }
        } else {
            None
        };

        let mut on_exception = Vec::new();
        let mut not_on_exception = Vec::new();

        loop {
            if self.at_statement_terminator() || self.at_eof() {
                break;
            }
            if self.check(TokenKind::OnKw) {
                self.advance();
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_eof()
                        && !self.check(TokenKind::Not)
                        && !self.check_identifier("END-XML")
                        && !self.check(TokenKind::Period)
                    {
                        on_exception.push(self.parse_statement()?);
                    }
                } else {
                    break;
                }
            } else if self.check(TokenKind::ExceptionKw) {
                self.advance();
                while !self.at_eof()
                    && !self.check(TokenKind::Not)
                    && !self.check_identifier("END-XML")
                    && !self.check(TokenKind::Period)
                {
                    on_exception.push(self.parse_statement()?);
                }
            } else if self.check(TokenKind::Not) {
                self.advance();
                self.eat(TokenKind::OnKw);
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                }
                while !self.at_eof()
                    && !self.check_identifier("END-XML")
                    && !self.check(TokenKind::Period)
                {
                    not_on_exception.push(self.parse_statement()?);
                }
            } else if self.check_identifier("END-XML") {
                self.advance();
                break;
            } else {
                break;
            }
        }

        let end_span = self.span();
        Ok(Statement::XmlParse(Box::new(XmlParseStatement {
            source,
            processing_procedure: proc_name,
            through,
            on_exception,
            not_on_exception,
            span: start_span.merge(&end_span),
        })))
    }

    // =========================================================================
    // JSON GENERATE / JSON PARSE
    // =========================================================================

    fn parse_json_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Json)?;

        if self.check(TokenKind::Generate) {
            self.advance();
            self.parse_json_generate(start_span)
        } else if self.check(TokenKind::Parse) {
            self.advance();
            self.parse_json_parse(start_span)
        } else {
            self.error("expected GENERATE or PARSE after JSON");
            Err(())
        }
    }

    fn parse_json_generate(&mut self, start_span: Span) -> Result<Statement, ()> {
        use cobol_ast::statement::{JsonGenerateStatement, JsonNameMapping};

        let target = self.parse_qualified_name()?;

        self.eat_identifier("FROM");
        let source = self.parse_qualified_name()?;

        let mut count = None;
        let mut name_mapping = Vec::new();
        let mut suppress = Vec::new();
        let mut on_exception = Vec::new();
        let mut not_on_exception = Vec::new();

        loop {
            if self.at_statement_terminator() || self.at_eof() {
                break;
            }
            if self.check_identifier("COUNT") {
                self.advance();
                self.eat(TokenKind::In);
                count = Some(self.parse_qualified_name()?);
            } else if self.check_identifier("NAME") {
                self.advance();
                if self.check(TokenKind::Of) {
                    self.advance();
                }
                let data_name = self.parse_qualified_name()?;
                self.eat_identifier("IS");
                if self.check(TokenKind::StringLiteral) {
                    let json_name = self.current().text.clone();
                    self.advance();
                    name_mapping.push(JsonNameMapping {
                        data_name,
                        json_name,
                    });
                } else if self.check_identifier("OMITTED") {
                    self.advance();
                    name_mapping.push(JsonNameMapping {
                        data_name,
                        json_name: "".into(),
                    });
                }
            } else if self.check_identifier("SUPPRESS") {
                self.advance();
                while !self.at_statement_terminator()
                    && !self.at_eof()
                    && !self.check_identifier("NAME")
                    && !self.check_identifier("END-JSON")
                    && !self.check(TokenKind::OnKw)
                    && !self.check(TokenKind::Not)
                {
                    suppress.push(self.parse_qualified_name()?);
                }
            } else if self.check(TokenKind::OnKw) {
                self.advance();
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_statement_terminator()
                        && !self.at_eof()
                        && !self.check(TokenKind::Not)
                        && !self.check_identifier("END-JSON")
                    {
                        on_exception.push(self.parse_statement()?);
                    }
                } else {
                    break;
                }
            } else if self.check(TokenKind::ExceptionKw) {
                self.advance();
                while !self.at_statement_terminator()
                    && !self.at_eof()
                    && !self.check(TokenKind::Not)
                    && !self.check_identifier("END-JSON")
                {
                    on_exception.push(self.parse_statement()?);
                }
            } else if self.check(TokenKind::Not) {
                self.advance();
                self.eat(TokenKind::OnKw);
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_statement_terminator()
                        && !self.at_eof()
                        && !self.check_identifier("END-JSON")
                    {
                        not_on_exception.push(self.parse_statement()?);
                    }
                }
            } else if self.check_identifier("END-JSON") {
                self.advance();
                break;
            } else {
                break;
            }
        }

        let end_span = self.span();
        Ok(Statement::JsonGenerate(Box::new(JsonGenerateStatement {
            target,
            source,
            count,
            name_mapping,
            suppress,
            on_exception,
            not_on_exception,
            span: start_span.merge(&end_span),
        })))
    }

    fn parse_json_parse(&mut self, start_span: Span) -> Result<Statement, ()> {
        use cobol_ast::statement::{JsonNameMapping, JsonParseStatement};

        let source = self.parse_qualified_name()?;

        self.eat_identifier("INTO");
        let target = self.parse_qualified_name()?;

        let mut name_mapping = Vec::new();
        let mut on_exception = Vec::new();
        let mut not_on_exception = Vec::new();

        loop {
            if self.at_statement_terminator() || self.at_eof() {
                break;
            }
            if self.check_identifier("NAME") || self.check_identifier("WITH") {
                if self.check_identifier("WITH") {
                    self.advance();
                }
                if self.check_identifier("NAME") {
                    self.advance();
                }
                if self.check(TokenKind::Of) {
                    self.advance();
                }
                let data_name = self.parse_qualified_name()?;
                self.eat_identifier("IS");
                if self.check(TokenKind::StringLiteral) {
                    let json_name = self.current().text.clone();
                    self.advance();
                    name_mapping.push(JsonNameMapping {
                        data_name,
                        json_name,
                    });
                }
            } else if self.check(TokenKind::OnKw) {
                self.advance();
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_statement_terminator()
                        && !self.at_eof()
                        && !self.check(TokenKind::Not)
                        && !self.check_identifier("END-JSON")
                    {
                        on_exception.push(self.parse_statement()?);
                    }
                } else {
                    break;
                }
            } else if self.check(TokenKind::ExceptionKw) {
                self.advance();
                while !self.at_statement_terminator()
                    && !self.at_eof()
                    && !self.check(TokenKind::Not)
                    && !self.check_identifier("END-JSON")
                {
                    on_exception.push(self.parse_statement()?);
                }
            } else if self.check(TokenKind::Not) {
                self.advance();
                self.eat(TokenKind::OnKw);
                if self.check(TokenKind::ExceptionKw) {
                    self.advance();
                    while !self.at_statement_terminator()
                        && !self.at_eof()
                        && !self.check_identifier("END-JSON")
                    {
                        not_on_exception.push(self.parse_statement()?);
                    }
                }
            } else if self.check_identifier("END-JSON") {
                self.advance();
                break;
            } else {
                break;
            }
        }

        let end_span = self.span();
        Ok(Statement::JsonParse(Box::new(JsonParseStatement {
            source,
            target,
            name_mapping,
            on_exception,
            not_on_exception,
            span: start_span.merge(&end_span),
        })))
    }

    // =========================================================================
    // Report writer statements
    // =========================================================================

    fn parse_initiate_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Initiate)?;
        let mut report_names = Vec::new();
        while !self.at_statement_terminator() && !self.at_eof() {
            if self.check(TokenKind::Identifier) {
                report_names.push(self.current().text.clone());
                self.advance();
            } else {
                break;
            }
        }
        let end_span = self.span();
        Ok(Statement::Initiate(InitiateStatement {
            report_names,
            span: start_span.merge(&end_span),
        }))
    }

    fn parse_generate_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Generate)?;
        let report_name = if self.check(TokenKind::Identifier) {
            let name = self.current().text.clone();
            self.advance();
            name
        } else {
            self.error("expected report or group name");
            return Err(());
        };
        let end_span = self.span();
        Ok(Statement::Generate(GenerateStatement {
            report_name,
            span: start_span.merge(&end_span),
        }))
    }

    fn parse_terminate_statement(&mut self) -> Result<Statement, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Terminate)?;
        let mut report_names = Vec::new();
        while !self.at_statement_terminator() && !self.at_eof() {
            if self.check(TokenKind::Identifier) {
                report_names.push(self.current().text.clone());
                self.advance();
            } else {
                break;
            }
        }
        let end_span = self.span();
        Ok(Statement::Terminate(TerminateStatement {
            report_names,
            span: start_span.merge(&end_span),
        }))
    }

    // =========================================================================
    // Helpers
    // =========================================================================

    fn at_statement_terminator(&self) -> bool {
        if self.check(TokenKind::Period)
            || self.at_eof()
            || self.is_end_keyword(self.current().kind)
        {
            return true;
        }
        // NOT followed by a phrase keyword signals the start of a
        // NOT ON SIZE ERROR / NOT AT END / NOT INVALID KEY / NOT ON OVERFLOW /
        // NOT ON EXCEPTION phrase.  This must terminate the current statement
        // so the outer phrase parser can handle it.
        if self.check(TokenKind::Not) {
            let next = self.peek(1).kind;
            if matches!(
                next,
                TokenKind::OnKw | TokenKind::At | TokenKind::InvalidKey
            ) {
                return true;
            }
        }
        false
    }

    fn is_end_keyword(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::EndIf
                | TokenKind::EndEvaluate
                | TokenKind::EndPerform
                | TokenKind::EndCall
                | TokenKind::EndRead
                | TokenKind::EndWrite
                | TokenKind::EndRewrite
                | TokenKind::EndDelete
                | TokenKind::EndStart
                | TokenKind::EndReturn
                | TokenKind::EndString
                | TokenKind::EndUnstring
                | TokenKind::EndAccept
                | TokenKind::EndDisplay
                | TokenKind::EndCompute
                | TokenKind::EndAdd
                | TokenKind::EndSubtract
                | TokenKind::EndMultiply
                | TokenKind::EndDivide
                | TokenKind::EndSort
                | TokenKind::EndMerge
                | TokenKind::Else
        )
    }
}
