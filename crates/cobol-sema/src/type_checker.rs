// COBOL Semantic Analysis - Type checking
//
// Performs basic type compatibility verification for COBOL statements:
// - MOVE: source/target type compatibility
// - COMPUTE/ADD/SUBTRACT/MULTIPLY/DIVIDE: numeric operands required
// - DISPLAY: any type is acceptable
// - Comparisons: operands must be comparable

use cobol_ast::expr::{Expr, FigurativeConstant, Literal, QualifiedName};
use cobol_ast::proc_div::{Paragraph, ProcedureDivision};
use cobol_ast::program::CobolProgram;
use cobol_ast::statement::*;
use cobol_diagnostics::{Diagnostic, DiagnosticReporter};

use crate::symbol_table::{CobolType, SymbolTable};

/// Walks the AST and checks type compatibility of operations.
pub struct TypeChecker<'a> {
    table: &'a SymbolTable,
    reporter: &'a mut DiagnosticReporter,
}

impl<'a> TypeChecker<'a> {
    pub fn new(table: &'a SymbolTable, reporter: &'a mut DiagnosticReporter) -> Self {
        Self { table, reporter }
    }

    /// Runs type checking on a COBOL program.
    pub fn check(&mut self, program: &CobolProgram) {
        if let Some(ref proc) = program.procedure {
            self.check_procedure_division(proc);
        }
        for nested in &program.nested_programs {
            self.check(nested);
        }
    }

    fn check_procedure_division(&mut self, proc: &ProcedureDivision) {
        for section in &proc.sections {
            for para in &section.paragraphs {
                self.check_paragraph(para);
            }
        }
        for para in &proc.paragraphs {
            self.check_paragraph(para);
        }
    }

    fn check_paragraph(&mut self, para: &Paragraph) {
        for sentence in &para.sentences {
            for stmt in &sentence.statements {
                self.check_statement(stmt);
            }
        }
    }

