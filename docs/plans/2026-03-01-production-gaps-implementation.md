# COBOL コンパイラ 製品化ギャップ分析と実装計画

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 教育用レベルから業務用レベルへ引き上げるため、5つの主要ギャップを解消する

**Architecture:** 既存パイプライン `Source → Lexer → Parser → Sema → HIR → C codegen → clang → native binary` の HIR・codegen 層を中心に拡張

**Tech Stack:** Rust (コンパイラ本体), C (生成コード), cobol-runtime (Rust staticlib with C ABI)

---

## 現状評価

### 動作済み機能
| カテゴリ | 状態 |
|----------|------|
| 基本ステートメント (MOVE/ADD/IF/PERFORM/DISPLAY等) | ✅ |
| EVALUATE → IF デシュガー | ✅ |
| 順次ファイル I/O (OPEN/READ/WRITE/CLOSE) | ✅ |
| STRING/UNSTRING (基本) | ✅ |
| PERFORM THRU / VARYING / UNTIL / TIMES | ✅ |
| COPY/REPLACE プリプロセッサ | ✅ |
| 参照修飾 VAR(start:length) | ✅ |
| 88レベル条件名 | ✅ |
| ariadne 診断レンダリング | ✅ |
| E2E ネイティブバイナリテスト (14本) | ✅ |
| テスト合計 388本、clippy/fmt クリーン | ✅ |

### 製品化に必要な未実装機能（本計画の対象）

| # | ギャップ | 優先度 | 影響 |
|---|----------|--------|------|
| 1 | OCCURS / テーブル（配列） | 最高 | テーブルを使う全COBOLが失敗 |
| 2 | COMP-3 / 10進数演算 | 高 | 金融計算で精度崩壊 |
| 3 | エラーハンドリング (ON SIZE ERROR等) | 中 | ファイルI/O・算術の異常が無視される |
| 4 | 象徴定数 (HIGH-VALUE等) | 低 | 一部の比較・初期化が不正 |
| 5 | FILE STATUS 変数 | 低 | ファイル操作結果をプログラムから参照不可 |

---

## Phase 1: OCCURS / テーブル（配列）サポート

**規模: L** | **依存: なし（独立して実装可能）**

### 現状の詳細分析

| レイヤー | 状態 | 詳細 |
|----------|------|------|
| AST/Parser | ✅ 完全 | `OccursClause` (min/max, DEPENDING ON, KEY, INDEXED BY) 定義済み。`QualifiedName.subscripts` で添字パース済み |
| Sema | ⚠️ 部分的 | 添字の名前解決のみ。型チェック・範囲検証なし |
| HIR | ❌ 欠落 | `lower_expr` が subscripts を無視（`lower.rs:1299`）。`HirDataItem` に occurs なし |
| Codegen | ❌ 欠落 | 全変数がスカラー。配列宣言・添字アクセスなし |

### 実装タスク

#### Task 1-1: HIR 型拡張
**Files:** `crates/cobol-hir/src/hir.rs`

- `HirExpr` に `Subscript { variable: SmolStr, subscripts: Vec<HirExpr> }` バリアント追加
- `HirDataItem` に `occurs: Option<u32>` フィールド追加（最大繰り返し数、None = スカラー）
- `HirMoveTarget` に `Subscript` バリアント追加
- Display/Debug トレイト実装を更新

#### Task 1-2: HIR ローワリング更新
**Files:** `crates/cobol-hir/src/lower.rs`

- `lower_expr`（1296行）: `Expr::Identifier(qname)` で `qname.subscripts` が空でない場合 → `HirExpr::Subscript` を生成
- `lower_data_item`（166行）: `item.occurs.as_ref().map(|o| o.max)` を `HirDataItem.occurs` に設定
- グループ項目の OCCURS: 親の OCCURS を子の全リーフ項目に伝播
- INDEXED BY: `OccursClause.indexed_by` の各名前を `HirType::Index` の合成 `HirDataItem` として出力

#### Task 1-3: C コード生成
**Files:** `crates/cobol-codegen/src/codegen.rs`

