# TASKS

マイルストーン: M1
ゴール: NIST CCVS 85 の 100% pass に向けた仕様かい離台帳と実装順序を確定する

## ワークフロールール

- タスク着手時にステータスを 🚧 に更新する
- タスク完了時にステータスを ✅ に更新する
- DependsOn のタスクがすべて ✅ でないタスクには着手しない

## ステータス表記ルール

| Status | 意味 |
| ---- | ----- |
| ⏳ | 未着手、TODO |
| 🚧 | 作業中、IN_PROGRESS |
| 🧪 | 確認待ち、REVIEW |
| ✅ | 完了、DONE |
| 🚫 | 中止、CANCELLED |

## タスク一覧

| ID | Status | Summary | DependsOn |
| ---- | ---- | ---- | ---- |
| TASK-001 | ✅ | 現行NIST結果の仕様別ベースラインを固定する | - |
| TASK-002 | ✅ | CCVS出力判定の共通分類表を作る | TASK-001 |
| TASK-003 | ✅ | 19件のCErrをcodegen欠陥別に再現する | TASK-001 |
| TASK-004 | ✅ | 制御フロー仕様差分の代表reproを作る | TASK-002 |
| TASK-005 | ✅ | ファイルI/O仕様差分の代表reproを作る | TASK-002 |
| TASK-006 | ✅ | 数値変換と算術仕様差分の代表reproを作る | TASK-002 |
| TASK-007 | ✅ | 組込み関数仕様差分の代表reproを作る | TASK-002 |
| TASK-008 | ✅ | COPYと診断仕様差分の代表reproを作る | TASK-002 |
| TASK-009 | ✅ | REPORT/SORT/SEGMENT仕様差分を分離する | TASK-002,TASK-005 |
| TASK-010 | ✅ | 100% passまでの実装ロードマップを確定する | TASK-003,TASK-004,TASK-005,TASK-006,TASK-007,TASK-008,TASK-009 |
| TASK-011 | ✅ | Backlogを実装タスクへ再編する | TASK-010 |
| IMPL-001 | ✅ | HIR data item初期化とcodegen構造体契約を修正する | TASK-003 |
| IMPL-002 | ✅ | scalar storageとbyte pointerのruntime ABIを分離する | IMPL-001 |
| IMPL-003 | ✅ | decimal/display値をhelper ABI単位で修正する | IMPL-001,TASK-006 |
| IMPL-004 | ✅ | sort key flatten由来のCErrを修正する | IMPL-001,TASK-009 |
| IMPL-005 | ✅ | linkage group layout由来のCErrを修正する | IMPL-001,TASK-006 |
| IMPL-006 | ✅ | NIST summary/first-fail parserを共通化する | TASK-002 |
| IMPL-007 | ✅ | warning countとcompile log分類を安定化する | IMPL-006,TASK-008 |
| IMPL-008 | ✅ | print/report/stdout captureをrunnerへ統合する | IMPL-006,TASK-009 |
| IMPL-009 | ✅ | blank reportとsummary不在のreasonを再分類する | IMPL-008 |
| IMPL-010 | ✅ | `PERFORM`/`PERFORM THRU`のHIR CFGを再設計する | IMPL-006,TASK-004 |
| IMPL-011 | ✅ | `GO TO`/`ALTER`/fallthroughのdispatch契約を実装する | IMPL-010,TASK-008 |
| IMPL-012 | ✅ | `USE FOR DEBUGGING`とdeclarative突入を実装する | IMPL-010 |
| IMPL-012A | ✅ | `ALL REFERENCES`と識別子debug対象を文脈別に実装する | IMPL-010 |
| IMPL-012B | ✅ | `USE FOR DEBUGGING`のOPEN/MERGE/DISABLE対象イベントを実装する | IMPL-012A,IMPL-014,IMPL-029 |
| IMPL-012C | ✅ | `ALL PROCEDURES`のsection/paragraph二重突入順を実装する | IMPL-011,IMPL-012A |
| IMPL-010A | ✅ | `PERFORM section`復帰境界とsection内paragraph範囲を修正する | IMPL-010 |
| IMPL-018A | ✅ | 添字付き数値項目とREDEFINESの更新/比較を修正する | IMPL-010A,IMPL-012 |
| IMPL-013 | ✅ | 例外句とprogram terminationの制御辺を実装する | IMPL-010 |
| IMPL-014 | ✅ | sequential fileのopen/read/write/status状態機械を実装する | IMPL-013,TASK-005 |
| IMPL-015 | ✅ | indexed fileのkey/cursor/invalid-key状態機械を実装する | IMPL-014 |
| IMPL-016 | ✅ | relative fileのrelative key/cursor/delete状態機械を実装する | IMPL-014 |
| IMPL-017 | ⏳ | LINAGEとWRITE ADVANCINGのoutput positioningを実装する | IMPL-008,IMPL-014 |
| IMPL-018 | 🚧 | PICTURE metadataをsemaからruntimeまで一貫して運ぶ | IMPL-003,TASK-006 |
| IMPL-019 | 🚧 | MOVE conversionをsource/target category単位で一元化する | IMPL-018 |
| IMPL-020 | ⏳ | decimal算術、丸め、SIZE ERRORを一元化する | IMPL-018 |
| IMPL-021 | ⏳ | edited numeric formatter/parserを実装する | IMPL-019 |
| IMPL-022 | ⏳ | intrinsic argument flattenと戻り値categoryを実装する | IMPL-018,TASK-007 |
| IMPL-023 | ⏳ | numeric/math/aggregate intrinsicを実装する | IMPL-020,IMPL-022 |
| IMPL-024 | ⏳ | ordinal/string/date/random intrinsicを実装する | IMPL-022 |
| IMPL-025 | ⏳ | COPY REPLACINGのtoken単位置換を実装する | TASK-008 |
| IMPL-026 | ⏳ | copybook library-nameとcontinuation/quote処理を実装する | IMPL-025 |
| IMPL-027 | ✅ | obsolete/non-conforming warning診断を構文単位で実装する | IMPL-007 |
| IMPL-028 | ⏳ | Report Writer lifecycleとcounterを実装する | IMPL-008,IMPL-017,TASK-009 |
| IMPL-029 | ⏳ | SORT/RELEASE/RETURNとsort key compareを実装する | IMPL-004,IMPL-014,IMPL-020,TASK-009 |
| IMPL-030 | ⏳ | MERGEとsame sort-merge areaを実装する | IMPL-029 |
| IMPL-031 | ⏳ | segmentation runtime stateとdiagnostic境界を実装する | IMPL-011,IMPL-027,TASK-009 |
| IMPL-032 | ⏳ | NIST full gateをCI必須checkへ昇格する | IMPL-001-IMPL-031 |
| IMPL-033 | ⏳ | 残余未分類FAILを仕様カテゴリへ再分類して0にする | IMPL-032 |

## タスク詳細（補足が必要な場合のみ）

### TASK-001

- 補足: 2026-05-03 の再実行結果は
  `391 total / 112 pass / 260 fail / 0 ready / 19 CErr / 0 RErr / 28%`。
- 補足: 主な未達は `NC 71 fail + 8 CErr`, `SQ 48 fail`,
  `IF 32 fail`, `IX 24 fail`, `RL 18 fail`,
  `ST 16 fail + 6 CErr`。
- 注意: `Ready` は 0 なので、未実行問題ではなく仕様実装差分として扱う。
- 成果物: モジュール別、状態別、仕様カテゴリ別のベースライン表。

#### TASK-001 成果物

実行条件:

- 実行日: 2026-05-03
- コマンド: `make nist-run NIST_JOBS=5`
- 集計コマンド: `make nist-summary`
- CErr分類コマンド: `make nist-compile-errors`
- 判定前提: `Ready` が 0 なので、全コンパイル成功分は実行済み

モジュール別ベースライン:

| Module | Total | Pass | Fail | Ready | CErr | RErr | Rate |
| ---- | ----: | ----: | ----: | ----: | ----: | ----: | ----: |
| CM | 9 | 6 | 3 | 0 | 0 | 0 | 66% |
| DB | 15 | 5 | 10 | 0 | 0 | 0 | 33% |
| EX | 1 | 0 | 1 | 0 | 0 | 0 | 0% |
| IC | 25 | 15 | 8 | 0 | 2 | 0 | 60% |
| IF | 45 | 13 | 32 | 0 | 0 | 0 | 28% |
| IX | 29 | 5 | 24 | 0 | 0 | 0 | 17% |
| NC | 95 | 16 | 71 | 0 | 8 | 0 | 16% |
| OB | 5 | 1 | 4 | 0 | 0 | 0 | 20% |
| RL | 26 | 8 | 18 | 0 | 0 | 0 | 30% |
| RW | 6 | 0 | 6 | 0 | 0 | 0 | 0% |
| SG | 13 | 1 | 9 | 0 | 3 | 0 | 7% |
| SM | 13 | 3 | 10 | 0 | 0 | 0 | 23% |
| SQ | 84 | 36 | 48 | 0 | 0 | 0 | 42% |
| ST | 25 | 3 | 16 | 0 | 6 | 0 | 12% |
| TOTAL | 391 | 112 | 260 | 0 | 19 | 0 | 28% |

状態別ベースライン:

| Status | Count | 解釈 |
| ---- | ----: | ---- |
| PASS | 112 | 現行実装でCCVS判定が通過している |
| FAIL | 260 | 実行済みだがCCVS期待結果と一致していない |
| COMPILE_ERROR | 19 | 生成Cまたはcodegen契約が破綻している |
| COMPILED/Ready | 0 | コンパイル済み未実行は残っていない |
| RUNTIME_ERROR | 0 | 異常終了として残ったものはない |

仕様カテゴリ別ベースライン:

| 仕様カテゴリ | 主な対象 | 現在の症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| C codegen契約 | IC, NC, SG, ST | 19 CErr | `cobol-codegen`, HIR data model |
| 制御フロー | CM, DB, IF, NC, OB | 0件実行相当のFAILや段落不達 | parser, HIR lowering, codegen |
| 数値・算術・PICTURE | NC, IC, CM, IF | 大量のCCVS FAILとdecimal型不整合 | sema, codegen, runtime decimal |
| ファイルI/O | IX, RL, SQ, ST | detail行不一致、cursor/status不一致 | HIR metadata, codegen, runtime file I/O |
| SORT/MERGE | ST, SQ, OB | sort出力と後続READの不一致 | codegen, runtime sort/merge |
| REPORT/出力捕捉 | RW, SG, SQ, ST | blank reportとsummary不在 | runtime output, NIST verifier |
| COPY/診断 | SM, SG, SQ, RL | warning flag不一致、source操作差分 | preprocessor, sema diagnostics |
| intrinsic function | IF | 関数結果・境界値不一致 | runtime intrinsic, type conversion |

CErrファミリ別ベースライン:

| CErrファミリ | Count | Programs |
| ---- | ----: | ---- |
| int64をpointer引数へ渡す生成C | 7 | SG104A, SG105A, SG106A, ST108A, ST118A, ST125A, ST127A |
| generated macro name collision | 3 | NC401M, ST104A, ST106A |
| CobolDecimalをint64引数へ渡す生成C | 3 | NC118A, NC123A, NC177A |
| char配列への直接代入 | 1 | NC125A |
| char配列をinteger引数へ渡す生成C | 1 | NC109M |
| int64 pointerをinteger引数へ渡す生成C | 1 | NC224A |
| CobolDecimalをlong long引数へ渡す生成C | 1 | NC105A |
| group value/raw型名不整合 | 2 | IC106A, IC216A |

未達規模別の優先度:

| 優先 | 対象 | 理由 |
| ---- | ---- | ---- |
| 1 | CErr 19件 | コンパイル不能は後続FAIL分類を妨げる |
| 2 | NC 79件未達 | COBOL核仕様で横断影響が最大 |
| 3 | SQ/IX/RL/ST 106件FAIL | ファイルI/O状態機械の共通欠陥が疑われる |
| 4 | IF 32件FAIL | intrinsicと条件/数値変換の共通欠陥が疑われる |
| 5 | RW/SG/SM/OB/DB/CM | 出力、COPY、REPORT、制御フローの残余を切り分ける |

### TASK-002

- 補足: 現在の失敗理由は `ccvs-first-fail` が 184 件、
  `detail-paragraph-mismatch` が 32 件、
  `no-decisive-ccvs-summary` が 25 件、
  `blank-or-empty-report` が 12 件、
  `warning-flags-missing` が 7 件。
- 注意: これは最終原因ではなく観測分類である。
  ここで実装修正を始めず、各失敗を仕様カテゴリへ写像する。
- 成果物: CCVS出力、最初の失敗段落、期待値/実値、生成物有無を
  同じ形式で読める分類表。

#### TASK-002 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/**/*.reason`
- 参照実装: `tests/nist/run_nist.sh`, `tests/nist/verifiers/lib.sh`
- 前提: TASK-001のベースラインで `Ready` は 0

共通分類表:

| Reason | Count | 判定意味 | 主な証跡 | 次の分類先 |
| ---- | ----: | ---- | ---- | ---- |
| `ccvs-first-fail` | 184 | CCVS帳票にFAIL段落がある | `.log`, first fail details | TASK-004,TASK-005,TASK-006,TASK-007,TASK-009 |
| `detail-paragraph-mismatch` | 32 | 期待段落数と出力段落数が違う | `.log`, detail rows | TASK-004,TASK-005,TASK-009 |
| `no-decisive-ccvs-summary` | 25 | PASS/FAIL summaryを決定できない | `.log`, output channel | TASK-002,TASK-009 |
| `blank-or-empty-report` | 12 | 実行後の帳票出力が空 | `.log`, print file `P` | TASK-004,TASK-005,TASK-009 |
| `warning-flags-missing` | 7 | 期待されるcompile warningが不足 | `.compile.log` | TASK-008 |

分類ルール:

| 入力 | 抽出する値 | 目的 |
| ---- | ---- | ---- |
| `.status` | `PASS`, `FAIL`, `COMPILE_ERROR` | 集計状態を確定する |
| `.reason` | 共通reason名 | FAILの観測分類を確定する |
| `.log` | CCVS pass/fail count | 実行結果のずれを定量化する |
| `.log` | first failing paragraph | 仕様カテゴリの入口を特定する |
| `.log` | expected/computed detail | 数値、I/O、制御差分を分ける |
| `.compile.log` | warning/error count | 診断不足とCErrを分ける |
| print file `P` | 帳票出力の有無 | 出力捕捉とREPORT系を分ける |

reasonから仕様カテゴリへの一次写像:

| Reason | 制御フロー | ファイルI/O | 数値/算術 | intrinsic | COPY/診断 | REPORT/SORT |
| ---- | ---- | ---- | ---- | ---- | ---- | ---- |
| `ccvs-first-fail` | 候補 | 候補 | 候補 | 候補 | - | 候補 |
| `detail-paragraph-mismatch` | 候補 | 候補 | - | - | - | 候補 |
| `no-decisive-ccvs-summary` | 候補 | 候補 | - | - | - | 候補 |
| `blank-or-empty-report` | 候補 | 候補 | - | - | - | 候補 |
| `warning-flags-missing` | - | - | - | - | 確定候補 | - |

代表例:

| Reason | 代表例 | 読み方 |
| ---- | ---- | ---- |
| `ccvs-first-fail` | `NC101A`, `IF101A`, `IX104A` | 失敗段落の仕様を読む |
| `detail-paragraph-mismatch` | `IF`, `ST`, `RL`の複数件 | 到達段落数と制御/I/Oを読む |
| `no-decisive-ccvs-summary` | `DB104A`, `EXEC85`, `SQ109M` | 出力経路と帳票形式を読む |
| `blank-or-empty-report` | `SM103A`, `SQ303M`, `RW301M` | 帳票未生成か別経路出力を読む |
| `warning-flags-missing` | `SG`, `IX`, `SQ`, `RL`, `NC` | sema診断ルールを読む |

TASK-002時点での判断:

- `ccvs-first-fail` は最大分類だが、原因分類としては粗い。
  TASK-004以降では first failing paragraph と expected/computed detail を
  読んで仕様カテゴリへ分解する。
- `detail-paragraph-mismatch` は制御フロー不達とI/O cursor不一致の両方を
  含むため、TASK-004とTASK-005の代表reproで切り分ける。
- `no-decisive-ccvs-summary` と `blank-or-empty-report` は、実装FAILと
  verifier/出力捕捉不足が混在している。TASK-009で分離する。
- `warning-flags-missing` は実行時FAILではなく、compile-time diagnosticの
  仕様未実装としてTASK-008へ渡す。

### TASK-003

- 補足: CErrは、整数/ポインタ不整合 7 件、
  macro/name collision 3 件、decimal/int不整合 4 件、
  配列代入 2 件、group raw/value型名不整合 2 件、
  pointer/int不整合 1 件に分かれる。
- 注意: CErrはNIST全体の入口を塞ぐため、FAILより先にcodegen契約として潰す。
- 成果物: 各CErrファミリの最小COBOL入力、期待されるC表現、責務crate一覧。

#### TASK-003 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/**/*.compile.log`
- 参照コマンド: `make nist-compile-errors`
- 参照生成C: `.nist/work/run/**/nist_preproc_*.c`
- 判定前提: CErrはC compilerで停止したprogram単位で数える

CErr program別の一次分類:

| Primary family | Count | Programs |
| ---- | ----: | ---- |
| scalar storageをpointer引数へ渡す生成C | 9 | SG104A, SG105A, SG106A, ST104A, ST106A, ST108A, ST118A, ST125A, ST127A |
| Decimal値をinteger helper引数へ渡す生成C | 4 | NC105A, NC118A, NC123A, NC177A |
| char配列へ直接代入する生成C | 2 | NC125A, NC401M |
| display numeric配列をinteger値として扱う生成C | 1 | NC109M |
| reference modification対象をinteger値として扱う生成C | 1 | NC224A |
| linkage groupのraw/value typedef不整合 | 2 | IC106A, IC216A |
| TOTAL | 19 | - |

補助的に観測された警告:

| Warning | Programs | 扱い |
| ---- | ---- | ---- |
| generated macro name collision | NC401M, ST104A, ST106A | CErr主因ではなく別修正として扱う |

再現ファミリ詳細:

| Family | 代表program | 生成Cの壊れ方 | 期待するC表現 |
| ---- | ---- | ---- | ---- |
| scalar storage pointer | SG104A | `memcpy(dst, scalar, n)` | scalarは `&scalar`、配列はそのまま渡す |
| Decimal helper mismatch | NC118A | `cobol_decimal_from_int(decimal, ...)` | decimal sourceはcopy/add helperへ渡す |
| char array assignment | NC125A | `char_array = CobolDecimal` | display/edit変換後に`memcpy`する |
| display numeric as integer | NC109M | `llabs(char_array)` | `cobol_display_to_int64(ptr, len)`を挟む |
| refmod pointer as integer | NC224A | `llabs(int64_t*)` | 参照変更はsliceを値化してから変換する |
| linkage group typedef | IC106A | `_grp_*_val_*_t`未定義 | raw/value layout判定をlinkageにも揃える |

縮小repro入力の形:

| Family | COBOL入力の最小形 | 入口 |
| ---- | ---- | ---- |
| scalar storage pointer | `SORT` recordにbinary keyとdisplay keyを混在させる | ST/SG sort record flatten |
| Decimal helper mismatch | decimal itemへdecimal itemを含む算術結果を格納する | NC arithmetic codegen |
| char array assignment | numeric edited/display table itemに`VALUE`を持たせる | NC data initialization |
| display numeric as integer | display numeric group memberを数値DISPLAYへ渡す | NC DISPLAY/MOVE conversion |
| refmod pointer as integer | OCCURS/添字付き項目の参照変更を数値化する | NC reference modification |
| linkage group typedef | `LINKAGE SECTION`のgroup引数を`USING`で受ける | IC CALL linkage lowering |

責務crate:

| Family | 主責務 | 関連責務 |
| ---- | ---- | ---- |
| scalar storage pointer | `crates/cobol-codegen/src/data.rs` | `expr.rs`, runtime sort ABI |
| Decimal helper mismatch | `crates/cobol-codegen/src/expr.rs` | `stmt.rs`, `cobol-runtime/src/decimal.rs` |
| char array assignment | `crates/cobol-codegen/src/data.rs` | `context.rs`, HIR picture metadata |
| display numeric as integer | `crates/cobol-codegen/src/expr.rs` | runtime display conversion |
| refmod pointer as integer | `crates/cobol-codegen/src/expr.rs` | HIR `ReferenceModification` lowering |
| linkage group typedef | `crates/cobol-codegen/src/codegen.rs` | `cobol-hir/src/lower.rs` |

