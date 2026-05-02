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
| TASK-002 | ⏳ | CCVS出力判定の共通分類表を作る | TASK-001 |
| TASK-003 | ⏳ | 19件のCErrをcodegen欠陥別に再現する | TASK-001 |
| TASK-004 | ⏳ | 制御フロー仕様差分の代表reproを作る | TASK-002 |
| TASK-005 | ⏳ | ファイルI/O仕様差分の代表reproを作る | TASK-002 |
| TASK-006 | ⏳ | 数値変換と算術仕様差分の代表reproを作る | TASK-002 |
| TASK-007 | ⏳ | 組込み関数仕様差分の代表reproを作る | TASK-002 |
| TASK-008 | ⏳ | COPYと診断仕様差分の代表reproを作る | TASK-002 |
| TASK-009 | ⏳ | REPORT/SORT/SEGMENT仕様差分を分離する | TASK-002,TASK-005 |
| TASK-010 | ⏳ | 100% passまでの実装ロードマップを確定する | TASK-003,TASK-004,TASK-005,TASK-006,TASK-007,TASK-008,TASK-009 |

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

### TASK-003

- 補足: CErrは、整数/ポインタ不整合 7 件、
  macro/name collision 3 件、decimal/int不整合 4 件、
  配列代入 2 件、group raw/value型名不整合 2 件、
  pointer/int不整合 1 件に分かれる。
- 注意: CErrはNIST全体の入口を塞ぐため、FAILより先にcodegen契約として潰す。
- 成果物: 各CErrファミリの最小COBOL入力、期待されるC表現、責務crate一覧。

### TASK-004

- 補足: 対象は `PERFORM`, `GO TO`, `ALTER`, `DECLARATIVES`,
  `USE`, `AT END`, `INVALID KEY`, `ON SIZE ERROR`。
- 補足: 影響範囲は `IF`, `NC`, `DB`, `RL`, `RW`, `SM`, `SQ`, `OB`。
- 成果物: COBOL制御移譲仕様とHIR/codegen/runtime責務の差分表。

### TASK-005

- 補足: 対象は sequential/indexed/relative の `OPEN`, `READ`,
  `WRITE`, `REWRITE`, `DELETE`, `START`, cursor, file status。
- 補足: 影響範囲は `IX`, `RL`, `SQ`, `ST`, `SG`, `OB`, `NC`。
- 成果物: ファイル組織ごとの状態遷移表と代表repro。

### TASK-006

- 補足: 対象は `MOVE`, `ADD`, `SUBTRACT`, `MULTIPLY`, `DIVIDE`,
  `COMPUTE`, `ROUNDED`, `SIZE ERROR`, `PICTURE`, edited numeric。
- 補足: 影響範囲は `NC`, `IC`, `CM`, `IF`。
- 成果物: 数値カテゴリ、scale、符号、丸め、桁あふれの仕様表と代表repro。

### TASK-007

- 補足: 対象は `IF` の数学、統計、文字列、日時系 intrinsic function。
- 注意: 個別関数ごとではなく、引数変換、戻り値カテゴリ、境界値、
  丸めの共通規則を先に定義する。
- 成果物: intrinsic共通変換モデルと失敗関数の分類表。

### TASK-008

- 補足: 対象は `COPY`, `REPLACING`, library-name, continuation,
  compile-time warning flags。
- 補足: 影響範囲は `SM`, `SG`, `SQ`, `RL`, `NC`。
- 成果物: source manipulation と sema診断ルールの差分表。

### TASK-009

- 補足: 対象は Report Writer、SORT/MERGE、segmentation、printer/output capture。
- 注意: ファイルI/Oと出力捕捉の不備に巻き込まれるため、TASK-005の分類後に分離する。
- 成果物: REPORT/SORT/SEGMENT固有欠陥と共通I/O欠陥の切り分け表。

### TASK-010

- 補足: 実装順は `CErr解消 -> 判定精度改善 -> 制御フロー -> ファイルI/O -> 数値/関数 -> 残余モジュール` を基本線にする。
- 注意: 各実装タスクは必ず縮小e2eテストを追加してからNISTモジュールを再実行する。
- 成果物: 100% passまでの実装マイルストーン、CI gate、完了条件一覧。

## Backlog一覧

| ID | Status | Summary | DependsOn |
| ---- | ---- | ---- | ---- |
| BACKLOG-001 | ⏳ | CErrファミリをcodegen契約単位で修正する | TASK-003 |
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
