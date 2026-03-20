// COBOL Semantic Analysis - Name resolution
//
// Walks the AST to:
// 1. Register all data items from DATA DIVISION into the symbol table
// 2. Register all paragraphs and sections from PROCEDURE DIVISION
// 3. Verify that names referenced in statements are defined
// 4. Handle qualified names (e.g. MOVE A OF B TO C)

use cobol_ast::data_div::{ConditionValueItem, DataDivision, DataItem, FileDescription, Usage};
use cobol_ast::expr::{Condition, Expr, FigurativeConstant, Literal, QualifiedName};
use cobol_ast::picture::{PictureCategory, PictureClause};
use cobol_ast::proc_div::{Paragraph, ProcedureDivision};
use cobol_ast::program::CobolProgram;
use cobol_ast::statement::*;
use cobol_common::Span;
use cobol_diagnostics::{Diagnostic, DiagnosticReporter};
use smol_str::SmolStr;

use crate::picture_analyzer::PictureAnalyzer;
use crate::symbol_table::{CobolType, Symbol, SymbolKind, SymbolTable};

/// Walks the AST and populates the symbol table with definitions,
/// then resolves all name references.
pub struct NameResolver<'a> {
    table: &'a mut SymbolTable,
    reporter: &'a mut DiagnosticReporter,
}

impl<'a> NameResolver<'a> {
    pub fn new(table: &'a mut SymbolTable, reporter: &'a mut DiagnosticReporter) -> Self {
        Self { table, reporter }
    }

    /// Runs both registration and resolution phases on a COBOL program.
    pub fn resolve(&mut self, program: &CobolProgram) {
        // Phase 1: register definitions.
        self.register_program(program);

        // Phase 2: resolve references.
        self.resolve_references(program);
    }

    // -----------------------------------------------------------------------
    // Phase 1: Definition registration
    // -----------------------------------------------------------------------

    fn register_program(&mut self, program: &CobolProgram) {
        // Register the program name.
        self.table.define(Symbol {
            name: program.identification.program_id.clone(),
            kind: SymbolKind::Program,
            data_type: None,
            span: program.identification.span,
            parent_name: None,
        });

        // Register data items.
        if let Some(ref data) = program.data {
            self.register_data_division(data);
        }

        // Register procedure names.
        if let Some(ref proc) = program.procedure {
            self.register_procedure_division(proc);
        }

        // Handle nested programs.
        for nested in &program.nested_programs {
            self.register_program(nested);
        }
    }

    fn register_data_division(&mut self, data: &DataDivision) {
        // File section.
        for fd in &data.file_section {
            self.register_file_description(fd);
        }

        // Working-storage section.
        for item in &data.working_storage {
            self.register_data_item(item, None);
        }

        // Local-storage section.
        for item in &data.local_storage {
            self.register_data_item(item, None);
        }

        // Linkage section.
        for item in &data.linkage {
            self.register_data_item(item, None);
        }

        // Screen section.
        for item in &data.screen {
            self.register_data_item(item, None);
        }

        // Communication section.
        for item in &data.communication {
            self.register_data_item(item, None);
        }

        // Report section.
        for item in &data.report {
            self.register_data_item(item, None);
        }
    }

    fn register_file_description(&mut self, fd: &FileDescription) {
        self.table.define(Symbol {
            name: fd.file_name.clone(),
            kind: SymbolKind::FileDescription {
                file_name: fd.file_name.clone(),
            },
            data_type: None,
            span: fd.span,
            parent_name: None,
        });

        // Register record items under the file.
        for item in &fd.items {
            self.register_data_item(item, Some(&fd.file_name));
        }
    }

