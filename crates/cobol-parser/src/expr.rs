// COBOL Parser - Expression and condition parsing

use cobol_ast::expr::*;
use cobol_lexer::token::TokenKind;
use smol_str::SmolStr;

use crate::parser::Parser;

impl Parser {
    // =========================================================================
    // Qualified names
    // =========================================================================

    /// Parse a qualified data name.
    ///
    /// Grammar: identifier [OF|IN identifier]... [(subscript, ...)]
    pub fn parse_qualified_name(&mut self) -> Result<QualifiedName, ()> {
        let start_span = self.span();
        let name = self.expect_identifier()?;

        let mut qualifiers = Vec::new();
        while self.check(TokenKind::Of) || self.check(TokenKind::In) {
            self.advance();
            let qual = self.expect_identifier()?;
            qualifiers.push(qual);
        }

        let mut subscripts = Vec::new();
        let mut ref_mod = None;
        if self.check(TokenKind::LeftParen) {
            // Peek ahead to decide: reference modification (contains ':')
            // vs. subscript. We scan tokens inside the parens looking for
            // a colon at the top nesting level.
            if !self.is_reference_modification_ahead() {
                self.advance(); // consume '('
                loop {
                    let expr = self.parse_expr()?;
                    subscripts.push(expr);
                    // COBOL subscripts can be separated by comma or space
                    let _ = self.eat(TokenKind::Comma);
                    // Stop if we hit ')' or end of input
                    if self.check(TokenKind::RightParen) || self.at_eof() {
                        break;
                    }
                }
                self.expect(TokenKind::RightParen)?;
            } else {
                // Reference modification: consume (start:length)
                let (ref_start, ref_length) = self.parse_reference_modification()?;
                ref_mod = Some((Box::new(ref_start), ref_length.map(Box::new)));
            }
        }

        // After subscripts, there may also be a reference modification
        // e.g. TABLE(IDX)(1:3)
        if ref_mod.is_none()
            && self.check(TokenKind::LeftParen)
            && self.is_reference_modification_ahead()
        {
            let (ref_start, ref_length) = self.parse_reference_modification()?;
            ref_mod = Some((Box::new(ref_start), ref_length.map(Box::new)));
        }

        let end_span = self.span();

        Ok(QualifiedName {
            name,
            qualifiers,
            subscripts,
            ref_mod,
            span: start_span.merge(&end_span),
        })
    }

