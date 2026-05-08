# 製品レベルまでの残課題一覧

更新日: 2026-05-08

この文書は「まだ残っている課題」だけを記録する。
ここでの「製品レベル」は、実データ、複数 OS、長時間実行、障害時の診断性、
互換性維持まで含めて安全に運用できる状態を指す。

## 判定基準

- 実装済みであることと、製品レベルであることを分けて判断する
- NIST CCVS や単体テストで確認済みでも、実運用上の耐久性、
  障害復旧、互換性、運用監視が不足する場合は残課題として扱う
- `Partial` または `Experimental` と記録されている機能は、
  明示的な完了基準と検証結果がない限り製品レベル未達として扱う
- ABI 変更、I/O 仕様、例外制御、decimal 算術、並行実行は
  互換性・正確性への影響が大きいため優先的に管理する

## P0: 残課題管理の整合性

### 実装済みと production-ready の区別を文書化する

- 現状:
  - `docs/cobol-standards.md` と `docs/user-guide.md` には
    `Partial` / `Experimental` / production-not-complete の記述が残っている
  - 一方で、この文書は以前「未解消項目はない」としていた
- 不足:
  - 機能が「存在する」状態と「製品運用に耐える」状態の判定が分離されていない
  - 完了済み扱いにできる検証基準が機能ごとに明確でない
- 完了条件:
  - 各 runtime 領域について、対応範囲、非対応範囲、検証済み条件を明記する
  - `docs/cobol-standards.md`、`docs/user-guide.md`、本書の状態表現を一致させる

## P1: Runtime ABI と実行基盤

### raw/opaque な ABI 契約を typed descriptor に寄せる

- 現状:
  - `docs/runtime-abi-contract.md` は、communication runtime と
    JSON / XML runtime に raw/opaque な契約が残っていると記録している
  - communication runtime は destination、error-key、area-count などを長い生引数列で受ける
- 不足:
  - 引数順、長さ、stride、領域数の対応を型で検証できない
  - ABI 互換性レビューの対象が広く、変更時に破壊的変更を見落としやすい
  - generated C と runtime の契約を単体で検証しにくい
- 完了条件:
  - communication、JSON、XML の ABI を named descriptor と明示的な `repr(C)` 型へ整理する
  - `emit_c_declarations`、runtime 実装、codegen 利用箇所を同一変更単位で更新する
  - ABI layout の snapshot test または C compile test を追加する

### CALL / GOBACK / exception のスレッド安全性と復帰意味を固める

- 現状:
  - `program.rs` は CALL stack をグローバル `Mutex<Vec<usize>>` で保持する
  - `exception.rs` は exception state をグローバル `Mutex` で保持する
  - 復帰制御は `setjmp` / `longjmp` のアドレスを runtime が保存する方式である
- 不足:
  - スレッドごとの CALL stack / exception state になっていない
  - handler stack 上限超過時の診断と失敗動作が弱い
  - `RESUME target` は実質的に状態クリアのみで、完全な復帰先制御ではない
  - FFI 境界、panic、longjmp の組み合わせに対する安全性レビューが必要
- 完了条件:
  - CALL stack と exception state の thread-local 化または明示的な実行コンテキスト化を行う
  - `RESUME` の対応範囲を仕様化し、不完全な範囲は診断または非対応として扱う
  - nested CALL、nested exception、threaded execution の native test を追加する

## P1: File I/O

### Indexed / relative file の実運用耐久性を強化する

- 現状:
  - runtime は sequential、line sequential、relative、indexed file I/O を実装している
  - indexed file はファイルをスキャンしてインメモリ index を構築する
  - file table と close-with-lock 状態はプロセス内グローバル状態で管理される
- 不足:
  - 永続 index がなく、大容量 indexed file で起動・再構築コストが大きい
  - 複数プロセスから同一ファイルを扱う排他制御が弱い
  - クラッシュ途中の write / rewrite / delete に対する復旧方針がない
  - OS ごとの lock、改行、path、permission 差異の体系的検証が不足している
- 完了条件:
  - indexed file の永続 index または明示的な非対応範囲を設計する
  - 複数プロセス排他、クラッシュ復旧、巨大ファイルの性能基準を決める
  - Linux、macOS、Windows で file I/O の代表ケースを CI で検証する

### File status と declaratives の end-to-end 検証を増やす

- 現状:
  - `FILE STATUS` と基本的な I/O status code は実装されている
  - `DECLARATIVES` / `USE AFTER EXCEPTION` は `Partial` と記録されている
- 不足:
  - ファイル例外から declaratives へ制御が渡る end-to-end ケースが十分でない
  - status code と declarative dispatch の優先順位を網羅的に固定できていない
- 完了条件:
  - sequential、relative、indexed の主要異常系を native test と NIST regression に追加する
  - `INVALID KEY`、`AT END`、declaratives、`FILE STATUS` の相互作用を仕様化する

## P1: Decimal / Numeric Runtime

### decimal 算術の桁あふれ・丸め・例外通知を厳密化する

- 現状:
  - `CobolDecimal` は scaled integer として表現される
  - 演算時に size に収まるよう clamp / truncate する処理がある
- 不足:
  - 桁あふれが単なる値の丸め込みになり、`ON SIZE ERROR` や例外通知と分離しにくい
  - `i128` から `i64` へ戻す境界で、あふれ検出の明示性が不足している
  - 金融系で必要になる丸め規則、符号、scale 伝播の仕様固定が不足している