    fn check_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Move(m) => self.check_move(m),
            Statement::Compute(c) => self.check_compute(c),
            Statement::Add(a) => self.check_add(a),
            Statement::Subtract(s) => self.check_subtract(s),
            Statement::Multiply(m) => self.check_multiply(m),
            Statement::Divide(d) => self.check_divide(d),
            Statement::If(i) => {
                self.check_statements(&i.then_body);
                self.check_statements(&i.else_body);
            }
            Statement::Evaluate(e) => {
                for when in &e.when_clauses {
                    self.check_statements(&when.body);
                }
                self.check_statements(&e.when_other);
            }
            Statement::Perform(p) => match &p.kind {
                PerformKind::Simple { body } => self.check_statements(body),
                PerformKind::Times { body, .. } => self.check_statements(body),
                PerformKind::Until { body, .. } => self.check_statements(body),
                PerformKind::Varying { body, .. } => self.check_statements(body),
                PerformKind::ProcedureName { .. } => {}
            },
            Statement::Call(c) => {
                self.check_statements(&c.on_overflow);
                self.check_statements(&c.on_exception);
                self.check_statements(&c.not_on_exception);
            }
            // Display accepts any type.
            Statement::Display(_) => {}
            // Other statements: recurse into nested statement lists.
            Statement::Read(r) => {
                self.check_statements(&r.at_end);
                self.check_statements(&r.not_at_end);
                self.check_statements(&r.invalid_key);
                self.check_statements(&r.not_invalid_key);
            }
            Statement::Write(w) => {
                self.check_statements(&w.invalid_key);
                self.check_statements(&w.not_invalid_key);
                self.check_statements(&w.at_eop);
                self.check_statements(&w.not_at_eop);
            }
            Statement::String(s) => {
                self.check_statements(&s.on_overflow);
                self.check_statements(&s.not_on_overflow);
            }
            Statement::Unstring(u) => {
                self.check_statements(&u.on_overflow);
                self.check_statements(&u.not_on_overflow);
            }
            _ => {}
        }
    }

    fn check_statements(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            self.check_statement(stmt);
        }
    }

    /// Checks MOVE statement type compatibility.
    ///
    /// COBOL MOVE rules (simplified):
    /// - Numeric to numeric: OK
    /// - Alphanumeric to alphanumeric: OK
    /// - Alphabetic to alphabetic/alphanumeric: OK
    /// - Numeric to alphanumeric: OK (converted to display)
    /// - Alphanumeric to numeric: OK (if source is numeric string)
    /// - Group to/from anything: OK (treated as alphanumeric)
    /// - Numeric-edited/alphanumeric-edited targets: OK from compatible sources
    fn check_move(&mut self, m: &MoveStatement) {
        let source_type = self.resolve_expr_type(&m.from);

        for target in &m.to {
            let target_type = self.resolve_expr_type(target);

            if let (Some(src), Some(tgt)) = (&source_type, &target_type) {
                if !self.is_move_compatible(src, tgt) {
                    self.reporter.report(
                        Diagnostic::warning(
                            "COBC-W200",
                            format!(
                                "MOVE from {} to {} may lose data or truncate",
                                src.detail_name(),
                                tgt.detail_name()
                            ),
                        )
                        .with_label(m.span, "incompatible types in MOVE")
                        .with_note(format!(
                            "source type: {}, target type: {}",
                            src.detail_name(),
                            tgt.detail_name()
                        )),
                    );
                }
            }
        }
    }

    /// Checks COMPUTE statement: all operands must be numeric.
    fn check_compute(&mut self, c: &ComputeStatement) {
        self.check_expr_is_numeric(&c.expr, "COMPUTE expression");

        for target in &c.targets {
            let tgt_type = self.resolve_name_type(&target.target);
            if let Some(ref t) = tgt_type {
                if !t.is_numeric() && !matches!(t, CobolType::NumericEdited { .. }) {
                    self.reporter.report(
                        Diagnostic::error(
                            "COBC-E200",
                            format!("COMPUTE target must be numeric, found {}", t.detail_name()),
                        )
                        .with_label(target.target.span, "non-numeric target")
                        .with_note(format!(
                            "COMPUTE requires a numeric target, but this item is {}",
                            t.detail_name()
                        )),
                    );
                }
            }
        }

        self.check_statements(&c.on_size_error);
        self.check_statements(&c.not_on_size_error);
    }

    /// Checks ADD statement: all operands must be numeric.
    /// When CORRESPONDING is used, group items are valid operands.
    fn check_add(&mut self, a: &AddStatement) {
        if !a.corresponding {
            for op in &a.operands {
                self.check_expr_is_numeric(op, "ADD operand");
            }
            for t in &a.to {
                self.check_target_is_numeric(&t.target, "ADD TO target");
            }
            for g in &a.giving {
                self.check_target_is_numeric(&g.target, "ADD GIVING target");
            }
        }
        self.check_statements(&a.on_size_error);
        self.check_statements(&a.not_on_size_error);
    }

    /// Checks SUBTRACT statement: all operands must be numeric.
    /// When CORRESPONDING is used, group items are valid operands.
    fn check_subtract(&mut self, s: &SubtractStatement) {
        if !s.corresponding {
            for op in &s.operands {
                self.check_expr_is_numeric(op, "SUBTRACT operand");
            }
            for t in &s.from {
                self.check_target_is_numeric(&t.target, "SUBTRACT FROM target");
            }
            for g in &s.giving {
                self.check_target_is_numeric(&g.target, "SUBTRACT GIVING target");
            }
        }
        self.check_statements(&s.on_size_error);
        self.check_statements(&s.not_on_size_error);
    }

    /// Checks MULTIPLY statement: all operands must be numeric.
    fn check_multiply(&mut self, m: &MultiplyStatement) {
        self.check_expr_is_numeric(&m.operand, "MULTIPLY operand");
        for t in &m.by {
            self.check_target_is_numeric(&t.target, "MULTIPLY BY target");
        }
        for g in &m.giving {
            self.check_target_is_numeric(&g.target, "MULTIPLY GIVING target");
        }
        self.check_statements(&m.on_size_error);
        self.check_statements(&m.not_on_size_error);
    }

    /// Checks DIVIDE statement: all operands must be numeric.
    fn check_divide(&mut self, d: &DivideStatement) {
        self.check_expr_is_numeric(&d.operand, "DIVIDE operand");
        for t in &d.into {
            self.check_target_is_numeric(&t.target, "DIVIDE INTO target");
        }
        for g in &d.giving {
            self.check_target_is_numeric(&g.target, "DIVIDE GIVING target");
        }
        if let Some(ref r) = d.remainder {
            self.check_target_is_numeric(r, "DIVIDE REMAINDER target");
        }
        self.check_statements(&d.on_size_error);
        self.check_statements(&d.not_on_size_error);
    }

    // -----------------------------------------------------------------------
    // Helper methods
    // -----------------------------------------------------------------------

    /// Checks that an expression evaluates to a numeric type.
    fn check_expr_is_numeric(&mut self, expr: &Expr, context: &str) {
        let expr_type = self.resolve_expr_type(expr);
        if let Some(ref t) = expr_type {
            if !t.is_numeric() && !matches!(t, CobolType::NumericEdited { .. }) {
                let span = self.expr_span(expr);
                self.reporter.report(
                    Diagnostic::error(
                        "COBC-E201",
                        format!("{} must be numeric, found {}", context, t.detail_name()),
                    )
                    .with_label(span, "non-numeric operand")
                    .with_note(format!(
                        "arithmetic operations require numeric operands, \
                         but this item is {}",
                        t.detail_name()
                    )),
                );
            }
        }
    }

    /// Checks that a target name refers to a numeric data item.
    fn check_target_is_numeric(&mut self, name: &QualifiedName, context: &str) {
        let name_type = self.resolve_name_type(name);
        if let Some(ref t) = name_type {
            if !t.is_numeric() && !matches!(t, CobolType::NumericEdited { .. }) {
                self.reporter.report(
                    Diagnostic::error(
                        "COBC-E202",
                        format!("{} must be numeric, found {}", context, t.detail_name()),
                    )
                    .with_label(name.span, "non-numeric data item")
                    .with_note(format!(
                        "arithmetic operations require numeric targets, \
                         but this item is {}",
                        t.detail_name()
                    )),
                );
            }
        }
    }

    /// Determines whether a MOVE from `source` to `target` is type-compatible.
    fn is_move_compatible(&self, source: &CobolType, target: &CobolType) -> bool {
        // Group items are always alphanumeric-compatible.
        if source.is_group() || target.is_group() {
            return true;
        }

        match (source, target) {
            // Numeric to numeric (including floats): always compatible
            (s, t) if s.is_numeric() && t.is_numeric() => true,

            // Numeric to numeric-edited: OK.
            (s, CobolType::NumericEdited { .. }) if s.is_numeric() => true,

            // Alphanumeric to alphanumeric: OK.
            (s, t) if s.is_alphanumeric() && t.is_alphanumeric() => true,

            // Alphabetic to alphabetic or alphanumeric: OK.
            (CobolType::Alphabetic { .. }, CobolType::Alphabetic { .. }) => true,
            (CobolType::Alphabetic { .. }, CobolType::Alphanumeric { .. }) => true,

            // Numeric to alphanumeric: OK (display conversion).
            (s, t) if s.is_numeric() && t.is_alphanumeric() => true,

            // Alphanumeric to numeric: OK (if source contains numeric data).
            (s, t) if s.is_alphanumeric() && t.is_numeric() => true,

            // Alphanumeric to alphanumeric-edited: OK.
            (s, CobolType::AlphanumericEdited { .. }) if s.is_alphanumeric() => true,

            // Alphabetic to alphanumeric-edited: OK.
            (CobolType::Alphabetic { .. }, CobolType::AlphanumericEdited { .. }) => true,

            // National types: compatible with each other.
            (CobolType::National { .. }, CobolType::National { .. }) => true,

            // Boolean: only to boolean.
            (CobolType::Boolean, CobolType::Boolean) => true,

            // Pointer: only to pointer.
            (CobolType::Pointer, CobolType::Pointer) => true,

            // Otherwise: potentially incompatible.
            _ => false,
        }
    }

    /// Resolves the type of an expression.
    fn resolve_expr_type(&self, expr: &Expr) -> Option<CobolType> {
        match expr {
            Expr::Literal(lit) => Some(self.literal_type(lit)),
            Expr::Identifier(qname) => self.resolve_name_type(qname),
            Expr::BinaryOp { .. } | Expr::UnaryOp { .. } | Expr::Paren { .. } => {
                // Arithmetic expressions produce numeric results.
                Some(CobolType::Numeric {
                    size: 18,
                    decimal_places: 0,
                    is_signed: true,
                })
            }
            Expr::FunctionCall { .. } => {
                // Intrinsic functions - assume numeric for now.
                Some(CobolType::Numeric {
                    size: 18,
                    decimal_places: 0,
                    is_signed: true,
                })
            }
            Expr::ReferenceModification { .. } => {
                // Reference modification always produces an alphanumeric result.
                Some(CobolType::Alphanumeric { size: 0 })
            }
        }
    }

    /// Resolves the type of a qualified name by looking it up in the symbol table.
    fn resolve_name_type(&self, qname: &QualifiedName) -> Option<CobolType> {
        let sym = if qname.qualifiers.is_empty() {
            self.table.lookup(&qname.name)
        } else {
            self.table.lookup_qualified(&qname.name, &qname.qualifiers)
        };
        sym.and_then(|s| s.data_type.clone())
    }

    /// Returns the type of a literal value.
    fn literal_type(&self, lit: &Literal) -> CobolType {
        match lit {
            Literal::Integer(_) => CobolType::Numeric {
                size: 18,
                decimal_places: 0,
                is_signed: true,
            },
            Literal::Decimal(d) => {
                let decimal_places = d.find('.').map_or(0, |pos| (d.len() - pos - 1) as u32);
                CobolType::Numeric {
                    size: 18,
                    decimal_places,
                    is_signed: true,
                }
            }
            Literal::String(s) => CobolType::Alphanumeric {
                size: s.len() as u32,
            },
            Literal::HexString(s) => CobolType::Alphanumeric {
                size: (s.len() / 2) as u32,
            },
            Literal::National(s) => CobolType::National {
                size: s.len() as u32,
            },
            Literal::Boolean(_) => CobolType::Boolean,
            Literal::FigurativeConstant(fc) => match fc {
                FigurativeConstant::Zero => CobolType::Numeric {
                    size: 1,
                    decimal_places: 0,
                    is_signed: false,
                },
                FigurativeConstant::Space
                | FigurativeConstant::HighValue
                | FigurativeConstant::LowValue
                | FigurativeConstant::Quote
                | FigurativeConstant::All(_) => CobolType::Alphanumeric { size: 1 },
                FigurativeConstant::Null => CobolType::Pointer,
            },
        }
    }

    /// Extracts the span from an expression.
    fn expr_span(&self, expr: &Expr) -> cobol_common::Span {
        match expr {
            Expr::Identifier(qname) => qname.span,
            Expr::BinaryOp { span, .. } => *span,
            Expr::UnaryOp { span, .. } => *span,
            Expr::Paren { span, .. } => *span,
            Expr::FunctionCall { span, .. } => *span,
            Expr::Literal(_) => cobol_common::Span::dummy(),
            Expr::ReferenceModification { span, .. } => *span,
        }
    }
}
