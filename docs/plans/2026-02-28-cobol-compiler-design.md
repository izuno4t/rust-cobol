# COBOL Compiler Design Document

Date: 2026-02-28

## Overview

COBOL-85からCOBOL 2023までを網羅する本番環境向けCOBOLコンパイラ。
Rustで実装し、LLVMバックエンドでクロスプラットフォーム（Linux/macOS/Windows）のネイティブコードを生成する。

## Requirements

- **用途**: 本番環境での商用利用
- **実装言語**: Rust
- **バックエンド**: LLVM（inkwell経由）
- **ターゲット**: クロスプラットフォーム（Linux/macOS/Windows）
- **対応標準**: COBOL-85, COBOL 2002, COBOL 2014, COBOL 2023
- **ランタイム**: Rust自作（libcobolrt）

## Architecture

### Approach

モノリシック・パイプライン方式。Cargo workspaceで各フェーズをcrateとして分離。

### Project Structure

```
cobol-compiler/
├── Cargo.toml                 # workspace定義
├── crates/
│   ├── cobol-driver/          # CLIドライバ・コンパイル制御
│   ├── cobol-lexer/           # 字句解析（フリー/固定フォーマット対応）
│   ├── cobol-parser/          # 構文解析 → AST生成
│   ├── cobol-ast/             # AST定義（全COBOL構文ノード）
│   ├── cobol-sema/            # 意味解析（型チェック、名前解決、スコープ）
│   ├── cobol-hir/             # 高レベル中間表現
│   ├── cobol-mir/             # 低レベル中間表現
│   ├── cobol-codegen/         # LLVM IRコード生成
│   ├── cobol-runtime/         # ランタイムライブラリ（libcobolrt）
│   ├── cobol-diagnostics/     # エラー報告・診断メッセージ
│   └── cobol-common/          # 共通ユーティリティ・型定義
├── tests/                     # 統合テスト・COBOL テストスイート
├── stdlib/                    # 標準ライブラリCOBOLソース
└── docs/                      # ドキュメント
```

### Compilation Pipeline

```
COBOL Source (.cob/.cbl)
    → [cobol-lexer] 字句解析 → トークンストリーム
    → [cobol-parser] 構文解析 → AST (Untyped)
    → [cobol-sema] 意味解析 → AST (Typed) + シンボルテーブル
    → [cobol-hir] HIR変換 → 高レベル中間表現
    → [cobol-mir] MIR変換 → 低レベル中間表現 (SSA)
    → [cobol-codegen] LLVM IR生成
    → [LLVM] 最適化 → ネイティブコード
    → 実行ファイル + libcobolrt リンク
```

## Lexer Design (cobol-lexer)

### Source Formats

- **固定フォーマット** (COBOL-85): 1-6列=行番号, 7列=インジケータ, 8-11列=A領域, 12-72列=B領域
- **フリーフォーマット** (COBOL 2002+): 列制約なし, `>>SOURCE FORMAT IS FREE`ディレクティブ
- **可変フォーマット**: コンパイラオプションで列幅指定

### Token Types

- 予約語 (COBOL-85: ~300語, COBOL 2023: ~500語以上)
- 識別子 (ユーザー定義名、段落名、セクション名、データ名)
- リテラル (数値、英数字、日本語/UTF-8、16進、ブール)
- 特殊文字 (ピリオド、カンマ、括弧、算術演算子)
- COPYBOOKインクルード処理 (COPY/REPLACE文)

### Implementation

- 手書きレキサー（固定フォーマットのため）
- COPYBOOKはレキサーレベルでインクルード展開

## Parser Design (cobol-parser)

### Approach

- 再帰下降パーサー（手書き）
- COBOLの文法は文脈依存性が強いためパーサージェネレータ不適
- エラー回復: ピリオド（文終端）までスキップして解析続行

### Supported Divisions

1. IDENTIFICATION DIVISION — プログラム識別情報
2. ENVIRONMENT DIVISION — 環境設定 (FILE-CONTROL, I-O-CONTROL)
3. DATA DIVISION — データ定義 (WORKING-STORAGE, LOCAL-STORAGE, LINKAGE, FILE, SCREEN)
4. PROCEDURE DIVISION — 手続き (文、段落、セクション)

### AST Design

- 全構文ノードにソース位置情報（Span）保持
- レベル番号ベースのデータ階層をツリー構造で表現
- COBOL 2002+ OOP構文対応 (CLASS-ID, METHOD-ID, INVOKE)

## Semantic Analysis (cobol-sema)

### Name Resolution

- COBOL特有の修飾名解決 (`MOVE A OF B TO C OF D`)
- セクション・段落名のスコープ管理
- プログラム間参照 (CALL文)
- OF/IN修飾による曖昧性解決

### Type System

