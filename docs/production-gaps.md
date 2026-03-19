# 製品レベルまでの残課題一覧

作成日: 2026-03-01
更新日: 2026-03-19
現在のテスト数: 567（ユニット + E2E）
前回24項目（v1）完了済み。v2残課題の Phase 1-8 完了。Phase 9 部分完了。

---

## 凡例

- **parser** — パーサ未実装（パースエラーになる）
- **codegen** — コード生成が不十分
- **runtime** — ランタイム関数が不足/スタブ
- **sema** — 意味解析が不足
- **hir** — HIR loweringが不足
- **test** — E2E検証が不十分

---

## 1. 致命的：パースすらできない

### 1-1. JSON GENERATE / JSON PARSE 文 — parser

- **場所**: `proc_div.rs` の `parse_statement()` に
  JSON系トークンのマッチなし
- **問題**: TokenKind::Json は lexer に存在するが、パーサが未対応
- **影響**: JSON文を含むプログラムは全てパースエラー
- **HIR/codegen**: HirStatement::JsonGenerate/JsonParse は定義済み、
  codegenもランタイム呼び出し生成済み
- **規格**: COBOL 2014+

### 1-2. XML GENERATE / XML PARSE 文 — parser

- **場所**: `proc_div.rs` の `parse_statement()` に
  XML系トークンのマッチなし
- **問題**: TokenKind::Xml は lexer に存在するが、パーサが未対応
- **影響**: XML文を含むプログラムは全てパースエラー
- **HIR/codegen**: HirStatement::XmlGenerate/XmlParse は定義済み、
  codegenも生成済み
- **規格**: COBOL 2002+

### 1-3. VALIDATE 文 — parser

- **場所**: `proc_div.rs` の `parse_statement()` に
  Validateトークンのマッチなし
- **問題**: TokenKind::Validate は lexer に存在するが、パーサが未対応
- **影響**: VALIDATE文を含むプログラムは全てパースエラー
- **HIR/codegen**: HirStatement::Validate は定義済み
- **規格**: COBOL 2014+

---

## 2. 高：コンパイルは通るが機能が不足

### 2-1. 組み込み関数（数学系）が未実装 — codegen

- **場所**: `codegen.rs` 3229-3293行 —
  emit_expr の IntrinsicCall マッチ
- **現在対応済み**: LENGTH, NUMVAL, NUMVAL-C, MAX, MIN,
  MOD, INTEGER, INTEGER-PART, ORD, CHAR
- **未対応一覧**:
  - ABS, SQRT, EXP, LOG, LOG10（数学関数）
  - SIN, COS, TAN, ASIN, ACOS, ATAN（三角関数）
  - FLOOR, CEILING, FRACTION-PART（丸め）
  - MEAN, MEDIAN, VARIANCE, STANDARD-DEVIATION（統計）
  - SUM（集約）
  - FACTORIAL, REM, RANDOM, ANNUITY, PRESENT-VALUE
- **修正方針**: C標準ライブラリの対応する関数にマッピング
- **規格**: COBOL-85+（一部COBOL 2002+）

### 2-2. 組み込み関数（文字列系）が未実装 — codegen

- **場所**: `codegen.rs` — emit_expr の IntrinsicCall マッチ
- **現在対応済み**: UPPER-CASE, LOWER-CASE, REVERSE,
  TRIM, LENGTH
- **未対応一覧**:
  - CONCATENATE（文字列連結）
  - SUBSTITUTE, SUBSTITUTE-CASE（文字列置換）
  - NATIONAL-OF, DISPLAY-OF（文字コード変換）
  - HEX-OF, HEX-TO-CHAR（16進変換）
  - ORD-MAX, ORD-MIN（順序位置）
  - STORED-CHAR-LENGTH
- **修正方針**: ランタイム関数を追加し、codegenでマッピング
- **規格**: COBOL 2002+

### 2-3. 組み込み関数（日付系）が未実装 — codegen, runtime

