# COBOL Compiler Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** COBOL-85〜COBOL 2023対応の本番環境向けCOBOLコンパイラをRust + LLVMで構築する

**Architecture:** Cargo workspaceによるモノリシック・パイプライン方式。字句解析→構文解析→意味解析→HIR→MIR→LLVM IR→ネイティブコードの段階的変換。各フェーズを独立crateとして分離し、テスト容易性とモジュール性を確保。

**Tech Stack:** Rust, LLVM (inkwell), ariadne, clap, serde, num-bigint, rust_decimal

**Design Doc:** `docs/plans/2026-02-28-cobol-compiler-design.md`

---

## Phase Overview

| Phase | Description | Tasks |
|-------|-------------|-------|
| 1 | 基盤構築（workspace, common, diagnostics） | 1-5 |
| 2 | レキサー（固定/フリーフォーマット） | 6-12 |
| 3 | AST定義 | 13-16 |
| 4 | パーサー（基本COBOL-85構文） | 17-25 |
| 5 | 意味解析（基本） | 26-30 |
| 6 | HIR/MIR中間表現 | 31-35 |
| 7 | LLVMコード生成 | 36-41 |
| 8 | ランタイムライブラリ（コア） | 42-48 |
| 9 | CLIドライバ・統合 | 49-52 |
| 10 | COBOL-85完全対応 | 53-60 |
| 11 | COBOL 2002拡張 | 61-67 |
| 12 | COBOL 2014拡張 | 68-72 |
| 13 | COBOL 2023拡張 | 73-80 |

---

## Phase 1: 基盤構築

### Task 1: Cargo Workspace初期化

**Files:**
- Create: `Cargo.toml`
- Create: `crates/cobol-common/Cargo.toml`
- Create: `crates/cobol-common/src/lib.rs`
- Create: `crates/cobol-diagnostics/Cargo.toml`
- Create: `crates/cobol-diagnostics/src/lib.rs`
- Create: `crates/cobol-lexer/Cargo.toml`
- Create: `crates/cobol-lexer/src/lib.rs`
- Create: `crates/cobol-ast/Cargo.toml`
- Create: `crates/cobol-ast/src/lib.rs`
- Create: `crates/cobol-parser/Cargo.toml`
- Create: `crates/cobol-parser/src/lib.rs`
- Create: `crates/cobol-sema/Cargo.toml`
- Create: `crates/cobol-sema/src/lib.rs`
- Create: `crates/cobol-hir/Cargo.toml`
- Create: `crates/cobol-hir/src/lib.rs`
- Create: `crates/cobol-mir/Cargo.toml`
- Create: `crates/cobol-mir/src/lib.rs`
- Create: `crates/cobol-codegen/Cargo.toml`
- Create: `crates/cobol-codegen/src/lib.rs`
- Create: `crates/cobol-runtime/Cargo.toml`
- Create: `crates/cobol-runtime/src/lib.rs`
- Create: `crates/cobol-driver/Cargo.toml`
- Create: `crates/cobol-driver/src/main.rs`
- Create: `.gitignore`

**Step 1: Create workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/cobol-common",
    "crates/cobol-diagnostics",
    "crates/cobol-lexer",
    "crates/cobol-ast",
    "crates/cobol-parser",
    "crates/cobol-sema",
    "crates/cobol-hir",
    "crates/cobol-mir",
    "crates/cobol-codegen",
    "crates/cobol-runtime",
    "crates/cobol-driver",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
# Internal crates
cobol-common = { path = "crates/cobol-common" }
cobol-diagnostics = { path = "crates/cobol-diagnostics" }
cobol-lexer = { path = "crates/cobol-lexer" }
cobol-ast = { path = "crates/cobol-ast" }
cobol-parser = { path = "crates/cobol-parser" }
cobol-sema = { path = "crates/cobol-sema" }
cobol-hir = { path = "crates/cobol-hir" }
cobol-mir = { path = "crates/cobol-mir" }
cobol-codegen = { path = "crates/cobol-codegen" }
cobol-runtime = { path = "crates/cobol-runtime" }