    /// Look ahead from a `(` token to determine whether the parenthesized
    /// expression is a reference modification (`VAR(start:length)`) rather
    /// than a subscript (`TABLE(index)`). We scan tokens at the top
    /// parenthesis nesting level looking for a `:`.
    pub(crate) fn is_reference_modification_ahead(&self) -> bool {
        // We are positioned at '('. Peek past it.
        let mut offset = 1; // skip the '('
        let mut depth = 1i32;
        loop {
            let tok = self.peek(offset);
            match tok.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return false; // Reached matching ')' without seeing ':'
                    }
                }
                TokenKind::Colon if depth == 1 => return true,
                TokenKind::Eof => return false,
                _ => {}
            }
            offset += 1;
        }
    }

    /// Look ahead from a `(` to determine whether the matching `)` is
    /// followed by a comparison operator (=, >, <, etc.).  If so, the
    /// parentheses group an arithmetic expression, not a condition.
    fn is_paren_followed_by_comparison(&self) -> bool {
        let mut offset = 1; // skip the '('
        let mut depth = 1i32;
        loop {
            let tok = self.peek(offset);
            match tok.kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check what follows ')'
                        let after = self.peek(offset + 1);
                        return matches!(
                            after.kind,
                            TokenKind::Equals
                                | TokenKind::GreaterThan
                                | TokenKind::LessThan
                                | TokenKind::GreaterEqual
                                | TokenKind::LessEqual
                                | TokenKind::NotEqual
                                | TokenKind::Greater
                                | TokenKind::Less
                                | TokenKind::Equal
                        );
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            offset += 1;
        }
    }

    /// Parse reference modification: `(start : length)` or `(start :)`.
    ///
    /// Called when the current token is `(` and we already know this is
    /// a reference modification (contains `:`).
    pub(crate) fn parse_reference_modification(&mut self) -> Result<(Expr, Option<Expr>), ()> {
        self.expect(TokenKind::LeftParen)?;
        let start = self.parse_expr()?;
        self.expect(TokenKind::Colon)?;
        let length = if self.check(TokenKind::RightParen) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::RightParen)?;
        Ok((start, length))
    }

    // =========================================================================
    // Arithmetic expressions
    // =========================================================================

    /// Parse an arithmetic expression using precedence climbing.
    pub fn parse_expr(&mut self) -> Result<Expr, ()> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<Expr, ()> {
        let mut left = self.parse_multiplicative()?;

        loop {
            if self.check(TokenKind::Plus) {
                let start_span = self.span();
                self.advance();
                let right = self.parse_multiplicative()?;
                let end_span = self.span();
                left = Expr::BinaryOp {
                    op: ArithOp::Add,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: start_span.merge(&end_span),
                };
            } else if self.check(TokenKind::Minus) {
                let start_span = self.span();
                self.advance();
                let right = self.parse_multiplicative()?;
                let end_span = self.span();
                left = Expr::BinaryOp {
                    op: ArithOp::Subtract,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: start_span.merge(&end_span),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ()> {
        let mut left = self.parse_power()?;

        loop {
            if self.check(TokenKind::Star) {
                let start_span = self.span();
                self.advance();
                let right = self.parse_power()?;
                let end_span = self.span();
                left = Expr::BinaryOp {
                    op: ArithOp::Multiply,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: start_span.merge(&end_span),
                };
            } else if self.check(TokenKind::Slash) {
                let start_span = self.span();
                self.advance();
                let right = self.parse_power()?;
                let end_span = self.span();
                left = Expr::BinaryOp {
                    op: ArithOp::Divide,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: start_span.merge(&end_span),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, ()> {
        let left = self.parse_unary()?;

        if self.check(TokenKind::DoubleStar) {
            let start_span = self.span();
            self.advance();
            let right = self.parse_power()?;
            let end_span = self.span();
            Ok(Expr::BinaryOp {
                op: ArithOp::Power,
                left: Box::new(left),
                right: Box::new(right),
                span: start_span.merge(&end_span),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, ()> {
        if self.check(TokenKind::Minus) {
            let start_span = self.span();
            self.advance();
            let operand = self.parse_primary()?;
            let end_span = self.span();
            Ok(Expr::UnaryOp {
                op: UnaryArithOp::Negate,
                operand: Box::new(operand),
                span: start_span.merge(&end_span),
            })
        } else if self.check(TokenKind::Plus) {
            let start_span = self.span();
            self.advance();
            let operand = self.parse_primary()?;
            let end_span = self.span();
            Ok(Expr::UnaryOp {
                op: UnaryArithOp::Positive,
                operand: Box::new(operand),
                span: start_span.merge(&end_span),
            })
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ()> {
        // Parenthesized expression
        if self.check(TokenKind::LeftParen) {
            let start_span = self.span();
            self.advance();
            let inner = self.parse_expr()?;
            self.expect(TokenKind::RightParen)?;
            let end_span = self.span();
            return Ok(Expr::Paren {
                inner: Box::new(inner),
                span: start_span.merge(&end_span),
            });
        }

        // Literals
        if let Some(lit) = self.try_parse_literal() {
            return Ok(Expr::Literal(lit));
        }

        // FUNCTION call
        if self.check(TokenKind::Function) {
            return self.parse_function_call();
        }

        // Identifier (possibly qualified), possibly with reference modification
        if self.check(TokenKind::Identifier) || self.current().kind.is_keyword() {
            let start_span = self.span();
            let qn = self.parse_qualified_name()?;

            // parse_qualified_name now consumes reference modification itself.
            // If ref_mod is present, wrap in ReferenceModification expr.
            if let Some((ref_start, ref_length)) = qn.ref_mod.clone() {
                let end_span = self.span();
                let qn_no_ref = QualifiedName {
                    ref_mod: None,
                    ..qn
                };
                return Ok(Expr::ReferenceModification {
                    variable: qn_no_ref,
                    start: ref_start,
                    length: ref_length,
                    span: start_span.merge(&end_span),
                });
            }
            return Ok(Expr::Identifier(qn));
        }

        self.error("expected expression");
        Err(())
    }

    /// Try to parse a literal, returning None if the current token is not a
    /// literal.
    pub(crate) fn try_parse_literal(&mut self) -> Option<Literal> {
        match self.current().kind {
            TokenKind::IntegerLiteral => {
                let tok = self.advance();
                // Note: integers exceeding i64 range silently become 0
                let val: i64 = tok.text.parse().unwrap_or(0);
                Some(Literal::Integer(val))
            }
            TokenKind::DecimalLiteral => {
                let tok = self.advance();
                Some(Literal::Decimal(tok.text.to_string()))
            }
            TokenKind::StringLiteral => {
                let tok = self.advance();
                let s = tok.text.as_str();
                let stripped = if s.len() >= 2 { &s[1..s.len() - 1] } else { s };
                Some(Literal::String(SmolStr::from(stripped)))
            }
            TokenKind::HexLiteral => {
                let tok = self.advance();
                Some(Literal::HexString(tok.text.clone()))
            }
            TokenKind::BooleanLiteral => {
                let tok = self.advance();
                Some(Literal::Boolean(tok.text.clone()))
            }
            TokenKind::NationalLiteral => {
                let tok = self.advance();
                Some(Literal::National(tok.text.clone()))
            }
            TokenKind::Zero => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::Zero))
            }
            TokenKind::Space => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::Space))
            }
            TokenKind::HighValue => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::HighValue))
            }
            TokenKind::LowValue => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::LowValue))
            }
            TokenKind::Quote => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::Quote))
            }
            TokenKind::Null => {
                self.advance();
                Some(Literal::FigurativeConstant(FigurativeConstant::Null))
            }
            TokenKind::All => {
                self.advance();
                // ALL followed by a string literal
                if let Some(lit) = self.try_parse_literal() {
                    let s = match &lit {
                        Literal::String(s) => s.clone(),
                        Literal::FigurativeConstant(FigurativeConstant::Zero) => "0".into(),
                        Literal::FigurativeConstant(FigurativeConstant::Space) => " ".into(),
                        _ => " ".into(),
                    };
                    Some(Literal::FigurativeConstant(FigurativeConstant::All(s)))
                } else {
                    Some(Literal::FigurativeConstant(FigurativeConstant::All(
                        " ".into(),
                    )))
                }
            }
            // Signed numeric literals: +NNN or -NNN
            TokenKind::Plus | TokenKind::Minus => {
                let sign = self.current().kind;
                let sign_char = if sign == TokenKind::Minus { "-" } else { "+" };
                self.advance();
                match self.current().kind {
                    TokenKind::IntegerLiteral => {
                        let tok = self.advance();
                        let text = format!("{}{}", sign_char, tok.text);
                        let val: i64 = text.parse().unwrap_or(0);
                        Some(Literal::Integer(val))
                    }
                    TokenKind::DecimalLiteral => {
                        let tok = self.advance();
                        let text = format!("{}{}", sign_char, tok.text);
                        Some(Literal::Decimal(text))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn parse_function_call(&mut self) -> Result<Expr, ()> {
        let start_span = self.span();
        self.expect(TokenKind::Function)?;

        let name = self.expect_identifier()?;

        let mut args = Vec::new();
        if self.check(TokenKind::LeftParen) {
            self.advance();
            while !self.check(TokenKind::RightParen) && !self.at_eof() {
                let arg = self.parse_expr()?;
                args.push(arg);
                self.eat(TokenKind::Comma);
            }
            self.expect(TokenKind::RightParen)?;
        }

        let end_span = self.span();
        Ok(Expr::FunctionCall {
            name,
            args,
            span: start_span.merge(&end_span),
        })
    }

    // =========================================================================
    // Conditional expressions
    // =========================================================================

    /// Parse a condition (boolean expression).
    pub fn parse_condition(&mut self) -> Result<Condition, ()> {
        self.parse_or_condition()
    }

    fn parse_or_condition(&mut self) -> Result<Condition, ()> {
        let mut left = self.parse_and_condition()?;

        while self.check(TokenKind::Or) {
            self.advance();

            // Handle abbreviated conditions: IF A > B OR < C
            // Also: IF A > B OR NOT < C, IF A > B OR EQUAL TO C
            // Skip noise word IS after AND/OR in abbreviated context
            if self.check_identifier("IS") {
                let next = self.peek(1).kind;
                if next == TokenKind::Not || is_comparison_op_kind(next) {
                    self.advance(); // skip IS
                }
            }
            let is_not = self.check(TokenKind::Not);
            let has_abbrev = if is_not {
                // Peek past NOT to see if a comparison op follows
                is_comparison_op_kind(self.peek(1).kind)
            } else {
                self.is_comparison_op()
            };
            if has_abbrev {
                if let Some(ref left_expr) = extract_comparison_left(&left) {
                    if is_not {
                        self.advance(); // skip NOT
                    }
                    let op = self.parse_comparison_op()?;
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let mut abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    if is_not {
                        abbreviated = Condition::Not(Box::new(abbreviated));
                    }
                    left = Condition::Or(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            // Handle subject-only abbreviated: IF A = B OR 10
            // The operator and subject are inherited from the previous comparison.
            // But NOT if the literal is followed by a comparison operator
            // (e.g. OR 20 LESS THAN X), which is a new full condition.
            if is_abbreviated_subject_only(self.current().kind)
                && !is_comparison_op_kind(self.peek(1).kind)
                && self.peek(1).kind != TokenKind::Not
            {
                if let Some((ref left_expr, op)) = extract_comparison_left_and_op(&left) {
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    left = Condition::Or(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            // Handle identifier-based abbreviated: IF A = B OR C OR D - 1
            // An identifier inherits both the subject and operator from the
            // previous comparison.  Exclude cases where the identifier starts
            // a new full condition (followed by comparison op, subscript,
            // qualifier, or NOT).
            if self.current().kind == TokenKind::Identifier
                && !is_comparison_op_kind(self.peek(1).kind)
                && self.peek(1).kind != TokenKind::Not
                && self.peek(1).kind != TokenKind::Of
                && self.peek(1).kind != TokenKind::In
                && self.peek(1).kind != TokenKind::LeftParen
            {
                if let Some((ref left_expr, op)) = extract_comparison_left_and_op(&left) {
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    left = Condition::Or(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            let right = self.parse_and_condition()?;
            left = Condition::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and_condition(&mut self) -> Result<Condition, ()> {
        let mut left = self.parse_not_condition()?;

        while self.check(TokenKind::And) {
            self.advance();

            // Handle abbreviated conditions: IF A > B AND < C
            // Also: IF A > B AND NOT < C, IF A > B AND IS NOT < C
            // Skip noise word IS after AND/OR in abbreviated context
            if self.check_identifier("IS") {
                // Peek ahead: IS NOT <cmp-op> or IS <cmp-op>
                let next = self.peek(1).kind;
                if next == TokenKind::Not || is_comparison_op_kind(next) {
                    self.advance(); // skip IS
                }
            }
            let is_not = self.check(TokenKind::Not);
            let has_abbrev = if is_not {
                is_comparison_op_kind(self.peek(1).kind)
            } else {
                self.is_comparison_op()
            };
            if has_abbrev {
                if let Some(ref left_expr) = extract_comparison_left(&left) {
                    if is_not {
                        self.advance(); // skip NOT
                    }
                    let op = self.parse_comparison_op()?;
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let mut abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    if is_not {
                        abbreviated = Condition::Not(Box::new(abbreviated));
                    }
                    left = Condition::And(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            // Handle subject-only abbreviated: IF A = B AND 10
            if is_abbreviated_subject_only(self.current().kind)
                && !is_comparison_op_kind(self.peek(1).kind)
                && self.peek(1).kind != TokenKind::Not
            {
                if let Some((ref left_expr, op)) = extract_comparison_left_and_op(&left) {
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    left = Condition::And(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            // Handle identifier-based abbreviated: IF A = B AND C AND D - 1
            if self.current().kind == TokenKind::Identifier
                && !is_comparison_op_kind(self.peek(1).kind)
                && self.peek(1).kind != TokenKind::Not
                && self.peek(1).kind != TokenKind::Of
                && self.peek(1).kind != TokenKind::In
                && self.peek(1).kind != TokenKind::LeftParen
            {
                if let Some((ref left_expr, op)) = extract_comparison_left_and_op(&left) {
                    let right_expr = self.parse_expr()?;
                    let span = self.span();
                    let abbreviated = Condition::Comparison {
                        left: left_expr.clone(),
                        op,
                        right: right_expr,
                        span,
                    };
                    left = Condition::And(Box::new(left), Box::new(abbreviated));
                    continue;
                }
            }

            let right = self.parse_not_condition()?;
            left = Condition::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_not_condition(&mut self) -> Result<Condition, ()> {
        if self.check(TokenKind::Not) {
            self.advance();
            let cond = self.parse_primary_condition()?;
            Ok(Condition::Not(Box::new(cond)))
        } else {
            self.parse_primary_condition()
        }
    }

    fn parse_primary_condition(&mut self) -> Result<Condition, ()> {
        // Parenthesized condition or parenthesized expression
        if self.check(TokenKind::LeftParen) {
            // Look ahead past the matching ')' to see if a comparison operator
            // follows. If so, the parens group an expression (e.g. (A + B) = C),
            // not a condition. Fall through to parse_expr which will handle it.
            if !self.is_paren_followed_by_comparison() {
                self.advance();
                let cond = self.parse_condition()?;
                self.expect(TokenKind::RightParen)?;
                return Ok(Condition::Paren(Box::new(cond)));
            }
        }

        // Parse an expression, then check for comparison/class/sign
        let expr = self.parse_expr()?;

        let has_is = self.check_identifier("IS");
        if has_is {
            self.advance();
        }

        let is_not = self.check(TokenKind::Not);
        if is_not {
            self.advance();
        }

        // Class conditions
        if self.check(TokenKind::Numeric) {
            self.advance();
            return Ok(Condition::ClassCondition {
                operand: expr,
                class: ClassType::Numeric,
                not: is_not,
                span: self.span(),
            });
        }
        if self.check(TokenKind::Alphabetic) {
            self.advance();
            return Ok(Condition::ClassCondition {
                operand: expr,
                class: ClassType::Alphabetic,
                not: is_not,
                span: self.span(),
            });
        }
        if self.check(TokenKind::AlphabeticLower) {
            self.advance();
            return Ok(Condition::ClassCondition {
                operand: expr,
                class: ClassType::AlphabeticLower,
                not: is_not,
                span: self.span(),
            });
        }
        if self.check(TokenKind::AlphabeticUpper) {
            self.advance();
            return Ok(Condition::ClassCondition {
                operand: expr,
                class: ClassType::AlphabeticUpper,
                not: is_not,
                span: self.span(),
            });
        }

        // Sign conditions
        if self.check(TokenKind::Positive) {
            self.advance();
            return Ok(Condition::SignCondition {
                operand: expr,
                sign: SignType::Positive,
                not: is_not,
                span: self.span(),
            });
        }
        if self.check(TokenKind::Negative) {
            self.advance();
            return Ok(Condition::SignCondition {
                operand: expr,
                sign: SignType::Negative,
                not: is_not,
                span: self.span(),
            });
        }
        if self.check(TokenKind::Zero) {
            self.advance();
            return Ok(Condition::SignCondition {
                operand: expr,
                sign: SignType::Zero,
                not: is_not,
                span: self.span(),
            });
        }

        // Comparison operators
        if self.is_comparison_op() {
            let op = self.parse_comparison_op()?;
            let op = if is_not { negate_compare_op(op) } else { op };
            let right = self.parse_expr()?;
            let span = self.span();
            return Ok(Condition::Comparison {
                left: expr,
                op,
                right,
                span,
            });
        }

        if has_is || is_not {
            self.error("expected condition after IS/NOT");
            return Err(());
        }

        // Condition name (level 88)
        if let Expr::Identifier(qn) = expr {
            return Ok(Condition::ConditionName(qn));
        }

        self.error("expected condition");
        Err(())
    }

    /// Check if the current token is a comparison operator.
    pub(crate) fn is_comparison_op(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Equals
                | TokenKind::GreaterThan
                | TokenKind::LessThan
                | TokenKind::GreaterEqual
                | TokenKind::LessEqual
                | TokenKind::NotEqual
                | TokenKind::Greater
                | TokenKind::Less
                | TokenKind::Equal
        )
    }

    /// Parse a comparison operator.
    pub(crate) fn parse_comparison_op(&mut self) -> Result<CompareOp, ()> {
        match self.current().kind {
            TokenKind::Equals => {
                self.advance();
                Ok(CompareOp::Equal)
            }
            TokenKind::GreaterThan => {
                self.advance();
                Ok(CompareOp::GreaterThan)
            }
            TokenKind::LessThan => {
                self.advance();
                Ok(CompareOp::LessThan)
            }
            TokenKind::GreaterEqual => {
                self.advance();
                Ok(CompareOp::GreaterEqual)
            }
            TokenKind::LessEqual => {
                self.advance();
                Ok(CompareOp::LessEqual)
            }
            TokenKind::NotEqual => {
                self.advance();
                Ok(CompareOp::NotEqual)
            }
            TokenKind::Greater => {
                self.advance();
                self.eat(TokenKind::Than);
                if self.check(TokenKind::Or) {
                    self.advance();
                    self.eat(TokenKind::Equal);
                    self.eat(TokenKind::To);
                    return Ok(CompareOp::GreaterEqual);
                }
                Ok(CompareOp::GreaterThan)
            }
            TokenKind::Less => {
                self.advance();
                self.eat(TokenKind::Than);
                if self.check(TokenKind::Or) {
                    self.advance();
                    self.eat(TokenKind::Equal);
                    self.eat(TokenKind::To);
                    return Ok(CompareOp::LessEqual);
                }
                Ok(CompareOp::LessThan)
            }
            TokenKind::Equal => {
                self.advance();
                self.eat(TokenKind::To);
                Ok(CompareOp::Equal)
            }
            _ => {
                self.error("expected comparison operator");
                Err(())
            }
        }
    }
}

/// Negate a comparison operator (for NOT EQUAL, etc.).
fn negate_compare_op(op: CompareOp) -> CompareOp {
    match op {
        CompareOp::Equal => CompareOp::NotEqual,
        CompareOp::NotEqual => CompareOp::Equal,
        CompareOp::GreaterThan => CompareOp::LessEqual,
        CompareOp::LessThan => CompareOp::GreaterEqual,
        CompareOp::GreaterEqual => CompareOp::LessThan,
        CompareOp::LessEqual => CompareOp::GreaterThan,
    }
}

/// Check if the current token could be the start of a subject-only abbreviated
/// combined relation (a literal or figurative constant, NOT an identifier to avoid
/// ambiguity with condition-name OR).
fn is_abbreviated_subject_only(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IntegerLiteral
            | TokenKind::DecimalLiteral
            | TokenKind::StringLiteral
            | TokenKind::Zero
            | TokenKind::Space
            | TokenKind::Quote
            | TokenKind::LowValue
            | TokenKind::HighValue
    )
}

/// Check if a token kind is a comparison operator (standalone function for peek use).
fn is_comparison_op_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Equals
            | TokenKind::GreaterThan
            | TokenKind::LessThan
            | TokenKind::GreaterEqual
            | TokenKind::LessEqual
            | TokenKind::NotEqual
            | TokenKind::Greater
            | TokenKind::Less
            | TokenKind::Equal
    )
}

/// Extract the left-hand expression from the rightmost/deepest comparison
/// in a condition tree. Used for abbreviated combined relations like:
/// `IF A > B OR < C` where `A` must be reused as the left operand.
fn extract_comparison_left(cond: &Condition) -> Option<Expr> {
    match cond {
        Condition::Comparison { left, .. } => Some(left.clone()),
        Condition::And(_, right) | Condition::Or(_, right) => extract_comparison_left(right),
        Condition::Not(inner) => extract_comparison_left(inner),
        _ => None,
    }
}

/// Extract both the left-hand expression and the comparison operator from
/// the rightmost/deepest comparison in a condition tree.
/// Used for subject-only abbreviated combined relations like:
/// `IF A = B OR 10` where both `A` and `=` must be reused.
fn extract_comparison_left_and_op(cond: &Condition) -> Option<(Expr, CompareOp)> {
    match cond {
        Condition::Comparison { left, op, .. } => Some((left.clone(), *op)),
        Condition::And(_, right) | Condition::Or(_, right) => extract_comparison_left_and_op(right),
        Condition::Not(inner) => extract_comparison_left_and_op(inner),
        _ => None,
    }
}