- **場所**: `codegen.rs` / `intrinsic.rs`
- **現在対応済み**: CURRENT-DATE
- **未対応一覧**:
  - DATE-OF-INTEGER, INTEGER-OF-DATE
  - DAY-OF-INTEGER, INTEGER-OF-DAY
  - DATE-TO-YYYYMMDD, YEAR-TO-YYYY, DAY-TO-YYYYDDD
  - TEST-DATE-YYYYMMDD, TEST-DAY-YYYYDDD
  - WHEN-COMPILED
  - LOCALE-DATE, LOCALE-TIME
- **修正方針**: ランタイムに日付変換関数を追加
- **規格**: COBOL-85+（一部COBOL 2002+）

### 2-4. RENAMES句（レベル66）がHIR/codegenで無視 — hir, codegen

- **場所**: パーサは `parse_renames_clause()` で対応済み、
  AST `DataItem.renames` に格納
- **問題**: HIR loweringでrenames情報を一切処理しない
- **影響**: レベル66のデータ項目参照が未定義動作
- **修正方針**: HIRでREDEFINES類似のエイリアス生成
- **規格**: COBOL-85+

### 2-5. 添字付きターゲットの算術文 — codegen

- **場所**: `codegen.rs` — 算術文のターゲット生成
- **問題**: `ADD 1 TO TABLE(IDX)` のような添字付きターゲットが
  正しく生成されないケースがある
- **影響**: テーブル要素への直接算術が壊れる可能性
  （COMPUTEで代替可能）
- **修正方針**: ターゲット生成時に添字展開を統一
- **規格**: COBOL-85+

### 2-6. SORT INPUT/OUTPUT PROCEDUREの動作不完全 — codegen, runtime, test

- **場所**: `codegen.rs` 2001-2120行、`sort_merge.rs`
- **問題**: プロシージャ名は保持されるが、
  RELEASE/RETURNとの連携が不完全。E2E検証なし
- **影響**: 手続き型ソートが正しく動作しない可能性
- **修正方針**: RELEASE→ソートバッファ追加、
  RETURN→ソートバッファ取得のランタイム連携
- **規格**: COBOL-85+

### 2-7. DECLARATIVES USE文とファイルI/O例外の連携 — codegen

- **場所**: `codegen.rs` 70-174行 —
  declarativesコード生成済みだが
- **問題**: ファイルI/O操作時に
  DECLARATIVES例外ハンドラが自動発火しない
- **影響**: USE AFTER EXCEPTION ON file のハンドラが呼ばれない
- **修正方針**: ファイルI/O後にステータスチェック→
  declarativeハンドラ呼び出しを挿入
- **規格**: COBOL-85+

### 2-8. 例外のネスト・伝播が不完全 — runtime

- **場所**: `exception.rs` — setjmp/longjmpベースの例外
- **問題**: 例外ハンドラ内での例外、
  CALLスタック越えの例外伝播が未対応
- **影響**: 複雑な例外処理フローが壊れる
- **修正方針**: 例外スタックのネスト管理
- **規格**: COBOL 2002+

### 2-9. ファイルI/Oのステータスコードが不完全 — runtime

- **場所**: `file_io.rs` — 各操作のステータスコード
- **問題**: 一部の操作で正しいFILE STATUSコードが設定されない
- **影響**: FILE STATUS変数を参照する
  エラーハンドリングが壊れる
- **修正方針**: 全操作でCOBOL標準のステータスコード
  (00, 10, 21, 22, 23, 30, 35, etc.)を返す
- **規格**: COBOL-85+

### 2-10. 索引ファイル（INDEXED）のREWRITE/DELETE — runtime, test

- **場所**: `file_io.rs`
- **問題**: 基本的なOPEN/READ/WRITEは動くが、
  REWRITE/DELETEの動作がE2E未検証
- **影響**: 索引ファイル更新処理が壊れる可能性
- **規格**: COBOL-85+

### 2-11. 相対ファイル（RELATIVE）操作 — runtime, test

- **場所**: `file_io.rs`
- **問題**: 相対ファイルの各アクセスモード
  （SEQUENTIAL/RANDOM/DYNAMIC）の動作がE2E未検証
- **影響**: 相対ファイルを使うプログラムの信頼性が不明
- **規格**: COBOL-85+

### 2-12. ランタイムテストのファイルID競合 — runtime, test