    fn register_data_item(&mut self, item: &DataItem, parent_name: Option<&SmolStr>) {
        let name = match &item.name {
            Some(n) => n.clone(),
            None => return, // FILLER items are not registered.
        };

        // Determine if this is a group item (has children or no PICTURE).
        let is_group = !item.children.is_empty();

        // Determine the data type from the PICTURE clause or USAGE.
        let data_type = self.determine_data_type(item, is_group);

        if item.level == 88 {
            // Level 88 condition names.
            let values: Vec<String> = item
                .condition_values
                .iter()
                .flat_map(|cv| {
                    cv.values.iter().map(|v| match v {
                        ConditionValueItem::Single(lit) => format_literal(lit),
                        ConditionValueItem::Range { from, to } => {
                            format!("{} THRU {}", format_literal(from), format_literal(to))
                        }
                    })
                })
                .collect();

            self.table.define(Symbol {
                name,
                kind: SymbolKind::ConditionName { values },
                data_type: Some(CobolType::Boolean),
                span: item.span,
                parent_name: parent_name.cloned(),
            });
        } else {
            let kind = SymbolKind::DataItem {
                level: item.level,
                is_group,
            };

            self.table.define(Symbol {
                name: name.clone(),
                kind,
                data_type,
                span: item.span,
                parent_name: parent_name.cloned(),
            });

            // Register INDEXED BY names from the OCCURS clause as index data items.
            if let Some(ref occurs) = item.occurs {
                for idx_name in &occurs.indexed_by {
                    self.table.define(Symbol {
                        name: idx_name.clone(),
                        kind: SymbolKind::DataItem {
                            level: 1,
                            is_group: false,
                        },
                        data_type: Some(CobolType::Index),
                        span: occurs.span,
                        parent_name: Some(name.clone()),
                    });
                }
            }

            // Register children with this item as parent.
            for child in &item.children {
                self.register_data_item(child, Some(&name));
            }
        }
    }

    fn determine_data_type(&self, item: &DataItem, is_group: bool) -> Option<CobolType> {
        if is_group {
            return Some(CobolType::Group { size: 0 });
        }

        // Check USAGE clause for special types.
        if let Some(usage) = &item.usage {
            match usage {
                Usage::Index => return Some(CobolType::Index),
                Usage::Pointer | Usage::FunctionPointer => return Some(CobolType::Pointer),
                Usage::FloatShort | Usage::Comp1 => return Some(CobolType::FloatShort),
                Usage::FloatLong | Usage::Comp2 => return Some(CobolType::FloatLong),
                Usage::FloatExtended => return Some(CobolType::FloatExtended),
                _ => {}
            }
        }

        // Derive type from PICTURE clause if present.
        if let Some(ref pic) = item.picture {
            let analyzed = PictureAnalyzer::analyze(&pic.raw_string, pic.span);
            return Some(picture_to_cobol_type(&analyzed));
        }

        None
    }