# External dependencies
ariadne = "0.4"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
num-bigint = "0.4"
rust_decimal = "1"
unicode-segmentation = "1"
thiserror = "2"
smol_str = "0.3"
tempfile = "3"
```

**Step 2: Create each crate's Cargo.toml and initial lib.rs/main.rs**

Each crate gets a minimal Cargo.toml with `package.version.workspace = true` and `package.edition.workspace = true`, and an empty `src/lib.rs` (or `src/main.rs` for cobol-driver).

Key dependencies per crate:
- `cobol-common`: `thiserror`, `smol_str`, `serde`
- `cobol-diagnostics`: `cobol-common`, `ariadne`
- `cobol-lexer`: `cobol-common`, `cobol-diagnostics`, `smol_str`
- `cobol-ast`: `cobol-common`, `smol_str`, `serde`
- `cobol-parser`: `cobol-common`, `cobol-diagnostics`, `cobol-lexer`, `cobol-ast`
- `cobol-sema`: `cobol-common`, `cobol-diagnostics`, `cobol-ast`
- `cobol-hir`: `cobol-common`, `cobol-ast`
- `cobol-mir`: `cobol-common`, `cobol-hir`
- `cobol-codegen`: `cobol-common`, `cobol-mir`, `cobol-runtime` (inkwell added later)
- `cobol-runtime`: `cobol-common`, `num-bigint`, `rust_decimal`
- `cobol-driver`: `cobol-common`, `cobol-diagnostics`, `cobol-lexer`, `cobol-parser`, `cobol-sema`, `cobol-hir`, `cobol-mir`, `cobol-codegen`, `cobol-runtime`, `clap`

**Step 3: Create .gitignore**

```
/target
**/*.rs.bk
Cargo.lock
```

**Step 4: Build and verify**

Run: `cargo build`
Expected: Successful build with no errors

**Step 5: Initialize git and commit**

```bash
git init
git add -A
git commit -m "chore: initialize cargo workspace with all crates"
```

---

### Task 2: cobol-common — 共通型定義

**Files:**
- Create: `crates/cobol-common/src/span.rs`
- Create: `crates/cobol-common/src/source.rs`
- Create: `crates/cobol-common/src/cobol_standard.rs`
- Create: `crates/cobol-common/src/source_format.rs`
- Modify: `crates/cobol-common/src/lib.rs`
- Test: `crates/cobol-common/src/span.rs` (inline tests)

**Step 1: Write tests for Span**

```rust
// crates/cobol-common/src/span.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(0, 10, FileId(0));
        assert_eq!(span.start, 0);
        assert_eq!(span.end, 10);
        assert_eq!(span.file_id, FileId(0));
    }

    #[test]
    fn test_span_merge() {
        let a = Span::new(0, 5, FileId(0));
        let b = Span::new(8, 15, FileId(0));
        let merged = a.merge(b);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 15);
    }

    #[test]
    fn test_span_len() {
        let span = Span::new(5, 10, FileId(0));
        assert_eq!(span.len(), 5);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p cobol-common`
Expected: FAIL — Span not defined

**Step 3: Implement Span, FileId, source types**

```rust
// crates/cobol-common/src/span.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub file_id: FileId,
}

impl Span {
    pub fn new(start: u32, end: u32, file_id: FileId) -> Self {
        Self { start, end, file_id }
    }

    pub fn dummy() -> Self {
        Self { start: 0, end: 0, file_id: FileId(0) }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            file_id: self.file_id,
        }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
```

```rust
// crates/cobol-common/src/source.rs
use crate::span::FileId;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub name: String,
    pub content: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    pub fn new(id: FileId, name: String, content: String) -> Self {
        let line_starts = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| (i + 1) as u32))
            .collect();
        Self { id, name, content, line_starts }
    }

    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let col = offset - self.line_starts[line];
        (line as u32 + 1, col + 1)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}
```

```rust
// crates/cobol-common/src/cobol_standard.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CobolStandard {
    Cobol85,
    Cobol2002,
    Cobol2014,
    Cobol2023,
}

impl Default for CobolStandard {
    fn default() -> Self {
        Self::Cobol2023
    }
}
```

```rust
// crates/cobol-common/src/source_format.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceFormat {
    Fixed,
    Free,
    Variable,
}

impl Default for SourceFormat {
    fn default() -> Self {
        Self::Fixed
    }
}
```

```rust
// crates/cobol-common/src/lib.rs
pub mod span;
pub mod source;
pub mod cobol_standard;
pub mod source_format;

pub use span::{Span, FileId};
pub use source::SourceFile;
pub use cobol_standard::CobolStandard;
pub use source_format::SourceFormat;
```

**Step 4: Run tests**

Run: `cargo test -p cobol-common`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add crates/cobol-common/
git commit -m "feat(common): add Span, FileId, SourceFile, CobolStandard, SourceFormat"
```

---

### Task 3: cobol-diagnostics — エラー報告基盤

**Files:**
- Create: `crates/cobol-diagnostics/src/diagnostic.rs`
- Create: `crates/cobol-diagnostics/src/reporter.rs`
- Modify: `crates/cobol-diagnostics/src/lib.rs`
- Test: `crates/cobol-diagnostics/src/diagnostic.rs` (inline tests)

**Step 1: Write tests for Diagnostic**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::{Span, FileId};

    #[test]
    fn test_error_creation() {
        let diag = Diagnostic::error("E0001", "Undefined data name 'WS-NAME'")
            .with_span(Span::new(10, 17, FileId(0)));
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, "E0001");
    }

    #[test]
    fn test_warning_creation() {
        let diag = Diagnostic::warning("W0001", "Unused data item 'WS-UNUSED'");
        assert_eq!(diag.severity, Severity::Warning);
    }

    #[test]
    fn test_diagnostic_with_note() {
        let diag = Diagnostic::error("E0002", "Type mismatch")
            .with_note("Expected PIC 9(5), found PIC X(10)");
        assert_eq!(diag.notes.len(), 1);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p cobol-diagnostics`
Expected: FAIL

**Step 3: Implement Diagnostic, Severity, DiagnosticReporter**

```rust
// crates/cobol-diagnostics/src/diagnostic.rs
use cobol_common::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: &str) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_string(),
            message: message.to_string(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn warning(code: &str, message: &str) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_string(),
            message: message.to_string(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn info(code: &str, message: &str) -> Self {
        Self {
            severity: Severity::Info,
            code: code.to_string(),
            message: message.to_string(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.labels.push(Label {
            span,
            message: String::new(),
        });
        self
    }

    pub fn with_label(mut self, span: Span, message: &str) -> Self {
        self.labels.push(Label {
            span,
            message: message.to_string(),
        });
        self
    }

    pub fn with_note(mut self, note: &str) -> Self {
        self.notes.push(note.to_string());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
```

```rust
// crates/cobol-diagnostics/src/reporter.rs
use crate::diagnostic::{Diagnostic, Severity};

#[derive(Debug, Default)]
pub struct DiagnosticReporter {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Warning).count()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p cobol-diagnostics`
Expected: All tests PASS

**Step 5: Commit**

```bash
git add crates/cobol-diagnostics/
git commit -m "feat(diagnostics): add Diagnostic, Severity, DiagnosticReporter"
```

---

### Task 4: cobol-common — SourceMap（ファイル管理）

**Files:**
- Create: `crates/cobol-common/src/source_map.rs`
- Modify: `crates/cobol-common/src/lib.rs`
- Test: `crates/cobol-common/src/source_map.rs` (inline tests)

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_file() {
        let mut sm = SourceMap::new();
        let id = sm.add_file("test.cob".to_string(), "IDENTIFICATION DIVISION.".to_string());
        let file = sm.get_file(id).unwrap();
        assert_eq!(file.name, "test.cob");
    }

    #[test]
    fn test_multiple_files() {
        let mut sm = SourceMap::new();
        let id1 = sm.add_file("a.cob".to_string(), "A".to_string());
        let id2 = sm.add_file("b.cob".to_string(), "B".to_string());
        assert_ne!(id1, id2);
        assert_eq!(sm.get_file(id1).unwrap().name, "a.cob");
        assert_eq!(sm.get_file(id2).unwrap().name, "b.cob");
    }
}
```

**Step 2: Run test, verify fail**

Run: `cargo test -p cobol-common`
Expected: FAIL

**Step 3: Implement SourceMap**

```rust
// crates/cobol-common/src/source_map.rs
use crate::source::SourceFile;
use crate::span::FileId;

#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, name: String, content: String) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile::new(id, name, content));
        id
    }

    pub fn get_file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p cobol-common`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cobol-common/
git commit -m "feat(common): add SourceMap for multi-file management"
```

---

### Task 5: git初期化とCI設定

**Files:**
- Create: `rust-toolchain.toml`
- Create: `.github/workflows/ci.yml` (optional)
- Create: `rustfmt.toml`
- Create: `clippy.toml`

**Step 1: Create rust-toolchain.toml**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

**Step 2: Create rustfmt.toml**

```toml
edition = "2021"
max_width = 100
```

**Step 3: Verify formatting and lints**

Run: `cargo fmt --check && cargo clippy -- -D warnings`
Expected: No issues

**Step 4: Commit**

```bash
git add rust-toolchain.toml rustfmt.toml
git commit -m "chore: add rust-toolchain, rustfmt config"
```

---

## Phase 2: レキサー

### Task 6: Token定義

**Files:**
- Create: `crates/cobol-lexer/src/token.rs`
- Modify: `crates/cobol-lexer/src/lib.rs`
- Test: `crates/cobol-lexer/src/token.rs` (inline tests)

**Step 1: Write tests for Token**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::{Span, FileId};

    #[test]
    fn test_token_creation() {
        let token = Token {
            kind: TokenKind::Identifier,
            text: "WS-NAME".into(),
            span: Span::new(0, 7, FileId(0)),
        };
        assert_eq!(token.kind, TokenKind::Identifier);
        assert_eq!(token.text, "WS-NAME");
    }

    #[test]
    fn test_keyword_lookup() {
        assert_eq!(TokenKind::from_keyword("IDENTIFICATION"), Some(TokenKind::Identification));
        assert_eq!(TokenKind::from_keyword("DIVISION"), Some(TokenKind::Division));
        assert_eq!(TokenKind::from_keyword("NOTAKEYWORD"), None);
    }

    #[test]
    fn test_keyword_case_insensitive() {
        assert_eq!(TokenKind::from_keyword("identification"), Some(TokenKind::Identification));
        assert_eq!(TokenKind::from_keyword("Division"), Some(TokenKind::Division));
    }
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement Token and TokenKind**

TokenKind should include all COBOL reserved words (300+ for COBOL-85, growing to 500+ for COBOL 2023). Start with the core set:

```rust
// crates/cobol-lexer/src/token.rs
use cobol_common::Span;
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: SmolStr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Literals
    IntegerLiteral,
    DecimalLiteral,
    StringLiteral,
    HexLiteral,
    BooleanLiteral,
    NationalLiteral,

    // Identifiers
    Identifier,

    // Division keywords
    Identification,
    Environment,
    Data,
    Procedure,
    Division,
    Section,

    // IDENTIFICATION DIVISION
    ProgramId,
    AuthorKw,      // Avoiding collision with "Author" if needed
    DateWritten,
    DateCompiled,
    Installation,
    Security,
    ClassId,
    MethodId,
    InterfaceId,
    FunctionId,
    FactoryKw,
    ObjectKw,

    // ENVIRONMENT DIVISION
    Configuration,
    SourceComputer,
    ObjectComputer,
    SpecialNames,
    Repository,
    InputOutput,
    FileControl,
    IoControl,
    Select,
    Assign,
    Organization,
    Sequential,
    Indexed,
    Relative,
    AccessMode,
    Dynamic,
    Random,
    RecordKey,
    AlternateRecordKey,
    FileStatus,

    // DATA DIVISION
    File,
    WorkingStorage,
    LocalStorage,
    Linkage,
    Communication,
    Report,
    Screen,
    Fd,
    Sd,
    Pic,
    Picture,
    Value,
    Values,
    Redefines,
    Renames,
    Occurs,
    Times,
    Depending,
    Ascending,
    Descending,
    Key,
    Usage,
    Display,
    Computational,
    Comp,
    Comp1,
    Comp2,
    Comp3,
    Comp4,
    Comp5,
    Binary,
    PackedDecimal,
    Index,
    Pointer,
    FunctionPointer,
    SignKw,
    Leading,
    Trailing,
    Separate,
    Justified,
    Blank,
    When,
    Zero,
    Zeroes,
    Zeros,
    Space,
    Spaces,
    HighValue,
    HighValues,
    LowValue,
    LowValues,
    Quote,
    Quotes,
    All,
    Filler,
    External,
    Global,
    GroupUsage,
    National,
    Typedef,

    // PROCEDURE DIVISION keywords
    Move,
    To,
    Add,
    Subtract,
    Multiply,
    Divide,
    Compute,
    Giving,
    Remainder,
    Rounded,
    SizeError,
    OnSizeError,
    NotOnSizeError,
    Corresponding,
    Corr,
    If,
    Then,
    Else,
    EndIf,
    Evaluate,
    Also,
    TrueKw,
    FalseKw,
    Other,
    EndEvaluate,
    Perform,
    Thru,
    Through,
    Varying,
    Until,
    After,
    EndPerform,
    Go,
    GoTo,
    Call,
    Using,
    By,
    Reference,
    Content,
    Returning,
    EndCall,
    Cancel,
    Stop,
    Run,
    Accept,
    From,
    EndAccept,
    EndDisplay,
    Open,
    Close,
    Read,
    Write,
    Rewrite,
    Delete,
    Start,
    Return,
    EndRead,
    EndWrite,
    EndRewrite,
    EndDelete,
    EndStart,
    EndReturn,
    Input,
    Output,
    IoMode,
    Extend,
    Into,
    At,
    End,
    NotAtEnd,
    InvalidKey,
    NotInvalidKey,
    With,
    Lock,
    NoLock,
    String,
    Unstring,
    Delimited,
    Delimiter,
    Count,
    Overflow,
    NotOnOverflow,
    EndString,
    EndUnstring,
    Inspect,
    Tallying,
    Replacing,
    Converting,
    Before,
    Initial,
    Merge,
    Sort,
    OnKw,
    Release,
    Set,
    Up,
    Down,
    Continue,
    Exit,
    Program,
    Method,
    Goback,
    Initialize,
    Initiate,
    Terminate,
    Generate,
    Suppress,
    Raise,
    Resume,
    Allocate,
    Free,
    Validate,
    Invoke,
    New,
    Self_,
    Super,
    Null,
    Nulls,

    // Intrinsic function keyword
    Function,

    // Conditional
    Not,
    And,
    Or,
    Greater,
    Less,
    Equal,
    Than,
    Numeric,
    Alphabetic,
    AlphabeticLower,
    AlphabeticUpper,
    Positive,
    Negative,
    ClassKw,

    // File I/O
    Line,
    Lines,
    Page,
    Advancing,
    Recording,
    Mode,
    Standard1,
    Standard2,
    Block,
    Contains,
    Records,
    Record,
    Characters,
    Label,
    Omitted,
    Data,  // Note: also used in DATA DIVISION
    Linage,
    Footing,
    Top,
    Bottom,
    CodeSet,
    ReportKw,
    Padding,
    Character,

    // COPY/REPLACE
    Copy,
    Replace,
    Replacing,  // (duplicate — same as above)
    Off,
    Suppress_,
    In,
    Of,

    // COBOL 2002+ OOP
    Class,
    Inherits,
    Property,
    Get,
    SetKw,

    // COBOL 2014/2023
    Json,
    Xml,
    Parse,
    FloatShort,
    FloatLong,
    FloatExtended,

    // Operators and punctuation
    Period,           // .
    Comma,            // ,
    Semicolon,        // ;
    LeftParen,        // (
    RightParen,       // )
    Colon,            // :
    Plus,             // +
    Minus,            // -
    Star,             // *
    Slash,            // /
    DoubleStar,       // **
    Equals,           // =
    GreaterThan,      // >
    LessThan,         // <
    GreaterEqual,     // >=
    LessEqual,        // <=
    NotEqual,         // <>  or NOT =
    DoubleColon,      // ::
    EqualGreater,     // =>

    // Special
    LevelNumber,      // 01-49, 66, 77, 88
    Eof,
    Newline,
    Error,

    // Compiler directives
    CompilerDirective,
}

impl TokenKind {
    pub fn from_keyword(word: &str) -> Option<TokenKind> {
        let upper = word.to_uppercase();
        match upper.as_str() {
            "IDENTIFICATION" => Some(Self::Identification),
            "ENVIRONMENT" => Some(Self::Environment),
            "DATA" => Some(Self::Data),
            "PROCEDURE" => Some(Self::Procedure),
            "DIVISION" => Some(Self::Division),
            "SECTION" => Some(Self::Section),
            "PROGRAM-ID" => Some(Self::ProgramId),
            "WORKING-STORAGE" => Some(Self::WorkingStorage),
            "LOCAL-STORAGE" => Some(Self::LocalStorage),
            "LINKAGE" => Some(Self::Linkage),
            "FILE" => Some(Self::File),
            "SCREEN" => Some(Self::Screen),
            "FD" => Some(Self::Fd),
            "SD" => Some(Self::Sd),
            "PIC" | "PICTURE" => Some(Self::Pic),
            "VALUE" => Some(Self::Value),
            "MOVE" => Some(Self::Move),
            "TO" => Some(Self::To),
            "ADD" => Some(Self::Add),
            "SUBTRACT" => Some(Self::Subtract),
            "MULTIPLY" => Some(Self::Multiply),
            "DIVIDE" => Some(Self::Divide),
            "COMPUTE" => Some(Self::Compute),
            "IF" => Some(Self::If),
            "ELSE" => Some(Self::Else),
            "END-IF" => Some(Self::EndIf),
            "PERFORM" => Some(Self::Perform),
            "END-PERFORM" => Some(Self::EndPerform),
            "EVALUATE" => Some(Self::Evaluate),
            "END-EVALUATE" => Some(Self::EndEvaluate),
            "DISPLAY" => Some(Self::Display),
            "ACCEPT" => Some(Self::Accept),
            "STOP" => Some(Self::Stop),
            "RUN" => Some(Self::Run),
            "CALL" => Some(Self::Call),
            "END-CALL" => Some(Self::EndCall),
            "OPEN" => Some(Self::Open),
            "CLOSE" => Some(Self::Close),
            "READ" => Some(Self::Read),
            "WRITE" => Some(Self::Write),
            "COPY" => Some(Self::Copy),
            "REPLACE" => Some(Self::Replace),
            "GIVING" => Some(Self::Giving),
            "USING" => Some(Self::Using),
            "RETURNING" => Some(Self::Returning),
            "BY" => Some(Self::By),
            "REFERENCE" => Some(Self::Reference),
            "CONTENT" => Some(Self::Content),
            "THRU" | "THROUGH" => Some(Self::Thru),
            "VARYING" => Some(Self::Varying),
            "UNTIL" => Some(Self::Until),
            "AFTER" => Some(Self::After),
            "NOT" => Some(Self::Not),
            "AND" => Some(Self::And),
            "OR" => Some(Self::Or),
            "GREATER" => Some(Self::Greater),
            "LESS" => Some(Self::Less),
            "EQUAL" => Some(Self::Equal),
            "THAN" => Some(Self::Than),
            "ALSO" => Some(Self::Also),
            "WHEN" => Some(Self::When),
            "OTHER" => Some(Self::Other),
            "TRUE" => Some(Self::TrueKw),
            "FALSE" => Some(Self::FalseKw),
            "ZERO" | "ZEROS" | "ZEROES" => Some(Self::Zero),
            "SPACE" | "SPACES" => Some(Self::Space),
            "HIGH-VALUE" | "HIGH-VALUES" => Some(Self::HighValue),
            "LOW-VALUE" | "LOW-VALUES" => Some(Self::LowValue),
            "QUOTE" | "QUOTES" => Some(Self::Quote),
            "ALL" => Some(Self::All),
            "CORRESPONDING" | "CORR" => Some(Self::Corresponding),
            "REDEFINES" => Some(Self::Redefines),
            "RENAMES" => Some(Self::Renames),
            "OCCURS" => Some(Self::Occurs),
            "TIMES" => Some(Self::Times),
            "DEPENDING" => Some(Self::Depending),
            "ASCENDING" => Some(Self::Ascending),
            "DESCENDING" => Some(Self::Descending),
            "KEY" => Some(Self::Key),
            "USAGE" => Some(Self::Usage),
            "COMP" | "COMPUTATIONAL" => Some(Self::Comp),
            "COMP-1" => Some(Self::Comp1),
            "COMP-2" => Some(Self::Comp2),
            "COMP-3" => Some(Self::Comp3),
            "COMP-4" => Some(Self::Comp4),
            "COMP-5" => Some(Self::Comp5),
            "BINARY" => Some(Self::Binary),
            "PACKED-DECIMAL" => Some(Self::PackedDecimal),
            "INDEX" => Some(Self::Index),
            "POINTER" => Some(Self::Pointer),
            "SIGN" => Some(Self::SignKw),
            "LEADING" => Some(Self::Leading),
            "TRAILING" => Some(Self::Trailing),
            "SEPARATE" => Some(Self::Separate),
            "FILLER" => Some(Self::Filler),
            "EXTERNAL" => Some(Self::External),
            "GLOBAL" => Some(Self::Global),
            "FUNCTION" => Some(Self::Function),
            "CONTINUE" => Some(Self::Continue),
            "EXIT" => Some(Self::Exit),
            "PROGRAM" => Some(Self::Program),
            "GOBACK" => Some(Self::Goback),
            "GO" => Some(Self::Go),
            "INITIALIZE" => Some(Self::Initialize),
            "STRING" => Some(Self::String),
            "UNSTRING" => Some(Self::Unstring),
            "INSPECT" => Some(Self::Inspect),
            "TALLYING" => Some(Self::Tallying),
            "REPLACING" => Some(Self::Replacing),
            "CONVERTING" => Some(Self::Converting),
            "BEFORE" => Some(Self::Before),
            "INITIAL" => Some(Self::Initial),
            "SORT" => Some(Self::Sort),
            "MERGE" => Some(Self::Merge),
            "RELEASE" => Some(Self::Release),
            "RETURN" => Some(Self::Return),
            "SET" => Some(Self::Set),
            "UP" => Some(Self::Up),
            "DOWN" => Some(Self::Down),
            "SELECT" => Some(Self::Select),
            "ASSIGN" => Some(Self::Assign),
            "ORGANIZATION" => Some(Self::Organization),
            "SEQUENTIAL" => Some(Self::Sequential),
            "INDEXED" => Some(Self::Indexed),
            "RELATIVE" => Some(Self::Relative),
            "ACCESS" => Some(Self::AccessMode),
            "DYNAMIC" => Some(Self::Dynamic),
            "RANDOM" => Some(Self::Random),
            "RECORD" => Some(Self::RecordKey),
            "ALTERNATE" => Some(Self::AlternateRecordKey),
            "FILE-STATUS" | "STATUS" => Some(Self::FileStatus),
            "OF" => Some(Self::Of),
            "IN" => Some(Self::In),
            "INPUT" => Some(Self::Input),
            "OUTPUT" => Some(Self::Output),
            "I-O" => Some(Self::IoMode),
            "EXTEND" => Some(Self::Extend),
            "INTO" => Some(Self::Into),
            "AT" => Some(Self::At),
            "END" => Some(Self::End),
            "WITH" => Some(Self::With),
            "LOCK" => Some(Self::Lock),
            "FROM" => Some(Self::From),
            "ON" => Some(Self::OnKw),
            "SIZE" => Some(Self::SizeError),
            "OVERFLOW" => Some(Self::Overflow),
            "DELIMITED" => Some(Self::Delimited),
            "DELIMITER" => Some(Self::Delimiter),
            "COUNT" => Some(Self::Count),
            "ROUNDED" => Some(Self::Rounded),
            "REMAINDER" => Some(Self::Remainder),
            "CANCEL" => Some(Self::Cancel),
            "DELETE" => Some(Self::Delete),
            "REWRITE" => Some(Self::Rewrite),
            "START" => Some(Self::Start),
            "NULL" | "NULLS" => Some(Self::Null),
            "SELF" => Some(Self::Self_),
            "SUPER" => Some(Self::Super),
            "CLASS-ID" => Some(Self::ClassId),
            "METHOD-ID" => Some(Self::MethodId),
            "INTERFACE-ID" => Some(Self::InterfaceId),
            "FUNCTION-ID" => Some(Self::FunctionId),
            "FACTORY" => Some(Self::FactoryKw),
            "OBJECT" => Some(Self::ObjectKw),
            "INVOKE" => Some(Self::Invoke),
            "NEW" => Some(Self::New),
            "RAISE" => Some(Self::Raise),
            "RESUME" => Some(Self::Resume),
            "ALLOCATE" => Some(Self::Allocate),
            "FREE" => Some(Self::Free),
            "JSON" => Some(Self::Json),
            "XML" => Some(Self::Xml),
            "NATIONAL" => Some(Self::National),
            "TYPEDEF" => Some(Self::Typedef),
            "VALIDATE" => Some(Self::Validate),
            "PROPERTY" => Some(Self::Property),
            "INHERITS" => Some(Self::Inherits),
            "END-READ" => Some(Self::EndRead),
            "END-WRITE" => Some(Self::EndWrite),
            "END-REWRITE" => Some(Self::EndRewrite),
            "END-DELETE" => Some(Self::EndDelete),
            "END-START" => Some(Self::EndStart),
            "END-RETURN" => Some(Self::EndReturn),
            "END-STRING" => Some(Self::EndString),
            "END-UNSTRING" => Some(Self::EndUnstring),
            "END-ACCEPT" => Some(Self::EndAccept),
            "END-DISPLAY" => Some(Self::EndDisplay),
            "GENERATE" => Some(Self::Generate),
            "INITIATE" => Some(Self::Initiate),
            "TERMINATE" => Some(Self::Terminate),
            _ => None,
        }
    }

    pub fn is_keyword(&self) -> bool {
        !matches!(self, Self::Identifier | Self::IntegerLiteral | Self::DecimalLiteral
            | Self::StringLiteral | Self::HexLiteral | Self::BooleanLiteral
            | Self::NationalLiteral | Self::LevelNumber
            | Self::Period | Self::Comma | Self::Semicolon
            | Self::LeftParen | Self::RightParen | Self::Colon
            | Self::Plus | Self::Minus | Self::Star | Self::Slash
            | Self::DoubleStar | Self::Equals | Self::GreaterThan | Self::LessThan
            | Self::GreaterEqual | Self::LessEqual | Self::NotEqual
            | Self::DoubleColon | Self::EqualGreater
            | Self::Eof | Self::Newline | Self::Error | Self::CompilerDirective)
    }
}
```

**Step 4: Run tests, verify pass**

Run: `cargo test -p cobol-lexer`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/cobol-lexer/
git commit -m "feat(lexer): add Token and TokenKind with COBOL keyword lookup"
```

---

### Task 7: レキサー — 固定フォーマットソースリーダー

**Files:**
- Create: `crates/cobol-lexer/src/source_reader.rs`
- Test: inline tests

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use cobol_common::SourceFormat;

    #[test]
    fn test_fixed_format_sequence_area() {
        let source = "000100 IDENTIFICATION DIVISION.                                         \n";
        let reader = SourceReader::new(source, SourceFormat::Fixed);
        let lines: Vec<_> = reader.lines().collect();
        assert_eq!(lines[0].sequence_area, "000100");
        assert_eq!(lines[0].indicator, ' ');
    }

    #[test]
    fn test_fixed_format_comment_line() {
        let source = "000100*THIS IS A COMMENT                                                 \n";
        let reader = SourceReader::new(source, SourceFormat::Fixed);
        let lines: Vec<_> = reader.lines().collect();
        assert!(lines[0].is_comment());
    }

    #[test]
    fn test_fixed_format_continuation() {
        let source = "000100 MOVE \"HELLO                                                       \n\
                       000200-    \"WORLD\" TO WS-VAR.                                          \n";
        let reader = SourceReader::new(source, SourceFormat::Fixed);
        let lines: Vec<_> = reader.lines().collect();
        assert!(lines[1].is_continuation());
    }

    #[test]
    fn test_free_format_line() {
        let source = "IDENTIFICATION DIVISION.\n";
        let reader = SourceReader::new(source, SourceFormat::Free);
        let lines: Vec<_> = reader.lines().collect();
        assert_eq!(lines[0].content_text(), "IDENTIFICATION DIVISION.");
    }
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement SourceReader**

SourceReader preprocesses raw source text into logical lines, handling fixed/free format column rules, continuation lines, and comment detection. The actual implementation parses lines according to fixed-format column rules (1-6 sequence, 7 indicator, 8-72 content area) or free-format rules.

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git commit -m "feat(lexer): add SourceReader for fixed/free format handling"
```

---

### Task 8: レキサー — コアレキサー実装

**Files:**
- Create: `crates/cobol-lexer/src/lexer.rs`
- Modify: `crates/cobol-lexer/src/lib.rs`
- Test: inline + integration tests

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
        let mut tokens = Vec::new();
        loop {
            let tok = lexer.next_token();
            if tok.kind == TokenKind::Eof { break; }
            tokens.push(tok);
        }
        tokens
    }

    #[test]
    fn test_lex_identification_division() {
        let src = "       IDENTIFICATION DIVISION.                                          ";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::Identification);
        assert_eq!(tokens[1].kind, TokenKind::Division);
        assert_eq!(tokens[2].kind, TokenKind::Period);
    }

    #[test]
    fn test_lex_level_number() {
        let src = "       01  WS-NAME PIC X(10).                                            ";
        let tokens = lex(src);
        assert_eq!(tokens[0].kind, TokenKind::LevelNumber);
        assert_eq!(tokens[0].text.as_str(), "01");
    }

    #[test]
    fn test_lex_string_literal() {
        let src = "       MOVE \"HELLO\" TO WS-NAME.                                          ";
        let tokens = lex(src);
        let string_tok = tokens.iter().find(|t| t.kind == TokenKind::StringLiteral).unwrap();
        assert_eq!(string_tok.text.as_str(), "\"HELLO\"");
    }

    #[test]
    fn test_lex_numeric_literal() {
        let src = "       MOVE 42 TO WS-COUNT.                                              ";
        let tokens = lex(src);
        let num_tok = tokens.iter().find(|t| t.kind == TokenKind::IntegerLiteral).unwrap();
        assert_eq!(num_tok.text.as_str(), "42");
    }

    #[test]
    fn test_lex_decimal_literal() {
        let src = "       COMPUTE WS-AMT = 3.14.                                            ";
        let tokens = lex(src);
        let dec_tok = tokens.iter().find(|t| t.kind == TokenKind::DecimalLiteral).unwrap();
        assert_eq!(dec_tok.text.as_str(), "3.14");
    }

    #[test]
    fn test_lex_operators() {
        let src = "       COMPUTE X = A + B * C / D - E ** 2.                               ";
        let tokens = lex(src);
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Plus));
        assert!(kinds.contains(&TokenKind::Star));
        assert!(kinds.contains(&TokenKind::Slash));
        assert!(kinds.contains(&TokenKind::Minus));
        assert!(kinds.contains(&TokenKind::DoubleStar));
    }
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement Lexer**

Core lexer that:
- Reads characters from preprocessed source (via SourceReader)
- Handles whitespace skipping (significant in fixed format)
- Recognizes keywords vs identifiers (case insensitive)
- Handles numeric literals (integers, decimals, signed)
- Handles string literals (single and double quoted, with continuation)
- Handles special characters and operators
- Detects COBOL level numbers (01-49, 66, 77, 88)
- Reports lexer errors via DiagnosticReporter

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git commit -m "feat(lexer): implement core lexer with fixed/free format support"
```

---

### Task 9: レキサー — PICTURE句トークン化

**Files:**
- Modify: `crates/cobol-lexer/src/lexer.rs`
- Test: inline tests

PICTURE句は特殊な文字列（`9(5)V99`, `X(10)`, `Z,ZZZ,ZZ9.99`等）で、通常のトークン化ルールとは異なる。PICまたはPICTUREキーワードの後に特殊なPICTURE文字列トークンとしてレキシングする。

**Step 1: Write tests**

```rust
#[test]
fn test_lex_picture_clause() {
    let src = "       01  WS-AMT PIC S9(7)V99.                                          ";
    let tokens = lex(src);
    // After PIC, the picture string should be a single token
    let pic_idx = tokens.iter().position(|t| t.kind == TokenKind::Pic).unwrap();
    assert_eq!(tokens[pic_idx + 1].kind, TokenKind::PictureString);
    assert_eq!(tokens[pic_idx + 1].text.as_str(), "S9(7)V99");
}
```

**Steps 2-5: TDD cycle and commit**

---

### Task 10: レキサー — COPY文処理

**Files:**
- Create: `crates/cobol-lexer/src/copybook.rs`
- Test: inline tests

COPY文でCOPYBOOKファイルをインクルード展開する処理。レキサーレベルでソーステキストを展開。

**Step 1-5: TDD cycle**

---

### Task 11: レキサー — フリーフォーマット対応

**Files:**
- Modify: `crates/cobol-lexer/src/source_reader.rs`
- Modify: `crates/cobol-lexer/src/lexer.rs`
- Test: inline tests

`>>SOURCE FORMAT IS FREE` ディレクティブの検出と、フリーフォーマットでのレキシング。コメントは`*>`で開始。

**Step 1-5: TDD cycle**

---

### Task 12: レキサー — 統合テストとコミット

**Files:**
- Create: `tests/lexer_integration.rs`

完全なCOBOLプログラムをレキシングする統合テスト。

```rust
// tests/lexer_integration.rs
use cobol_lexer::{Lexer, TokenKind};
use cobol_common::{FileId, SourceFormat};

#[test]
fn test_lex_hello_world() {
    let source = r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. HELLO-WORLD.
       PROCEDURE DIVISION.
           DISPLAY "Hello, World!".
           STOP RUN.
"#;
    let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Fixed);
    let tokens = lexer.lex_all();
    assert!(!tokens.iter().any(|t| t.kind == TokenKind::Error));
    // Verify key tokens exist
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Identification));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::ProgramId));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Display));
    assert!(tokens.iter().any(|t| t.kind == TokenKind::Stop));
}
```

---

## Phase 3: AST定義

### Task 13: AST — プログラム構造ノード

**Files:**
- Create: `crates/cobol-ast/src/program.rs`
- Create: `crates/cobol-ast/src/ident_div.rs`
- Create: `crates/cobol-ast/src/env_div.rs`
- Modify: `crates/cobol-ast/src/lib.rs`

プログラム全体、IDENTIFICATION DIVISION、ENVIRONMENT DIVISION のAST定義。

### Task 14: AST — DATA DIVISION ノード

**Files:**
- Create: `crates/cobol-ast/src/data_div.rs`
- Create: `crates/cobol-ast/src/picture.rs`
- Create: `crates/cobol-ast/src/data_item.rs`

データ項目定義、PICTURE句、レベル番号階層、USAGE、VALUE句。

### Task 15: AST — PROCEDURE DIVISION ノード

**Files:**
- Create: `crates/cobol-ast/src/proc_div.rs`
- Create: `crates/cobol-ast/src/statement.rs`
- Create: `crates/cobol-ast/src/expr.rs`

手続き文（MOVE, COMPUTE, IF, PERFORM, EVALUATE, DISPLAY, CALL等）、式、条件式。

### Task 16: AST — OOP・COBOL 2002+ノード

**Files:**
- Create: `crates/cobol-ast/src/oop.rs`
- Create: `crates/cobol-ast/src/modern.rs`

CLASS-ID、METHOD-ID、INTERFACE-ID、INVOKE、JSON/XML関連ノード。

---

## Phase 4: パーサー

### Task 17: パーサー — フレームワーク

**Files:**
- Create: `crates/cobol-parser/src/parser.rs`
- Create: `crates/cobol-parser/src/error.rs`

パーサーの基本構造: トークンストリームの消費、先読み（lookahead）、エラー回復メカニズム。

### Task 18: パーサー — IDENTIFICATION DIVISION

IDENTIFICATION DIVISION の解析。PROGRAM-ID、AUTHOR等。

### Task 19: パーサー — ENVIRONMENT DIVISION

CONFIGURATION SECTION、INPUT-OUTPUT SECTION、FILE-CONTROLの解析。

### Task 20: パーサー — DATA DIVISION (基本)

WORKING-STORAGE SECTION、レベル番号、PIC句、VALUE句の解析。

### Task 21: パーサー — DATA DIVISION (拡張)

FILE SECTION、LINKAGE SECTION、LOCAL-STORAGE SECTION、SCREEN SECTION。
REDEFINES、RENAMES、OCCURS、DEPENDING ON。

### Task 22: パーサー — PROCEDURE DIVISION (基本文)

MOVE、COMPUTE、ADD/SUBTRACT/MULTIPLY/DIVIDE、DISPLAY、ACCEPT、STOP RUN。

### Task 23: パーサー — PROCEDURE DIVISION (制御構造)

IF/ELSE/END-IF、EVALUATE/WHEN/END-EVALUATE、PERFORM/END-PERFORM。
GO TO、EXIT。

### Task 24: パーサー — PROCEDURE DIVISION (I/O・CALL)

OPEN、CLOSE、READ、WRITE、REWRITE、DELETE、START。
CALL、CANCEL、STRING、UNSTRING、INSPECT。

### Task 25: パーサー — 統合テスト

完全なCOBOLプログラムのパースとAST検証。

---

## Phase 5: 意味解析

### Task 26: シンボルテーブル

プログラム名、段落名、セクション名、データ名のスコープ付きシンボルテーブル。

### Task 27: 名前解決

修飾名解決（OF/IN）、データ名の一意性検証、段落/セクション参照の解決。

### Task 28: 型チェック

PIC句からの型推論、MOVE/COMPUTE文の型互換性チェック、サイズ検証。

### Task 29: PICTURE句解析エンジン

PICTURE文字列の完全解析、メモリサイズ計算、編集パターン解析。

### Task 30: 意味解析 — 統合テスト

型エラー、未定義参照、スコープ違反の検出テスト。

---

## Phase 6: HIR/MIR

### Task 31: HIR定義

高レベル中間表現のノード定義。COBOL構文の脱糖。

### Task 32: AST→HIR変換

PERFORM VARYING→ループ、EVALUATE→分岐、ファイルI/O→ランタイム呼び出し。

### Task 33: MIR定義

SSA形式の低レベルIR。基本ブロック、phi関数、プリミティブ操作。

### Task 34: HIR→MIR変換

SSA変換、メモリレイアウト確定、BCD演算展開。

### Task 35: IR — 統合テスト

AST→HIR→MIR変換の正確性検証。

---

## Phase 7: LLVMコード生成

### Task 36: inkwell統合

LLVMコンテキスト、モジュール、ビルダーの初期化。inkwell crateのセットアップ。

### Task 37: 基本データ型のLLVM IR生成

数値型（整数、BCD）、文字列型、グループ項目のLLVM構造体へのマッピング。

### Task 38: 算術演算のLLVM IR生成

ADD、SUBTRACT、MULTIPLY、DIVIDE、COMPUTE式のIR生成。BCD演算のランタイム呼び出し。

### Task 39: 制御フローのLLVM IR生成

IF/ELSE、PERFORM、EVALUATE、GO TO のLLVM基本ブロック生成。

### Task 40: ファイルI/O・CALL文のLLVM IR生成

ランタイムライブラリ関数呼び出しの生成。

### Task 41: コード生成 — 統合テスト

Hello Worldプログラムのコンパイル→実行テスト。

---

## Phase 8: ランタイムライブラリ

### Task 42: BCD演算エンジン

パック10進数（COMP-3）の加減乗除、比較、変換。

### Task 43: 文字列操作

MOVE（文字列間）、STRING、UNSTRING、INSPECT、TRANSFORM。

### Task 44: 数値⇔文字列変換

PICTURE編集に基づくフォーマッティング（数値→表示文字列）。

### Task 45: ファイルI/O — 順編成

OPEN、CLOSE、READ、WRITE for SEQUENTIAL files。

### Task 46: ファイルI/O — 索引編成

B-Treeベースの索引ファイル。OPEN、READ、WRITE、REWRITE、DELETE、START。

### Task 47: 組み込み関数

FUNCTION CURRENT-DATE、LENGTH、TRIM、UPPER-CASE、LOWER-CASE、MAX、MIN等。

### Task 48: SORT/MERGE

外部ソートアルゴリズム。SORT文、MERGE文、INPUT/OUTPUT PROCEDURE。

---

## Phase 9: CLIドライバ・統合

### Task 49: CLIインターフェース

clap deriveでコマンドライン引数解析。--std, --source-format, -O, -o, -g等。

### Task 50: コンパイルパイプライン統合

レキサー→パーサー→意味解析→HIR→MIR→コード生成→リンクの全パイプライン接続。

### Task 51: エラー報告の統合

ariadneベースのリッチエラー出力。ソース位置表示、波線、色付き。

### Task 52: E2Eテスト

複数のCOBOLプログラムをコンパイル→実行→出力検証するE2Eテストスイート。

---

## Phase 10: COBOL-85完全対応

### Task 53-60: COBOL-85残機能

- SORT/MERGE文完全対応
- REPORT WRITER
- COMMUNICATION SECTION
- デバッグモード (USE FOR DEBUGGING)
- INSPECT TALLYING/REPLACING/CONVERTING完全対応
- PERFORM THRU完全対応
- 88レベル条件名の完全対応
- NIST COBOL85テストスイート全パス

---

## Phase 11: COBOL 2002拡張

### Task 61-67: COBOL 2002新機能

- OOP (CLASS-ID, METHOD-ID, INVOKE, INHERITS)
- ユーザー定義関数 (FUNCTION-ID)
- LOCAL-STORAGE SECTION
- フリーフォーマット (`>>SOURCE FORMAT IS FREE`)
- BOOLEAN型
- ポインタ操作拡張
- 例外処理 (RAISE/RESUME)

---

## Phase 12: COBOL 2014拡張

### Task 68-72: COBOL 2014新機能

- FLOAT-SHORT/FLOAT-LONG/FLOAT-EXTENDED
- TYPEDEF
- 動的メモリ管理 (ALLOCATE/FREE)
- VALIDATE文
- テーブル拡張

---

## Phase 13: COBOL 2023拡張

### Task 73-80: COBOL 2023新機能

- UTF-8/Unicodeネイティブサポート
- JSON GENERATE/JSON PARSE
- XML GENERATE/XML PARSE
- インターフェース拡張 (INTERFACE-ID)
- 非同期処理
- スレッドサポート
- デリゲート/ファンクションポインタ拡張
- 最終統合テスト・全標準バージョンテスト

---

## Notes

- 各Phase完了時にコードレビュー実施
- Phase 7完了後に最初のE2E（Hello World）コンパイル→実行を達成
- NIST COBOL85テストスイートはPhase 10で全パスを目標
- 各TaskはTDD（テスト先行）で実装