- **場所**: `file_io.rs`, `sort_merge.rs` のテスト
- **問題**: 複数テストがファイルID 500/501を共有。
  並列実行時にグローバルFILE_TABLEで競合
- **影響**: `test_merge_two_files` が間欠的に失敗
- **修正方針**: テストごとにユニークなファイルIDを使用、
  またはserial_test
- **規格**: N/A（テスト品質）

---

## 3. 中：機能が骨格のみ / 未接続

### 3-1. SCREEN SECTION — hir, codegen

- **場所**: パーサは汎用DataItemとしてパース、
  HIRでは条件名収集のみ
- **問題**: SCREEN固有の属性（LINE, COLUMN, BLANK,
  HIGHLIGHT, FOREGROUND-COLOR等）が無視
- **影響**: ACCEPT/DISPLAYのスクリーン指定が動作しない
- **修正方針**: ncurses/termios系のランタイム関数を追加し、
  SCREEN項目を専用HIRノードに変換
- **規格**: COBOL-85拡張

### 3-2. REPORT SECTION — hir, codegen

- **場所**: パーサは汎用DataItemとしてパース、
  HIRでは条件名収集のみ
- **問題**: REPORT固有の属性（TYPE, LINE, COLUMN,
  SOURCE, SUM, GROUP INDICATE等）が無視
- **影響**: INITIATE/GENERATE/TERMINATEが機能しない
- **修正方針**: レポートライタのランタイム実装が必要
- **規格**: COBOL-85+

### 3-3. COMMUNICATION SECTION — hir, codegen

- **場所**: パーサは汎用DataItemとしてパース、
  HIRでは条件名収集のみ
- **問題**: COMMUNICATION固有の記述が無視
- **影響**: SEND/RECEIVE/ENABLE/DISABLE/PURGEが機能しない
- **備考**: COBOL 2002で廃止。実装優先度は最低
- **規格**: COBOL-85（COBOL 2002で廃止）

### 3-4. OOP機能のE2E検証不足 — test

- **場所**: `codegen.rs` 4367-4465行 —
  emit_classes は実装済み
- **問題**: CLASS-ID/INTERFACE-ID/METHOD-ID/INVOKEの
  codegenは存在するが、E2E実行検証なし
- **影響**: 実際のプログラムで動くか不明
- **修正方針**: OOP COBOLプログラムのE2Eテスト追加
- **規格**: COBOL 2002+

### 3-5. ユーザ定義関数（FUNCTION-ID）のE2E検証不足 — test

- **場所**: `codegen.rs` 4470-4510行 —
  emit_functions は実装済み
- **問題**: FUNCTION-IDのcodegenは存在するが、
  E2E実行検証なし
- **影響**: ユーザ定義関数を使うプログラムの動作が不明
- **規格**: COBOL 2002+

### 3-6. TYPEDEF のE2E検証不足 — test

- **場所**: `codegen.rs` 4513-4522行 —
  emit_typedefs は実装済み
- **問題**: codegenは存在するが、E2E実行検証なし
- **規格**: COBOL 2014+

### 3-7. サブプログラム CALL + GOBACK のE2E検証不足 — test

- **場所**: codegen/runtime はsetjmp/longjmpベースで実装済み
- **問題**: E2Eネイティブ実行での往復検証が不十分
- **影響**: 複数プログラムの連携動作が不明
- **規格**: COBOL-85+

### 3-8. PERFORM THRU のセクション横断 — test

- **場所**: `codegen.rs` —
  セクション名をパラグラフとして登録済み
- **問題**: 複数セクションをまたぐ
  PERFORM THRUのE2E検証が不十分
- **規格**: COBOL-85+

### 3-9. NATIONAL（日本語/Unicode）データ型 — sema, codegen, runtime

- **場所**: lexer/parser/ASTで部分的に扱われるが、
  codegen/runtimeで未対応
- **問題**: PIC N のデータ、NATIONAL-OF/DISPLAY-OF変換、
  日本語文字列操作が未対応
- **影響**: 日本語COBOLプログラムのデータ処理が壊れる
- **修正方針**: UTF-16/UTF-32ベースの
  NATIONALランタイム関数を実装