TASK-003時点での判断:

- CErrの最大群はSORT/SEGMENT固有ではなく、scalar storageとbyte pointerの
  codegen契約違反である。ST/SGを個別修正せず、データ表現変換を直す。
- `macro redefined` はログ分類では目立つが、現在のC compiler停止主因は
  別の型不一致である。BACKLOG-001では警告も別チェックとして残す。
- NC系CErrはdecimal/display/reference modificationの値表現境界が崩れている。
  TASK-006より前に、Cへ出す型変換契約を固定する必要がある。
- IC系CErrはCALL/linkageのgroup layout契約違反である。
  制御フローではなく、linkage data modelとして扱う。

### TASK-004

- 補足: 対象は `PERFORM`, `GO TO`, `ALTER`, `DECLARATIVES`,
  `USE`, `AT END`, `INVALID KEY`, `ON SIZE ERROR`。
- 補足: 影響範囲は `IF`, `NC`, `DB`, `RL`, `RW`, `SM`, `SQ`, `OB`。
- 成果物: COBOL制御移譲仕様とHIR/codegen/runtime責務の差分表。

#### TASK-004 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/**/*.reason`, `.nist/results/**/*.log`
- 参照ソース: `.nist/programs/**/**/*.cob`
- 前提: TASK-002のreason分類から制御移譲に関係する候補を抽出する

制御フロー仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| debug declaratives | `USE FOR DEBUGGING` | DEBUG内容やPROC-NAMEがずれる | parser, HIR lowering, codegen |
| transfer statements | `PERFORM`, `GO TO`, `ALTER` | 到達段落数や遷移先がずれる | parser, HIR CFG, codegen |
| exception phrases | `AT END`, `INVALID KEY`, `ON SIZE ERROR` | 例外経路が発火しない | HIR CFG, codegen, runtime status |
| program termination | `STOP`, `GOBACK`, `EXIT PROGRAM` | 実行停止や呼出元復帰がずれる | codegen, runtime program ABI |
| call/linkage control | `CALL`, `ON OVERFLOW` | 呼出先結果や例外句がずれる | HIR linkage, codegen, runtime program ABI |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| CF-001 | DB101A | `ccvs-first-fail` | `START PROGRAM` debug内容が`FALL THROUGH`になる | `USE FOR DEBUGGING`の起動契機 |
| CF-002 | NC302M | `warning-flags-missing` | `ALTER`関連の警告数が不足 | `ALTER`の解析と診断 |
| CF-003 | DB104A | `no-decisive-ccvs-summary` | SORT/AT-END系debug出力を決定できない | `USE FOR DEBUGGING`とSORT入出力 |
| CF-004 | IC223A | `ccvs-first-fail` | CALL戻り値と`ON OVERFLOW`句がずれる | `CALL`後の復帰と例外句 |
| CF-005 | OB/NC1M系 | `ccvs-first-fail` | `STOP literal`後も実行が継続する | `STOP`の終端意味論 |
| CF-006 | IF101A | `detail-paragraph-mismatch` | 期待26段落に対して16段落のみ出力 | `PERFORM THRU`範囲と条件分岐 |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| SQ201M | `INVALID KEY`やLINAGEを含み、I/O状態が主因候補 | TASK-005,TASK-009 |
| CM101M | communication statusと`NO DATA`句が主因候補 | TASK-009 |
| SM205A | COPY SD REPLACINGによる入力生成差分が主因候補 | TASK-008 |
| IF128A | `ORD-MAX`関数値が主因候補 | TASK-007 |

制御フローreproの最小形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| CF-001 | `DECLARATIVES`内に`USE FOR DEBUGGING ON paragraph`を置く | 対象段落突入時にDEBUG内容が一致する |
| CF-002 | `ALTER A TO PROCEED TO B`後に`GO TO A`を実行する | 変更後の遷移先へ一度だけ移る |
| CF-003 | `SORT ... INPUT PROCEDURE ... OUTPUT PROCEDURE`を持つ | procedure突入順とdebug hookが一致する |
| CF-004 | `CALL`成功/失敗と`ON OVERFLOW`を分ける | 成功時は通常継続、失敗時は例外句へ移る |
| CF-005 | `STOP "literal"`の直後に到達不能DISPLAYを置く | STOP後の文は実行されない |
| CF-006 | `PERFORM A THRU C`の中に`GO TO`とfallthroughを置く | 範囲終了と戻り先が一致する |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| parser | `ALTER`, `USE`, 例外句、`PERFORM THRU`境界をASTで落とさない |
| sema | 段落/section名、debug対象、ALTER対象の解決を検証する |
| HIR lowering | 通常辺、例外辺、debug辺、perform return辺を明示する |
| codegen | `_goto_target`とperform returnの優先順を一貫させる |
| runtime | program終端、CALL失敗、file statusを例外句へ伝える |
| verifier | 到達段落、debug content、終端後出力を同じ形式で記録する |

TASK-004時点での判断:

- 制御フロー不具合は、単なる`GO TO`不足ではなく、通常辺、例外辺、
  debug declarative、program terminationを同じCFGで扱えていない疑いが強い。
- `IF101A`のようなdetail段落不足は、intrinsic不一致だけでは説明できない。
  ただし最終分類はCF-006の縮小reproで確認する。
- I/O例外句は制御フローとI/O状態機械の境界なので、TASK-005と重複させず、
  TASK-004では「例外句へ制御が渡るか」だけを確認対象にする。

### TASK-005

- 補足: 対象は sequential/indexed/relative の `OPEN`, `READ`,
  `WRITE`, `REWRITE`, `DELETE`, `START`, cursor, file status。
- 補足: 影響範囲は `IX`, `RL`, `SQ`, `ST`, `SG`, `OB`, `NC`。
- 成果物: ファイル組織ごとの状態遷移表と代表repro。

#### TASK-005 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/{IX,RL,SQ,ST,OB,RW}/**/*.reason`,
  `.nist/results/{IX,RL,SQ,ST,OB,RW}/**/*.log`
- 参照ソース: `.nist/programs/{IX,RL,SQ,ST,OB,RW}/**/*.cob`
- 前提: TASK-002のreason分類から、ファイル組織、access mode、
  file status、cursor、例外句に関係する候補を抽出する

ファイルI/O仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| sequential basic | `OPEN`, `READ`, `WRITE`, `CLOSE`, `AT END` | record count、AT END、statusがずれる | HIR file metadata, runtime sequential state |
| indexed random/sequential | `RECORD KEY`, `READ`, `REWRITE`, `INVALID KEY` | key検索とstatusがずれる | sema key resolution, runtime indexed store |
| indexed dynamic | `ALTERNATE RECORD KEY`, `START`, `READ NEXT` | cursor順序と診断warningが不足 | runtime cursor, sema diagnostics |
| relative random/dynamic | `RELATIVE KEY`, `READ NEXT`, `START` | relative keyとcursor位置がずれる | runtime relative store/cursor |
| file exception phrases | `INVALID KEY`, `NOT INVALID KEY`, `AT END` | 例外句への分岐条件がずれる | HIR exception edges, codegen, runtime status |
| output positioning | `LINAGE`, `WRITE ADVANCING`, `END-OF-PAGE` | line/page counterがずれる | runtime output control, verifier |
| sort file boundary | `SD`, sort input/output file | sort前後のREAD/WRITE結果がずれる | runtime sort/merge, shared I/O |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| IO-001 | IX104A | `ccvs-first-fail` | keyed `READ`でcomputed keyが`101`、期待が`1` | indexed randomのkey検索とstatus |
| IO-002 | IX401M | `warning-flags-missing` | dynamic/alternate key系warningが10件期待に対して6件 | indexed dynamicと診断 |
| IO-003 | RL205A | `ccvs-first-fail` | `READ NEXT`でcomputedが`9`、期待が`0` | relative cursorと`START` |
| IO-004 | RL301M | `blank-or-empty-report` | relative randomの帳票が空 | relative `READ/WRITE/REWRITE/DELETE` |
| IO-005 | SQ103A | `ccvs-first-fail` | declarativeが少なくとも一度入った扱いになる | sequential `AT END`と`USE`境界 |
| IO-006 | SQ205A | `ccvs-first-fail` | 501件期待のREAD結果が空扱いになる | sequential file statusと複数file |
| IO-007 | SQ201M | `ccvs-first-fail` | `OPEN`後のLINAGE counterが`0`、期待が`1` | `LINAGE`と`WRITE ADVANCING` |
| IO-008 | OB/SQ1A系 | `ccvs-first-fail` | sequential fileの読取error countがずれる | sequential write/read内容保持 |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| ST109A | variable-length sequential fileをSORT連鎖へ渡す入口 | TASK-009 |
| ST115A | SORT用のsequential入力生成と後続SORTが混在 | TASK-009 |
| RW101A | `INITIATE REPORT`とpage counterが主因 | TASK-009 |
| SQ201M | LINAGEはI/Oだが帳票位置決めとverifier差分も含む | TASK-005,TASK-009 |

縮小repro入力の形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| IO-001 | indexed fileへkey付きrecordを書き、close/open後に既存keyと欠番keyを読む | 既存keyはrecord一致、欠番keyは`INVALID KEY`とstatus一致 |
| IO-002 | alternate key付きindexed fileで`START`後に`READ NEXT`する | alternate key順のcursorとwarning数が一致する |
| IO-003 | relative fileに欠番を含むrecordを書き、`START`後に`READ NEXT`する | cursorが次の有効relative recordへ進む |
| IO-004 | relative randomで`READ`, `WRITE`, `REWRITE`, `DELETE`を成功/失敗双方で行う | `INVALID KEY`と`NOT INVALID KEY`が期待どおり分岐する |
| IO-005 | sequential fileをN件書いてN+1回読む | N+1回目だけ`AT END`になり、不要な`USE`へ入らない |
| IO-006 | 複数sequential fileを同時に開き、各file statusを検証する | fileごとのstatusとrecord countが独立する |
| IO-007 | `LINAGE`付きoutput fileで`WRITE AFTER ADVANCING PAGE`を行う | line/page counterと`END-OF-PAGE`が一致する |
| IO-008 | sequential fileへ固定長recordを連続write/readする | record内容と件数が完全一致する |

ファイル組織ごとの状態遷移:

| Organization | State | 有効操作 | 主なstatus/例外 |
| ---- | ---- | ---- | ---- |
| sequential | closed | `OPEN INPUT/OUTPUT/EXTEND` | open失敗時はfile status設定 |
| sequential | open input | `READ`, `CLOSE` | EOFで`AT END`、通常READで次recordへ進む |
| sequential | open output/extend | `WRITE`, `CLOSE` | 書込後もcursorをrecord末尾に保つ |
| indexed | closed | `OPEN INPUT/OUTPUT/I-O/EXTEND` | key定義と重複条件を初期化する |
| indexed | open random | `READ`, `WRITE`, `REWRITE`, `DELETE` | key不在/重複で`INVALID KEY` |
| indexed | open dynamic | `START`, `READ NEXT`, `READ PREVIOUS` | `START`がcursor基準を設定する |
| relative | closed | `OPEN INPUT/OUTPUT/I-O/EXTEND` | relative key領域を初期化する |
| relative | open random | `READ`, `WRITE`, `REWRITE`, `DELETE` | relative key不在/重複で`INVALID KEY` |
| relative | open dynamic | `START`, `READ NEXT` | 欠番を飛ばし次の有効recordへ進む |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| parser | `SELECT`, `ASSIGN`, `ORGANIZATION`, `ACCESS MODE`, key句、LINAGE句を落とさない |
| sema | record key、alternate key、relative key、file status項目を解決する |
| HIR lowering | file metadata、通常辺、例外辺、cursor操作を明示する |
| codegen | file操作ごとにruntime ABIへmetadataとstatus格納先を渡す |
| runtime | organization/access mode別の状態機械と永続storeを実装する |
| verifier | 出力file、print file、record countを同じ基準で集計する |

TASK-005時点での判断:

- IX201AはPASSしているため、indexed I/O全体が未実装というより、
  dynamic access、alternate key、cursor、status更新が未達の中心である。
- RL系FAILはrelative keyそのものより、`START`後のcursor、欠番skip、
  delete/rewrite後の状態更新が未達候補である。
- SQ系FAILはsequential read/writeの基礎、file status、`AT END`、
  LINAGE/output positioningが混在している。
- ST/RWはファイルI/Oを踏むが、SORT/REPORT固有仕様が主因になるため、
  TASK-005では共通I/O境界だけを扱い、固有仕様はTASK-009で分離する。

### TASK-006

- 補足: 対象は `MOVE`, `ADD`, `SUBTRACT`, `MULTIPLY`, `DIVIDE`,
  `COMPUTE`, `ROUNDED`, `SIZE ERROR`, `PICTURE`, edited numeric。
- 補足: 影響範囲は `NC`, `IC`, `CM`, `IF`。
- 成果物: 数値カテゴリ、scale、符号、丸め、桁あふれの仕様表と代表repro。

#### TASK-006 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/{NC,IC,CM,IF}/**/*.reason`,
  `.nist/results/{NC,IC,CM,IF}/**/*.compile.log`
- 参照ソース: `.nist/programs/{NC,IC,CM,IF}/**/*.cob`
- 前提: TASK-002のreason分類から、MOVE、算術、PICTURE、数値条件、
  CALL/通信statusの数値表現、intrinsic境界に関係する候補を抽出する

数値仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| numeric storage model | display numeric, binary, packed/decimal | computedが0化、桁/scaleが落ちる | sema picture metadata, runtime decimal |
| MOVE conversion | numeric to numeric, numeric to display, edited numeric | MOVE後の値、符号、桁詰めがずれる | HIR conversion, codegen, runtime formatting |
| arithmetic scale | `ADD`, `SUBTRACT`, `MULTIPLY`, `DIVIDE` | 小数桁、商、余り、符号がずれる | decimal arithmetic, result coercion |
| rounded and overflow | `ROUNDED`, `ON SIZE ERROR` | overflow発火、丸め、結果保持がずれる | decimal rounding, exception edge |
| PICTURE editing | `+`, `-`, `Z`, `*`, `/`, currency, decimal point | edited出力とblank/zero suppressionがずれる | picture parser, formatter |
| numeric comparison | numeric condition, figurative constants, class | 条件TRUE/FALSEが逆になる | value conversion, comparison semantics |
| table/subscript numeric | numeric subscript, index, SEARCH | 添字値や探索結果がずれる | subscript conversion, bounds, SEARCH |
| inter-module numeric ABI | `CALL USING`, communication status | 表示値は同じでも判定がFAILになる | linkage layout, argument conversion |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| NUM-001 | NC104A | `ccvs-first-fail` | `MOVE NUMERIC INTEGER`で`5`期待が`0`になる | numeric MOVEとscale合わせ |
| NUM-002 | NC101A | `ccvs-first-fail` | `MULTIPLY BY`で`8888889`期待が`0`になる | multiplication result coercion |
| NUM-003 | NC207A | `ccvs-first-fail` | qualified `ADD`で`2`期待が`0`になる | qualified item解決とADD |
| NUM-004 | NC203A | `ccvs-first-fail` | DIVIDE remainderが`10.8`期待に対して`10` | division quotient/remainder scale |
| NUM-005 | NC218A | `ccvs-first-fail` | overflowすべき箇所で結果が`****`扱いになる | `ON SIZE ERROR`と結果保持 |
| NUM-006 | NC124A | `ccvs-first-fail` | `PICTURE + AND -`でblank期待が`+` | sign editingとblank suppression |
| NUM-007 | NC114M | `ccvs-first-fail` | `/`編集で符号と桁配置がずれる | edited numeric formatter |
| NUM-008 | NC103A | `ccvs-first-fail` | numeric equalityで表示桁が一致しない | numeric comparison coercion |
| NUM-009 | NC225A | `ccvs-first-fail` | `EVALUATE`のnumeric conditionが期待分岐に入らない | numeric condition評価 |
| NUM-010 | NC136A | `ccvs-first-fail` | numeric literal subscriptで期待要素を参照しない | subscript numeric conversion |
| NUM-011 | IC237A | `ccvs-first-fail` | CALL戻り値の表示上は同値だがFAIL | linkage numeric ABI |

CErrからTASK-006へ入る入口:

| Family | NIST例 | 生成Cの壊れ方 | TASK-006で固定する契約 |
| ---- | ---- | ---- | ---- |
| Decimal helper mismatch | NC105A, NC118A, NC123A, NC177A | decimal値をinteger helperへ渡す | decimal source/valueを型別helperで受ける |
| char array assignment | NC125A, NC401M | char配列へdecimal結果を直接代入する | formatted byte列として格納する |
| display numeric as integer | NC109M | display numericをinteger値として扱う | display numericを値化してから演算する |
| refmod pointer as integer | NC224A | reference modificationをinteger値として扱う | sliceを型変換してから数値化する |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| IF101A | intrinsic結果以前に段落到達数が不足 | TASK-004,TASK-007 |
| IF128A | `ORD-MAX`固有の戻り値規則が主因 | TASK-007 |
| CM101M | communication status wordの数値表示を含むが通信仕様が主因 | TASK-009 |
| CM201M | 帳票が空で、数値処理以前に出力捕捉が不明 | TASK-009 |

縮小repro入力の形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| NUM-001 | `PIC 9`から`PIC 9V9`、`PIC S9`、display numericへ`MOVE`する | 桁寄せ、scale、符号がPICTUREどおりになる |
| NUM-002 | integer/display/decimalを混在させて`MULTIPLY BY`する | 中間decimal値と受け側PICTUREへの格納が一致する |
| NUM-003 | group配下の同名qualified itemへ`ADD`する | 名前解決後の対象だけが更新される |
| NUM-004 | `DIVIDE ... GIVING ... REMAINDER`を小数付きで実行する | 商と余りのscaleが保持される |
| NUM-005 | 桁あふれする算術を`ON SIZE ERROR`付きで実行する | overflow時は例外句へ入り、規定どおり結果を保持する |
| NUM-006 | 正負ゼロを`PIC +`, `PIC -`, `PIC Z`, `PIC *`へMOVEする | blank/zero/sign suppressionが一致する |
| NUM-007 | `PIC 9/999`, currency, decimal pointを含むedited項目へMOVEする | 文字配置と符号位置が一致する |
| NUM-008 | display numericとdecimal numericを`IF =`, `<`, `>`で比較する | 表示表現ではなく数値として比較される |
| NUM-009 | `EVALUATE TRUE`とnumeric conditionを組み合わせる | 期待分岐だけに入る |
| NUM-010 | numeric literal、data item、indexでtableを参照する | 添字変換と境界判定が一致する |
| NUM-011 | subprogramへnumeric itemを`CALL USING`し、戻り値を更新する | caller/calleeのscale、符号、layoutが一致する |

数値表現契約:

| Boundary | 入力 | 内部表現 | 出力 |
| ---- | ---- | ---- | ---- |
| literal parse | integer/decimal literal | sign, coefficient, scale | contextごとにcoerce |
| display numeric read | `PIC 9`, `PIC S9`, edited source | decimal value + validity | arithmetic/comparisonへ渡す |
| arithmetic intermediate | operands | arbitrary precision decimal相当 | result PICTUREへ丸め/桁詰め |
| result store | decimal intermediate | target PICTURE metadata | binary/display/edited bytes |
| comparison | mixed numeric operands | common numeric value | boolean |
| exception edge | overflow/divide error | status + optional preserved target | `ON SIZE ERROR`/通常辺 |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| parser | PICTURE文字列、edited記号、算術句、`ROUNDED`、`SIZE ERROR`句を落とさない |
| sema | category、digits、scale、sign、edited/numeric-edited、usageをHIRへ渡す |
| HIR lowering | conversion node、算術中間型、例外辺、target metadataを明示する |
| codegen | helper選択を値型ではなくsource/target categoryで決める |
| runtime decimal | 四則演算、丸め、比較、overflow判定、division remainderを提供する |
| runtime picture | display numericとedited numericのparse/formatを一元化する |
| verifier | computed/correctの表示差分を数値値と帳票文字列のどちらで見るか分ける |

TASK-006時点での判断:

- NC系の大量FAILは、個々の`ADD`や`MOVE`の局所バグではなく、
  decimal中間値、PICTURE metadata、格納時coercionの契約不足として扱う。
- `HirDataItem`へ追加された`picture`と`is_numeric_edited`は、
  codegen初期化だけ埋める対処では足りない。semaからruntime formatterまで
  同じmetadataを運ぶ必要がある。
- CErrのdecimal/display系はTASK-003のcodegen契約問題だが、
  修正方針はTASK-006の数値表現契約に従わせる。
- IFのintrinsicは数値変換を使うが、関数固有規則はTASK-007で扱う。
  TASK-006では引数/戻り値の共通coercionだけを境界として定義する。

### TASK-007

- 補足: 対象は `IF` の数学、統計、文字列、日時系 intrinsic function。
- 注意: 個別関数ごとではなく、引数変換、戻り値カテゴリ、境界値、
  丸めの共通規則を先に定義する。
- 成果物: intrinsic共通変換モデルと失敗関数の分類表。

#### TASK-007 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/IF/**/*.reason`, `.nist/results/IF/**/*.log`
- 参照ソース: `.nist/programs/IF/**/*.cob`
- 前提: TASK-006の数値表現契約を使うが、関数固有規則はTASK-007で扱う

intrinsic仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| argument coercion | numeric, alphanumeric, table `ALL`, nested function | 段落不足や値の0化、巨大値化 | HIR call lowering, runtime conversion |
| numeric scalar | `INTEGER`, `INTEGER-PART`, `MOD`, `REM` | truncation/floor、符号、小数処理がずれる | runtime intrinsic numeric |
| math transcendental | `LOG`, `LOG10`, `SQRT`, 三角関数系 | 許容誤差内の数値比較へ到達しない | runtime math, decimal/float bridge |
| aggregate/statistical | `MAX`, `MIN`, `MEAN`, `MEDIAN`, `MIDRANGE`, `RANGE`, `SUM`, `VARIANCE` | 引数列、table展開、戻り値scaleがずれる | argument flatten, aggregate runtime |
| ordinal/string | `CHAR`, `ORD`, `ORD-MAX`, `ORD-MIN` | 文字集合の序数、最大/最小位置がずれる | collating sequence, ordinal runtime |
| string transform | `LENGTH`, `LOWER-CASE`, `UPPER-CASE`, `REVERSE` | 文字列結果、長さ、nested functionがずれる | string runtime, result storage |
| numeric string parse | `NUMVAL`, `NUMVAL-C` | 符号、通貨記号、桁区切り、小数点がずれる | parser/runtime numeric string |
| date/time | `CURRENT-DATE`, `WHEN-COMPILED`, date/day integer変換 | 可変値検証、日付基準、世紀/通算日がずれる | date runtime, verifier tolerance |
| random | `RANDOM` | seed、範囲、再現性の期待とずれる | runtime random state |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| FUN-001 | IF105A | `ccvs-first-fail` | `CHAR`で`+`期待が制御文字になる | collating sequenceと1-origin序数 |
| FUN-002 | IF111A | `ccvs-first-fail` | `INTEGER`で`0`期待が巨大値になる | floor/truncationと引数coercion |
| FUN-003 | IF114A | `ccvs-first-fail` | `INTEGER-PART`で`4`期待が巨大値になる | integer-partの符号処理 |
| FUN-004 | IF119A | `ccvs-first-fail` | `MAX`で`7`期待が壊れた値になる | aggregate引数列と比較 |
| FUN-005 | IF123A | `ccvs-first-fail` | `MIN`で`0`期待が壊れた値になる | aggregate最小値と戻り値category |
| FUN-006 | IF128A | `ccvs-first-fail` | `ORD-MAX`で位置`3`期待が`1`になる | 最大値の位置、同値時規則 |
| FUN-007 | IF129A | `ccvs-first-fail` | `ORD-MIN`で位置`1`期待が`2`になる | 最小値の位置、同値時規則 |
| FUN-008 | IF113A | `ccvs-first-fail` | `INTEGER-OF-DAY`で`400`期待が`678` | day-of-yearから通算日の変換 |
| FUN-009 | IF107A | `ccvs-first-fail` | `CURRENT-DATE`の可変値判定がFAIL | 日時形式とverifier許容条件 |
| FUN-010 | IF142A | `ccvs-first-fail` | `WHEN-COMPILED`の可変値判定がFAIL | compile timestampの固定/形式 |
| FUN-011 | IF101A系 | `detail-paragraph-mismatch` | 期待段落数より実行段落が少ない | intrinsic式を含む制御到達 |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| IF101A, IF102A, IF103A, IF104A | `detail-paragraph-mismatch`で、関数値以前に到達段落が不足 | TASK-004 |
| IF116A, IF117A, IF120A, IF121A | 数学/統計関数だが段落不足が先に出ている | TASK-004,TASK-007 |
| IF126A | `NUMVAL-C`は文字列parseとdecimal格納の境界 | TASK-006,TASK-007 |
| IF131A | `RANDOM`はseed/stateの再現性を別途固定する必要がある | TASK-007,TASK-010 |

縮小repro入力の形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| FUN-001 | `COMPUTE N = FUNCTION ORD("+")`, `MOVE FUNCTION CHAR(N)` | `ORD`と`CHAR`が同じcollating sequenceで往復する |
| FUN-002 | `FUNCTION INTEGER(0.00032)`, `FUNCTION INTEGER(-9.763)` | COBOL定義どおりの整数化になる |
| FUN-003 | `FUNCTION INTEGER-PART(4.578)`, `FUNCTION INTEGER-PART(-9.763)` | 小数部を除いた値が返る |
| FUN-004 | `FUNCTION MAX(5, 6, 10, 3, 7)`とtable `ALL` | 最大値と戻り値categoryが一致する |
| FUN-005 | `FUNCTION MIN(5, 6, 10, 3, 7)`と英数字混在 | 比較規則と戻り値categoryが一致する |
| FUN-006 | `FUNCTION ORD-MAX(5, 3, 2, 8, 3, 1)` | 最大値の出現位置が1-originで返る |
| FUN-007 | `FUNCTION ORD-MIN(5, 3, 2, 8, 3, 1)` | 最小値の出現位置が1-originで返る |
| FUN-008 | `FUNCTION INTEGER-OF-DATE(16010101)`と`INTEGER-OF-DAY(1601001)` | 1601-01-01基準の通算日が一致する |
| FUN-009 | `MOVE FUNCTION CURRENT-DATE TO X`して形式と単調性を見る | year/month/day/time/offset形式と比較が通る |
| FUN-010 | `MOVE FUNCTION WHEN-COMPILED TO X`を複数回読む | 実行中に値が変わらず形式が一致する |
| FUN-011 | `PERFORM UNTIL FUNCTION INTEGER(ARG) < 0`のような条件式 | 関数式が制御条件でも同じ評価になる |

intrinsic共通変換モデル:

| Boundary | 入力 | 処理 | 出力 |
| ---- | ---- | ---- | ---- |
| argument collect | literal, data item, table `ALL`, nested function | 左から評価し、関数ごとの引数列へflattenする | typed argument list |
| numeric coercion | integer, decimal, display numeric | TASK-006のdecimal値へ変換する | numeric runtime value |
| alphanumeric coercion | literal, display, edited result | byte列と長さを保持する | string runtime value |
| return category | function definition | numeric/alphanumeric/integer/dateなどを決定する | target MOVE/COMPUTEへ渡す |
| tolerance compare | floating/math/statistical result | NIST許容範囲か表示値で比較する | verifier判定 |
| variable value | current/compiled date, random | 再現性条件と形式条件を分ける | runtime value + verifier metadata |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| parser | `FUNCTION name(args)`、nested function、table `ALL`をASTで保持する |
| sema | 関数名、引数数、引数category、戻り値categoryを解決する |
| HIR lowering | function call node、argument flatten、return categoryを明示する |
| codegen | runtime intrinsic helperへ型付き引数列を渡し、target conversionを共通化する |
| runtime intrinsic | 関数ファミリ別の仕様実装を持つ |
| runtime date/random | 可変値の形式、基準日、seed/stateを管理する |
| verifier | 可変日時、浮動小数許容、文字列完全一致を区別して判定する |

TASK-007時点での判断:

- IFの`detail-paragraph-mismatch`は、intrinsic個別値だけでは説明できない。
  まずTASK-004の制御到達と組み合わせて、関数式が条件やPERFORM境界で
  評価される経路を確認する。
- 値が出ているFAILでは、`CHAR/ORD`, `INTEGER/INTEGER-PART`,
  `MAX/MIN`, `ORD-MAX/ORD-MIN`, date/timeが代表的な入口である。
- TASK-006のdecimal変換を使うが、`ORD-MAX`の位置返却、`CURRENT-DATE`の
  形式、`WHEN-COMPILED`の固定性のような関数固有規則はTASK-007で実装する。
- verifierは数値完全一致だけでなく、日時形式、浮動小数許容範囲、
  randomの再現性条件を扱える必要がある。

### TASK-008

- 補足: 対象は `COPY`, `REPLACING`, library-name, continuation,
  compile-time warning flags。
- 補足: 影響範囲は `SM`, `SG`, `SQ`, `RL`, `NC`。
- 成果物: source manipulation と sema診断ルールの差分表。

#### TASK-008 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/{SM,SG,SQ,RL,NC,IX}/**/*.reason`,
  `.nist/results/{SM,SG,SQ,RL,NC,IX}/**/*.compile.log`
- 参照ソース: `.nist/programs/{SM,SG,SQ,RL,NC,IX}/**/*.cob`
- 前提: TASK-002の`warning-flags-missing`とSM系COPY失敗を主対象にする

source manipulation / 診断仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| basic COPY expansion | `COPY text-name` | 帳票が空、段落数不足 | preprocessor, source map |
| COPY REPLACING pseudo-text | `COPY ... REPLACING ==a== BY ==b==` | pseudo text置換結果がずれる | tokenizer, replacement engine |
| copybook section placement | ENV/DATA/PROCEDURE内COPY | 展開位置により構文/実行結果がずれる | preprocessor, parser handoff |
| library-name resolution | qualified library-name | wrong libraryのtextを読む | copybook resolver |
| continuation and quote handling | continuation line, quote literal | 長い引用符列や文字列置換がずれる | lexer, fixed-format source handling |
| diagnostic warning flags | obsolete/non-conforming feature | 期待warning数に足りない | sema diagnostics, verifier warning counter |
| generated C macro hygiene | repeated data names after COPY | C macro redefinition warning | codegen naming, not source semantics |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| SRC-001 | SM201A | `ccvs-first-fail` | `PSEUDO TEXT`のCOPY-TEST-11がFAIL | pseudo-text replacement |
| SRC-002 | SM205A | `ccvs-first-fail` | `COPY SD REPLACING`でEOFを早く読む | SD copybookとREPLACING |
| SRC-003 | SM206A | `ccvs-first-fail` | cascaded replacementが7回、期待5回 | cascading replacement抑止 |
| SRC-004 | SM207A | `ccvs-first-fail` | qualified library-nameでwrong libraryを読む | library-name解決 |
| SRC-005 | SM208A | `ccvs-first-fail` | single charと160 quotesの置換がずれる | continuation/quote replacement |
| DIAG-001 | SG302M | `warning-flags-missing` | segmentation level 1でwarning 1件期待、0件 | segmentation obsolete warning |
| DIAG-002 | SG303M | `warning-flags-missing` | segmentation level 2でwarning 4件期待、0件 | segmentation obsolete warning群 |
| DIAG-003 | SG401M | `warning-flags-missing` | segmentation moduleでwarning 2件期待、0件 | `SEGMENT-LIMIT`診断 |
| DIAG-004 | RL302M | `warning-flags-missing` | relative random系でwarning 4件期待、1件 | file-control non-conforming warnings |
| DIAG-005 | SQ302M | `warning-flags-missing` | sequential file句でwarning 4件期待、0件 | `LABEL RECORDS`, `VALUE OF`等 |
| DIAG-006 | NC302M | `warning-flags-missing` | obsolete featureでwarning 7件期待、2件 | `ALTER`, 明示宛先なし`GO TO`, `STOP literal` |
| DIAG-007 | IX401M | `warning-flags-missing` | indexed dynamic系でwarning 10件期待、6件 | indexed non-conforming warnings |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| SG102A, SG103A, SG201A, SG202A, SG203A | segmentation実行意味論が主因 | TASK-009 |
| RL302M, SQ302M, IX401M | warning不足はTASK-008だがI/O実行意味論は別 | TASK-005 |
| NC302M | warning不足はTASK-008だが`ALTER`実行意味論は別 | TASK-004 |
| SM103A, SM106A, SM301M | blank reportでCOPY以前に出力捕捉も疑う | TASK-009 |

縮小repro入力の形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| SRC-001 | copybook内のtoken列をpseudo-textで置換する | 区切り文字、空白、句点を含めて期待textになる |
| SRC-002 | `SD`句を含むcopybookを`COPY ... REPLACING`で展開する | sort descriptionの置換後sourceが完全に読まれる |
| SRC-003 | 置換後textがさらに別の置換対象に見えるcopybookを使う | 規定どおりcascaded replacementしない |
| SRC-004 | 同名copybookを異なるlibrary-name配下に置く | qualified library-nameの指定先だけを読む |
| SRC-005 | continuationを含む長いquote列をsingle charへ置換する | literal境界を壊さず置換する |
| DIAG-001 | `SEGMENT-LIMIT`を含む最小program | segmentation関連warningが期待数出る |
| DIAG-004 | `LABEL RECORDS`, `VALUE OF`, random/relative句を含むFD | non-conforming file句warningが出る |
| DIAG-006 | `ALTER`, 明示宛先なし`GO TO`, `STOP "literal"`を含むprogram | obsolete feature warningが各箇所で出る |
| DIAG-007 | indexed dynamic, alternate key, invalid-key句を含むprogram | indexed feature warningが各句で出る |

診断ルール:

| Rule | 対象構文 | 期待 |
| ---- | ---- | ---- |
| obsolete feature | `ALTER`, 明示宛先なし`GO TO`, `STOP literal`, `DATE-COMPILED` | 出現箇所ごとにwarningを出す |
| source manipulation | `COPY ... REPLACING` | 非準拠source manipulationとしてwarningを出す |
| segmentation | `SEGMENT-LIMIT`, segmentation section | segmentation featureとしてwarningを出す |
| indexed extension | `ORGANIZATION INDEXED`, `ACCESS DYNAMIC`, `RECORD KEY`, `ALTERNATE KEY` | indexed非準拠featureとしてwarningを出す |
| relative/sequential file extensions | `LABEL RECORDS`, `VALUE OF`, random/relative句 | file-control非準拠featureとしてwarningを出す |
| warning counting | compile log | verifierがCOBC warningだけを数える |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| lexer | fixed-format continuation、quote literal、pseudo-text delimiterを保持する |
| preprocessor | copybook探索、library-name解決、REPLACING、source mapを実装する |
| parser | 展開後sourceをdivision/section文脈に渡す |
| sema diagnostics | obsolete/non-conforming featureを構文単位で警告する |
| codegen | COPY後の重複名でもC macro衝突を避ける |
| verifier | expected warning flag数と実際のCOBC warning数を安定して数える |

TASK-008時点での判断:

- SM系FAILは単なるwarning不足ではなく、COPY展開後sourceが期待と違う。
  preprocessorのtoken単位置換、copybook探索、continuation処理を先に固定する。
- `warning-flags-missing`は実行時FAILではなく、compile-time diagnosticの
  仕様不足である。I/Oや制御フローの実行修正とは別に扱う。
- 生成Cのmacro redefinition warningはCOPY診断ではなくcodegen hygieneである。
  CErrやC警告削減には効くが、NISTのwarning flag期待とは別カウントにする。
- warningの正否は文言完全一致より、COBC warningとしての件数、対象構文、
  source locationを安定させることを優先する。

### TASK-009

- 補足: 対象は Report Writer、SORT/MERGE、segmentation、printer/output capture。
- 注意: ファイルI/Oと出力捕捉の不備に巻き込まれるため、TASK-005の分類後に分離する。
- 成果物: REPORT/SORT/SEGMENT固有欠陥と共通I/O欠陥の切り分け表。

#### TASK-009 成果物

実行条件:

- 実行日: 2026-05-03
- 対象: `.nist/results/{RW,ST,SG,SM,CM}/**/*.reason`,
  `.nist/results/{RW,ST,SG,SM,CM}/**/*.compile.log`
- 参照ソース: `.nist/programs/{RW,ST,SG,SM,CM}/**/*.cob`
- 前提: TASK-005で共通ファイルI/O境界を先に切り出したため、
  TASK-009ではREPORT/SORT/SEGMENT固有仕様と出力捕捉だけを扱う

REPORT/SORT/SEGMENT仕様カテゴリ:

| Category | COBOL機能 | 主な未達症状 | 主な責務 |
| ---- | ---- | ---- | ---- |
| Report Writer lifecycle | `INITIATE`, `GENERATE`, `TERMINATE` | page counterが0のまま、reportが空 | parser, HIR, runtime report |
| report counters | `PAGE-COUNTER`, `LINE-COUNTER` | 初期値と更新がずれる | runtime report state |
| printer/output capture | print file, report output | `blank-or-empty-report`, summary不在 | runtime output, NIST verifier |
| SORT lifecycle | `SORT`, `RELEASE`, `RETURN`, input/output procedure | sort結果、到達段落、EOFがずれる | HIR sort CFG, runtime sort |
| MERGE lifecycle | `MERGE`, same sort-merge area | 段落数不足、merge出力不一致 | runtime merge, codegen |
| sort key flatten | `ASCENDING/DESCENDING KEY`, group key, binary/display key | key値が壊れる、CErrになる | data layout, codegen, runtime sort ABI |
| collating sequence | `COLLATING SEQUENCE`, native sequence | sort順、文字列keyがずれる | runtime collation |
| segmentation runtime | `SEGMENT-LIMIT`, segment priority, last used state | 初期/最終state、遷移先がずれる | parser, HIR CFG, runtime segmentation |
| segmentation diagnostics | obsolete segmentation features | warning flag不足 | sema diagnostics |

代表repro候補:

| Repro | NIST例 | Reason | 観測症状 | 最小化する仕様 |
| ---- | ---- | ---- | ---- | ---- |
| RPT-001 | RW101A | `ccvs-first-fail` | `INITIATE`後のpage counterが1期待、0 | report lifecycle初期化 |
| RPT-002 | RW103A | `ccvs-first-fail` | `INITIATE`後のpage counter不一致が多数 | report counter更新 |
| RPT-003 | RW301M | `blank-or-empty-report` | report帳票が空 | Report Writer出力捕捉 |
| SORT-001 | ST134A | `ccvs-first-fail` | COMP sort keyで`-100`期待が巨大値 | sort key flattenと数値key |
| SORT-002 | ST136A | `ccvs-first-fail` | `RELEASE FROM`後のkey areaが0 | `RELEASE FROM`のrecord転送 |
| SORT-003 | ST135A | `ccvs-first-fail` | same areaでEOFが早すぎる | same sort areaとRETURN |
| SORT-004 | ST137A | `ccvs-first-fail` | native collating sequenceの期待値が壊れる | collating sequence |
| SORT-005 | ST139A | `detail-paragraph-mismatch` | MERGE系で期待11段落に対して2段落 | MERGE control flow |
| SORT-006 | ST109A | `no-decisive-ccvs-summary` | sort入力生成後のsummaryを決定できない | sort前後I/Oとverifier境界 |
| SEG-001 | SG103A | `ccvs-first-fail` | initial stateが2期待、9 | segmentation初期state |
| SEG-002 | SG202A | `ccvs-first-fail` | last used stateが23期待、2 | segmentation last-used state |
| SEG-003 | SG203A | `ccvs-first-fail` | PARA-37開始判定が逆 | segment priorityと到達順 |
| OUT-001 | CM201M | `blank-or-empty-report` | 帳票が空 | 非REPORT系出力捕捉 |
| OUT-002 | SM103A | `blank-or-empty-report` | COPY系帳票が空 | COPY後実行とprint capture境界 |

共通I/O欠陥との切り分け:

| NIST例 | 固有候補 | 共通I/O候補 | 扱い |
| ---- | ---- | ---- | ---- |
| ST109A, ST115A | SORT入力/出力procedureとsummary | sequential file生成 | TASK-009でsort境界、TASK-005でsequential基礎 |
| ST135A | same sort areaとRETURN | EOF/file status | TASK-009でsame area、TASK-005でEOF |
| ST139A, ST140A, ST144A, ST147A | MERGE lifecycle | 入出力file cursor | TASK-009でMERGE、TASK-005でfile cursor |
| RW301M, RW302M | Report Writer出力 | print file capture | TASK-009で両方確認 |
| SM103A, SM106A, SM301M | COPY展開後の実行 | print file capture | TASK-008でCOPY、TASK-009で出力捕捉 |
| CM201M | communication出力 | print file capture | TASK-009で出力捕捉、通信仕様は後続 |

他タスクへ渡す候補:

| NIST例 | 理由 | 渡し先 |
| ---- | ---- | ---- |
| ST104A, ST106A, ST108A, ST118A, ST125A, ST127A | sort record flatten由来のCErr | TASK-003 |
| ST134A | 数値sort keyの表示/格納も絡む | TASK-006,TASK-009 |
| SG302M, SG303M, SG401M | warning不足が主因 | TASK-008 |
| SM201A, SM205A, SM206A, SM207A, SM208A | COPY/REPLACINGが主因 | TASK-008 |
| CM101M, CM202M | communication status/queue意味論が主因 | TASK-010以降 |

縮小repro入力の形:

| Repro | 最小COBOL構造 | 期待観測 |
| ---- | ---- | ---- |
| RPT-001 | `REPORT SECTION`にpage counterを置き、`INITIATE`直後に検査する | page counterが1になる |
| RPT-002 | `GENERATE`を複数回行い、line/page counterを検査する | counterがreport writer規則どおり進む |
| RPT-003 | `INITIATE`から`TERMINATE`まで実行し、print fileを読む | report bodyとsummaryが捕捉される |
| SORT-001 | binary/display混在keyで`SORT ON ASCENDING KEY`する | key値で正しく並ぶ |
| SORT-002 | `RELEASE record FROM work-record`後に`RETURN`する | FROM sourceの内容がsort recordへ入る |
| SORT-003 | same sort-merge areaを使って`SORT`/`RETURN`する | EOFとrecord countが一致する |
| SORT-004 | `COLLATING SEQUENCE`を指定して文字keyをsortする | 指定順で並ぶ |
| SORT-005 | `MERGE ... OUTPUT PROCEDURE`を最小入力2本で実行する | procedure到達順と出力順が一致する |
| SEG-001 | `SEGMENT-LIMIT`付きprogramで初期paragraphを記録する | initial stateが期待値になる |
| SEG-002 | 複数segmentを遷移し、last used stateを記録する | 最終使用segmentが期待値になる |
| OUT-001 | `DISPLAY`/print fileを使う最小programを実行する | runner/verifierが同じ出力を読む |

実装責務:

| Layer | 責務 |
| ---- | ---- |
| parser | `REPORT SECTION`, `RD`, `SORT`, `MERGE`, `RELEASE`, `RETURN`, segmentation句を保持する |
| sema | report counter、sort file、sort key、merge input、segment priorityを解決する |
| HIR lowering | report lifecycle、sort/merge procedure CFG、segmentation stateを明示する |
| codegen | sort key flatten、same area、report counter更新、segment dispatchを一貫して出す |
| runtime report | report writer状態、counter、print outputを管理する |
| runtime sort/merge | sort store、key compare、collation、release/return、merge cursorを管理する |
| runtime output | print file、stdout、report fileをNIST verifierが読める場所へ流す |
| verifier | blank report、summary不在、sort chain生成物を区別して記録する |

TASK-009時点での判断:

- RW101AからRW104Aは、I/O以前にReport Writerの`INITIATE`時counter初期化が
  未達である。RW301M/RW302Mは出力捕捉も合わせて確認する。
- ST系は、共通sequential I/Oだけではなく、sort/mergeのrecord転送、
  key flatten、collating sequence、procedure CFGが独立した欠陥である。
- SG系はwarning不足をTASK-008へ渡し、実行時のinitial/last-used stateと
  segment priorityをTASK-009固有として扱う。
- `blank-or-empty-report`はREPORT専用ではないため、runtime outputと
  verifier captureの共通欠陥として明示的に分ける。

### TASK-010

- 補足: 実装順は `CErr解消 -> 判定精度改善 -> 制御フロー -> ファイルI/O -> 数値/関数 -> 残余モジュール` を基本線にする。
- 注意: 各実装タスクは必ず縮小e2eテストを追加してからNISTモジュールを再実行する。
- 成果物: 100% passまでの実装マイルストーン、CI gate、完了条件一覧。

#### TASK-010 成果物

実行条件:

- 実行日: 2026-05-03
- 入力: TASK-003からTASK-009の成果物
- 目的: `391 total / 391 pass / 0 fail / 0 ready / 0 CErr / 0 RErr`へ到達する
  実装順、検証gate、完了条件を固定する
- 前提: 対症療法ではなく、COBOL仕様境界ごとに縮小repro、実装、NIST再実行、
  CI gate強化を同じ単位で進める

実装マイルストーン:

| Milestone | 目的 | 主対象 | 完了条件 | Gate |
| ---- | ---- | ---- | ---- | ---- |
| M2 | CErrを0にする | TASK-003, TASK-006, TASK-009 | 19 CErrが0、生成C警告が原因分類済み | `make nist-compile-errors`が0件 |
| M3 | 判定精度を固定する | TASK-002, TASK-005, TASK-009 | summary不在、blank report、warning countの誤分類をなくす | `make nist-summary`でReady/RErrが0、reason分類が一意 |
| M4 | 制御フローをHIR契約化する | TASK-004, TASK-008, TASK-009 | `PERFORM`, `GO TO`, `ALTER`, `USE`, 例外句、program終端の縮小e2eが通る | DB/IF/NC/OBの制御reproが全PASS |
| M5 | ファイルI/O状態機械を実装する | TASK-005 | sequential/indexed/relativeのcursor、status、例外句が仕様どおり動く | IX/RL/SQのI/O reproが全PASS |
| M6 | 数値/PICTUREを一元化する | TASK-006 | MOVE、算術、丸め、SIZE ERROR、edited numericが同じmetadataで動く | NC/ICの数値reproが全PASS |
| M7 | intrinsic functionを仕様化する | TASK-007 | 引数flatten、戻り値category、日時/乱数/許容誤差を扱える | IFの関数reproが全PASS |
| M8 | COPY/診断を仕様化する | TASK-008 | COPY REPLACING、library-name、continuation、warning countが一致 | SM/SG/SQ/RL/IXの診断reproが全PASS |
| M9 | REPORT/SORT/SEGMENTを実装する | TASK-009 | report counter、sort/merge、segment state、output captureが一致 | RW/ST/SG/SM/CMの固有reproが全PASS |
| M10 | NIST 100%をCI必須化する | 全タスク | 391件すべてPASSし、未分類reasonが0 | full NIST gateが必須check |

実装順序:

| Order | 作業単位 | 先に潰す理由 | 後続へ渡す契約 |
| ----: | ---- | ---- | ---- |
| 1 | CErr/codegen契約 | コンパイル不能のままではFAIL分類が不正確になる | HIR data item、decimal/display、sort key、linkageのC ABI |
| 2 | verifier/runner判定精度 | summary不在やblank reportを実装FAILと誤認しないため | NIST結果、warning count、print outputの観測契約 |
| 3 | 制御フローCFG | 到達段落不足は値不一致より上位の欠陥であるため | 通常辺、例外辺、debug辺、perform return辺 |
| 4 | ファイルI/O状態機械 | IX/RL/SQ/ST/RWの多数FAILがcursor/statusに依存するため | file metadata、status格納、record cursor、例外句 |
| 5 | 数値/PICTURE | NC/IC/IF/CMの値不一致が共通変換に依存するため | decimal中間値、PICTURE metadata、store coercion |
| 6 | intrinsic function | 数値変換とCFGが固まらないと関数値を正しく判定できないため | typed argument list、return category、variable value metadata |
| 7 | COPY/診断 | 実行意味論とwarning期待を分けて検証するため | source map、copybook resolution、diagnostic counter |
| 8 | REPORT/SORT/SEGMENT | I/O、CFG、数値key、出力捕捉を横断するため | report state、sort store、merge cursor、segment dispatch |
| 9 | 残余モジュール横断 | CM/DB/EX/OBなどの複合仕様を最後に残すため | 仕様別の未分類ゼロ状態 |

CI gate一覧:

| Gate | 実行タイミング | コマンド | 必須条件 |
| ---- | ---- | ---- | ---- |
| local-build | 各実装chunk後 | `make build` | build成功 |
| focused-e2e | 各縮小repro追加後 | `cargo test -p cobol-driver --test e2e_test <test-name>` | 追加/影響テストがPASS |
| crate-suite | crate境界変更後 | `cargo test -p <crate> --all-targets` | 対象crateがPASS |
| compile-errors | M2完了時と以後 | `make nist-compile-errors` | 0件 |
| nist-module | 各仕様カテゴリ完了時 | `make nist-run MODULE=<module>` | 対象moduleの対象reasonが解消 |
| nist-summary | 各NIST再実行後 | `make nist-summary` | Ready/RErrが0、分類が更新済み |
| lint | PR前 | `make lint` | clippy/rustfmt/cspellがPASS |
| full-regression | PR前とM10 | `make clean test lint` | workspace testとlintがPASS |
| full-nist | M10 | `make nist-run NIST_JOBS=5` | `391 pass / 0 fail / 0 CErr / 0 RErr` |

完了条件:

| Level | 条件 |
| ---- | ---- |
| repro完了 | NIST由来の縮小COBOL入力、期待結果、失敗理由、対象crateが記録されている |
| 実装chunk完了 | 縮小e2eが先に追加され、修正後に同テストと対象crate testがPASSしている |
| 仕様カテゴリ完了 | 関連するNIST moduleまたはprogramが再実行され、同じreasonが残っていない |
| マイルストーン完了 | gateがPASSし、残ったFAIL/CErrが次マイルストーンへ分類済み |
| NIST完了 | TOTALが`391 pass / 0 fail / 0 ready / 0 CErr / 0 RErr`で、CI必須gateとして固定済み |

100%到達までの追跡指標:

| Metric | Baseline | Target | 用途 |
| ---- | ----: | ----: | ---- |
| Pass | 112 | 391 | 進捗の最終指標 |
| Fail | 260 | 0 | 仕様差分の残量 |
| CErr | 19 | 0 | codegen契約破綻の残量 |
| RErr | 0 | 0 | runtime異常を増やしていないことの確認 |
| Ready | 0 | 0 | 未実行を残していないことの確認 |
| Unclassified reason | 未固定 | 0 | 判定器/分類表の成熟度 |

リスクと抑止策:

| Risk | 症状 | 抑止策 |
| ---- | ---- | ---- |
| CErr修正が値FAILを増やす | COMPILE_ERRORがFAILへ移るだけでPASSが増えない | CErr修正時に最小e2eと対象NIST programを必ず並走する |
| verifier誤判定 | blank reportやsummary不在を実装FAILと混同する | M3で出力捕捉、warning count、summary抽出を先に固定する |
| 仕様境界の混線 | I/O修正でSORT/REPORT固有FAILを追う | TASK-005とTASK-009の切り分け表をgate条件に使う |
| 数値の局所修正 | MOVEだけ通り算術やintrinsicで壊れる | TASK-006のdecimal/PICTURE契約を共通helperへ集約する |
| warning件数の過剰最適化 | NIST期待だけに合わせて診断が不安定になる | 構文単位、source location、COBC warning countを同時に記録する |

TASK-010時点での判断:

- 100% passの実装単位は、NIST program単位ではなく仕様境界単位にする。
  program単位の修正は複数仕様が混ざり、原因が再び見えなくなるためである。
- 最初の実装修正はCErr 0化だが、成功条件は`COMPILE_ERROR -> FAIL`ではない。
  CErrファミリごとに縮小e2eを作り、生成C契約が仕様カテゴリへ接続されたことを
  確認してから完了扱いにする。
- 判定器の精度改善は実装修正の前提である。`blank-or-empty-report`、
  `no-decisive-ccvs-summary`、`warning-flags-missing`を実装欠陥と観測欠陥に
  分けられない状態では、100% passまでの残作業を正しく測れない。
- M10のCI gateは`pass == total`だけでなく、`Fail/Ready/CErr/RErr == 0`を
  同時に要求する。部分改善を完了扱いにしないため、この条件を最終gateにする。

### TASK-011

- 補足: Backlog一覧の各項目を、次マイルストーンで実行可能な通常タスクへ
  昇格、分割、並べ替えする。
- 注意: 1タスクの粒度はレビュー可能な2から6時間程度に収める。
- 成果物: M2以降の実装タスク一覧、依存関係、完了条件。

#### TASK-011 成果物

実行条件:

- 実行日: 2026-05-03
- 入力: TASK-010の実装マイルストーン、BACKLOG-001からBACKLOG-009
- 目的: Backlogを実装開始可能な粒度へ分割し、依存関係と完了条件を固定する
- 前提: 各実装タスクは、縮小e2e追加、実装修正、対象NIST再実行、
  関連gate確認までを含む

M2以降の実装タスク一覧:

| ID | Milestone | Summary | DependsOn | Source |
| ---- | ---- | ---- | ---- | ---- |
| IMPL-001 | M2 | HIR data item初期化とcodegen構造体契約を修正する | TASK-003 | BACKLOG-001 |
| IMPL-002 | M2 | scalar storageとbyte pointerのruntime ABIを分離する | IMPL-001 | BACKLOG-001 |
| IMPL-003 | M2 | decimal/display値をhelper ABI単位で修正する | IMPL-001,TASK-006 | BACKLOG-001,BACKLOG-005 |
| IMPL-004 | M2 | sort key flatten由来のCErrを修正する | IMPL-001,TASK-009 | BACKLOG-001,BACKLOG-008 |
| IMPL-005 | M2 | linkage group layout由来のCErrを修正する | IMPL-001,TASK-006 | BACKLOG-001 |
| IMPL-006 | M3 | NIST summary/first-fail parserを共通化する | TASK-002 | BACKLOG-002 |
| IMPL-007 | M3 | warning countとcompile log分類を安定化する | IMPL-006,TASK-008 | BACKLOG-002,BACKLOG-007 |
| IMPL-008 | M3 | print/report/stdout captureをrunnerへ統合する | IMPL-006,TASK-009 | BACKLOG-002,BACKLOG-008 |
| IMPL-009 | M3 | blank reportとsummary不在のreasonを再分類する | IMPL-008 | BACKLOG-002 |
| IMPL-010 | M4 | `PERFORM`/`PERFORM THRU`のHIR CFGを再設計する | IMPL-006,TASK-004 | BACKLOG-003 |
| IMPL-011 | M4 | `GO TO`/`ALTER`/fallthroughのdispatch契約を実装する | IMPL-010,TASK-008 | BACKLOG-003 |
| IMPL-012 | M4 | `USE FOR DEBUGGING`とdeclarative突入を実装する | IMPL-010 | BACKLOG-003 |
| IMPL-013 | M4 | 例外句とprogram terminationの制御辺を実装する | IMPL-010 | BACKLOG-003 |
| IMPL-014 | M5 | sequential fileのopen/read/write/status状態機械を実装する | IMPL-013,TASK-005 | BACKLOG-004 |
| IMPL-015 | M5 | indexed fileのkey/cursor/invalid-key状態機械を実装する | IMPL-014 | BACKLOG-004 |
| IMPL-016 | M5 | relative fileのrelative key/cursor/delete状態機械を実装する | IMPL-014 | BACKLOG-004 |
| IMPL-017 | M5 | LINAGEとWRITE ADVANCINGのoutput positioningを実装する | IMPL-008,IMPL-014 | BACKLOG-004 |
| IMPL-018 | M6 | PICTURE metadataをsemaからruntimeまで一貫して運ぶ | IMPL-003,TASK-006 | BACKLOG-005 |
| IMPL-019 | M6 | MOVE conversionをsource/target category単位で一元化する | IMPL-018 | BACKLOG-005 |
| IMPL-020 | M6 | decimal算術、丸め、SIZE ERRORを一元化する | IMPL-018 | BACKLOG-005 |
| IMPL-021 | M6 | edited numeric formatter/parserを実装する | IMPL-019 | BACKLOG-005 |
| IMPL-022 | M7 | intrinsic argument flattenと戻り値categoryを実装する | IMPL-018,TASK-007 | BACKLOG-006 |
| IMPL-023 | M7 | numeric/math/aggregate intrinsicを実装する | IMPL-020,IMPL-022 | BACKLOG-006 |
| IMPL-024 | M7 | ordinal/string/date/random intrinsicを実装する | IMPL-022 | BACKLOG-006 |
| IMPL-025 | M8 | COPY REPLACINGのtoken単位置換を実装する | TASK-008 | BACKLOG-007 |
| IMPL-026 | M8 | copybook library-nameとcontinuation/quote処理を実装する | IMPL-025 | BACKLOG-007 |
| IMPL-027 | M8 | obsolete/non-conforming warning診断を構文単位で実装する | IMPL-007,IMPL-026 | BACKLOG-007 |
| IMPL-028 | M9 | Report Writer lifecycleとcounterを実装する | IMPL-008,IMPL-017,TASK-009 | BACKLOG-008 |
| IMPL-029 | M9 | SORT/RELEASE/RETURNとsort key compareを実装する | IMPL-004,IMPL-014,IMPL-020,TASK-009 | BACKLOG-008 |
| IMPL-030 | M9 | MERGEとsame sort-merge areaを実装する | IMPL-029 | BACKLOG-008 |
| IMPL-031 | M9 | segmentation runtime stateとdiagnostic境界を実装する | IMPL-011,IMPL-027,TASK-009 | BACKLOG-008 |
| IMPL-032 | M10 | NIST full gateをCI必須checkへ昇格する | IMPL-001-IMPL-031 | BACKLOG-009 |
| IMPL-033 | M10 | 残余未分類FAILを仕様カテゴリへ再分類して0にする | IMPL-032 | BACKLOG-009 |

レビュー可能な粒度への分割結果:

| Backlog | 分割後 | 判断 |
| ---- | ---- | ---- |
| BACKLOG-001 | IMPL-001からIMPL-005 | CErrはABIファミリごとに分け、M2で先行する |
| BACKLOG-002 | IMPL-006からIMPL-009 | 判定器は実装修正より先に観測契約として固定する |
| BACKLOG-003 | IMPL-010からIMPL-013 | CFGは`PERFORM`、dispatch、debug、例外/終端へ分ける |
| BACKLOG-004 | IMPL-014からIMPL-017 | file organizationごとに状態機械を分ける |
| BACKLOG-005 | IMPL-018からIMPL-021 | PICTURE metadata、MOVE、算術、editedを順に積む |
| BACKLOG-006 | IMPL-022からIMPL-024 | intrinsicは共通call契約を先に置き、関数群を分ける |
| BACKLOG-007 | IMPL-025からIMPL-027 | source操作と診断warningを別タスクにする |
| BACKLOG-008 | IMPL-028からIMPL-031 | REPORT、SORT、MERGE、SEGMENTを固有runtime単位に分ける |
| BACKLOG-009 | IMPL-032からIMPL-033 | CI gate化と残余分類を最後に分ける |

各実装タスクの共通完了条件:

- 対象仕様の縮小e2eを追加し、修正前に失敗を確認する。
- 実装後、追加e2eと既存の直接影響テストがPASSする。
- 対象NIST programまたはmoduleを再実行し、該当reasonが残っていないことを確認する。
- 新しいFAIL/CErrが出た場合は、既存タスクへ紐づけるか、未分類としてIMPL-033へ送る。
- `make nist-summary`の集計値を更新し、Pass/Fail/CErr/RErr/Readyの変化を記録する。

マイルストーン別gate:

| Milestone | 実装タスク | Gate |
| ---- | ---- | ---- |
| M2 | IMPL-001からIMPL-005 | `make nist-compile-errors`が0件 |
| M3 | IMPL-006からIMPL-009 | `blank-or-empty-report`と`no-decisive-ccvs-summary`の誤分類が0 |
| M4 | IMPL-010からIMPL-013 | DB/IF/NC/OBの制御reproが全PASS |
| M5 | IMPL-014からIMPL-017 | IX/RL/SQのI/O reproが全PASS |
| M6 | IMPL-018からIMPL-021 | NC/ICの数値reproが全PASS |
| M7 | IMPL-022からIMPL-024 | IFのintrinsic reproが全PASS |
| M8 | IMPL-025からIMPL-027 | SM/SG/SQ/RL/IXのCOPY/診断reproが全PASS |
| M9 | IMPL-028からIMPL-031 | RW/ST/SG/SM/CMの固有reproが全PASS |
| M10 | IMPL-032からIMPL-033 | `391 pass / 0 fail / 0 ready / 0 CErr / 0 RErr` |

