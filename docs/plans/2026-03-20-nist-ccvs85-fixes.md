# NIST CCVS 85 全モジュール通過 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** NIST CCVS 85 全12モジュールのテストをクリアする（PASS判定）

**Architecture:** コンパイルエラーを影響範囲の大きい順に修正し、各修正後にrun_nist.shで回帰確認。INSPECT判定のプログラムは実行出力を検証してPASS/FAILを判定。

**Tech Stack:** Rust (parser/sema/codegen), C (runtime/generated code), Perl (extract.pl), Bash (run_nist.sh)

---

## Phase A: 広範囲パーサー修正（〜178プログラム解消見込み）

### Task A1: `DATA RECORD IS` 句パーサー対応

**影響:** SQ 74件 + ST 25件 + RW 6件 + DB 15件 + SG 13件 + NC一部 = 133+件

**Files:**
- Modify: `crates/cobol-parser/src/data_div.rs` (parse_file_description, ~line 113-160)

**修正内容:**
- FD句パースループ内に `DATA RECORD IS` / `DATA RECORDS ARE` ブランチ追加
- `TokenKind::Data` の次が `TokenKind::Record` の場合にマッチ
- `IS`/`ARE` をオプション消費
- 1つ以上のレコード名識別子をパース
- 廃止予定句なので警告は不要、単にスキップでも可

### Task A2: NC104A/NC105A の `DATA RECORD IS` 起因エラー修正

Task A1で同時解消される見込み。

### Task A3: ベースライン再取得

全12モジュールで `run_nist.sh --all` を実行し、修正後の正確な状態を記録。

---

## Phase B: NC モジュール修正（46 COMPILE_ERROR → 0目標）

### Task B1: パーサー修正群（11プログラム）

| サブタスク | 対象 | 内容 |
|---|---|---|
| B1a | NC103A, NC174A, NC254A | `SOURCE-COMPUTER. computer-name.` リテラル値パース |
| B1b | NC201A | `PERFORM ... TEST BEFORE/AFTER` パース |
| B1c | NC211A, NC225A, NC250A | 参照変更 `(start:length)` パース |
| B1d | NC235A, NC245A | `IS NUMERIC` / `IS ALPHABETIC` クラス条件パース |
| B1e | NC102A | `PERFORM proc IDENTIFIER TIMES` パース |
| B1f | NC302M | `ALTER` 文パース、`STOP "literal"` パース |
| B1g | NC109M | 継続行での長い識別子 |

### Task B2: Sema修正群（14プログラム）

| サブタスク | 対象 | 内容 |
|---|---|---|
| B2a | NC206A-NC209A, NC238A, NC246A | 修飾名(OF/IN)解決の修正 |
| B2b | NC202A, NC252A, NC253A | CORRESPONDING展開 / RENAMES(66レベル) |
| B2c | NC108M, NC116A | SPECIAL-NAMES スイッチ条件名の登録 |
| B2d | NC127A | FDレコード名のスコープ解決 |
| B2e | NC204M, NC218A, NC248A | WITH/TRUE/部分参照の識別子問題 |

### Task B3: Codegen修正群（18プログラム）

| サブタスク | 対象 | 内容 |
|---|---|---|
| B3a | NC132A, NC133A | OCCURS項目の配列C宣言 |
| B3b | NC122A, NC221A, NC237A | 多次元/OCCURS配列のC型 |
| B3c | NC125A, NC135A, NC224A, NC243A | 英数字/数値混合型のC変換 |
| B3d | NC123A | COMP-3/PACKED-DECIMAL型のC表現 |
| B3e | NC109M, NC244A | REDEFINES union アクセス |
| B3f | NC233A, NC237A, NC247A | INDEXED BY 変数のC宣言 |
| B3g | NC222A | FDレコードレベル変数の生成 |
| B3h | NC102A | 段落関数の重複定義防止 |
| B3i | NC302M | ALTER文のcodegen |

### Task B4: INSPECT判定プログラムの検証（48プログラム）

コンパイル・実行は成功しているがPASS/FAILの自動判定ができていないもの。
出力にCCVS結果行(PASS/FAIL)が含まれるか確認し、判定ロジックを改善。

---

## Phase C: IC モジュール修正（20 COMPILE_ERROR → 0目標）

### Task C1: コンマ区切り識別子リスト（8件）
- `MOVE ZERO TO DN3, DN4` / `CALL ... USING DN1, DN2`
- `PROCEDURE DIVISION USING a, b, c`

### Task C2: 複数プログラム構成（5件）
- `END PROGRAM` 後の次の `IDENTIFICATION DIVISION` パース

### Task C3: `USE GLOBAL AFTER ERROR PROCEDURE`（3件）

### Task C4: その他IC固有修正（4件）
- struct重複定義、expected integer、undefined data name

---

## Phase D: IF モジュール（再実行で解消見込み）

ビルドは現在成功しているため、再実行で45件全て解消の可能性大。
残存エラーがあれば個別対応。

---

## Phase E: 残りモジュール修正

### Task E1: IX — DECLARATIVES節パース（29件）
### Task E2: SM — COPY ... OF library-name 対応（13件）
### Task E3: RL — RELATIVE組織ファイルcodegen（26件）
### Task E4: ST/RW/DB/SG — Task A1で解消後の残存エラー修正
### Task E5: OB — 確認（サンプルは成功済み）

---

## 進捗記録

修正完了ごとに `docs/production-gaps.md` Phase 7 セクションの結果テーブルを更新する。