- 完了条件:
  - arithmetic operation ごとに overflow、rounding、size error の発生条件を明文化する
  - codegen と runtime の責務分担を固定し、`ON SIZE ERROR` 連携テストを追加する
  - 境界値、最大桁、負数、小数桁、edited numeric の回帰テストを拡充する

## P2: Partial / Experimental Runtime 機能

### SCREEN SECTION を製品機能にするか限定機能にするか決める

- 現状:
  - runtime は ANSI escape sequence による cursor 制御と line input を提供する
  - 実装コメント上も simplified implementation とされている
- 不足:
  - 端末種別、Windows console、非対話実行、フォーム入力、属性制御の網羅性がない
  - screen I/O の自動テストと CI 検証が不足している
- 完了条件:
  - 対応端末と非対応端末を明記する
  - 製品対象にする場合は pseudo-terminal test と Windows smoke を追加する
  - 対象外にする場合は compile-time diagnostic または user guide の制限として明記する

### COMMUNICATION SECTION を実通信基盤として扱うか決める

- 現状:
  - communication runtime は in-process queue model として実装されている
  - fixture / script による NIST 向け設定読み込みを持つ
- 不足:
  - 複数プロセス、永続化、外部 message broker、認証、監査ログ、再送制御がない
  - 製品の通信機能として期待される運用監視と障害復旧がない
- 完了条件:
  - in-process simulation として提供するのか、実通信 runtime として拡張するのかを決める
  - 実通信対象にする場合は backend abstraction、永続化、監視、認証の設計を追加する
  - simulation に限定する場合は標準対応表と user guide に制限を明記する

### SORT / MERGE と report writer の partial 範囲を閉じる

- 現状:
  - `SORT` / `MERGE` basic flow は `Partial`
  - `SORT ... INPUT PROCEDURE` / `OUTPUT PROCEDURE` も production-complete ではない
  - report writer は基本 lifecycle と group line 出力に限定される
- 不足:
  - 大容量 sort、外部 sort、temporary file 管理、異常終了時 cleanup の運用設計が不足している
  - report layout formatting の標準準拠範囲が不足している
- 完了条件:
  - sort/merge のデータ量、memory、temporary file、安定性の基準を決める
  - report writer の対応 layout と非対応 layout を標準表に明記する
  - representative native tests と NIST module regression を追加する

### COBOL 2002+ / 2014+ / 2023 runtime 機能の範囲を固定する

- 現状:
  - intrinsic functions、OO、`FUNCTION-ID`、`TYPEDEF`、`VALIDATE`、
    `ALLOCATE` / `FREE`、2023-only features に partial / experimental が残る
- 不足:
  - どの標準モードで何を製品対応とするかが機能ごとに固定されていない
  - runtime 側のメモリ管理、object dispatch、validation constraint の失敗動作が弱い
- 完了条件:
  - 標準別 feature matrix を production-ready / partial / unsupported に再分類する
  - partial 機能は compile-time diagnostic、runtime diagnostic、または明示制限を持たせる

## P2: Threading

### threading runtime の同期プリミティブを運用可能にする

- 現状:
  - `threading.rs` は basic threading と mutex primitive を提供する
  - mutex は `AtomicBool` spinlock である
- 不足:
  - 公平性、待機タイムアウト、所有者検証、deadlock 診断がない
  - thread panic は stderr 出力に留まり、COBOL 側へ構造化された失敗として返らない
  - runtime の他のグローバル状態が thread-safe な実行コンテキストとして整理されていない
- 完了条件:
  - mutex を OS-backed primitive または明示的な限定仕様へ変更する
  - thread failure、join、resource limit の status contract を定義する
  - file I/O、CALL/exception、communication と組み合わせた並行実行テストを追加する

## P3: 運用・検証

### Cross-platform runtime validation を拡充する

- 現状:
  - Windows は smoke test 方針が記録されている
  - x86 Linux runtime validation の文書はある
- 不足:
  - Windows では full NIST、file I/O、screen、path/permission 差異まで検証していない
  - macOS、Linux x86、Windows の差異を runtime compatibility matrix として管理していない
- 完了条件:
  - OS 別に runtime 機能の検証レベルを記録する
  - Windows で file I/O と native generated C 実行の代表ケースを CI に追加する
  - Linux x86 で NIST と benchmark の定期確認を維持する

### 長時間・大容量・障害注入テストを追加する

- 現状:
  - 単体テスト、driver e2e、NIST regression が中心である
- 不足:
  - 大容量 indexed file、長時間 sort、繰り返し CALL、並行実行、異常終了復旧の検証が不足している
  - performance regression の閾値と failure triage の基準がない
- 完了条件:
  - runtime stress test suite を追加する
  - 大容量、長時間、障害注入、並行実行の代表ケースを定期実行する
  - benchmark と correctness regression の結果を release 判定に含める

## メモ

- この文書は「残課題一覧」なので、完了済み項目は残さない
- 完了済みの対応状況は
  [docs/cobol-standards.md](./cobol-standards.md) と
  [docs/user-guide.md](./user-guide.md) に反映する
- 課題を完了扱いにする場合は、該当する検証コマンド、対象 OS、
  既知の非対応範囲を併記してから削除する