TASK-011時点での判断:

- Backlogは残タスク置き場としては維持するが、実装開始単位はIMPLタスクへ昇格する。
  以後の実装着手はBacklog IDではなくIMPL IDを指定する。
- M2のCErr修正は一括対応しない。ABIファミリ別に分割し、`COMPILE_ERROR -> FAIL`
  だけの進捗を完了扱いにしない。
- M3をM4以降より先に置く。判定器と出力捕捉が不確かなまま実装へ入ると、
  根本原因と観測欠陥が再び混ざるためである。
- M10のCI必須化は全実装完了後に行う。途中でgateだけ厳しくしても、
  仕様実装の完了条件にはならない。

## 実装タスク詳細

### IMPL-001

- 実施日: 2026-05-03
- 対象: `crates/cobol-hir/src/hir.rs`, `crates/cobol-hir/src/lower.rs`,
  `crates/cobol-codegen/src/compiler.rs`, `crates/cobol-codegen/src/data.rs`
- 変更: 合成HIR用の`HirDataItem::new`と`with_initial_value`を追加し、
  暗黙レジスタ、INDEXED BY項目、codegenテスト用データを共通初期化契約へ寄せた。
- 完了条件: `HirDataItem`へ`picture`や`is_numeric_edited`のようなmetadataが追加されても、
  合成HIRやcodegenテストの初期化漏れでE0063が再発しない。
- 確認: `cargo check -p cobol-codegen --tests`,
  `cargo test -p cobol-hir -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver --test e2e_test`,
  `make clean test lint`がPASS。
- NIST確認: `make nist-compile-errors`は19件のまま。残件はscalar/pointer ABI、
  decimal/display helper ABI、sort key flatten、linkage group layoutであり、
  IMPL-002からIMPL-005へ継続する。

### IMPL-005 実施記録

- 対象: `crates/cobol-codegen/src/context.rs`,
  `crates/cobol-codegen/src/codegen.rs`, `crates/cobol-codegen/src/data.rs`,
  `crates/cobol-codegen/src/stmt.rs`, `crates/cobol-codegen/src/expr.rs`
- 変更: raw display layoutを持つlinkage/group memberについて、display numeric
  metadataのmember名をdedup済みC名へ合わせ、linkage parameterのtypedefをraw layoutへ
  切り替えた。加えてraw display numeric targetを`CobolDecimal` targetから除外し、
  参照修飾値を数値表示へ渡す前に`NUMVAL`相当のint互換式へ正規化した。
- 完了条件: linkage group layout、raw display numeric、reference modification由来の
  CErrが残らず、CErr集計が0件になる。
- 確認: `cargo test -p cobol-codegen --all-targets`、`make release`、
  `make nist-run MODULE=NC PROGRAM=NC125A`,
  `make nist-run MODULE=NC PROGRAM=NC224A`,
  `make nist-compile-errors`が完了。`make nist-compile-errors`は`Total: 0`。
- NIST確認: NC125A/NC224Aは`COMPILE_ERROR`ではなくFAILへ進んだ。残件はedited
  numericの期待値差分と実行時segfaultであり、後続のIMPL-018からIMPL-021および
  IMPL-033の分類対象へ送る。

### IMPL-006からIMPL-009 実施記録

- 対象: `tests/nist/run_nist.sh`, `tests/nist/verifiers/lib.sh`
- 変更: `run_nist.sh`に残っていた古いCCVS summary判定を
  `verifiers/lib.sh`の`verifier_standard_ccvs`へ委譲し、summary count、
  footer error、expected warning、compile warning countも同じlib関数へ寄せた。
  さらに空出力と非CCVS出力を`empty-report|...`、
  `undecidable-ccvs-output|...`へ再分類し、後続実装タスクへ紐づくreasonを残すようにした。
- 完了条件: first-fail detail、warning count、空出力、summary不在の分類がrunner内で
  二重実装にならず、`blank-or-empty-report`と`no-decisive-ccvs-summary`の汎用reasonが
  残らない。
- 確認: `bash -n tests/nist/run_nist.sh`,
  `bash -n tests/nist/verifiers/lib.sh`,
  `make nist-run MODULE=NC PROGRAM=NC125A`,
  `make nist-run MODULE=CM PROGRAM=CM201M`,
  `make nist-run MODULE=NC PROGRAM=NC224A`,
  `make nist-run`, `make nist-compile-errors`を実行。
- NIST確認: 全体は`391 total / 114 pass / 277 fail / 0 ready / 0 CErr / 0 RErr`。
  `blank-or-empty-report`と`no-decisive-ccvs-summary`の完全一致reasonは0件。
  warning不足は`warning-flags-missing`として残り、診断実装不足としてIMPL-027へ送る。

### IMPL-010 実施記録

- 対象: `crates/cobol-codegen/src/stmt.rs`,
  `crates/cobol-codegen/src/expr.rs`,
  `crates/cobol-codegen/src/compiler.rs`
- 変更: `PERFORM VARYING ... AFTER ...`の内側制御変数をループ終了後に
  `FROM`値へ戻す処理を追加し、NC201AのPERFORM入れ子期待を満たすようにした。
  併せて制御フローreproを塞いでいたnumeric OCCURS要素の関数引数変換を修正し、
  `IND(B)`のような数値配列要素を`NUMVAL`のポインタ引数ではなく数値値として扱うようにした。
- 完了条件: `PERFORM`/`PERFORM THRU`由来の代表到達不足を解消し、
  残ったFAILを制御フロー以外の仕様カテゴリへ送る。
- 確認: `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=NC PROGRAM=NC201A`,
  `make nist-run MODULE=NC PROGRAM=NC202A`,
  `make nist-run MODULE=IF PROGRAM=IF101A`,
  `make nist-compile-errors`を実行。
- NIST確認: `NC201A`は`PASS (59 passed)`。`IF101A`はsegfaultと
  `detail-paragraph-mismatch|expected 26 paragraph case(s), got 16`が消え、
  `25 passed, 1 failed`へ移行した。残りの`F-ACOS-18`は`IND(5) / 9`の
  数値除算/関数値差分であり、IMPL-022からIMPL-023へ送る。
  `NC202A`はCErrではなく通常FAILへ移行し、`make nist-compile-errors`は0件。

### IMPL-011 実施記録

- 対象: `crates/cobol-ast/src/statement.rs`,
  `crates/cobol-parser/src/proc_div.rs`, `crates/cobol-hir/src/hir.rs`,
  `crates/cobol-hir/src/lower.rs`, `crates/cobol-codegen/src/context.rs`,
  `crates/cobol-codegen/src/stmt.rs`,
  `crates/cobol-driver/tests/e2e_test.rs`
- 変更: `ALTER A TO B C TO D ...` の複数ペアをparserで捨てず、
  AST/HIR/codegenまで保持して、各alterable paragraphのdispatch stateを更新するようにした。
  単一ALTER、複数ALTER、debug declarative連携の縮小e2eを確認した。
- 完了条件: `GO TO`、`GO TO DEPENDING ON`、単一ALTER、複数ALTERが
  `_goto_target`とalter dispatch stateで一貫して遷移する。
- 確認: `cargo test -p cobol-driver --test e2e_test test_native_alter_multiple_pairs_redirect_each_go_to_target`,
  `cargo test -p cobol-driver --test e2e_test test_native_alter_redirects_go_to_target`,
  `cargo test -p cobol-driver --test e2e_test test_native_use_for_debugging_alter_sets_debug_context`,
  `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-parser --all-targets`, `make release`,
  `make nist-run MODULE=NC PROGRAM=NC303M`,
  `make nist-run MODULE=NC PROGRAM=NC401M`,
  `make nist-compile-errors`を実行。
- NIST確認: `NC303M`は`PASS (4 warning flag(s) matched expected count)`。
  `NC401M`は実行意味論ではなく`warning-flags-missing|expected 40 warning flag(s), got 4`
  として残り、IMPL-027へ送る。`make nist-compile-errors`は0件。

### IMPL-012 実施記録（進行中）

- 対象: `crates/cobol-lexer/src/lexer.rs`,
  `crates/cobol-preprocessor/src/lib.rs`,
  `crates/cobol-hir/src/lower.rs`,
  `crates/cobol-codegen/src/codegen.rs`,
  `crates/cobol-driver/tests/e2e_test.rs`,
  `tests/nist/run_nist.sh`
- 変更: `SOURCE-COMPUTER ... WITH DEBUGGING MODE`をコンパイル時debug modeとして扱い、
  modeなしの固定形式`D`行をpreprocessor/lexerでコメント相当にした。
  `USE FOR DEBUGGING`はmodeありの場合だけHIRへ下ろし、
  実行時debug switchは`COBOL_DEBUGGING_MODE=OFF`でdeclarative dispatchを抑止する。
  さらに`ALL PROCEDURES`がdebug declarative自身へ再入しないよう、
  `_dispatch_debug_declarative`で`suppress`中のdispatchを無効化した。
- 根本原因: 旧実装は`USE FOR DEBUGGING`の存在だけでdebug declarativeを有効化し、
  compile-time mode、object-time switch、固定形式`D`行を分離していなかった。
  またpreprocessorが固定形式をFreeへ正規化するため、D行判定をlexerだけに置くと
  NIST実行経路ではindicator情報が消えていた。