- `emit_single_data_item`（227行）: `occurs` ありの場合 `int64_t name[N];` を出力
- `emit_expr`（1540行）: `HirExpr::Subscript` → `name[(idx - 1)]`（COBOL 1始まり → C 0始まり）
- `emit_data_init`（271行）: OCCURS 項目は for ループで初期化
- `emit_display_operand`, `emit_move_to`, `emit_condition`: Subscript 対応追加
- `find_data_item_size`: OCCURS 項目は1要素のサイズを返す

#### Task 1-4: テスト
**Files:** `crates/cobol-driver/tests/e2e_test.rs`

```cobol
*> テスト1: 基本的なテーブルアクセス
01  WS-TABLE PIC 9(3) OCCURS 10 TIMES.
    MOVE 42 TO WS-TABLE(3).
    DISPLAY WS-TABLE(3).        *> → "42"

*> テスト2: 変数添字
01  WS-IDX PIC 9(2) VALUE 5.
    MOVE 99 TO WS-TABLE(WS-IDX).
    DISPLAY WS-TABLE(5).        *> → "99"

*> テスト3: ループでのテーブル操作
    PERFORM VARYING WS-IDX FROM 1 BY 1 UNTIL WS-IDX > 10
        MOVE WS-IDX TO WS-TABLE(WS-IDX)
    END-PERFORM.
```

---

## Phase 2: COMP-3 / 10進数演算

**規模: L** | **依存: なし（Phase 1 と並行可能）**

### 現状の詳細分析

| レイヤー | 状態 | 詳細 |
|----------|------|------|
| Runtime | ✅ 完全 | `CobolDecimal` + 8関数（add/sub/mul/div/cmp/from_int/from_string/to_display）実装済み |
| HIR | ✅ 正確 | `Numeric { size, decimal_places, is_signed }`, `Comp3 { size, decimal_places }` |
| Codegen | ❌ 未接続 | 全数値が `int64_t`。ランタイム関数未呼び出し |

### 方針: スケールド整数アプローチ
- `decimal_places > 0` の変数のみ `CobolDecimal` を使用
- `decimal_places == 0` の整数はそのまま `int64_t`（変更不要、パフォーマンス維持）
- ランタイムの `CobolDecimal` は既にスケールド整数方式（value=12345, scale=2 で 123.45 を表現）

### ランタイム関数一覧（`crates/cobol-runtime/src/decimal.rs`）

| 関数 | シグネチャ | 用途 |
|------|-----------|------|
| `cobol_decimal_add` | `(a: *const, b: *const, r: *mut)` | 加算（スケール自動調整） |
| `cobol_decimal_sub` | `(a: *const, b: *const, r: *mut)` | 減算 |
| `cobol_decimal_mul` | `(a: *const, b: *const, r: *mut)` | 乗算（スケール=a+b） |
| `cobol_decimal_div` | `(a: *const, b: *const, r: *mut)` | 除算（ゼロ除算→0） |
| `cobol_decimal_cmp` | `(a: *const, b: *const) → i32` | 比較（-1/0/1） |
| `cobol_decimal_from_int` | `(value: i64, scale: i32, r: *mut)` | 整数→decimal |
| `cobol_decimal_from_string` | `(ptr: *const, len: u32, r: *mut)` | 文字列→decimal |
| `cobol_decimal_to_display` | `(d: *const, buf: *mut, len: u32, pic: *const, pic_len: u32) → u32` | 表示用フォーマット |

### 実装タスク

#### Task 2-1: CobolDecimal 宣言の生成
**Files:** `crates/cobol-codegen/src/codegen.rs`

- `emit_runtime_declarations`（102行以降）: `CobolDecimal` 構造体定義と 8 つの extern 関数宣言を追加
- ヘルパー `needs_decimal(data_type: &HirType) -> bool` を追加（`decimal_places > 0` または `Comp3` で true）

#### Task 2-2: データ項目宣言の修正
**Files:** `crates/cobol-codegen/src/codegen.rs`

- `emit_single_data_item`（227行）: `needs_decimal` が true の場合 `static CobolDecimal name;` を出力
- `emit_single_data_init`（277行）: `CobolDecimal` の初期化
  ```c
  name = (CobolDecimal){ .value = 12345, .scale = 2, .size = 7, .is_signed = 1 };
  ```