    fn register_procedure_division(&mut self, proc: &ProcedureDivision) {
        // Register sections and their paragraphs.
        for section in &proc.sections {
            self.table.define(Symbol {
                name: section.name.clone(),
                kind: SymbolKind::Section,
                data_type: None,
                span: section.span,
                parent_name: None,
            });

            for para in &section.paragraphs {
                self.table.define(Symbol {
                    name: para.name.clone(),
                    kind: SymbolKind::Paragraph,
                    data_type: None,
                    span: para.span,
                    parent_name: Some(section.name.clone()),
                });
            }
        }

        // Register top-level paragraphs.
        for para in &proc.paragraphs {
            self.table.define(Symbol {
                name: para.name.clone(),
                kind: SymbolKind::Paragraph,
                data_type: None,
                span: para.span,
                parent_name: None,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Phase 2: Name reference resolution
    // -----------------------------------------------------------------------

    fn resolve_references(&mut self, program: &CobolProgram) {
        if let Some(ref proc) = program.procedure {
            self.resolve_procedure_division(proc);
        }
        for nested in &program.nested_programs {
            self.resolve_references(nested);
        }
    }

    fn resolve_procedure_division(&mut self, proc: &ProcedureDivision) {
        for section in &proc.sections {
            for para in &section.paragraphs {
                self.resolve_paragraph(para);
            }
        }
        for para in &proc.paragraphs {
            self.resolve_paragraph(para);
        }
    }

    fn resolve_paragraph(&mut self, para: &Paragraph) {
        for sentence in &para.sentences {
            for stmt in &sentence.statements {
                self.resolve_statement(stmt);
            }
        }
    }

    fn resolve_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Move(m) => {
                self.resolve_expr(&m.from);
                for target in &m.to {
                    self.resolve_expr(target);
                }
            }
            Statement::Compute(c) => {
                for target in &c.targets {
                    self.resolve_qualified_name(&target.target);
                }
                self.resolve_expr(&c.expr);
                self.resolve_statements(&c.on_size_error);
                self.resolve_statements(&c.not_on_size_error);
            }
            Statement::Add(a) => {
                for op in &a.operands {
                    self.resolve_expr(op);
                }
                for t in &a.to {
                    self.resolve_qualified_name(&t.target);
                }
                for g in &a.giving {
                    self.resolve_qualified_name(&g.target);
                }
                self.resolve_statements(&a.on_size_error);
                self.resolve_statements(&a.not_on_size_error);
            }
            Statement::Subtract(s) => {
                for op in &s.operands {
                    self.resolve_expr(op);
                }
                for t in &s.from {
                    self.resolve_qualified_name(&t.target);
                }
                for g in &s.giving {
                    self.resolve_qualified_name(&g.target);
                }
                self.resolve_statements(&s.on_size_error);
                self.resolve_statements(&s.not_on_size_error);
            }
            Statement::Multiply(m) => {
                self.resolve_expr(&m.operand);
                for t in &m.by {
                    self.resolve_qualified_name(&t.target);
                }
                for g in &m.giving {
                    self.resolve_qualified_name(&g.target);
                }
                self.resolve_statements(&m.on_size_error);
                self.resolve_statements(&m.not_on_size_error);
            }
            Statement::Divide(d) => {
                self.resolve_expr(&d.operand);
                for t in &d.into {
                    self.resolve_qualified_name(&t.target);
                }
                for g in &d.giving {
                    self.resolve_qualified_name(&g.target);
                }
                if let Some(ref r) = d.remainder {
                    self.resolve_qualified_name(r);
                }
                self.resolve_statements(&d.on_size_error);
                self.resolve_statements(&d.not_on_size_error);
            }
            Statement::Display(d) => {
                for op in &d.operands {
                    self.resolve_expr(op);
                }
            }
            Statement::Accept(a) => {
                self.resolve_qualified_name(&a.target);
            }
            Statement::If(i) => {
                self.resolve_condition(&i.condition);
                self.resolve_statements(&i.then_body);
                self.resolve_statements(&i.else_body);
            }
            Statement::Evaluate(e) => {
                for subj in &e.subjects {
                    match subj {
                        EvaluateSubject::Expr(expr) => self.resolve_expr(expr),
                        EvaluateSubject::Condition(cond) => self.resolve_condition(cond),
                        _ => {}
                    }
                }
                for when in &e.when_clauses {
                    for obj_group in &when.objects {
                        for obj in obj_group {
                            self.resolve_when_object(obj);
                        }
                    }
                    self.resolve_statements(&when.body);
                }
                self.resolve_statements(&e.when_other);
            }
            Statement::Perform(p) => match &p.kind {
                PerformKind::Simple { body } => {
                    self.resolve_statements(body);
                }
                PerformKind::ProcedureName { procedure, through } => {
                    self.resolve_procedure_name(procedure, p.span);
                    if let Some(ref thru) = through {
                        self.resolve_procedure_name(thru, p.span);
                    }
                }
                PerformKind::Times { times, body } => {
                    self.resolve_expr(times);
                    self.resolve_statements(body);
                }
                PerformKind::Until {
                    condition, body, ..
                } => {
                    self.resolve_condition(condition);
                    self.resolve_statements(body);
                }
                PerformKind::Varying { varying, body, .. } => {
                    for v in varying {
                        self.resolve_qualified_name(&v.identifier);
                        self.resolve_expr(&v.from);
                        self.resolve_expr(&v.by);
                        self.resolve_condition(&v.until);
                    }
                    self.resolve_statements(body);
                }
            },
            Statement::GoTo(g) => {
                for target in &g.targets {
                    self.resolve_procedure_name(target, g.span);
                }
                if let Some(ref dep) = g.depending_on {
                    self.resolve_qualified_name(dep);
                }
            }
            Statement::Call(c) => {
                self.resolve_expr(&c.program);
                for param in &c.using {
                    self.resolve_expr(&param.value);
                }
                if let Some(ref ret) = c.returning {
                    self.resolve_qualified_name(ret);
                }
                self.resolve_statements(&c.on_overflow);
                self.resolve_statements(&c.on_exception);
                self.resolve_statements(&c.not_on_exception);
            }
            Statement::Set(s) => match &s.kind {
                SetKind::To { targets, value } => {
                    for t in targets {
                        self.resolve_qualified_name(t);
                    }
                    self.resolve_expr(value);
                }
                SetKind::UpDown { targets, value, .. } => {
                    for t in targets {
                        self.resolve_qualified_name(t);
                    }
                    self.resolve_expr(value);
                }
                SetKind::ConditionTrue { conditions, .. } => {
                    for c in conditions {
                        self.resolve_qualified_name(c);
                    }
                }
                SetKind::Address { target, source } => {
                    self.resolve_qualified_name(target);
                    self.resolve_qualified_name(source);
                }
            },
            Statement::Initialize(init) => {
                for t in &init.targets {
                    self.resolve_qualified_name(t);
                }
                for r in &init.replacing {
                    self.resolve_expr(&r.value);
                }
            }
            Statement::String(s) => {
                for src in &s.sources {
                    for item in &src.items {
                        self.resolve_expr(item);
                    }
                    if let StringDelimiter::Value(ref e) = src.delimited_by {
                        self.resolve_expr(e);
                    }
                }
                self.resolve_qualified_name(&s.into);
                if let Some(ref p) = s.pointer {
                    self.resolve_qualified_name(p);
                }
                self.resolve_statements(&s.on_overflow);
                self.resolve_statements(&s.not_on_overflow);
            }
            Statement::Unstring(u) => {
                self.resolve_qualified_name(&u.source);
                for d in &u.delimiters {
                    self.resolve_expr(&d.value);
                }
                for t in &u.into {
                    self.resolve_qualified_name(&t.target);
                    if let Some(ref d) = t.delimiter_in {
                        self.resolve_qualified_name(d);
                    }
                    if let Some(ref c) = t.count_in {
                        self.resolve_qualified_name(c);
                    }
                }
                if let Some(ref p) = u.pointer {
                    self.resolve_qualified_name(p);
                }
                if let Some(ref t) = u.tallying {
                    self.resolve_qualified_name(t);
                }
                self.resolve_statements(&u.on_overflow);
                self.resolve_statements(&u.not_on_overflow);
            }
            Statement::Inspect(insp) => {
                self.resolve_qualified_name(&insp.target);
                // Inspect kind details are complex; resolve identifiers within.
                self.resolve_inspect_kind(&insp.kind);
            }
            Statement::Read(r) => {
                if let Some(ref into) = r.into {
                    self.resolve_qualified_name(into);
                }
                if let Some(ref key) = r.key {
                    self.resolve_qualified_name(key);
                }
                self.resolve_statements(&r.at_end);
                self.resolve_statements(&r.not_at_end);
                self.resolve_statements(&r.invalid_key);
                self.resolve_statements(&r.not_invalid_key);
            }
            Statement::Write(w) => {
                self.resolve_qualified_name(&w.record_name);
                if let Some(ref from) = w.from {
                    self.resolve_expr(from);
                }
                self.resolve_statements(&w.invalid_key);
                self.resolve_statements(&w.not_invalid_key);
                self.resolve_statements(&w.at_eop);
                self.resolve_statements(&w.not_at_eop);
            }
            Statement::Rewrite(rw) => {
                self.resolve_qualified_name(&rw.record_name);
                if let Some(ref from) = rw.from {
                    self.resolve_expr(from);
                }
                self.resolve_statements(&rw.invalid_key);
                self.resolve_statements(&rw.not_invalid_key);
            }
            Statement::Delete(d) => {
                self.resolve_statements(&d.invalid_key);
                self.resolve_statements(&d.not_invalid_key);
            }
            Statement::Start(s) => {
                if let Some(ref kc) = s.key_condition {
                    self.resolve_qualified_name(&kc.key);
                }
                self.resolve_statements(&s.invalid_key);
                self.resolve_statements(&s.not_invalid_key);
            }
            Statement::Return(r) => {
                if let Some(ref into) = r.into {
                    self.resolve_qualified_name(into);
                }
                self.resolve_statements(&r.at_end);
                self.resolve_statements(&r.not_at_end);
            }
            Statement::Release(r) => {
                self.resolve_qualified_name(&r.record_name);
                if let Some(ref from) = r.from {
                    self.resolve_expr(from);
                }
            }
            Statement::Sort(s) => {
                for key in &s.keys {
                    for f in &key.fields {
                        self.resolve_qualified_name(f);
                    }
                }
            }
            Statement::Merge(m) => {
                for key in &m.keys {
                    for f in &key.fields {
                        self.resolve_qualified_name(f);
                    }
                }
            }
            // Simple statements with no name references.
            Statement::StopRun
            | Statement::Goback
            | Statement::Continue
            | Statement::ExitProgram
            | Statement::ExitParagraph
            | Statement::ExitSection => {}

            // File I/O statements where only file name strings are used.
            Statement::Open(_) | Statement::Close(_) => {}

            // Less common statements - basic handling.
            Statement::Cancel(c) => {
                for p in &c.programs {
                    self.resolve_expr(p);
                }
            }
            Statement::Raise(_) | Statement::Resume(_) => {}
            Statement::Invoke(inv) => {
                self.resolve_expr(&inv.object);
                self.resolve_expr(&inv.method);
                for p in &inv.using {
                    self.resolve_expr(&p.value);
                }
                if let Some(ref ret) = inv.returning {
                    self.resolve_qualified_name(ret);
                }
            }
            Statement::Allocate(a) => {
                match &a.target {
                    AllocateTarget::DataName(n) => self.resolve_qualified_name(n),
                    AllocateTarget::Characters(e) => self.resolve_expr(e),
                }
                if let Some(ref ret) = a.returning {
                    self.resolve_qualified_name(ret);
                }
            }
            Statement::Free(f) => {
                for t in &f.targets {
                    self.resolve_qualified_name(t);
                }
            }
            Statement::JsonGenerate(j) => {
                self.resolve_qualified_name(&j.target);
                self.resolve_qualified_name(&j.source);
                if let Some(ref c) = j.count {
                    self.resolve_qualified_name(c);
                }
                self.resolve_statements(&j.on_exception);
                self.resolve_statements(&j.not_on_exception);
            }
            Statement::JsonParse(j) => {
                self.resolve_qualified_name(&j.source);
                self.resolve_qualified_name(&j.target);
                self.resolve_statements(&j.on_exception);
                self.resolve_statements(&j.not_on_exception);
            }
            Statement::XmlGenerate(x) => {
                self.resolve_qualified_name(&x.target);
                self.resolve_qualified_name(&x.source);
                if let Some(ref c) = x.count {
                    self.resolve_qualified_name(c);
                }
                self.resolve_statements(&x.on_exception);
                self.resolve_statements(&x.not_on_exception);
            }
            Statement::XmlParse(x) => {
                self.resolve_qualified_name(&x.source);
                self.resolve_statements(&x.on_exception);
                self.resolve_statements(&x.not_on_exception);
            }
            Statement::Validate(v) => {
                self.resolve_qualified_name(&v.target);
            }
            Statement::Search(s) => {
                self.resolve_qualified_name(&s.table_name);
                if let Some(ref v) = s.varying {
                    self.resolve_qualified_name(v);
                }
                self.resolve_statements(&s.at_end);
                for w in &s.when_clauses {
                    self.resolve_condition(&w.condition);
                    self.resolve_statements(&w.body);
                }
            }
            // Report writer statements — no name resolution needed for now
            Statement::Initiate(_) | Statement::Generate(_) | Statement::Terminate(_) => {}
        }
    }

    fn resolve_statements(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            self.resolve_statement(stmt);
        }
    }

    fn resolve_qualified_name(&mut self, qname: &QualifiedName) {
        let found = if qname.qualifiers.is_empty() {
            self.table.lookup(&qname.name).is_some()
        } else {
            self.table
                .lookup_qualified(&qname.name, &qname.qualifiers)
                .is_some()
        };

        if !found {
            self.report_undefined_name(&qname.name, qname.span);
        }

        // Resolve subscript expressions.
        for sub in &qname.subscripts {
            self.resolve_expr(sub);
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Identifier(qname) => {
                self.resolve_qualified_name(qname);
            }
            Expr::BinaryOp { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.resolve_expr(operand);
            }
            Expr::Paren { inner, .. } => {
                self.resolve_expr(inner);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.resolve_expr(arg);
                }
            }
            Expr::Literal(_) => {}
            Expr::ReferenceModification {
                variable,
                start,
                length,
                ..
            } => {
                self.resolve_qualified_name(variable);
                self.resolve_expr(start);
                if let Some(len) = length {
                    self.resolve_expr(len);
                }
            }
        }
    }

