# Production COBOL Compiler — Phase 1〜9 実装計画

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** rust-cobolを製品レベルのCOBOLコンパイラにする

**Architecture:** 既存パイプライン（Lexer→Parser→Sema→HIR→C codegen→clang→binary）を拡張。
各Phaseは独立して実装・検証可能。全Phase完了後にNIST CCVS 85で適合性検証。

**Tech Stack:** Rust, C codegen, clang, ariadne (diagnostics)

---

## Phase 1: パーサ修正（JSON/XML/VALIDATE）

### Task 1-1: VALIDATE文のAST・パーサ・lowering追加

**Files:**
- Modify: `crates/cobol-ast/src/statement.rs` — Statement enumにValidate追加、ValidateStatement構造体追加
- Modify: `crates/cobol-parser/src/proc_div.rs` — parse_statement()にTokenKind::Validate追加、parse_validate_statement()追加
- Modify: `crates/cobol-hir/src/lower.rs` — lower_statementにStatement::Validate追加、lower_validate()追加
- Test: `crates/cobol-driver/tests/e2e_test.rs` — VALIDATE文のパーステスト

### Task 1-2: JSON GENERATE/PARSE文のパーサ追加

**Files:**
- Modify: `crates/cobol-parser/src/proc_div.rs` — parse_statement()にTokenKind::Json追加、parse_json_statement()追加
- Test: `crates/cobol-driver/tests/e2e_test.rs` — JSON文のパース→HIR→Cテスト

### Task 1-3: XML GENERATE/PARSE文のパーサ追加

**Files:**
- Modify: `crates/cobol-parser/src/proc_div.rs` — parse_statement()にTokenKind::Xml追加、parse_xml_statement()追加
- Test: `crates/cobol-driver/tests/e2e_test.rs` — XML文のパース→HIR→Cテスト

## Phase 2: 組み込み関数の充実

### Task 2-1: 数学関数（ABS, SQRT, SIN, COS, etc.）

**Files:**
- Modify: `crates/cobol-runtime/src/intrinsic.rs` — ランタイム関数追加
- Modify: `crates/cobol-codegen/src/codegen.rs` — emit_exprのFunctionCallマッチに追加
- Test: E2Eネイティブ実行テスト

### Task 2-2: 日付関数（DATE-OF-INTEGER, INTEGER-OF-DATE, etc.）

**Files:**
- Modify: `crates/cobol-runtime/src/intrinsic.rs`
- Modify: `crates/cobol-codegen/src/codegen.rs`
- Test: E2Eネイティブ実行テスト

### Task 2-3: 文字列関数（CONCATENATE, SUBSTITUTE, etc.）

**Files:**
- Modify: `crates/cobol-runtime/src/intrinsic.rs`
- Modify: `crates/cobol-codegen/src/codegen.rs`
- Test: E2Eネイティブ実行テスト

## Phase 3: データ処理の精度向上

### Task 3-1: RENAMES句（レベル66）のHIR/codegen対応

**Files:**
- Modify: `crates/cobol-hir/src/hir.rs` — HirDataItemにrenames追加
- Modify: `crates/cobol-hir/src/lower.rs` — lower_data_itemでrenames処理
- Modify: `crates/cobol-codegen/src/codegen.rs` — #define エイリアス生成
- Test: E2Eテスト

### Task 3-2: 添字付きターゲット算術の修正

**Files:**
- Modify: `crates/cobol-hir/src/hir.rs` — 算術文のターゲットをHirExpr化
- Modify: `crates/cobol-hir/src/lower.rs`
- Modify: `crates/cobol-codegen/src/codegen.rs`
- Test: E2Eテスト

### Task 3-3: ファイルI/Oステータスコード検証

- 調査結果: 既に主要コードは実装済み。E2Eテスト追加で検証。

## Phase 4: E2E検証の充実

### Task 4-1〜4-6: 各機能のE2Eテスト追加

SORT PROCEDURE, OOP, FUNCTION-ID, TYPEDEF, CALL+GOBACK, PERFORM THRU,
DECLARATIVES, 索引・相対ファイルの各E2Eテスト。

## Phase 5: 拡張機能

### Task 5-1: SCREEN SECTION
### Task 5-2: REPORT SECTION
### Task 5-3: NATIONALデータ型

## Phase 6: エッジケース・品質向上

### Task 6-1〜6-6: 各エッジケース修正

## Phase 7: NIST CCVS 85 適合性検証

### Task 7-1: テスト基盤構築
### Task 7-2: NC (Nucleus) モジュール通過
### Task 7-3: IF (Intrinsic Functions) モジュール通過
### Task 7-4: SQ/IX/RL (File I/O) モジュール通過
### Task 7-5: IC/SM/ST (残りモジュール) 通過

## Phase 8: 診断メッセージの品質向上

### Task 8-1: パースエラーの位置情報改善
### Task 8-2: 型エラーの詳細メッセージ
### Task 8-3: 警告レベル・エラーコード体系

## Phase 9: 実業務プログラム検証

### Task 9-1: GnuCOBOL互換テスト
### Task 9-2: オープンソースCOBOLプログラムの動作検証
### Task 9-3: 性能ベンチマーク
### Task 9-4: ドキュメント整備