- VALUE 句のパース: `HirLiteral::Decimal("123.45")` → value=12345, scale=2 に変換

#### Task 2-3: 算術ステートメントの修正
**Files:** `crates/cobol-codegen/src/codegen.rs`

- ADD/SUBTRACT/MULTIPLY/DIVIDE: ターゲットが decimal の場合ランタイム関数呼び出しに切り替え
  ```c
  // ADD A TO B (B is decimal)
  CobolDecimal _tmp;
  cobol_decimal_from_int(A, 0, &_tmp);
  cobol_decimal_add(&B, &_tmp, &B);
  ```
- COMPUTE: 式ツリーを `CobolDecimal` 一時変数の連鎖に分解する `emit_decimal_expr` ヘルパーを追加

#### Task 2-4: DISPLAY / MOVE / 比較の修正
**Files:** `crates/cobol-codegen/src/codegen.rs`

- DISPLAY: `cobol_decimal_to_display()` を呼び出し、PIC文字列は `HirType` のメタデータから生成
- MOVE: 整数↔decimal の変換に `cobol_decimal_from_int` / 除算を使用
- 比較（IF条件）: `cobol_decimal_cmp()` を使用（戻り値 -1/0/1）

#### Task 2-5: テスト
**Files:** `crates/cobol-driver/tests/e2e_test.rs`

```cobol
*> テスト1: 10進数の宣言と表示
01  WS-AMOUNT PIC S9(5)V99 VALUE 123.45.
    DISPLAY WS-AMOUNT.

*> テスト2: 10進数の加算
01  WS-A PIC 9(3)V99 VALUE 10.50.
01  WS-B PIC 9(3)V99 VALUE 20.25.
    ADD WS-A TO WS-B.
    DISPLAY WS-B.                *> → "30.75"

*> テスト3: 10進数の乗除算
    MULTIPLY WS-A BY WS-B.
    DIVIDE 3 INTO WS-RESULT.

*> テスト4: COMPUTE
01  WS-RESULT PIC 9(5)V99.
    COMPUTE WS-RESULT = WS-A * WS-B + 100.
```

---

## Phase 3: エラーハンドリング

**規模: M** | **依存: Phase 2 推奨（10進数のオーバーフロー検出に必要）**

### 現状の詳細分析

| 機能 | 状態 | 詳細 |
|------|------|------|
| ON SIZE ERROR | スタブ | `if (0) { ... }` デッドコード（codegen.rs:1471） |
| ON EXCEPTION | スタブ | 同上（codegen.rs:1507） |
| INVALID KEY (READ) | ⚠️ 部分 | AT END (fs==10) のみ実装 |
| INVALID KEY (WRITE/START) | スタブ | 戻り値無視、TODO コメント |
| ON OVERFLOW (STRING) | スタブ | ランタイムは戻り値あり、codegen が無視 |
| ON OVERFLOW (UNSTRING) | スタブ | 同上 |

### 実装タスク

#### Task 3-1: ON SIZE ERROR の実装
**Files:** `crates/cobol-codegen/src/codegen.rs`

- 算術後に結果の絶対値が PIC 容量（`10^size - 1`）を超えるかチェック
- 整数: `if (llabs(target) > max_val) { _size_error = 1; target = prev; }`
- decimal: `if (llabs(target.value) > pow_10_size) { ... }`
- ヘルパー `get_pic_max(name: &str, data_items: &[HirDataItem]) -> Option<i64>` を追加

#### Task 3-2: INVALID KEY の実装
**Files:** `crates/cobol-codegen/src/codegen.rs`

- WRITE（656行）: `cobol_file_write` の戻り値をキャプチャし、`fs == 22 || fs == 23` でチェック
- START（1032行）: 同様に戻り値チェック
- DELETE, REWRITE: 同様のパターン

ランタイムのファイルステータスコード:
- 00 = 成功, 10 = AT END, 22 = 重複キー, 23 = レコード未検出, 30 = I/Oエラー

#### Task 3-3: ON OVERFLOW の実装
**Files:** `crates/cobol-codegen/src/codegen.rs`