- **規格**: COBOL 2002+

---

## 4. 低：エッジケース / 細かい不足

### 4-1. EXIT文の完全なセマンティクス — parser

- **場所**: `proc_div.rs` 1555行 —
  `EXIT` 単体が `Statement::Continue` に変換
- **問題**: EXIT（パラグラフ終了）の正確なセマンティクスが
  CONTINUEと同一視
- **規格**: COBOL-85+

### 4-2. 数値編集PICTUREの検証不足 — sema

- **場所**: `picture_analyzer.rs`
- **問題**: 数値編集ピクチャ
  （Z, \*, CR, DB, +, -, B, 0, /）のバリデーションが限定的
- **規格**: COBOL-85+

### 4-3. グループ項目間のMOVEの精度 — codegen

- **場所**: `codegen.rs` — MOVE文のグループ間処理
- **問題**: グループ→グループのMOVEがバイト単位コピーに
  なるべきだが、個別変数コピーの可能性
- **規格**: COBOL-85+

### 4-4. EVALUATE ALSOの複雑なケース — test

- **場所**: パーサ・codegen共に実装済み
- **問題**: 3つ以上のALSOサブジェクト、
  ネストした条件のE2E検証不足
- **規格**: COBOL-85+

### 4-5. 参照変更（Reference Modification）のネスト — codegen

- **場所**: `codegen.rs` 3295-3307行
- **問題**: `WS-FIELD(1:3)(1:2)` のような
  多段参照変更が未対応
- **規格**: COBOL-85+

### 4-6. COPYBOOKの完全対応 — parser

- **場所**: lexerのSourceReaderレベル
- **問題**: COPY文のREPLACING句、ネストしたCOPY、
  ライブラリ名指定の完全性が未検証
- **規格**: COBOL-85+

---

## 5. 完了済み（v1の24項目）

v1で対応した24項目は全て完了済み。

1. GOTOラベル生成（旧1-1）
2. SORT USINGシグネチャ（旧1-2）
3. CORRESPONDINGフラット化（旧1-3）
4. グループ項目の構造体化（旧1-4）
5. GIVING対応（旧2-1）
6. INVALID KEY / ON OVERFLOW（旧2-2）
7. ACCEPT FROM DATE/TIME（旧2-3）
8. EXIT PARAGRAPH/SECTION（旧2-4）
9. GOBACK サブプログラム（旧2-5）
10. ALL figurative constant（旧2-6）
11. ファイル組織（旧2-7）
12. CALL ON EXCEPTION（旧2-8）
13. DECLARATIVES パース（旧3-1）
14. EVALUATE ALSO（旧3-2）
15. SORT INPUT/OUTPUT PROCEDURE（旧3-3）
16. PERFORM THRU セクション横断（旧3-4）
17. SET ADDRESS OF（旧3-5）
18. ALLOCATE CHARACTERS（旧3-6）
19. 組み込み関数文字列戻り値（旧3-7）
20. subscripted DISPLAY（旧3-12）
21. SCREEN/REPORT/COMMUNICATION HIR lowering（旧3-8）
    — 条件名収集のみ。3-1〜3-3で再掲
22. XML PARSE codegen（旧3-9）
    — codegenのみ。1-2でパーサ未実装として再掲
23. RAISE 例外伝播（旧3-10）
24. INVOKE OOP vtable（旧3-11）

---

## 6. 推奨実装順序

### Phase 1: パーサ修正 — 完了

全プログラムが少なくともパースできるようにする。

1. ~~JSON GENERATE / JSON PARSE のパーサ追加（1-1）~~ — 完了
2. ~~XML GENERATE / XML PARSE のパーサ追加（1-2）~~ — 完了
3. ~~VALIDATE のパーサ追加（1-3）~~ — 完了

### Phase 2: 組み込み関数の充実 — 完了

COBOL-85互換性の確保。

1. ~~数学関数の追加（2-1）~~ — 完了（38関数）
2. ~~日付関数の追加（2-3）~~ — 完了
3. ~~文字列関数の追加（2-2）~~ — 完了