- 確認: `cargo test -p cobol-preprocessor --all-targets`,
  `cargo test -p cobol-driver --test e2e_test use_for_debugging`,
  `cargo test -p cobol-driver --test e2e_test test_fixed_debug_lines_are_comments_without_source_debugging_mode`,
  `cargo test -p cobol-driver --test e2e_test test_native_use_for_debugging_all_procedures_does_not_reenter_declarative`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=DB PROGRAM=DB101A`,
  `make nist-run MODULE=DB PROGRAM=DB102A`,
  `make nist-run MODULE=DB PROGRAM=DB103M`,
  `make nist-run MODULE=DB PROGRAM=DB105A`,
  `make nist-run MODULE=DB`を実行。
- NIST確認: `DB101A`, `DB102A`, `DB103M`はPASS。
  `DB105A`はsegfaultが消え、通常の`ccvs-first-fail`へ移行した。
  DB全体は`15 total / 8 pass / 7 fail / 0 CErr / 0 RErr`。
- 未完了: `DB104A`は`GEN-LOOP`でSORT/DEBUG以前に止まるため、
  添字付き数値項目の更新または比較の欠陥として`IMPL-018A`へ再分類する。
  `DB201A`の残りは識別子debug発火タイミングとして`IMPL-012A`で継続する。
  `DB202A`から`DB205A`はOPEN/MERGE/DISABLE対象イベントとして`IMPL-012B`へ送る。
  `DB105A`の残りはsection/paragraph二重突入順として`IMPL-012C`へ送る。

### IMPL-012A 実施記録（完了）

- 対象: `crates/cobol-codegen/src/codegen.rs`,
  `crates/cobol-codegen/src/stmt.rs`
- 変更: `ALL REFERENCES OF data-name`を通常のprocedure-name debug対象から分離し、
  データ参照専用の`_dispatch_debug_reference`を追加した。
  `GO TO ... DEPENDING ON data-name`、`PERFORM VARYING/AFTER/UNTIL`、
  `MOVE`、`ADD`で識別子参照と更新後値を文脈別にdebug特殊レジスタへ設定する。
  `REDEFINES` aliasは通常declarativeだけへ発火させ、`ALL REFERENCES`の過剰発火を避ける。
  添字付き対象では`DEBUG-SUB-1`から`DEBUG-SUB-3`を空白埋め形式で設定する。
  修飾名は`ABC1 OF AB2 OF A1`のように保持し、`ALL REFERENCES OF AB2 OF A2`が
  `AB2 OF A1`へ誤発火しないようdebug対象を完全修飾単位で照合する。
- 確認: `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=DB PROGRAM=DB201A`,
  `make nist-run MODULE=DB`を実行。
- NIST確認: `DB201A`は`PASS (8 test(s) require inspection)`へ到達した。
  DB全体は`15 total / 9 pass / 5 fail / 1 CErr / 0 RErr / 60%`。
- 残件分類: `DB104A`は`IMPL-018A`、`DB202A`から`DB205A`は`IMPL-012B`、
  `DB105A`は`IMPL-012C`で継続する。

### IMPL-012B 実施記録（完了）

- 対象: `crates/cobol-hir/src/hir.rs`, `crates/cobol-hir/src/lower.rs`,
  `crates/cobol-codegen/src/codegen.rs`, `crates/cobol-codegen/src/context.rs`,
  `crates/cobol-codegen/src/stmt.rs`
- 変更: `USE FOR DEBUGGING ON file-name`がOPEN/CLOSE/READ/WRITE/REWRITE/DELETE/STARTを
  監視するようにし、`WRITE/REWRITE ... FROM`では暗黙MOVE後のrecord内容を
  `DEBUG-CONTENTS`へ設定する。`MERGE ... OUTPUT PROCEDURE`ではprocedure入口で
  `MERGE OUTPUT`内容を保持する。通信CD名ではENABLE/DISABLE/ACCEPT/SEND/RECEIVEの
  debugイベントを発火し、CDのrecord領域を`DEBUG-CONTENTS`へ設定する。
- 補足: `DEBUG-CONTENTS`は80文字固定ではなく長いrecord/CD内容とNULを含むgroup表現を
  保持できる内部バッファへ拡張した。
- 確認: `cargo fmt --all --check`, `cargo test -p cobol-codegen --all-targets`,
  `make release`, `make nist-run MODULE=DB PROGRAM=DB202A`,
  `make nist-run MODULE=DB PROGRAM=DB203A`,
  `make nist-run MODULE=DB PROGRAM=DB204A`,
  `make nist-run MODULE=DB PROGRAM=DB205A`,
  `make nist-run MODULE=DB`を実行。
- NIST確認: `DB202A`, `DB203A`, `DB204A`, `DB205A`はPASS。
  DB全体は`15 total / 13 pass / 1 fail / 1 CErr / 0 RErr / 86%`。
- 残件分類: `DB104A`は`IMPL-018A`、`DB105A`は`IMPL-012C`で継続する。

### IMPL-012C 実施記録（完了）

- 対象: `crates/cobol-codegen/src/codegen.rs`,
  `crates/cobol-codegen/src/data.rs`, `crates/cobol-codegen/src/stmt.rs`
- 変更: `ALL PROCEDURES`用に`_dispatch_debug_procedure`を追加し、
  data/file/CD用の`_dispatch_debug_declarative`と分離した。
  declarative呼び出し後に`_debug_event_explicit`を必ずリセットし、
  明示転送イベント後のfallthrough eventが古い`DEBUG-NAME`を引きずらないようにした。
  `ALTER proc TO ...`はprocedure-name debug eventとしてdispatchする。
- 変更: group内の`USAGE DISPLAY` numericをCOBOL物理レイアウト通り`char[]`で保持するようにし、
  `MOVE DEBUG-NAME TO PROC-NAME (i)`後に`BASE-NUMBER`が構造体paddingではなく
  文字位置6-8を参照するようにした。
- 確認: `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=DB PROGRAM=DB105A`, `make nist-run MODULE=DB`を実行。
- NIST確認: `DB105A`は`PASS (227 passed)`。
  DB全体は`15 total / 14 pass / 1 fail / 0 CErr / 0 RErr / 93%`。
- 残件分類: DBモジュールの残りは`DB104A`のみ。
  `DB104A`は添字付き数値項目またはREDEFINES/group比較の問題として`IMPL-018A`で継続する。

### IMPL-010A 実施記録（完了）

- 対象: `crates/cobol-codegen/src/stmt.rs`
- 変更: `PERFORM procedure THRU procedure`の範囲内にsectionが挟まる場合も
  section-awareな範囲展開を使うようにした。
  これにより`PERFORM PROC-118 THRU PROC-120`でsection entryの`PROC-119`を飛ばさず、
  section本体を呼んで直後paragraphも実行する。
- 確認: `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=DB PROGRAM=DB105A`を実行。
- NIST確認: `DB105A`の残り2件
  `PROC-119-PFM-C-10`, `PROC-139-PFM-C-11`が解消し、`DB105A`はPASS。

### IMPL-018A 実施記録（完了）

- 対象: `crates/cobol-codegen/src/codegen.rs`,
  `crates/cobol-codegen/src/stmt.rs`
- 根本原因: `DB104A`の`GEN-LOOP`は、添字付きDISPLAY numericの減算/比較ではなく、
  `PERFORM GEN-LOOP`内の`GO TO GEN-LOOP`が同一paragraph内ジャンプとして処理されず、
  外側entry dispatchの`_goto_target`へ漏れて呼び出し元が`GEN-LOOP`を再実行し続けていた。
- 変更: 孤立paragraph関数にも自分自身のローカルラベルを登録し、
  同一paragraphへの`GO TO`を関数内の`goto lbl_*`として解決するようにした。
  併せて`SORT ... INPUT PROCEDURE`/`OUTPUT PROCEDURE`突入時の
  `DEBUG-CONTENTS`を`SORT INPUT`/`SORT OUTPUT`に設定し、
  `USE AFTER ERROR PROCEDURE`突入時のdebug内容を`USE PROCEDURE`に設定した。
- 確認: `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=DB PROGRAM=DB104A`, `make nist-run MODULE=DB`を実行。
- NIST確認: `DB104A`は`PASS (3 test(s) require inspection)`。
  DB全体は`15 total / 15 pass / 0 fail / 0 CErr / 0 RErr / 100%`。

### IMPL-013 実施記録（完了）

- 対象: `crates/cobol-codegen/src/codegen.rs`,
  `crates/cobol-codegen/src/context.rs`, `crates/cobol-codegen/src/stmt.rs`
- 根本原因: `EXIT PROGRAM`を副プログラム内の単なるC関数`return`として生成していたため、
  section関数から副プログラムentry dispatcherへ戻った後に次paragraphへfallthroughし、
  CALL先本体が二重実行されていた。さらに、識別子CALLは同一生成単位内のnested programを
  `dlsym(RTLD_DEFAULT, ...)`だけで探しており、実行ファイルの動的symbol exportに依存していた。
  CALL先WORKING-STORAGEも呼出しごとに初期化しており、COBOLの保持規則から外れていた。
- 変更: 副プログラム内`EXIT PROGRAM`をCALLフレームへ戻す`cobol_goback()`として生成し、
  nested programへの識別子CALLは解決済みプログラム名と照合して直接呼び出すようにした。
  nested programのWORKING-STORAGE初期化は初回CALL時だけに限定した。
- 確認: `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver --test e2e_test exit_program`,
  `cargo test -p cobol-driver --test e2e_test call_on_exception`,
  `make release`, `make nist-run MODULE=IC PROGRAM=IC223A`,
  `make nist-run MODULE=IC`を実行。
- NIST確認: `IC223A`は`PASS (11 passed)`。
  IC全体は`25 total / 20 pass / 0 fail / 5 CErr / 0 RErr / 80%`。
  残る`IC106A`, `IC114A`, `IC207A`, `IC228A`, `IC235A`はcompile errorであり、
  IMPL-013の制御辺ではなく後続の個別実装タスクで扱う。

### IMPL-014 実施記録（進行中）

- 対象: `crates/cobol-parser/src/env_div.rs`, `crates/cobol-parser/src/lib.rs`,
  `crates/cobol-codegen/src/codegen.rs`, `crates/cobol-codegen/src/stmt.rs`
- 根本原因1: `SQ103A`では、`READ ... AT END`のEOF status更新直後に
  `USE AFTER STANDARD EXCEPTION` declarativeを無条件dispatchしていた。
  COBOLではstatement-levelの`AT END`句がEOFを処理する場合、file declarativeへ
  先に入ってはならない。
- 変更1: `READ`のfile status更新とdeclarative dispatchを分離し、
  `AT END`句または`INVALID KEY`句が該当statusを処理する場合は
  file declarative dispatchを抑止するようにした。
- 根本原因2: `SQ205A`の`SELECT SQ-FS1 ... STATUS GRP-STATUS-KEY-1`を
  `FILE STATUS`句として解析していなかったため、SQ-FS1のEOF statusが
  status data itemへ反映されず、declarative内のEOF判定が成立しなかった。
- 変更2: FILE-CONTROL内で標準形`FILE STATUS [IS] data-name`に加え、
  省略形`STATUS [IS] data-name`も同じ`file_status` ASTへ格納するようにした。
- 確認: `cargo test -p cobol-parser --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ103A`,
  `make nist-run MODULE=SQ PROGRAM=SQ205A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ103A`は`PASS (30 passed)`、`SQ205A`は`PASS (2 passed)`。
  SQ全体は`84 total / 47 pass / 37 fail / 0 CErr / 0 RErr / 55%`。
  残るFAILは`SQ105A`, `SQ107A`, `SQ108A`, `SQ109M`, `SQ110M`, `SQ116A`,
  `SQ117A`, `SQ123A`, `SQ124A`, `SQ133A`, `SQ134A`, `SQ136A`, `SQ137A`,
  `SQ138A`, `SQ144A`, `SQ156A`, `SQ201M`, `SQ206A`, `SQ208M`, `SQ209M`,
  `SQ211A`, `SQ212A`, `SQ214A`, `SQ218A`, `SQ219A`, `SQ220A`, `SQ221A`,
  `SQ222A`, `SQ223A`, `SQ224A`, `SQ225A`, `SQ226A`, `SQ227A`, `SQ228A`,
  `SQ302M`, `SQ303M`, `SQ401M`。
- 根本原因3: `USE AFTER ... INPUT/OUTPUT/I-O/EXTEND`のmode-based file declarativeを、
  ファイルのOPEN modeと照合せず、先に現れたmode declarativeへ無条件dispatchしていた。
  そのため`SQ105A`ではREAD EOFでOUTPUT declarativeが実行され、`SQ133A`では
  I-O open中のREAD EOFでI-O declarativeへ入らなかった。
- 変更3: 生成Cでファイルごとの現在OPEN modeを`FILE_MODE_*`として保持し、
  file declarative dispatchへ渡すようにした。OPEN成功時にmodeを設定し、
  CLOSE成功時に空へ戻す。
- 根本原因4: sequential REWRITE/WRITEのruntime状態機械が、EOF後REWRITE、
  REWRITE長さ不一致、sequential I-O modeでのWRITEを正常終了として扱っていた。
  さらにWRITE/REWRITE codegenが実際の01-level record長ではなくFD最大長を渡していた。
- 変更4: runtimeに「直前READが有効なrecordを選択しているか」を保持し、
  EOF後REWRITEを`43`、長さ不一致を`44`、sequential/line sequential I-O WRITEを`48`にした。
  codegenはWRITE/REWRITEに実record長を渡すようにした。
- 追加確認: `cargo test -p cobol-runtime --all-targets`,
  `cargo test -p cobol-runtime file_io::tests::test_sequential`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ105A`,
  `make nist-run MODULE=SQ PROGRAM=SQ133A`,
  `make nist-run MODULE=SQ PROGRAM=SQ134A`,
  `make nist-run MODULE=SQ PROGRAM=SQ137A`,
  `make nist-run MODULE=SQ PROGRAM=SQ138A`,
  `make nist-run MODULE=SQ PROGRAM=SQ144A`,
  `make nist-run MODULE=SQ PROGRAM=SQ156A`,
  `make nist-run MODULE=SQ PROGRAM=SQ226A`,
  `make nist-run MODULE=SQ PROGRAM=SQ228A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: 追加で`SQ105A`, `SQ133A`, `SQ134A`, `SQ137A`, `SQ138A`,
  `SQ144A`, `SQ156A`, `SQ226A`, `SQ228A`がPASS。
  SQ全体は`84 total / 55 pass / 29 fail / 0 CErr / 0 RErr / 65%`。
  残るFAILは`SQ106A`, `SQ107A`, `SQ108A`, `SQ109M`, `SQ110M`, `SQ116A`,
  `SQ117A`, `SQ123A`, `SQ124A`, `SQ136A`, `SQ201M`, `SQ206A`, `SQ208M`,
  `SQ209M`, `SQ211A`, `SQ212A`, `SQ214A`, `SQ218A`, `SQ219A`, `SQ220A`,
  `SQ221A`, `SQ222A`, `SQ223A`, `SQ224A`, `SQ225A`, `SQ227A`, `SQ302M`,
  `SQ303M`, `SQ401M`。
- 根本原因5: sequential READは一度AT ENDになった後の追加READでも常にstatus `10`を返していた。
  COBOLでは、AT END到達後に有効な次レコードがない状態でさらにREADするとstatus `46`になる。
- 変更5: runtimeのfile stateにAT END到達済みフラグを追加し、最初のEOFは`10`、
  以後のREADは次の成功READまで`46`を返すようにした。
- 追加確認: `cargo test -p cobol-runtime --all-targets`,
  `make nist-run MODULE=SQ PROGRAM=SQ136A`,
  `make nist-run MODULE=SQ PROGRAM=SQ137A`,
  `make nist-run MODULE=SQ PROGRAM=SQ138A`,
  `make nist-run MODULE=SQ PROGRAM=SQ105A`を実行。
- NIST確認: `SQ136A`がPASS。`SQ105A`, `SQ137A`, `SQ138A`はPASS維持。
- 追加NIST確認: `make nist-run MODULE=SQ`を再実行し、
  SQ全体は`84 total / 56 pass / 28 fail / 0 CErr / 0 RErr / 66%`。
  残るFAILは`SQ106A`, `SQ107A`, `SQ108A`, `SQ109M`, `SQ110M`, `SQ116A`,
  `SQ117A`, `SQ123A`, `SQ124A`, `SQ201M`, `SQ206A`, `SQ208M`, `SQ209M`,
  `SQ211A`, `SQ212A`, `SQ214A`, `SQ218A`, `SQ219A`, `SQ220A`, `SQ221A`,
  `SQ222A`, `SQ223A`, `SQ224A`, `SQ225A`, `SQ227A`, `SQ302M`, `SQ303M`,
  `SQ401M`。
- 根本原因6: `READ file INTO target`で、runtime READ先をFD recordではなく
  `INTO` targetにしていた。そのためFD record areaが更新されず、READ後にFD recordを
  参照するCCVS確認で古い内容が残っていた。
- 変更6: `READ ... INTO`は常にFD recordへ読み込み、READ成功後にFD recordから
  `INTO` targetへMOVE相当のcopy/truncate/padを行うようにした。
- 根本原因7: 可変長sequential fileのrecord境界と実record長をruntimeが保持していなかった。
  さらに`RECORD VARYING ... DEPENDING ON`のWRITE/REWRITEで、実record長として
  DEPENDING項目値ではなく01-level項目長を渡していた。
- 変更7: HIRへ可変長FD情報を追加し、`RECORD VARYING`および
  `RECORD CONTAINS min TO max`を可変長fileとしてcodegenへ伝搬するようにした。
  runtimeは可変長sequential recordを長さprefix付きで保存し、READ成功時の実record長を
  `cobol_file_current_record_length`で返す。codegenはREAD成功時にDEPENDING項目を更新し、
  WRITE/REWRITE時はDEPENDING項目値を実record長として渡す。
- 追加確認: `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-runtime --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ106A`,
  `make nist-run MODULE=SQ PROGRAM=SQ108A`,
  `make nist-run MODULE=SQ PROGRAM=SQ227A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ106A`は`PASS (69 passed)`、`SQ108A`は`PASS (8 passed)`、
  `SQ227A`は`PASS (16 passed)`。SQ全体は
  `84 total / 59 pass / 25 fail / 0 CErr / 0 RErr / 70%`。
  残るFAILは`SQ107A`, `SQ109M`, `SQ110M`, `SQ116A`, `SQ117A`,
  `SQ123A`, `SQ124A`, `SQ201M`, `SQ206A`, `SQ208M`, `SQ209M`,
  `SQ211A`, `SQ212A`, `SQ214A`, `SQ218A`, `SQ219A`, `SQ220A`,
  `SQ221A`, `SQ222A`, `SQ223A`, `SQ224A`, `SQ225A`, `SQ302M`,
  `SQ303M`, `SQ401M`。
- 根本原因8: `WRITE record FROM source`を単純な`memcpy(record, source, record_len)`で
  生成しており、FROM元がrecordより短い場合にCOBOLのMOVE相当の空白埋めが行われていなかった。
- 変更8: `WRITE ... FROM`ではrecord領域を空白初期化し、FROM元の既知byte長だけcopyするようにした。
- 追加確認: `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-runtime --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ117A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ117A`は`PASS (8 passed)`。SQ全体は
  `84 total / 60 pass / 24 fail / 0 CErr / 0 RErr / 71%`。
  残るFAILは`SQ107A`, `SQ109M`, `SQ110M`, `SQ116A`, `SQ123A`,
  `SQ124A`, `SQ201M`, `SQ206A`, `SQ208M`, `SQ209M`, `SQ211A`,
  `SQ212A`, `SQ214A`, `SQ218A`, `SQ219A`, `SQ220A`, `SQ221A`,
  `SQ222A`, `SQ223A`, `SQ224A`, `SQ225A`, `SQ302M`, `SQ303M`,
  `SQ401M`。
- 根本原因9: `CLOSE file REEL` / `CLOSE file UNIT`の付加句をparser/HIRが捨てており、
  通常の`CLOSE`として扱っていた。CCVSでは非reel/unit fileに対する該当CLOSEは
  file status `07`を返し、fileはopenのまま、file declarativeは実行されない。
- 変更9: parserが`CLOSE`付加句をASTへ保持し、HIR/codegenへ伝搬するようにした。
  `REEL`/`UNIT`はruntime closeを呼ばずstatus `07`のみをFILE STATUSへ反映し、
  declarative dispatchと`FILE_MODE_*`クリアを抑止する。
- 追加確認: `cargo test -p cobol-parser --all-targets`,
  `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ123A`,
  `make nist-run MODULE=SQ PROGRAM=SQ124A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ123A`は`PASS (9 passed)`、`SQ124A`は`PASS (19 passed)`。
  SQ全体は`84 total / 62 pass / 22 fail / 0 CErr / 0 RErr / 73%`。
  残るFAILは`SQ107A`, `SQ109M`, `SQ110M`, `SQ116A`, `SQ201M`,
  `SQ206A`, `SQ208M`, `SQ209M`, `SQ211A`, `SQ212A`, `SQ214A`,
  `SQ218A`, `SQ219A`, `SQ220A`, `SQ221A`, `SQ222A`, `SQ223A`,
  `SQ224A`, `SQ225A`, `SQ302M`, `SQ303M`, `SQ401M`。
- 根本原因10: `SQ116A`の`REWRITE ... FROM`残差はfile rewriteではなく、
  section内に畳み込まれたparagraph列での`PERFORM A THRU B`制御フローだった。
  monolithic section内の`PERFORM THRU` dispatcherはsectionローカルlabel idだけを
  範囲内遷移として扱っていた一方、呼び出された個別paragraph関数の`GO TO B`は
  top-level label idを返していた。そのため`GO TO CHECK-RECORD-EXIT`が
  PERFORM範囲外遷移として外側へ漏れ、後続paragraphが通常fallthroughで実行され、
  `RECORDS-IN-ERROR`を汚染していた。
- 変更10: `PERFORM ... THRU`の範囲内dispatch idに、sectionローカルidだけでなく
  top-level body label idも登録するようにした。これにより、section内から呼んだ
  個別paragraph関数が返す`GO TO`も同じPERFORM範囲内遷移として処理される。
  回帰確認として、section内`PERFORM CHECK-PARA THRU CHECK-EXIT`で
  `GO TO CHECK-EXIT`が中間paragraphを実行しないe2eを追加した。
- 追加確認: `cargo test -p cobol-driver --test e2e_test
  test_native_section_perform_thru_honors_goto_to_end_paragraph`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ116A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ116A`は`PASS (10 passed)`。SQ全体は
  `84 total / 70 pass / 14 fail / 0 CErr / 0 RErr / 83%`。
  残るFAILは`SQ107A`, `SQ109M`, `SQ110M`, `SQ201M`, `SQ206A`,
  `SQ208M`, `SQ209M`, `SQ211A`, `SQ212A`, `SQ214A`, `SQ225A`,
  `SQ302M`, `SQ303M`, `SQ401M`。
- 根本原因11: `SQ107A`は明示的な`RECORD VARYING`なしで同一FD配下に
  120 byteと151 byteの01レベルレコードを定義している。COBOLのFDでは
  複数の01レベルレコード記述が同じfile record areaを共有し、サイズが異なる場合は
  実レコード長がwrite/readごとに変わる。一方、HIRは可変長ファイル判定を
  `RECORD VARYING`または`RECORD CONTAINS min TO max`の明示句だけに限定していた。
  そのためruntimeは固定長151 byteとして読み、最初の120 byte短レコード後に
  sequential record境界がずれていた。
- 変更11: HIR loweringでFD配下の01レベルレコードサイズを比較し、サイズ差がある
  fileを`variable_record_files`へ追加するようにした。既存runtimeの可変長frame
  read/writeをそのまま使い、同長の複数01レコードは固定長のまま維持する。
  回帰確認として、複数01レコード長を持つFDのHIR判定テストと、短長2レコードを
  write/readするnative e2eを追加した。
- 追加確認: `cargo test -p cobol-hir multiple_record`,
  `cargo test -p cobol-hir equal_length`,
  `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver --test e2e_test
  test_native_fd_multiple_record_lengths_are_variable_records`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ107A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ107A`は`PASS (6 passed)`。SQ全体は
  `84 total / 71 pass / 13 fail / 0 CErr / 0 RErr / 84%`。
  残るFAILは`SQ109M`, `SQ110M`, `SQ201M`, `SQ206A`, `SQ208M`,
  `SQ209M`, `SQ211A`, `SQ212A`, `SQ214A`, `SQ225A`, `SQ302M`,
  `SQ303M`, `SQ401M`。
- 根本原因12: `SQ109M`と`SQ110M`は実行時のfile I/Oではなく、NIST固定形式
  preprocessed sourceの制御構造破壊だった。CCVSの実行対象切替用indicatorである
  `H`/`E`行を通常行へ戻さず、`I`/`F`行もコメント化していなかったため、
  lexerでは`H`/`E`/`I`/`F`が非標準indicatorとしてコメント扱いになった。
  その結果、`IF record-number = 325/196`の本体と終端periodが消え、次の独立した
  `IF record-number = 750/649`を誤って内側へ飲み込んだ。脱出条件が同時に成立しない
  nested IFとなり、CCVS帳票はヘッダだけでsummaryへ到達しなかった。
- 変更12: NIST preprocessorで`H`/`E`を通常行、`I`/`F`をコメント行へ正規化する
  ようにした。これによりCCVSのreel/unit系オプション行が意図通り有効化され、
  削除通知行は実行されない。
- 追加確認: `bash -n tests/nist/preprocess.sh`,
  `make nist-run MODULE=SQ PROGRAM=SQ109M`,
  `make nist-run MODULE=SQ PROGRAM=SQ110M`,
  `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ109M`と`SQ110M`はいずれも`PASS (6 passed)`。SQ全体は
  `84 total / 73 pass / 11 fail / 0 CErr / 0 RErr / 86%`。
  残るFAILは`SQ201M`, `SQ206A`, `SQ208M`, `SQ209M`, `SQ211A`,
  `SQ212A`, `SQ214A`, `SQ225A`, `SQ302M`, `SQ303M`, `SQ401M`。
- 分類13: `SQ201M`は`LINAGE IS ... WITH FOOTING ... LINES AT TOP/BOTTOM`と
  `WRITE ... ADVANCING`に対する`LINAGE-COUNTER`更新が未実装で、
  `LINAGE-COUNTER`がopen後もwrite後も0のままになる。これはfile open/read/writeの
  状態機械ではなく、既存タスク`IMPL-017`のLINAGE/output positioning範囲として扱う。
  IMPL-014では次のfile I/O系残件`SQ206A`へ進む。
- 根本原因14: `SQ206A`は`SAME RECORD AREA`そのものではなく、
  `OPEN INPUT SQ-FS1, SQ-FS3 OUTPUT SQ-FS4`のような複数mode groupを持つ
  `OPEN`文のparser不備だった。parserは`OUTPUT`を次のmode開始ではなく
  file-nameとして消費していたため、後続fileも前のmodeでHIR/codegenへ渡されていた。
  その結果、SQ-FS4/SQ-FS2がINPUTとして開かれ、作成対象fileのtruncate/writeが
  正しく行われていなかった。
- 変更14: `OPEN`文のfile-name列を読む際、`INPUT`/`OUTPUT`/`I-O`/`EXTEND`を
  次のmode group境界として扱うようにした。回帰確認として、
  `OPEN INPUT FILE-A, FILE-B OUTPUT FILE-C`が3 entriesかつFILE-CだけOUTPUTに
  なるparserテストを追加した。
- 追加確認: `cargo test -p cobol-parser test_parse_open_with_multiple_mode_groups`,
  `cargo test -p cobol-parser --all-targets`,
  `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ206A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ206A`は`PASS (4 passed)`。SQ全体は
  `84 total / 74 pass / 10 fail / 0 CErr / 0 RErr / 88%`。
  残るFAILは`SQ201M`, `SQ208M`, `SQ209M`, `SQ211A`, `SQ212A`,
  `SQ214A`, `SQ225A`, `SQ302M`, `SQ303M`, `SQ401M`。
- 分類15: `SQ208M`も`LINAGE`と`WRITE ... BEFORE/AFTER ADVANCING`の
  output positioningを目視確認するプログラムで、CCVS detail paragraph不足は
  `LINAGE-COUNTER`/ページ送り未実装に起因する。`SQ201M`と同じく`IMPL-017`範囲。
- 分類16: `SQ209M`も最小構成`LINAGE 40`と`WRITE ... ADVANCING PAGE/EOP`を
  目視確認するプログラムで、`PERFORM ... UNTIL LINAGE-COUNTER EQUAL 39`などが
  `LINAGE-COUNTER`未更新に依存して帳票summaryへ到達していない。`IMPL-017`範囲。
- 根本原因17: `SQ211A`の`CLOSE ... WITH LOCK`はparser/HIRまでは
  `WithLock`として保持されていたが、codegen/runtimeでは通常`CLOSE`と同じ
  `cobol_file_close`を呼んでいた。そのため`CLOSE WITH LOCK`後の再`OPEN INPUT`が
  成功し、期待file status `38`ではなく`00`になっていた。
- 変更17: runtimeにprocess-localなlocked path集合と
  `cobol_file_close_with_lock` ABIを追加し、`WithLock`時だけcodegenがこのABIを
  呼ぶようにした。再`OPEN`時にlocked pathを検出した場合はfile status `38`を返す。
  回帰確認として、`CLOSE WITH LOCK`後の再`OPEN`が`38`になるruntime/e2eを追加した。
- 追加確認: `cargo test -p cobol-runtime test_close_with_lock_blocks_reopen`,
  `cargo test -p cobol-driver --test e2e_test
  test_native_close_with_lock_sets_reopen_status_38`,
  `cargo test -p cobol-runtime --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ211A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ211A`は`PASS (4 passed)`。この時点のSQ全体は
  `84 total / 75 pass / 9 fail / 0 CErr / 0 RErr / 89%`。
  残るFAILは`SQ201M`, `SQ208M`, `SQ209M`, `SQ212A`, `SQ214A`,
  `SQ225A`, `SQ302M`, `SQ303M`, `SQ401M`。
- 根本原因18: `SQ212A`は`RECORD IS VARYING IN SIZE FROM 18 TO 2048
  DEPENDING ON RECORD-LENGTH`の境界違反を検証しているが、codegenは
  DEPENDING値を`0..最大record長`へ丸めるだけで、下限未満/上限超過を
  file status `44`にせず実際にwriteしていた。そのため15/16/17 byteの不正recordが
  fileへ混入し、後続readのrecord長検証が3件ずれていた。
- 変更18: HIRへ可変長recordの下限/上限を伝搬し、WRITE/REWRITE時に
  DEPENDING値が境界外ならruntime I/Oを呼ばずfile status `44`を設定するようにした。
  FILE STATUSおよびDECLARATIVESは既存のstatus更新経路を使う。回帰確認として、
  可変長record境界をHIRへ保持するtestと、境界外WRITEが`44`になるnative e2eを追加した。
- 根本原因19: 変更18の初回実装では、`RECORD VARYING DEPENDING data-name`のように
  明示`FROM`/`TO`がないFDで、下限を最大01-level record長にしていた。その結果、
  `SQ221A`の120 byte/151 byte混在recordのうち120 byte recordを誤って境界違反扱いにした。
- 変更19: 明示`FROM`/`TO`がない可変長FDでは、FD配下の01-level record記述から
  最小record長と最大record長を推定するようにした。回帰確認として、
  120/151 byteの複数01-level recordから`(120, 151)`を推定するHIR testを追加した。
- 追加確認: `cargo test -p cobol-hir variable_record_bounds`,
  `cargo test -p cobol-driver --test e2e_test
  test_native_variable_record_write_bounds_set_status_44`,
  `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ212A`,
  `make nist-run MODULE=SQ PROGRAM=SQ221A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ212A`は`PASS (1 passed)`、`SQ221A`は`PASS (6 passed)`。
  SQ全体は`84 total / 76 pass / 8 fail / 0 CErr / 0 RErr / 90%`。
  残るFAILは`SQ201M`, `SQ208M`, `SQ209M`, `SQ214A`, `SQ225A`,
  `SQ302M`, `SQ303M`, `SQ401M`。
- 根本原因20: `SQ214A`はsequential file自体ではなく、`WRITE record FROM group`で
  `OCCURS 1 TO 9 TIMES DEPENDING ON`を含む親groupを参照したときの有効長計算が
  仕様とずれていた。COBOLでは親group item参照時、ODO tableはDEPENDING現在値で
  指定されたoccurrenceだけが操作対象になるが、HIRはOCCURSの最大回数だけを保持し、
  DEPENDING項目名を捨てていた。そのためcodegenは最大サイズ全体をrecordへcopyし、
  `3 ACTIVE: 123`で止まるべきrecordに`456789`まで混入していた。
- 変更20: HIR data itemに`occurs_depending_on`を追加し、loweringで
  `OCCURS ... DEPENDING ON`の依存項目名を保持するようにした。codegenは
  `WRITE`/`REWRITE ... FROM`のsource長として、ODOを含むgroupの実効byte長を
  DEPENDING現在値から生成C式で計算する。通常の固定長groupやREAD INTOの既存挙動は
  変更しない。
- 追加確認: `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver --test e2e_test
  test_native_variable_record_write_bounds_set_status_44`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ214A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ214A`は`PASS (5 passed)`。SQ全体は
  `84 total / 77 pass / 7 fail / 0 CErr / 0 RErr / 91%`。
  残るFAILは`SQ201M`, `SQ208M`, `SQ209M`, `SQ225A`, `SQ302M`,
  `SQ303M`, `SQ401M`。
