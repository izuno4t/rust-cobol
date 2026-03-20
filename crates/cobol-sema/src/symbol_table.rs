// COBOL Semantic Analysis - Symbol table

use cobol_common::Span;
use smol_str::SmolStr;
use std::collections::HashMap;

/// A hierarchical symbol table for COBOL programs.
///
/// Manages nested scopes (program, section, paragraph) and supports
/// both simple and COBOL-style qualified name lookup (OF/IN).
#[derive(Debug, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    current_scope: usize,
}

/// A single scope in the symbol table hierarchy.
#[derive(Debug, Clone)]
pub struct Scope {
    pub name: SmolStr,
    pub kind: ScopeKind,
    pub symbols: HashMap<SmolStr, Vec<Symbol>>,
    pub parent: Option<usize>,
}

/// The kind of scope, determining namespace rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Program,
    Section,
    Paragraph,
}

/// A symbol defined in the COBOL program.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: SmolStr,
    pub kind: SymbolKind,
    pub data_type: Option<CobolType>,
    pub span: Span,
    /// Parent data name, used for COBOL qualified name resolution (OF/IN).
    pub parent_name: Option<SmolStr>,
}

/// Classification of a symbol entry.
#[derive(Debug, Clone)]
pub enum SymbolKind {
    DataItem {
        level: u8,
        is_group: bool,
    },
    FileDescription {
        file_name: SmolStr,
    },
    Paragraph,
    Section,
    Program,
    /// Level 88 condition name with associated values.
    ConditionName {
        values: Vec<String>,
    },
}

/// COBOL data types derived from PICTURE clauses and USAGE declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum CobolType {
    Alphabetic {
        size: u32,
    },
    Alphanumeric {
        size: u32,
    },
    Numeric {
        size: u32,
        decimal_places: u32,
        is_signed: bool,
    },
    NumericEdited {
        size: u32,
    },
    AlphanumericEdited {
        size: u32,
    },
    Group {
        size: u32,
    },
    Index,
    Pointer,
    Boolean,
    National {
        size: u32,
    },
    FloatShort,
    FloatLong,
    FloatExtended,
}

impl SymbolTable {
    /// Creates a new symbol table with an initial program-level scope.
    pub fn new() -> Self {
        let global_scope = Scope {
            name: SmolStr::new_static("<global>"),
            kind: ScopeKind::Program,
            symbols: HashMap::new(),
            parent: None,
        };
        Self {
            scopes: vec![global_scope],
            current_scope: 0,
        }
    }

    /// Enters a new scope, pushing it onto the scope stack.
    pub fn push_scope(&mut self, name: SmolStr, kind: ScopeKind) {
        let parent = Some(self.current_scope);
        let scope = Scope {
            name,
            kind,
            symbols: HashMap::new(),
            parent,
        };
        let idx = self.scopes.len();
        self.scopes.push(scope);
        self.current_scope = idx;
    }

    /// Leaves the current scope, returning to the parent.
    ///
    /// If already at the global scope, this is a no-op.
    pub fn pop_scope(&mut self) {
        if let Some(parent) = self.scopes[self.current_scope].parent {
            self.current_scope = parent;
        }
    }