### Phase 3: データ処理の精度向上 — 完了

1. ~~RENAMES句の実装（2-4）~~ — 完了（#define エイリアス）
2. ~~添字付きターゲット算術の修正（2-5）~~ — 完了（HirExpr化）
3. ~~ファイルI/Oステータスコードの完全化（2-9）~~ — 完了

### Phase 4: E2E検証の充実 — 完了

1. ~~SORT INPUT/OUTPUT PROCEDUREのE2E検証（2-6）~~ — 完了
2. ~~OOP機能のE2E検証（3-4）~~ — 完了
3. ~~ユーザ定義関数のE2E検証（3-5）~~ — 完了
4. ~~サブプログラムCALL+GOBACKのE2E検証（3-7）~~ — 完了
5. ~~DECLARATIVES例外連携の検証（2-7）~~ — 完了
6. ~~索引・相対ファイルのE2E検証（2-10, 2-11）~~ — 完了

### Phase 5: 拡張機能 — 完了

1. ~~SCREEN SECTION（3-1）~~ — 完了（ANSI エスケープ）
2. ~~REPORT SECTION（3-2）~~ — 完了（スタブ codegen）
3. ~~NATIONAL データ型（3-9）~~ — 完了（UTF-16 runtime）

### Phase 6: エッジケース・品質向上 — 完了

1. ~~残りの低優先度項目（4-1〜4-6）~~ — 完了
2. ~~フレーキーテストの修正（2-12）~~ — 完了

### Phase 7: NIST CCVS 85 適合性検証 — テスト基盤構築済み

NIST COBOL-85テストスイート（CCVS 85）による適合性検証。
約9,700個のテストケース、12モジュール構成。
GnuCOBOLはv2.2で99.79%通過（9,688/9,708）。

**構築済みの基盤:**

- `tests/nist/extract.pl` — newcob.val抽出スクリプト
- `tests/nist/run_nist.sh` — テスト実行・結果集計スクリプト
- Makefile統合（`make nist MODULE=NC`）

**未実施:** newcob.valのダウンロードと実際のテスト実行。

段階的に以下のモジュール順で通過率を上げる。

1. **NC (Nucleus)** — 約95プログラム。最優先
2. **SM (Source Manipulation)** — 約17プログラム
3. **IC (Inter-program Communication)** — 約25プログラム
4. **SQ (Sequential I/O)** — 順編成ファイルI/O
5. **IF (Intrinsic Functions)** — 組み込み関数
6. **IX (Indexed I/O)** — 索引編成ファイルI/O
7. **RL (Relative I/O)** — 相対編成ファイルI/O
8. **ST (SORT/MERGE)** — ソート・マージ
9. **RW (Report Writer)** — REPORT SECTION
10. **DB (Debugging)** — デバッグ機能
11. **SG (Segmentation)** — セグメント機能
12. **OB (Obsolete)** — 廃止予定機能（低優先）

製品版の基準: NC + IF + SQ + IC で95%以上通過。

### Phase 8: 診断メッセージの品質向上 — 完了

1. ~~パースエラーの位置情報改善~~ —
   完了（ariadne統合、rustc風ソース注釈付きカラー出力）
2. ~~型エラーの具体的なメッセージ~~ —
   完了（PIC情報付きの詳細エラー）
3. ~~警告レベルの導入~~ —
   完了（WarningLevel: All/Default/None/Error, `-W` フラグ）
4. ~~複数エラーの継続報告~~ —
   完了（既存のDiagnosticReporter蓄積方式を改善）
5. ~~エラーコード体系~~ —
   完了（`COBC-E001` 形式のエラーコード）

### Phase 9: 実業務プログラムでの検証 — 部分完了

1. **GnuCOBOL互換テスト** — 未実施
   （newcob.val取得後にNISTテストで代替検証可能）
2. **オープンソースCOBOLプログラムの動作検証** — 未実施
3. ~~性能ベンチマーク~~ —
   完了（tests/benchmark/ に算術・文字列・ファイルI/Oベンチマーク）
4. ~~ドキュメント整備~~ —
   完了（docs/user-guide.md, docs/cobol-standards.md）