    fn resolve_condition(&mut self, cond: &Condition) {
        match cond {
            Condition::Comparison { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Condition::ClassCondition { operand, .. } => {
                self.resolve_expr(operand);
            }
            Condition::SignCondition { operand, .. } => {
                self.resolve_expr(operand);
            }
            Condition::ConditionName(qname) => {
                self.resolve_qualified_name(qname);
            }
            Condition::And(a, b) | Condition::Or(a, b) => {
                self.resolve_condition(a);
                self.resolve_condition(b);
            }
            Condition::Not(c) | Condition::Paren(c) => {
                self.resolve_condition(c);
            }
        }
    }

    fn resolve_when_object(&mut self, obj: &WhenObject) {
        match obj {
            WhenObject::Expr(e) => self.resolve_expr(e),
            WhenObject::Condition(c) => self.resolve_condition(c),
            WhenObject::Range { from, to } => {
                self.resolve_expr(from);
                self.resolve_expr(to);
            }
            WhenObject::Not(inner) => self.resolve_when_object(inner),
            WhenObject::Any | WhenObject::True | WhenObject::False => {}
        }
    }

    fn resolve_procedure_name(&mut self, name: &SmolStr, span: Span) {
        // Look up as paragraph or section.
        if self.table.lookup(name).is_none() {
            self.report_undefined_procedure(name, span);
        }
    }

    fn resolve_inspect_kind(&mut self, kind: &InspectKind) {
        match kind {
            InspectKind::Tallying { tallying } => {
                for t in tallying {
                    self.resolve_qualified_name(&t.counter);
                    match &t.kind {
                        TallyingKind::All(e) | TallyingKind::Leading(e) => self.resolve_expr(e),
                        TallyingKind::Characters => {}
                    }
                    self.resolve_before_after(&t.before_after);
                }
            }
            InspectKind::Replacing { replacing } => {
                for r in replacing {
                    match &r.kind {
                        ReplacingKind::Characters(e) => self.resolve_expr(e),
                        ReplacingKind::All { from, to }
                        | ReplacingKind::Leading { from, to }
                        | ReplacingKind::First { from, to } => {
                            self.resolve_expr(from);
                            self.resolve_expr(to);
                        }
                    }
                    self.resolve_before_after(&r.before_after);
                }
            }
            InspectKind::TallyingReplacing {
                tallying,
                replacing,
            } => {
                self.resolve_inspect_kind(&InspectKind::Tallying {
                    tallying: tallying.clone(),
                });
                self.resolve_inspect_kind(&InspectKind::Replacing {
                    replacing: replacing.clone(),
                });
            }
            InspectKind::Converting {
                from,
                to,
                before_after,
            } => {
                self.resolve_expr(from);
                self.resolve_expr(to);
                self.resolve_before_after(before_after);
            }
        }
    }

    fn resolve_before_after(&mut self, items: &[BeforeAfter]) {
        for ba in items {
            self.resolve_expr(&ba.value);
        }
    }

    // -----------------------------------------------------------------------
    // Error reporting helpers
    // -----------------------------------------------------------------------

    fn report_undefined_name(&mut self, name: &SmolStr, span: Span) {
        self.reporter.report(
            Diagnostic::error("COBC-E100", format!("undefined data name '{}'", name))
                .with_label(span, "not found in DATA DIVISION")
                .with_note(
                    "verify the item is defined in WORKING-STORAGE, \
                        LOCAL-STORAGE, or LINKAGE SECTION",
                ),
        );
    }

    fn report_undefined_procedure(&mut self, name: &SmolStr, span: Span) {
        self.reporter.report(
            Diagnostic::error(
                "COBC-E101",
                format!("undefined paragraph or section '{}'", name),
            )
            .with_label(span, "not found in PROCEDURE DIVISION")
            .with_note("verify the paragraph or section name is spelled correctly"),
        );
    }
}

/// Converts a COBOL `PictureClause` to a `CobolType`.
fn picture_to_cobol_type(pic: &PictureClause) -> CobolType {
    match pic.category {
        PictureCategory::Alphabetic => CobolType::Alphabetic { size: pic.size },
        PictureCategory::Alphanumeric => CobolType::Alphanumeric { size: pic.size },
        PictureCategory::Numeric => CobolType::Numeric {
            size: pic.size,
            decimal_places: pic.decimal_positions,
            is_signed: pic.is_signed,
        },
        PictureCategory::NumericEdited => CobolType::NumericEdited { size: pic.size },
        PictureCategory::AlphanumericEdited => CobolType::AlphanumericEdited { size: pic.size },
        PictureCategory::National => CobolType::National { size: pic.size },
        PictureCategory::NationalEdited => CobolType::National { size: pic.size },
        PictureCategory::Boolean => CobolType::Boolean,
    }
}

/// Formats a literal for diagnostic messages.
fn format_literal(lit: &Literal) -> String {
    match lit {
        Literal::Integer(n) => n.to_string(),
        Literal::Decimal(s) => s.clone(),
        Literal::String(s) => format!("\"{}\"", s),
        Literal::HexString(s) => format!("X\"{}\"", s),
        Literal::Boolean(s) => format!("B\"{}\"", s),
        Literal::National(s) => format!("N\"{}\"", s),
        Literal::FigurativeConstant(fc) => match fc {
            FigurativeConstant::Zero => "ZERO".to_string(),
            FigurativeConstant::Space => "SPACE".to_string(),
            FigurativeConstant::HighValue => "HIGH-VALUE".to_string(),
            FigurativeConstant::LowValue => "LOW-VALUE".to_string(),
            FigurativeConstant::Quote => "QUOTE".to_string(),
            FigurativeConstant::All(ref s) => format!("ALL \"{}\"", s),
            FigurativeConstant::Null => "NULL".to_string(),
        },
    }
}