- 根本原因21: `SQ225A`は`OPEN EXTEND`対象fileが存在しない場合のfile statusを
  検証しているが、runtimeは`Extend`を`create(true).append(true)`で開いていた。
  COBOLの`OPEN EXTEND`は既存sequential fileを追記用に開く操作であり、未存在fileを
  暗黙作成してはならない。そのため期待status `35`ではなく`00`になり、
  `USE AFTER STANDARD ERROR PROCEDURE` declarativeにも入らなかった。
- 変更21: runtimeの`FileOpenMode::Extend`から未存在file作成を除き、OSのnot foundを
  既存のfile status `35`経路へ流すようにした。回帰確認として、
  未存在fileの`OPEN EXTEND`が`35`を返すruntime testを追加した。
- 追加確認: `cargo test -p cobol-runtime test_extend_missing_file_returns_not_found`,
  `cargo test -p cobol-runtime --all-targets`, `make release`,
  `make nist-run MODULE=SQ PROGRAM=SQ225A`, `make nist-run MODULE=SQ`を実行。
- NIST確認: `SQ225A`は`PASS (3 passed)`。SQ全体は
  `84 total / 78 pass / 6 fail / 0 CErr / 0 RErr / 92%`。
  残るFAILは`SQ201M`, `SQ208M`, `SQ209M`, `SQ302M`, `SQ303M`,
  `SQ401M`。
- 分類22: 残る`SQ302M`, `SQ303M`, `SQ401M`はsequential file runtimeではなく、
  CCVSが期待するobsolete/non-conforming warningの未出力である。`.reason`は
  `SQ302M`が`warning-flags-missing|expected 4 warning flag(s), got 0`、
  `SQ303M`が`empty-report|expected-flags=2|warning-flags=0`、
  `SQ401M`が`empty-report|expected-flags=18|warning-flags=0`で、compile logには
  対応warningが出ていない。対象featureは`LABEL RECORDS`, `VALUE OF`,
  `DATA RECORDS`, `MULTIPLE FILE TAPE`, `OPEN INPUT ... REVERSED`,
  `CLOSE ... WITH NO REWIND`, `OPEN ... WITH NO REWIND`, `READ ... NEXT`,
  `WRITE ... AT END-OF-PAGE`等の診断仕様であり、`IMPL-027`の
  obsolete/non-conforming warning診断範囲として扱う。
- 分類23: 残る`SQ201M`, `SQ208M`, `SQ209M`は既に分類済みのとおり
  `LINAGE`/`WRITE ... ADVANCING`のoutput positioning未実装であり、`IMPL-017`範囲。
- 完了判断: IMPL-014対象のsequential file open/read/write/status状態機械に起因する
  SQ残件は0になった。SQ残6件は`IMPL-017`または`IMPL-027`へ分類済みのため、
  IMPL-014は完了とする。

### IMPL-015 実施記録（進行中）

- 対象: `crates/cobol-ast/src/env_div.rs`, `crates/cobol-parser/src/env_div.rs`,
  `crates/cobol-hir/src/hir.rs`, `crates/cobol-hir/src/lower.rs`,
  `crates/cobol-codegen/src/codegen.rs`, `crates/cobol-codegen/src/stmt.rs`,
  `crates/cobol-runtime/src/abi.rs`, `crates/cobol-runtime/src/file_io.rs`
- 初期確認: IMPL-015着手時点のIXは`29 total / 5 pass / 24 fail / 0 CErr / 0 RErr / 17%`。
  主な先頭差分はindexed fileの`OPEN I-O`/`OPEN EXTEND` status、可変長indexed record、
  alternate key/START、same record area、warning診断だった。
- 根本原因1: parserは`SELECT OPTIONAL`を読み飛ばすだけでAST/HIRへ保持していなかった。
  そのためcodegen/runtimeはoptional indexed fileかどうかを判定できず、未存在fileに対する
  `OPEN I-O`/`OPEN EXTEND`でCOBOL期待の「fileを作成しつつstatus `05`」を実装できなかった。
  `IX216A`は`OPEN EXTEND`が`00`、`IX217A`は`OPEN I-O`が`35`になっていた。
- 変更1: `FileControlEntry`と`HirOpenEntry`へ`optional`を追加し、
  SELECT OPTIONALをAST→HIR→codegenへ伝搬した。runtimeには
  `cobol_file_open_indexed_optional`を追加し、optional indexed fileの未存在
  `OPEN I-O`/`OPEN EXTEND`ではfileを作成してstatus `05`を返すようにした。
  status `05`は正常open扱いとして`FILE_MODE`設定と可変長設定を行い、
  file declarative dispatchの対象外にした。
- 根本原因2: indexed fileでも`RECORD VARYING`がある場合はrecord境界を保持する必要があるが、
  runtimeの可変長record frame read/writeはsequential専用だった。`IX217A`は25件の240 byte recordと
  25件の200 byte recordを同じindexed fileへ書くため、固定240 byte境界でindex scan/readすると
  26件目以降のrecord番号がずれていた。
- 変更2: indexed fileでも`cobol_file_set_variable`後は長さprefix付き可変長frameでwrite/readし、
  index構築時もframeを読んで実record offsetを保持するようにした。可変長設定時にはindexed
  in-memory indexを再構築する。
- 根本原因3: `IX215A`のCErrはindexed runtimeではなく、debug event用group serializationが
  `REDEFINES`項目を通常の実体memberとして二重にcopyしようとしていたことだった。
  `REDEFINES`は同一storage上の別名なので、C structには対象memberが存在しない。
- 変更3: sort/debug group serialize/deserializeで`REDEFINES`項目を直列化対象から除外し、
  offsetも進めないようにした。
- 追加確認: `cargo test -p cobol-hir test_lower_select_optional_metadata`,
  `cargo test -p cobol-runtime test_optional_indexed_io_missing_file_creates_with_status_05`,
  `cargo test -p cobol-runtime test_variable_indexed_records_preserve_record_boundaries`,
  `cargo test -p cobol-parser --all-targets`, `cargo test -p cobol-hir --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `cargo test -p cobol-runtime --all-targets`,
  `make release`, `make nist-run MODULE=IX PROGRAM=IX216A`,
  `make nist-run MODULE=IX PROGRAM=IX217A`,
  `make nist-run MODULE=IX PROGRAM=IX215A`, `make nist-run MODULE=IX`を実行。
- NIST確認: `IX216A`は`PASS (14 passed)`、`IX217A`は`PASS (6 passed)`。
  `IX215A`はCOMPILE_ERRORから実行FAILへ進み、CErrは0になった。
  IX全体は`29 total / 17 pass / 12 fail / 0 CErr / 0 RErr / 58%`。
  残るFAILは`IX106A`, `IX108A`, `IX109A`, `IX112A`, `IX205A`, `IX206A`,
  `IX207A`, `IX208A`, `IX211A`, `IX215A`, `IX218A`, `IX401M`。
- 根本原因4: `IX109A`/`IX112A`はACCESS SEQUENTIAL indexed fileへの
  `WRITE`でprimary keyの昇順制約違反を検証しているが、runtimeのgeneric
  `cobol_file_write`はindexed fileでも単なるappendとして扱い、OPEN時に保持した
  primary key indexを参照していなかった。そのため降順keyのWRITEでもstatus `21`ではなく
  `00`になり、後続READの件数とEOF statusも連鎖して崩れていた。
- 変更4: indexed fileのgeneric `WRITE`をindexed key状態機械へ分岐し、
  ACCESS SEQUENTIALでは直前primary key以上でないWRITEをstatus `21`にした。
  同じ経路でduplicate keyをstatus `22`として検出し、成功時はin-memory indexを
  追記更新する。回帰確認として、降順keyのsequential indexed WRITEが`21`になる
  runtime testを追加した。
- 追加確認: `cargo test -p cobol-runtime
  test_sequential_indexed_write_rejects_descending_key`,
  `cargo test -p cobol-runtime --all-targets`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=IX PROGRAM=IX109A`,
  `make nist-run MODULE=IX PROGRAM=IX112A`, `make nist-run MODULE=IX`を実行。
- NIST確認: `IX109A`は`PASS (13 passed)`、`IX112A`は`PASS (7 passed)`。
  IX全体は`29 total / 19 pass / 10 fail / 0 CErr / 0 RErr / 65%`。
  残るFAILは`IX106A`, `IX108A`, `IX205A`, `IX206A`, `IX207A`, `IX208A`,
  `IX211A`, `IX215A`, `IX218A`, `IX401M`。
- 根本原因5: `IX108A`のDELETE系3件は、runtimeの削除失敗ではなく
  `DELETE ... INVALID KEY/NOT INVALID KEY`本文がHIRで破棄されていたことが原因だった。
  parser/ASTは両句を保持していたが、`HirStatement::Delete`には本文フィールドがなく、
  codegenも`cobol_file_delete`呼び出しとfile status更新だけを生成していた。
  そのためDELETE成功時に`NOT INVALID KEY`本文が実行されず、CCVSのswitch更新が欠落した。
- 変更5: `HirStatement::Delete`へ`invalid_key`/`not_invalid_key`を追加し、
  AST→HIR loweringで本文を保持するようにした。codegenはDELETE戻りstatusを使って
  `INVALID KEY`を非0、`NOT INVALID KEY`を0で分岐実行する。回帰確認として
  DELETE codegen testで両句本文が生成Cに残ることを確認した。
- 追加確認: `cargo test -p cobol-driver test_delete_statement_codegen`,
  `cargo test -p cobol-hir --all-targets`, `cargo test -p cobol-codegen --all-targets`,
  `make release`, `make nist-run MODULE=IX PROGRAM=IX108A`,
  `make nist-run MODULE=IX`を実行。
- NIST確認: `IX108A`は`PASS (32 passed)`。
  IX全体は`29 total / 20 pass / 9 fail / 0 CErr / 0 RErr / 68%`。
  残るFAILは`IX106A`, `IX205A`, `IX206A`, `IX207A`, `IX208A`,
  `IX211A`, `IX215A`, `IX218A`, `IX401M`。
- 分類6: `IX106A`はindexed file単独ではなく、relative/indexed/sequentialを同時に扱う
  統合ケースである。trace付き再実行では`SECT-0002-RIS101`の
  `WRITE-TEST-GF-02-01`でsegfaultし、生成Cはrelative fileの`READ RL-FR1`を
  `cobol_file_read_key(FILE_ID_RL_FR1, (const uint8_t*)RL_KEY, 3, ...)`としていた。
  整数のrelative keyをポインタ扱いしているためrecord key `1`がアドレス`0x1`になり、
  runtimeがkey bytesを読む時点で破綻する。これはrelative key ABI/codegenの欠陥であり、
  `IMPL-016`範囲へ分類する。
- 根本原因7: `IX205A`の`READ-TEST-F1-10`は`I-O-CONTROL. SAME RECORD IX-FD1 IX-FD2.`
  の共有record area検証である。parserはI-O-CONTROL paragraphを丸ごとskipしていたため、
  HIR/codegenは`IX-FD1`と`IX-FD2`のrecord bufferを独立させていた。
  その結果、直前の`READ IX-FD2 NEXT RECORD`で取得した内容が`IX-FD1R1-F-G-240`へ
  反映されず、CCVSは`IX-FD1`の古いrecord内容を観測していた。
- 変更7: `InputOutputSection`/`HirProgram`へ`same_record_areas`を追加し、
  I-O-CONTROLの`SAME RECORD`グループをAST→HIR→codegen contextへ伝搬した。
  codegenではREAD成功時に同じSAME RECORDグループ内のpeer record bufferへ
  読み取ったbytesをcopyする。
- 追加確認: `cargo test -p cobol-parser --all-targets`,
  `cargo test -p cobol-hir --all-targets`, `cargo test -p cobol-codegen --all-targets`,
  `make release`, `make nist-run MODULE=IX PROGRAM=IX205A`,
  `make nist-run MODULE=IX`を実行。
- NIST確認: `IX205A`は`PASS (12 passed)`。同じSAME RECORD欠陥に依存していた
  `IX206A`もPASSに戻った。IX全体は
  `29 total / 22 pass / 7 fail / 0 CErr / 0 RErr / 75%`。
  残るFAILは`IX106A`, `IX207A`, `IX208A`, `IX211A`, `IX215A`, `IX218A`,
  `IX401M`。
- 根本原因8: `IX207A`の最初の残差は`WRITE ... FROM FILE-RECORD-INFO (1)`ではなく、
  CCVS固定形式sourceのindicator列`T`/`U`を標準indicatorとして扱っていなかったことだった。
  NIST抽出sourceでは`T`行が有効行、`U`行が無効行として使われるが、lexer/preprocessorが
  両方を通常の無効indicatorとして落としていた。その結果、FD内のFILLER行が欠落し、
  alternate key offset/lengthが期待の`166/29`ではなく`142/5`として生成され、
  `START`後のcursor windowが誤っていた。
- 変更8: fixed-format normalize処理で`T/t`を通常行、`U/u`をcomment行へ正規化した。
  preprocessorとlexerの両方に同じ規則を入れ、`U`行はdebug line filterでもinactiveとして扱う。
  回帰確認として、preprocessor/lexerそれぞれに`T`行が残り`U`行が消えるテストを追加した。
- 根本原因9: `IX207A`の次の残差は、alternate key duplicatesの`READ NEXT`がstatus `02`を
  返したときに、codegenが成功扱いで後続処理を実行しながら同時に
  `USE AFTER STANDARD EXCEPTION` declarativeも呼んでいたことだった。COBOLの`02`は
  duplicate key付き成功状態であり、例外宣言手続きの対象ではない。`IX207A`ではこの誤発火が
  2回起き、CCVSのfail counterがちょうど2増えていた。
- 変更9: READの成功ガードを`_fs == 0 || _fs == 2`へ統一し、declarative dispatch、
  `READ INTO`/SAME RECORD copy、`NOT INVALID KEY`を同じ成功定義で分岐するようにした。
  回帰確認として、`READ`でstatus `02`がdeclarative dispatch条件から除外されるcodegen testを
  追加した。
- 追加確認: `cargo test -p cobol-lexer --all-targets`,
  `cargo test -p cobol-preprocessor --all-targets`,
  `cargo test -p cobol-driver test_write_from_subscripted_group_moves_to_record_area`,
  `cargo test -p cobol-driver test_read_duplicate_key_status_does_not_dispatch_declarative`,
  `cargo test -p cobol-codegen --all-targets`, `make release`,
  `make nist-run MODULE=IX PROGRAM=IX207A`, `make nist-run MODULE=IX`を実行。
- NIST確認: `IX207A`は`PASS (8 passed)`。IX全体は
  `29 total / 23 pass / 6 fail / 0 CErr / 0 RErr / 79%`。
  残るFAILは`IX106A`, `IX208A`, `IX211A`, `IX215A`, `IX218A`, `IX401M`。
- 根本原因10: `IX208A`の`START-TEST-GF-01`は`START IX-FS2.`のKEY句省略形を検証する。
  COBOLではKEY句を省略したindexed fileのSTARTは、FD record area内の主record keyを
  暗黙キーとして使う必要がある。現行HIRは`START`の省略KEYをそのまま`None`として保持し、
  codegenが`cobol_file_start(..., NULL, 0, 0, ...)`を生成していたため、runtimeはcursorを
  設定できず、後続`READ`が期待recordではなく単なる次recordを返していた。
- 変更10: HIR loweringの後処理で、indexed/relative fileの`START`省略KEYを
  FILE-CONTROLの`RECORD KEY`/`RELATIVE KEY`から補完するようにした。indexed fileでは
  `START file-name.`を`START file-name KEY IS EQUAL TO record-key.`相当のHIRへ正規化する。
  回帰確認として、HIR testとcodegen testで省略KEYがrecord key呼び出しへ変換されることを確認した。
- 追加確認: `cargo test -p cobol-hir test_lower_start_without_key_uses_indexed_record_key`,
  `cargo test -p cobol-driver test_start_without_key_uses_indexed_record_key`,
  `cargo test -p cobol-hir --all-targets`, `cargo test -p cobol-codegen --all-targets`,
  `make release`, `make nist-run MODULE=IX PROGRAM=IX208A`,
  `make nist-run MODULE=IX`を実行。
- NIST確認: `IX208A`は`PASS (29 passed)`。IX全体は
  `29 total / 24 pass / 5 fail / 0 CErr / 0 RErr / 82%`。
  残るFAILは`IX106A`, `IX211A`, `IX215A`, `IX218A`, `IX401M`。
- 根本原因11: `IX211A`はalternate keyでSTART/READしたrecordをREWRITEし、
  alternate key値が変更されても「現在のrecord pointer」はREWRITEで変更されないことを検証する。
  runtimeはREWRITE後にindexを再構築するだけで、active alternate index上の
  「次に読むべきentry」を保存していなかった。そのため、変更recordがkey順で後方へ移動すると、
  cursorが移動後のindex位置に引きずられ、次の`READ NEXT`が期待のrecord 183ではなく
  record 184を返していた。
- 変更11: indexed REWRITEでは、直前READで保持している`current_offset`を更新対象offsetとして使い、
  index再構築前にactive index上のnext entry offsetを保存する。再構築後は同じnext entry offsetの
  新しいindex位置へcursorを戻し、REWRITEでcurrent record pointerが変わらないようにした。
  回帰確認として、alternate key順でrecordが移動するREWRITE後も次のREAD NEXTが旧順序の
  次recordを返すruntime testを追加した。
- 追加確認: `cargo test -p cobol-runtime
  test_rewrite_preserves_next_position_in_active_alternate_index`,
  `cargo test -p cobol-runtime --all-targets`, `make release`,
  `make nist-run MODULE=IX PROGRAM=IX211A`, `make nist-run MODULE=IX`を実行。
