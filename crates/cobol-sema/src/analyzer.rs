// COBOL Semantic Analysis - Main analysis driver
//
// Orchestrates symbol table construction, name resolution,
// and type checking for a parsed COBOL program.

use cobol_ast::data_div::{DataDivision, DataItem, Usage};
use cobol_ast::proc_div::{ParamMode, ProcedureDivision};
use cobol_ast::statement::Statement;
use cobol_ast::CobolProgram;
use cobol_common::CobolStandard;
use cobol_diagnostics::{Diagnostic, DiagnosticReporter, WarningLevel};

use crate::name_resolver::NameResolver;
use crate::symbol_table::SymbolTable;
use crate::type_checker::TypeChecker;

/// The result of semantic analysis.
pub struct AnalysisResult {
    /// Whether any errors were detected during analysis.
    pub has_errors: bool,
    /// The populated symbol table.
    pub symbol_table: SymbolTable,
}

/// Top-level semantic analyzer that drives all analysis passes.
pub struct SemanticAnalyzer {
    symbol_table: SymbolTable,
    reporter: DiagnosticReporter,
    standard: CobolStandard,
}

impl SemanticAnalyzer {
    /// Creates a new semantic analyzer with an empty symbol table.
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            reporter: DiagnosticReporter::new(),
            standard: CobolStandard::default(),
        }
    }

    /// Creates a new semantic analyzer with a specific warning level.
    pub fn with_warning_level(warning_level: WarningLevel) -> Self {
        Self::with_warning_level_and_standard(warning_level, CobolStandard::default())
    }

    /// Creates a new semantic analyzer with a specific warning level and COBOL standard.
    pub fn with_warning_level_and_standard(
        warning_level: WarningLevel,
        standard: CobolStandard,
    ) -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            reporter: DiagnosticReporter::with_warning_level(warning_level),
            standard,
        }
    }

    /// Creates a new semantic analyzer with a specific COBOL standard.
    pub fn with_standard(standard: CobolStandard) -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            reporter: DiagnosticReporter::new(),
            standard,
        }
    }

    /// Runs all analysis passes on the given COBOL program.
    ///
    /// The analysis proceeds in order:
    /// 1. Name resolution: registers all definitions and verifies references
    /// 2. Type checking: validates type compatibility of operations
    pub fn analyze(&mut self, program: &CobolProgram) -> AnalysisResult {
        // Pass 1: Name resolution (includes symbol table construction).
        {
            let mut resolver = NameResolver::new(&mut self.symbol_table, &mut self.reporter);
            resolver.resolve(program);
        }

        // Pass 2: Type checking.
        {
            let mut checker = TypeChecker::new(&self.symbol_table, &mut self.reporter);
            checker.check(program);
        }

        self.check_standard_conformance(program);

        AnalysisResult {
            has_errors: self.reporter.has_errors(),
            symbol_table: std::mem::take(&mut self.symbol_table),
        }
    }

    /// Returns a reference to the symbol table.
    pub fn symbol_table(&self) -> &SymbolTable {
        &self.symbol_table
    }

    /// Takes ownership of the diagnostic reporter, replacing it with an empty one.
    pub fn take_diagnostics(&mut self) -> DiagnosticReporter {
        std::mem::take(&mut self.reporter)
    }

    fn check_standard_conformance(&mut self, program: &CobolProgram) {
        self.check_program_standard(program);
        for nested in &program.nested_programs {
            self.check_standard_conformance(nested);
        }
    }

    fn check_program_standard(&mut self, program: &CobolProgram) {
        if let Some(data) = &program.data {
            self.check_data_division_standard(data);
        }
        if let Some(procedure) = &program.procedure {
            self.check_procedure_division_standard(procedure);
        }
    }

    fn check_data_division_standard(&mut self, data: &DataDivision) {
        if !data.local_storage.is_empty() {
            self.require_standard(CobolStandard::Cobol2002, "LOCAL-STORAGE SECTION", data.span);
        }

        for item in data
            .file_section
            .iter()
            .flat_map(|fd| fd.items.iter())
            .chain(data.working_storage.iter())
            .chain(data.local_storage.iter())
            .chain(data.linkage.iter())
            .chain(data.screen.iter())
            .chain(data.report.iter())
        {
            self.check_data_item_standard(item);
        }
        for cd in &data.communication {
            for item in &cd.data_items {
                self.check_data_item_standard(item);
            }
        }
    }

    fn check_data_item_standard(&mut self, item: &DataItem) {
        match item.usage {
            Some(Usage::Pointer | Usage::FunctionPointer) => {
                self.require_standard(CobolStandard::Cobol2002, "pointer usage", item.span);
            }
            Some(Usage::FloatShort | Usage::FloatLong | Usage::FloatExtended) => {
                self.require_standard(CobolStandard::Cobol2014, "floating usage", item.span);
            }
            _ => {}
        }

        for child in &item.children {
            self.check_data_item_standard(child);
        }
    }

    fn check_procedure_division_standard(&mut self, procedure: &ProcedureDivision) {
        if procedure.returning.is_some() {
            self.require_standard(
                CobolStandard::Cobol2002,
                "PROCEDURE DIVISION RETURNING",
                procedure.span,
            );
        }
        for param in &procedure.using_params {
            if param.mode == ParamMode::ByValue {
                self.require_standard(CobolStandard::Cobol2002, "USING BY VALUE", param.span);
            }
        }
        for declarative in &procedure.declaratives {
            for paragraph in &declarative.paragraphs {
                self.check_sentences_standard(&paragraph.sentences);
            }
        }
        for section in &procedure.sections {
            for paragraph in &section.paragraphs {
                self.check_sentences_standard(&paragraph.sentences);
            }
        }
        for paragraph in &procedure.paragraphs {
            self.check_sentences_standard(&paragraph.sentences);
        }
    }

    fn check_sentences_standard(&mut self, sentences: &[cobol_ast::proc_div::Sentence]) {
        for sentence in sentences {
            self.check_statements_standard(&sentence.statements);
        }
    }

    fn check_statements_standard(&mut self, statements: &[Statement]) {
        for statement in statements {
            self.check_statement_standard(statement);
        }
    }

    fn check_statement_standard(&mut self, statement: &Statement) {
        match statement {
            Statement::Raise(s) => {
                self.require_standard(CobolStandard::Cobol2002, "RAISE", s.span);
            }
            Statement::Resume(s) => {
                self.require_standard(CobolStandard::Cobol2002, "RESUME", s.span);
            }
            Statement::Invoke(s) => {
                self.require_standard(CobolStandard::Cobol2002, "INVOKE", s.span);
                for param in &s.using {
                    if param.mode == ParamMode::ByValue {
                        self.require_standard(
                            CobolStandard::Cobol2002,
                            "CALL/INVOKE BY VALUE",
                            s.span,
                        );
                    }
                }
            }
            Statement::Allocate(s) => {
                self.require_standard(CobolStandard::Cobol2002, "ALLOCATE", s.span);
            }
            Statement::Free(s) => {
                self.require_standard(CobolStandard::Cobol2002, "FREE", s.span);
            }
            Statement::JsonGenerate(s) => {
                self.require_standard(CobolStandard::Cobol2014, "JSON GENERATE", s.span);
                self.check_statements_standard(&s.on_exception);
                self.check_statements_standard(&s.not_on_exception);
            }
            Statement::JsonParse(s) => {
                self.require_standard(CobolStandard::Cobol2014, "JSON PARSE", s.span);
                self.check_statements_standard(&s.on_exception);
                self.check_statements_standard(&s.not_on_exception);
            }
            Statement::XmlGenerate(s) => {
                self.require_standard(CobolStandard::Cobol2014, "XML GENERATE", s.span);
                self.check_statements_standard(&s.on_exception);
                self.check_statements_standard(&s.not_on_exception);
            }
            Statement::XmlParse(s) => {
                self.require_standard(CobolStandard::Cobol2014, "XML PARSE", s.span);
                self.check_statements_standard(&s.on_exception);
                self.check_statements_standard(&s.not_on_exception);
            }
            Statement::Validate(s) => {
                self.require_standard(CobolStandard::Cobol2014, "VALIDATE", s.span);
            }
            Statement::If(s) => {
                self.check_statements_standard(&s.then_body);
                self.check_statements_standard(&s.else_body);
            }
            Statement::Evaluate(s) => {
                for clause in &s.when_clauses {
                    self.check_statements_standard(&clause.body);
                }
                self.check_statements_standard(&s.when_other);
            }
            Statement::Perform(s) => match &s.kind {
                cobol_ast::statement::PerformKind::Simple { body }
                | cobol_ast::statement::PerformKind::Times { body, .. }
                | cobol_ast::statement::PerformKind::Until { body, .. }
                | cobol_ast::statement::PerformKind::Varying { body, .. } => {
                    self.check_statements_standard(body);
                }
                cobol_ast::statement::PerformKind::ProcedureName { .. } => {}
            },
            Statement::Read(s) => {
                self.check_statements_standard(&s.at_end);
                self.check_statements_standard(&s.not_at_end);
                self.check_statements_standard(&s.invalid_key);
                self.check_statements_standard(&s.not_invalid_key);
            }
            Statement::Write(s) => {
                self.check_statements_standard(&s.invalid_key);
                self.check_statements_standard(&s.not_invalid_key);
                self.check_statements_standard(&s.at_eop);
                self.check_statements_standard(&s.not_at_eop);
            }
            Statement::Rewrite(s) => {
                self.check_statements_standard(&s.invalid_key);
                self.check_statements_standard(&s.not_invalid_key);
            }
            Statement::Delete(s) => {
                self.check_statements_standard(&s.invalid_key);
                self.check_statements_standard(&s.not_invalid_key);
            }
            Statement::Start(s) => {
                self.check_statements_standard(&s.invalid_key);
                self.check_statements_standard(&s.not_invalid_key);
            }
            Statement::Return(s) => {
                self.check_statements_standard(&s.at_end);
                self.check_statements_standard(&s.not_at_end);
            }
            Statement::String(s) => {
                self.check_statements_standard(&s.on_overflow);
                self.check_statements_standard(&s.not_on_overflow);
            }
            Statement::Unstring(s) => {
                self.check_statements_standard(&s.on_overflow);
                self.check_statements_standard(&s.not_on_overflow);
            }
            Statement::Search(s) => {
                self.check_statements_standard(&s.at_end);
                for when_clause in &s.when_clauses {
                    self.check_statements_standard(&when_clause.body);
                }
            }
            Statement::Compute(s) => {
                self.check_statements_standard(&s.on_size_error);
                self.check_statements_standard(&s.not_on_size_error);
            }
            Statement::Add(s) => {
                self.check_statements_standard(&s.on_size_error);
                self.check_statements_standard(&s.not_on_size_error);
            }
            Statement::Subtract(s) => {
                self.check_statements_standard(&s.on_size_error);
                self.check_statements_standard(&s.not_on_size_error);
            }
            Statement::Multiply(s) => {
                self.check_statements_standard(&s.on_size_error);
                self.check_statements_standard(&s.not_on_size_error);
            }
            Statement::Divide(s) => {
                self.check_statements_standard(&s.on_size_error);
                self.check_statements_standard(&s.not_on_size_error);
            }
            Statement::Call(s) => {
                for param in &s.using {
                    if param.mode == ParamMode::ByValue {
                        self.require_standard(CobolStandard::Cobol2002, "CALL BY VALUE", s.span);
                    }
                }
                if s.returning.is_some() {
                    self.require_standard(CobolStandard::Cobol2002, "CALL RETURNING", s.span);
                }
                self.check_statements_standard(&s.on_overflow);
                self.check_statements_standard(&s.on_exception);
                self.check_statements_standard(&s.not_on_exception);
            }
            _ => {}
        }
    }

    fn require_standard(
        &mut self,
        required: CobolStandard,
        feature: &str,
        span: cobol_common::Span,
    ) {
        if self.standard.allows(required) {
            return;
        }
        self.reporter.report(
            Diagnostic::error(
                "COBC-E090",
                format!(
                    "{feature} requires {} or later, but --standard {} was selected",
                    required.as_cli_str(),
                    self.standard.as_cli_str()
                ),
            )
            .with_label(
                span,
                "feature is not available in the selected COBOL standard",
            ),
        );
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