- STRING（726行）: `cobol_string_concat` の戻り値（0=成功, 1=overflow）をチェック
- UNSTRING（789行）: `cobol_unstring` の戻り値を同様にチェック

#### Task 3-4: テスト

```cobol
*> ON SIZE ERROR テスト
01  WS-SMALL PIC 9(2) VALUE 99.
    ADD 1 TO WS-SMALL
        ON SIZE ERROR DISPLAY "OVERFLOW"
        NOT ON SIZE ERROR DISPLAY "OK"
    END-ADD.
    *> → "OVERFLOW"
```

---

## Phase 4: 象徴定数 + FILE STATUS 変数

**規模: S** | **依存: なし**

### 実装タスク

#### Task 4-1: 象徴定数の拡張
**Files:** `crates/cobol-hir/src/hir.rs`, `crates/cobol-hir/src/lower.rs`, `crates/cobol-codegen/src/codegen.rs`

現在の問題: `lower.rs:296` で HIGH-VALUE/LOW-VALUE/QUOTE/ALL/NULL が全て `HirLiteral::Zero` に簡略化されている

修正内容:
- `HirLiteral` に `HighValue`, `LowValue`, `Quote`, `Null` バリアント追加
- ローワリングで各定数を個別にマッピング
- Codegen での生成:
  - HIGH-VALUE → `memset(target, 0xFF, size)` (全ビット1)
  - LOW-VALUE → `memset(target, 0x00, size)` (全ビット0)
  - QUOTE → `memset(target, '"', size)` (ダブルクォート)
  - NULL → `(void*)0`

#### Task 4-2: FILE STATUS 変数の接続
**Files:** `crates/cobol-hir/src/hir.rs`, `crates/cobol-hir/src/lower.rs`, `crates/cobol-codegen/src/codegen.rs`

現在の問題: FILE STATUS 句はパーサーで解析済みだが、HIR/codegen に伝播されていない

修正内容:
- `HirProgram` に `file_descriptions: Vec<HirFileInfo>` 追加（file_name → file_status_var マッピング）
- ローワリング: `program.environment` の `FileControlEntry.file_status` を HIR に伝播
- Codegen: 各ファイル操作後に FILE STATUS 変数へ2桁のステータスコードを代入

#### Task 4-3: テスト

```cobol
*> HIGH-VALUE テスト
01  WS-HV PIC X(5).
    MOVE HIGH-VALUES TO WS-HV.

*> FILE STATUS テスト
01  WS-FS PIC XX.
FD  MYFILE FILE STATUS IS WS-FS.
    OPEN INPUT MYFILE.
    DISPLAY WS-FS.               *> → "00" or "35"
```

---

## フェーズ依存関係と実行順序

```
Phase 1 (OCCURS)     ─┐
                       ├── 並行実行可能
Phase 2 (Decimal)    ─┘
         │
         ▼ (Phase 2 完了後推奨)
Phase 3 (Error Handling)

Phase 4 (Fig.Const + FILE STATUS) ── 独立（いつでも実行可能）
```

## 検証方法

各 Phase 完了後:
1. `cargo test --workspace` — 全テスト合格（既存 + 新規）
2. `cargo clippy --workspace --all-targets -- -D warnings` — 警告なし
3. `cargo fmt --all -- --check` — フォーマット準拠
4. ネイティブバイナリ E2E テスト — 実際にコンパイル・実行して出力値を検証

## 主要ファイル一覧

| ファイル | Phase | 変更内容 |
|----------|-------|----------|
| `crates/cobol-hir/src/hir.rs` | 1,2,4 | Subscript, occurs, HirLiteral拡張, HirFileInfo |
| `crates/cobol-hir/src/lower.rs` | 1,2,4 | 添字保持, OCCURS伝播, 象徴定数, FILE STATUS |
| `crates/cobol-codegen/src/codegen.rs` | 1,2,3,4 | 配列宣言, decimal演算, エラーハンドリング, 全般 |
| `crates/cobol-runtime/src/decimal.rs` | (参照のみ) | C ABI シグネチャの確認用 |
| `crates/cobol-driver/tests/e2e_test.rs` | 1,2,3,4 | 各Phase の E2E テスト追加 |