- `PIC 9(n)` → 固定小数点数値型 (BCD or バイナリ)
- `PIC X(n)` → 固定長文字列型
- `PIC A(n)` → 英字型
- `PIC S9(n)V9(m)` → 符号付き固定小数点
- `COMP/COMP-1/COMP-2/COMP-3/COMP-5` → バイナリ/浮動小数点/パック10進数
- COBOL 2023: `FLOAT-SHORT`/`FLOAT-LONG`/`FLOAT-EXTENDED`
- レベル番号によるグループ項目/基本項目の構造解析
- REDEFINES/RENAMES検証

### PICTURE Clause Parser

- 専用パーサーでPICTURE文字列解析
- 編集項目 (`Z`, `*`, `$`, `,`, `.`, `CR`, `DB`等)
- メモリレイアウト・サイズ計算

## Intermediate Representations

### HIR (High-level IR)

- COBOL構文を脱糖した表現
- PERFORM VARYING → ループ構造
- EVALUATE → match/switch構造
- STRING/UNSTRING → 組み込み関数呼び出し
- ファイルI/O → ランタイムAPI呼び出し

### MIR (Low-level IR)

- SSA形式 (Static Single Assignment)
- メモリレイアウト確定
- BCD演算をプリミティブ操作に展開
- 制御フロー解析・データフロー解析の基盤

## Code Generation (cobol-codegen)

### LLVM IR Generation

- `inkwell` crate使用
- COBOL固有の最適化: BCD演算ベクトル化、テーブルアクセス最適化、PERFORM文インライン化
- DWARF デバッグ情報生成
- PGO (Profile-Guided Optimization) 対応

### Data Layout

- COBOL変数 → LLVM構造体
- REDEFINES → union/overlay
- OCCURS → 配列
- グループ項目 → 構造体

## Runtime Library (cobol-runtime / libcobolrt)

### Core Features

- BCD演算エンジン
- 文字列操作 (STRING/UNSTRING/INSPECT/TRANSFORM)
- ファイルI/O:
  - 順編成 (SEQUENTIAL)
  - 索引編成 (INDEXED) — B-Tree実装
  - 相対編成 (RELATIVE)
  - 行順編成 (LINE SEQUENTIAL)
- ソート/マージ (SORT/MERGE)
- 画面制御 (SCREEN SECTION / ACCEPT / DISPLAY)
- 組み込み関数 (60+関数)
- 例外処理 (COBOL 2002+ RAISE/RESUME)

### COBOL 2023 Features

- UTF-8/Unicodeネイティブサポート
- JSON GENERATE/JSON PARSE
- XML GENERATE/XML PARSE
- 動的メモリ割り当て (ALLOCATE/FREE)
- インターフェース (INTERFACE-ID)
- デリゲート/ファンクションポインタ
- 非同期処理
- スレッドサポート

## Error Reporting (cobol-diagnostics)

- `ariadne` crateでリッチなエラー表示
- エラーコード体系 (例: `E0001: Undefined data name 'XXX'`)
- 警告レベル設定 (error/warning/info/hint)
- COBOL固有の診断

## CLI Interface (cobol-driver)

```
cobolc [OPTIONS] <SOURCE_FILES>...

Options:
  -o <OUTPUT>           出力ファイル名
  -c                    コンパイルのみ
  -S                    アセンブリ出力
  --emit=llvm-ir        LLVM IR出力
  --emit=ast            AST出力
  --emit=hir            HIR出力
  -O0/-O1/-O2/-O3      最適化レベル
  --std=cobol85         COBOL標準バージョン指定
  --std=cobol2002
  --std=cobol2014
  --std=cobol2023
  --source-format=fixed/free  ソースフォーマット
  -I <DIR>              COPYBOOKディレクトリ
  -L <DIR>              ライブラリ検索パス
  -W<warning>           警告制御
  --dump-tokens         トークンダンプ
  --dump-ast            ASTダンプ
  -g                    デバッグ情報生成
  -v                    詳細出力
```

## Dependencies

| crate | Purpose |
|-------|---------|
| `inkwell` | LLVM Rust bindings |
| `ariadne` | Error reporting |
| `clap` | CLI argument parsing |
| `serde` | Serialization |
| `unicode-segmentation` | UTF-8 processing |
| `num-bigint` | Arbitrary precision integers |
| `rust_decimal` | Fixed-point decimal |
| `tempfile` | Test temp files |

## Testing Strategy

- **Unit tests**: 各crate内で `#[cfg(test)]`
- **Integration tests**: COBOLソース → コンパイル → 実行 → 出力検証
- **NIST COBOL85 test suite**: 業界標準適合性テスト (~300テスト)
- **Regression tests**: 各COBOL標準バージョンごと
- **Fuzzing**: `cargo-fuzz`でレキサー・パーサーの堅牢性テスト