- NIST確認: `IX211A`は`PASS (17 passed)`。IX全体は
  `29 total / 25 pass / 4 fail / 0 CErr / 0 RErr / 86%`。
  残るFAILは`IX106A`, `IX215A`, `IX218A`, `IX401M`。
- 根本原因12: `IX218A`は未存在OPTIONAL indexed fileに対する
  `OPEN INPUT`後の`READ`/`START`/random `READ`のfile statusと、処理済み例外句で
  USE宣言手続きが実行されないことを検証する。runtimeはOPTIONAL `OPEN INPUT`の
  未存在fileをopen済み空fileとして保持せず、後続READが`47`、STARTが`42`になっていた。
  またSTART codegenは`INVALID KEY`句がある場合でもfile status更新時にdeclarative dispatchを
  先に実行していた。
- 変更12: OPTIONAL indexed `OPEN INPUT`で未存在fileを空fileとしてopenし、status `05`で
  tableへ登録するようにした。これにより順READは`10`、START/random READは`23`を返す。
  START codegenはfile status更新ではdeclarativeを発火せず、`INVALID KEY`句がない未処理失敗時だけ
  USE宣言手続きを呼ぶようにした。
- 追加確認: `cargo test -p cobol-runtime
  test_optional_indexed_input_missing_file_behaves_as_empty_file`,
  `cargo test -p cobol-driver test_start_invalid_key_phrase_does_not_dispatch_declarative`,
  `cargo test -p cobol-runtime --all-targets`, `cargo test -p cobol-codegen --all-targets`,
  `make release`, `make nist-run MODULE=IX PROGRAM=IX218A`,
  `make nist-run MODULE=IX`を実行。
- NIST確認: `IX218A`は`PASS (6 passed)`。IX全体は
  `29 total / 26 pass / 3 fail / 0 CErr / 0 RErr / 89%`。
  残るFAILは`IX106A`, `IX215A`, `IX401M`。
- 根本原因13: `IX215A`の`START-TEST-GF-10`以降は、indexed `DELETE`が
  record area内のprimary keyではなく、直前cursorをprimary index位置として読み替えて
  削除対象を決めていたことが原因だった。CCVSは`MOVE key TO record-key; DELETE file`
  の形式で対象recordを指定するため、直前READ/STARTのcursorが別recordを指していると
  削除対象が取り違えられ、削除済みrecordへの`START`がinvalid keyにならなかった。
- 変更13: runtime ABIへ`cobol_file_delete_record(file, record_ptr, len)`を追加し、
  indexed DELETEのcodegenはFD record areaを渡すようにした。runtimeはprimary key offset/lengthを
  open時のindex定義から取り出し、record area内のkeyで削除対象offsetを決める。
  併せてDELETE後のcursorを削除entry位置へ戻し、次の`READ NEXT`が削除recordの直後を返すようにした。
- 根本原因14: `IX215A`のREWRITE系は、別の初期化READを挟んだあとに
  保存済みrecord内容をFD record areaへ戻して`REWRITE`する。indexed REWRITEもrecord area内の
  primary keyで更新対象を決める必要があるが、runtimeは直前READの`current_offset`を使っていたため、
  初期化READでcursorが別recordへ移動した後のREWRITEがduplicate/invalid key扱いになっていた。
- 変更14: indexed REWRITEはrecord area内のprimary keyから既存record offsetを検索して更新する。
  duplicate key検査もそのoffsetを自recordとして除外する。既存のalternate index cursor保持は維持し、
  REWRITE後の`READ NEXT`がREWRITE前のnext entryへ戻るようにした。
- 根本原因15: duplicateありalternate keyでは、同一key内の順序が「そのkeyへ入った順」である必要がある。
  旧実装はREWRITE後に全indexを物理file順で再構築していたため、record 176をduplicate keyへ入れた後に
  record 4を同じkeyへ入れるケースで、物理順のrecord 4が先に読まれていた。
- 変更15: indexed REWRITE後、duplicates指定のalternate indexでは更新recordを同一key groupの末尾へ
  再配置する。これにより、write/rewriteでduplicate keyへ入った順序を保持する。
- 根本原因16: `IX215A`のFD3は`IX-FD3-KEY`という同名項目をrecord key/alternate keyで
  親group修飾して使う。HIR loweringはqualified data nameをbase名だけに潰していたため、
  `IX-FD3-KEY OF IX-FD3-ALTKEY1-AREA`も`IX-FD3-KEY IN IX-FD3-RECKEY-AREA`も同じ
  `IX-FD3-KEY`になり、open時のalternate index offsetもREAD/START時のkey pointerも
  primary key側へ寄っていた。
- 変更16: FILE-CONTROL key句、random READ補完、START key句のloweringで、
  key名が親groupで修飾されている場合は含有key areaをHIR key名として保持する。
  これによりcodegenはkey areaの正しいpointer/length/offsetを使う。
- 追加確認: `cargo test -p cobol-runtime
  test_indexed_delete_record_uses_primary_key_from_record_area`,
  `cargo test -p cobol-runtime
  test_indexed_delete_record_positions_next_read_after_deleted_record`,
  `cargo test -p cobol-runtime
  test_indexed_rewrite_uses_primary_key_from_record_area_after_other_read`,
  `cargo test -p cobol-runtime
  test_indexed_rewrite_moves_duplicate_alternate_key_to_end_of_equal_key_group`,
  `cargo test -p cobol-hir test_lower_qualified_indexed_keys_use_containing_key_area`,
  `cargo test -p cobol-hir --all-targets`, `cargo test -p cobol-runtime --all-targets`,
  `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver test_delete_statement_codegen`, `make release`,
  `make nist-run MODULE=IX PROGRAM=IX212A`,
  `make nist-run MODULE=IX PROGRAM=IX215A`, `make nist-run MODULE=IX`を実行。
- NIST確認: `IX212A`は副作用修正後に`PASS (24 passed)`。
  `IX215A`は`PASS (33 passed)`。IX全体は
  `29 total / 27 pass / 2 fail / 0 CErr / 0 RErr / 93%`。
  残るFAILは`IX106A`, `IX401M`。`IX106A`はrelative key ABI/codegenで`IMPL-016`へ、
  `IX401M`はwarning診断で`IMPL-027`へ分類済み。

### IMPL-016 実施記録

- 対象: relative fileのrelative key/cursor/delete状態機械。
- 引き継ぎ: `IX106A`はindexed file単独ではなく、relative/indexed/sequential統合ケース。
  trace付き再実行では`SECT-0002-RIS101`の`WRITE-TEST-GF-02-01`でsegfaultし、
  生成Cはrelative fileの`READ RL-FR1`を
  `cobol_file_read_key(FILE_ID_RL_FR1, (const uint8_t*)RL_KEY, 3, ...)`としていた。
  整数relative keyをポインタ扱いしているためrecord key `1`がアドレス`0x1`になり、
  runtimeがkey bytesを読む時点で破綻する。
- 根本原因17: relative fileのrandom `READ`/`WRITE`/`DELETE`/`START`は、
  COBOLのrelative key数値をrecord numberとして渡す必要がある。しかしcodegenは
  indexed key向けのbyte pointer ABIへ同じ値を渡していたため、数値`1`などをCポインタとして
  解釈してsegfaultまたは不正record参照になっていた。
- 変更17: runtime ABIへrelative専用の
  `cobol_file_read_relative`/`cobol_file_write_relative`/
  `cobol_file_delete_relative`/`cobol_file_start_relative`を追加し、
  codegenはrelative organizationのrandom I/Oで数値式を`uint64_t` record numberとして渡す。
- 根本原因18: `IX106A`の初期化表は`RECORD-KEY-CONTENT`を
  `RECORD-KEY-DATA REDEFINES`で75要素tableとして参照する。生成Cは別struct pointerへのcastで
  `REDEFINES`を表現しており、最適化時のstrict aliasingでtable参照が壊れる余地があった。
- 変更18: native Cコンパイル時に`-fno-strict-aliasing`を付与し、
  COBOLの同一storage別名参照をC最適化で破壊しないようにした。回帰確認として、
  `REDEFINES`された`OCCURS`表を最適化native compileで読み取るe2e testを追加した。
- 根本原因19: NIST外部前処理の`XXXXXnnn`置換が単独プレースホルダではなく、
  文字列リテラル内部のデータ部分にも無条件適用されていた。`IX106A`では
  `"SSSSSTTTTT166WWWWWXXXXX060ALTKEY1..."`の`XXXXX060`が一時ファイルパスへ置換され、
  VALUE表そのものが壊れて225件中216件しかrelative fileへ書けなかった。
- 変更19: `tests/nist/preprocess.sh`のNIST placeholder置換を境界付きにし、
  文字列リテラル内部のテストデータを保持しつつ、単独の`XXXXX060`/`XXXXX063`などは
  従来通りファイルパスや環境値へ置換するようにした。
- 追加確認: `tests/nist/preprocess.sh`で`IX106A`のVALUE表が保持されること、
  `ST147A`の単独`XXXXX060`/`XXXXX063`が置換されることを確認した。
  `cargo test -p cobol-driver test_redefines_occurs_table_survives_optimized_native_compile`,
  `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-runtime --all-targets -- --test-threads=1`,
  `make release`, `NIST_COMPILE_CACHE=0 make nist-run MODULE=IX PROGRAM=IX106A`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=IX`を実行。
- NIST確認: `IX106A`は`PASS (10 passed)`。IX全体は
  `29 total / 28 pass / 1 fail / 0 CErr / 0 RErr / 96%`。
  残るFAILは`IX401M`のみで、warning診断不足として`IMPL-027`へ移行する。

### IMPL-027 実施記録

- 判断: `IMPL-027`の依存にあった`IMPL-026`はCOPY/library-name実装後の広い診断整備を想定したもの。
  現在の残件`IX401M`はNIST前処理後のindexed dynamic/non-conforming warning不足であり、
  COPY処理に依存しないため、DependsOnを`IMPL-007`へ縮小して着手可能にした。
- 根本原因20: `IX401M`は実行結果ではなくcompile warning flag数の検査である。
  期待10件に対して、parserはindexed intermediate相当の
  `ORGANIZATION IS INDEXED`、`ACCESS MODE IS DYNAMIC`、`RECORD KEY`、
  `READ ... INVALID KEY`など6件だけを出していた。
  high subsetで追加期待される`SELECT OPTIONAL`、`RESERVE n AREAS`、
  `ALTERNATE RECORD KEY`、`RECORD IS VARYING`には診断がなかった。
- 変更20: FILE-CONTROL解析で`SELECT OPTIONAL`、`RESERVE AREAS`、
  `ALTERNATE RECORD KEY`にwarningを追加した。FD解析では
  `RECORD IS VARYING`にwarningを追加した。
- 追加確認: `cargo test -p cobol-parser --all-targets`,
  `cargo test -p cobol-driver test_redefines_occurs_table_survives_optimized_native_compile`,
  `make release`, `NIST_COMPILE_CACHE=0 make nist-run MODULE=IX PROGRAM=IX401M`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=IX`を実行。
- NIST確認: `IX401M`は`PASS (10 warning flag(s) matched expected count)`。
  IX全体は`29 total / 29 pass / 0 fail / 0 CErr / 0 RErr / 100%`。

### IMPL-018/IMPL-019 実施記録（進行中）

- 最新ベースライン: `make nist-summary`を実行し、全体は
  `391 total / 266 pass / 125 fail / 0 ready / 0 CErr / 0 RErr / 68%`。
  `DB`, `IF`, `IX`は100% pass。残る最大阻害はruntime FAILである。
- CErr分類: `make nist-compile-errors`は`Total: 0`。
  DISPLAY numericを`char[]`として保持する経路と、`CobolDecimal`構造体として扱う
  算術・MOVE・debug codegen経路の契約不一致を解消した。
- 判断: 実行FAILの深掘りより先に、CErrを潰して未実行領域を実行可能にする。
  まずPICTURE/数値categoryの実装かい離として`IMPL-018`/`IMPL-019`へ戻し、
  `CobolDecimal`とdisplay numeric storageの生成C契約を修正する。
- 進捗: `NC118A`, `NC119A`, `NC117A`, `NC120A`は個別再実行でPASS。
  原因はDISPLAY numericを`char[]`として保持する経路と`CobolDecimal`構造体として扱う
  算術codegen経路の契約不一致、およびADD operand列で符号付き小数リテラルを前項の
  二項演算へ畳み込むparser解釈だった。
- 進捗: `NC132A`, `NC140A`, `RL105A`, `RL106A`は個別再実行でPASS。
  原因は`REDEFINES ... OCCURS`のDISPLAY numericを`int64_t*`へcastする
  data macro契約誤りと、cast rvalueに`&`を付けるdebug pointer生成だった。
- 進捗: `CM401M`はPASS。`IC235A`, `NC201A`, `NC202A`, `NC246A`,
  `NC250A`, `NC253A`はCErrからruntime FAILへ移行した。
- 進捗: `NC125A`は個別再実行でPASS。原因はnumeric-edited MOVE/ADDで
  DISPLAY numeric sourceのscaleを落としていたこと、浮動編集記号を整数桁として
  数えずに正規化で値を切り捨てていたこと、末尾カンマを小数点として扱っていたこと。
- 進捗: `NC124A`は個別再実行でPASS。原因はnumeric-editedの`+`/`-`/`$`浮動編集記号、
  `Z`だけのzero suppression、`P`の出力抑止とscale調整がPICTURE仕様とずれていたこと。
  さらに`PP9`などleading `P`のnumeric-to-numeric MOVEで、target scaleへ揃えた後に
  可視桁数だけを残す処理がなく、`.567`を`.007`へ切り詰められていなかった。
- 進捗: `NC126A`は個別再実行でPASS。原因は`*` zero suppression中の`B`挿入が
  空白のまま残り、`-*B*99`を`-***42`ではなく`-* *42`へ整形していたこと。
- 進捗: `NC170A`は個別再実行でPASS。原因は`$**.99`の固定通貨記号を
  zero-fill対象として`*`へ潰していたこと。`$`の直後に整数部`*`が続く場合は
  通貨記号を保持し、後続のzero suppression位置だけ`*`で埋めるようにした。
- 進捗: `NC105A`は個別再実行でPASS。原因はDISPLAY numeric targetへのMOVEで
  source/targetの小数桁を揃えず`9(5)V99`へ`99`を`.99`として格納していたこと、
  符号付きsourceから符号なしDISPLAY targetへMOVEする際に絶対値化していなかったこと、
  `---,999.99`の浮動`-`編集で固定整数`9`桁をゼロ埋めしていなかったこと。
- 進捗: `NC114M`は個別再実行でPASS。原因は数値sourceからalphanumeric-edited targetへ
  MOVEする経路で、符号付きsourceの表示符号を編集元文字列へ含めていたこと。
  `PIC XBXXX/XXX/XXX/XXX/XXXBXX`などのalphanumeric editedでは、挿入編集対象を
  数字列として扱い、source signは格納しない。
- 進捗: `NC116A`は個別再実行でPASS。原因は`SIGN LEADING/TRAILING SEPARATE`が
  HIRへ伝播せず、DISPLAY numericのstorage sizeとMOVE変換で符号バイトを通常桁として
  扱っていたこと。さらにgroup `SIGN`句の子項目継承、非分離`SIGN LEADING`の先頭
  overpunch格納、COMP/BINARYから別符号DISPLAYへのMOVE経路を補正した。
- 進捗: `NC104A`は個別再実行でPASS。原因はDISPLAY numeric targetへscaleを合わせる際、
  受け側整数桁へ収まらない高位桁を落とす前に`10^17`などを乗算してC側でoverflowして
  いたこと。また、別符号DISPLAY対応で通常の`int64_t` numeric sourceまでDISPLAY bytes
  と誤分類し、英数字MOVEで生メモリを読んでいた経路を補正した。
- 進捗: `NC123A`は個別再実行でPASS。原因は添字付きDISPLAY numeric operandを
  Decimal化する一部のADD/SUBTRACT経路で`decimal_places`を落としてscale 0として
  扱っていたこと、DISPLAY numeric targetへ内部Decimal値をtarget scaleへ戻さずに
  格納していたこと、`ROUNDED`/`ON SIZE ERROR`で固定PICTUREのscale/容量検査を
  target metadataではなく現在のDecimal状態に依存していたこと。さらに
  `GO TO ... DEPENDING ON`のHIRが`TABLE5-NUM (INDEX5)`のsubscriptを落としていたため、
  selectorを式として保持してDISPLAY numeric値を読ませるようにした。
- 進捗: `cobol-driver` e2e 216件はPASS。追加原因は、非DEBUGのfile declarativeでも
  debug helperを無条件生成していたこと、`PERFORM VARYING`構造テストが現行の
  `for (;;) + if break`生成契約ではなく古い`while`生成を期待していたこと、
  paragraph内から外部paragraphへ`GO TO`した際にtop-level dispatcher用の
  `_goto_target`番号をparagraphローカルdispatcherで解釈してALTER系が循環していたこと。
  debug event生成条件を`USE FOR DEBUGGING`存在時へ揃え、外部paragraph転送は
  `_goto_target`を設定して呼び出し元dispatcherへ`return`する契約に修正した。
- 進捗: `NC107A`は個別再実行でPASS。原因は`PERFORM paragraph THRU paragraph`の
  範囲内にSECTION見出しが挟まる場合、SECTION関数と配下paragraphを重複実行して
  through終端後のparagraphまで走っていたこと。また`PIC 9.999.999,99`のように
  `.`と`,`が両方あるnumeric-edited PICTUREで、常に`.`を実小数点として扱い、
  `DECIMAL-POINT IS COMMA`相当の右端`,`小数点を整形できていなかったこと。
  paragraph-to-paragraph THRUでは中間SECTION見出しを呼び出し対象から外し、
  両方の記号があるnumeric-edited PICTUREでは右端の記号を実小数点として扱う。
- 確認: `cargo test -p cobol-runtime string_ops::tests::test_store_numeric_display
  -- --nocapture`, `cargo test -p cobol-codegen --all-targets`,
  `cargo test -p cobol-driver --test e2e_test`, `cargo fmt --check`, `make release`,
  `cargo test -p cobol-runtime decimal::tests::test_decimal_to_display_decimal_point_comma_picture
  -- --nocapture`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=NC PROGRAM=NC116A`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=NC PROGRAM=NC104A`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=NC PROGRAM=NC123A`,
  `NIST_COMPILE_CACHE=0 make nist-run MODULE=NC PROGRAM=NC107A`,
  `make nist-summary`を実行。
- 次の作業: runtime FAIL 125件を、numeric edited formatter/parser、
  decimal算術/SIZE ERROR、MOVE CORRESPONDING、REDEFINES、PERFORM制御、
  REPORT/SORT/COMMUNICATIONへ再分類して、件数の大きい順に潰す。

## Backlog一覧

| ID | Status | Summary | DependsOn |
| ---- | ---- | ---- | ---- |
| BACKLOG-001 | ✅ | CErrファミリをcodegen契約単位で修正する | TASK-003 |
| BACKLOG-002 | ⏳ | 共通CCVSパーサをrunnerへ導入する | TASK-002 |
| BACKLOG-003 | ⏳ | COBOL制御フローをHIR契約として再設計する | TASK-004 |
| BACKLOG-004 | ⏳ | ファイルI/O runtime状態機械を実装する | TASK-005 |
| BACKLOG-005 | ⏳ | decimal演算とMOVE変換を一元化する | TASK-006 |
| BACKLOG-006 | ⏳ | intrinsic function変換規則を統一する | TASK-007 |
| BACKLOG-007 | ⏳ | COPY/REPLACINGと警告診断を仕様化する | TASK-008 |
| BACKLOG-008 | ⏳ | REPORT/SORT/SEGMENTを仕様単位で実装する | TASK-009 |
| BACKLOG-009 | ⏳ | NIST 100% passをCI必須条件として固定する | TASK-010 |

## Backlog詳細（補足が必要な場合のみ）

### BACKLOG-001

- 補足: `COMPILE_ERROR` を 0 にすることが、以後のFAIL分類精度を上げる前提。

### BACKLOG-002

- 補足: 現在の `ccvs-first-fail` 集約だけでは原因が粗すぎるため、実装修正前に観測粒度を上げる。

### BACKLOG-009

- 補足: 既存CI条件は `pass == total` と `Ready/Fail/CErr/RErr == 0` を要求する形にする。