    /// Defines a symbol in the current scope.
    pub fn define(&mut self, symbol: Symbol) {
        self.scopes[self.current_scope]
            .symbols
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol);
    }

    /// Looks up a symbol by name, searching from the current scope up to
    /// ancestor scopes. Returns the first match (or the unique one if only one exists).
    pub fn lookup(&self, name: &SmolStr) -> Option<&Symbol> {
        let mut scope_idx = self.current_scope;
        loop {
            if let Some(syms) = self.scopes[scope_idx].symbols.get(name) {
                if let Some(first) = syms.first() {
                    return Some(first);
                }
            }
            match self.scopes[scope_idx].parent {
                Some(parent) => scope_idx = parent,
                None => return None,
            }
        }
    }

    /// Performs COBOL qualified name lookup.
    ///
    /// In COBOL, names can be qualified with OF/IN, e.g. `FIELD OF RECORD`.
    /// The qualifiers list goes from innermost to outermost:
    /// `FIELD OF RECORD OF GROUP` => name="FIELD", qualifiers=["RECORD", "GROUP"].
    ///
    /// First finds all symbols matching `name` across all scopes, then
    /// filters by matching the qualifier chain against `parent_name`.
    pub fn lookup_qualified(&self, name: &SmolStr, qualifiers: &[SmolStr]) -> Option<&Symbol> {
        if qualifiers.is_empty() {
            return self.lookup(name);
        }

        // Collect all candidate symbols with the matching name from all scopes.
        let candidates: Vec<&Symbol> = self
            .scopes
            .iter()
            .filter_map(|scope| scope.symbols.get(name))
            .flat_map(|syms| syms.iter())
            .collect();

        // Find the candidate whose parent chain matches all qualifiers.
        candidates
            .into_iter()
            .find(|candidate| self.matches_qualifiers(candidate, qualifiers))
    }

    /// Returns a reference to the current scope.
    pub fn current_scope(&self) -> &Scope {
        &self.scopes[self.current_scope]
    }

    /// Checks whether a symbol's parent chain matches the given qualifier list.
    ///
    /// In COBOL, qualifiers do not need to be direct parents — they can be any
    /// ancestor.  For example, `FIELD OF GRANDPARENT` is valid even when
    /// FIELD's immediate parent is PARENT (a child of GRANDPARENT).  We walk
    /// up the ancestor chain for each qualifier, searching for a match at any
    /// level.
    fn matches_qualifiers(&self, symbol: &Symbol, qualifiers: &[SmolStr]) -> bool {
        let mut current_parent = &symbol.parent_name;
        for qualifier in qualifiers {
            // Walk up ancestors looking for this qualifier.
            let mut found = false;
            let mut limit = 50; // Safety limit against cycles
            loop {
                if limit == 0 {
                    break;
                }
                limit -= 1;
                match current_parent {
                    Some(parent_name) => {
                        if parent_name.eq_ignore_ascii_case(qualifier) {
                            // Found the qualifier in the ancestor chain.
                            // Move past it for the next qualifier.
                            if let Some(parent_sym) = self.find_symbol_anywhere(parent_name) {
                                current_parent = &parent_sym.parent_name;
                            } else {
                                current_parent = &None;
                            }
                            found = true;
                            break;
                        }
                        // Not a match at this level — move up.
                        if let Some(parent_sym) = self.find_symbol_anywhere(parent_name) {
                            current_parent = &parent_sym.parent_name;
                        } else {
                            current_parent = &None;
                        }
                    }
                    None => break,
                }
            }
            if !found {
                return false;
            }
        }
        true
    }

    /// Searches all scopes for a symbol with the given name.
    // TODO: Normalize symbol names to uppercase at registration time to avoid 2-pass lookup
    fn find_symbol_anywhere(&self, name: &SmolStr) -> Option<&Symbol> {
        for scope in &self.scopes {
            if let Some(syms) = scope.symbols.get(name) {
                if let Some(first) = syms.first() {
                    return Some(first);
                }
            }
        }
        // Fall back to case-insensitive search.
        let upper = name.to_ascii_uppercase();
        for scope in &self.scopes {
            for (key, syms) in &scope.symbols {
                if key.to_ascii_uppercase() == upper {
                    if let Some(first) = syms.first() {
                        return Some(first);
                    }
                }
            }
        }
        None
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl CobolType {
    /// Returns true if this type is numeric (can be used in arithmetic).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            CobolType::Numeric { .. }
                | CobolType::FloatShort
                | CobolType::FloatLong
                | CobolType::FloatExtended
                | CobolType::Index
        )
    }

    /// Returns true if this type is alphanumeric.
    pub fn is_alphanumeric(&self) -> bool {
        matches!(
            self,
            CobolType::Alphanumeric { .. } | CobolType::AlphanumericEdited { .. }
        )
    }

    /// Returns true if this type is alphabetic.
    pub fn is_alphabetic(&self) -> bool {
        matches!(self, CobolType::Alphabetic { .. })
    }

    /// Returns true if this type is a group item.
    pub fn is_group(&self) -> bool {
        matches!(self, CobolType::Group { .. })
    }

    /// Returns a human-readable name for this type.
    pub fn display_name(&self) -> &'static str {
        match self {
            CobolType::Alphabetic { .. } => "ALPHABETIC",
            CobolType::Alphanumeric { .. } => "ALPHANUMERIC",
            CobolType::Numeric { .. } => "NUMERIC",
            CobolType::NumericEdited { .. } => "NUMERIC-EDITED",
            CobolType::AlphanumericEdited { .. } => "ALPHANUMERIC-EDITED",
            CobolType::Group { .. } => "GROUP",
            CobolType::Index => "INDEX",
            CobolType::Pointer => "POINTER",
            CobolType::Boolean => "BOOLEAN",
            CobolType::National { .. } => "NATIONAL",
            CobolType::FloatShort => "FLOAT-SHORT",
            CobolType::FloatLong => "FLOAT-LONG",
            CobolType::FloatExtended => "FLOAT-EXTENDED",
        }
    }

    /// Returns a detailed human-readable description including PIC-like information.
    ///
    /// Example: "NUMERIC PIC 9(5)" or "ALPHANUMERIC PIC X(10)".
    pub fn detail_name(&self) -> String {
        match self {
            CobolType::Alphabetic { size } => format!("ALPHABETIC PIC A({})", size),
            CobolType::Alphanumeric { size } => format!("ALPHANUMERIC PIC X({})", size),
            CobolType::Numeric {
                size,
                decimal_places,
                is_signed,
            } => {
                let sign = if *is_signed { "S" } else { "" };
                if *decimal_places > 0 {
                    let integer_digits = size.saturating_sub(*decimal_places);
                    format!(
                        "NUMERIC PIC {}9({})V9({})",
                        sign, integer_digits, decimal_places
                    )
                } else {
                    format!("NUMERIC PIC {}9({})", sign, size)
                }
            }
            CobolType::NumericEdited { size } => {
                format!("NUMERIC-EDITED (size {})", size)
            }
            CobolType::AlphanumericEdited { size } => {
                format!("ALPHANUMERIC-EDITED (size {})", size)
            }
            CobolType::Group { size } => format!("GROUP (size {})", size),
            CobolType::Index => "INDEX".to_string(),
            CobolType::Pointer => "POINTER".to_string(),
            CobolType::Boolean => "BOOLEAN".to_string(),
            CobolType::National { size } => format!("NATIONAL PIC N({})", size),
            CobolType::FloatShort => "FLOAT-SHORT (COMP-1)".to_string(),
            CobolType::FloatLong => "FLOAT-LONG (COMP-2)".to_string(),
            CobolType::FloatExtended => "FLOAT-EXTENDED".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::span::FileId;

    fn make_symbol(name: &str, parent: Option<&str>) -> Symbol {
        Symbol {
            name: SmolStr::new(name),
            kind: SymbolKind::DataItem {
                level: 5,
                is_group: false,
            },
            data_type: Some(CobolType::Alphanumeric { size: 10 }),
            span: Span::new(0, 0, FileId(0)),
            parent_name: parent.map(SmolStr::new),
        }
    }

    #[test]
    fn test_define_and_lookup() {
        let mut table = SymbolTable::new();
        table.define(make_symbol("WS-NAME", None));

        assert!(table.lookup(&SmolStr::new("WS-NAME")).is_some());
        assert!(table.lookup(&SmolStr::new("WS-MISSING")).is_none());
    }

    #[test]
    fn test_scope_push_pop() {
        let mut table = SymbolTable::new();
        table.define(make_symbol("GLOBAL-VAR", None));

        table.push_scope(SmolStr::new("SECTION-1"), ScopeKind::Section);
        table.define(make_symbol("LOCAL-VAR", None));

        // Both should be visible from the child scope.
        assert!(table.lookup(&SmolStr::new("GLOBAL-VAR")).is_some());
        assert!(table.lookup(&SmolStr::new("LOCAL-VAR")).is_some());

        table.pop_scope();

        // Back in global scope: only global visible.
        assert!(table.lookup(&SmolStr::new("GLOBAL-VAR")).is_some());
        assert!(table.lookup(&SmolStr::new("LOCAL-VAR")).is_none());
    }

    #[test]
    fn test_qualified_lookup() {
        let mut table = SymbolTable::new();

        // Define: FIELD OF RECORD
        table.define(Symbol {
            name: SmolStr::new("RECORD"),
            kind: SymbolKind::DataItem {
                level: 1,
                is_group: true,
            },
            data_type: Some(CobolType::Group { size: 20 }),
            span: Span::new(0, 0, FileId(0)),
            parent_name: None,
        });
        table.define(Symbol {
            name: SmolStr::new("FIELD"),
            kind: SymbolKind::DataItem {
                level: 5,
                is_group: false,
            },
            data_type: Some(CobolType::Alphanumeric { size: 10 }),
            span: Span::new(0, 0, FileId(0)),
            parent_name: Some(SmolStr::new("RECORD")),
        });

        let result = table.lookup_qualified(&SmolStr::new("FIELD"), &[SmolStr::new("RECORD")]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().name.as_str(), "FIELD");

        // Wrong qualifier should fail.
        let result = table.lookup_qualified(&SmolStr::new("FIELD"), &[SmolStr::new("WRONG")]);
        assert!(result.is_none());
    }

    #[test]
    fn test_current_scope() {
        let mut table = SymbolTable::new();
        assert_eq!(table.current_scope().kind, ScopeKind::Program);

        table.push_scope(SmolStr::new("SEC"), ScopeKind::Section);
        assert_eq!(table.current_scope().kind, ScopeKind::Section);

        table.pop_scope();
        assert_eq!(table.current_scope().kind, ScopeKind::Program);
    }

    #[test]
    fn test_cobol_type_is_numeric() {
        assert!(CobolType::Numeric {
            size: 5,
            decimal_places: 0,
            is_signed: false
        }
        .is_numeric());
        assert!(CobolType::FloatShort.is_numeric());
        assert!(CobolType::Index.is_numeric());
        assert!(!CobolType::Alphanumeric { size: 10 }.is_numeric());
    }

    #[test]
    fn test_detail_name_numeric() {
        let t = CobolType::Numeric {
            size: 5,
            decimal_places: 0,
            is_signed: false,
        };
        assert_eq!(t.detail_name(), "NUMERIC PIC 9(5)");
    }

    #[test]
    fn test_detail_name_signed_decimal() {
        let t = CobolType::Numeric {
            size: 9,
            decimal_places: 2,
            is_signed: true,
        };
        assert_eq!(t.detail_name(), "NUMERIC PIC S9(7)V9(2)");
    }

    #[test]
    fn test_detail_name_alphanumeric() {
        let t = CobolType::Alphanumeric { size: 10 };
        assert_eq!(t.detail_name(), "ALPHANUMERIC PIC X(10)");
    }

    #[test]
    fn test_detail_name_group() {
        let t = CobolType::Group { size: 0 };
        assert_eq!(t.detail_name(), "GROUP (size 0)");
    }

    #[test]
    fn test_detail_name_float() {
        assert_eq!(CobolType::FloatShort.detail_name(), "FLOAT-SHORT (COMP-1)");
        assert_eq!(CobolType::FloatLong.detail_name(), "FLOAT-LONG (COMP-2)");
    }
}
