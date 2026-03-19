// COBOL Semantic Analysis - Main analysis driver
//
// Orchestrates symbol table construction, name resolution,
// and type checking for a parsed COBOL program.

use cobol_ast::CobolProgram;
use cobol_diagnostics::{DiagnosticReporter, WarningLevel};

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
}

impl SemanticAnalyzer {
    /// Creates a new semantic analyzer with an empty symbol table.
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            reporter: DiagnosticReporter::new(),
        }
    }

    /// Creates a new semantic analyzer with a specific warning level.
    pub fn with_warning_level(warning_level: WarningLevel) -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            reporter: DiagnosticReporter::with_warning_level(warning_level),
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
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
