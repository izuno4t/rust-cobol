# COBOL 適合性検証計画

更新日: 2026-05-08

この文書は、COBOL 仕様調査を実装検証へ落とし込むためのトレーサビリティを
管理する。仕様書やベンダー資料の引用だけでは実装検証にならないため、
各仕様領域について、実行可能なテスト、判定基準、未検証範囲を明示する。

## 結論

- 実装検証の主軸は、単体テスト、E2E テスト、NIST CCVS85、必要に応じた
  GnuCOBOL 比較である。
- ISO/IEC 1989:2023 の本文を直接確認できていない範囲は、
  conformance claim に使わない。
- NIST CCVS85 は COBOL-85 系の強い検証資産だが、COBOL 2002 以降の機能を
  検証しない。
- 適合性検証は製品化判断の一部であり、それだけでは製品化可否を決めない。
  製品化判断は [COBOL 製品化判断基準](./cobol-product-readiness.md) で扱う。
- `docs/cobol-specification.md` は仕様調査、本文書は実装検証の管理に使う。

## 検証の定義

このリポジトリで「仕様に対して検証済み」と呼ぶには、少なくとも次を満たす。

- 仕様領域が明確に定義されている
- 対応するテストまたは外部検証スイートがある
- 成功条件が機械的に判定できる
- 失敗、未対応、手動確認が `PASS` と混同されない
- 処理系定義または拡張仕様は標準機能と分けて文書化されている

文書、実装済みコード、手元で成功したサンプルだけでは検証済みとは扱わない。

## 検証レベル

| レベル | 意味 | 判定に使える根拠 |
| --- | --- | --- |
| `Verified` | 対象機能が自動テストで継続的に判定されている | `cargo test`、E2E、NIST `PASS` |
| `Partially verified` | 主要経路は検証済みだが、仕様範囲全体ではない | 一部 E2E、限定的な NIST module |
| `Review required` | 生成物やログは確認できるが、自動合否ではない | `make nist-audit`、`INSPECT` 結果 |
| `Unverified` | 仕様に対する実行可能な検証がない | 文書のみ、実装のみ、手動サンプルのみ |

## 現在の検証資産

| 資産 | コマンド | 主な対象 | 限界 |
| --- | --- | --- | --- |
| Rust unit tests | `cargo test --workspace` | lexer、parser、sema、HIR、runtime、codegen の局所仕様 | COBOL 標準全体の網羅性は示さない |
| Driver E2E tests | `cargo test --test e2e_test --package cobol-driver` | COBOL 入力から native 実行までの代表経路 | テストケースに書いた範囲だけを検証する |
| Preprocess/Lex tests | `cargo test --test preprocess_lex_test --package cobol-driver` | `COPY`、`REPLACE`、固定形式継続行 | source manipulation 全体の網羅ではない |
| Toolchain tests | `cargo test --test toolchain_test --package cobol-driver` | driver と runtime archive 解決 | 言語仕様検証ではない |
| NIST CCVS85 | `make nist-prepare`、`make nist-run MODULE=NC` | COBOL-85 系の標準検証 | 2002 以降の機能を検証しない |
| NIST audit | `make nist-audit MODULE=NC` | HIR/C 生成物の監査 | `PASS` 判定ではない |
| GnuCOBOL comparison | `make nist-compare MODULE=NC` | 生成物差分や移植性の補助確認 | GnuCOBOL の挙動が標準そのものとは限らない |

## 仕様領域別マトリクス

| 仕様領域 | 主な検証資産 | 現在の判定 | 未検証または不足している点 |
| --- | --- | --- | --- |
| Division と基本プログラム構造 | parser unit、HIR unit、E2E `core_pipeline` | Verified | 複数 compilation group の網羅は限定的 |
| 固定形式ソース | lexer unit、preprocess/lex、NIST | Partially verified | ベンダー差のある境界条件 |
| 自由形式ソース | lexer unit、parser unit、E2E | Partially verified | COBOL 2002 以降の細部制約 |
| `COPY` / `REPLACE` | preprocessor unit、preprocess/lex、NIST `SM` | Partially verified | 複雑な pseudo-text、copy library 差分 |
| Data Division と `PICTURE` | sema unit、runtime decimal tests、E2E | Partially verified | すべての edited picture と locale 差分 |
| 算術 statement | runtime decimal tests、E2E、NIST `NC` | Partially verified | 例外条件、丸め、overflow の完全性 |
| 条件式と制御構造 | parser unit、E2E、NIST `NC` | Partially verified | 省略条件や collating sequence の網羅 |
| `PERFORM` / `GO TO` | E2E、NIST `NC` / `SG` | Partially verified | segmentation は製品レベル未達 |
| 文字列操作 | runtime string tests、E2E、NIST `NC` | Partially verified | national data との組み合わせ |
| `CALL` / linkage | E2E、NIST `IC` | Partially verified | 再帰、動的 binding、他言語連携 |
| Sequential file I/O | runtime file tests、E2E、NIST `SQ` | Partially verified | OS 差、lock、障害復旧 |
| Indexed file I/O | runtime file tests、E2E、NIST `IX` | Partially verified | 永続 index、複数 process、alternate key の網羅 |
| Relative file I/O | runtime file tests、E2E、NIST `RL` | Partially verified | ランダム/順次混在経路の網羅 |
| `SORT` / `MERGE` | runtime sort tests、E2E、NIST `ST` | Partially verified | input/output procedure の全形式 |
| Report Writer | E2E、NIST `RW` | Partially verified | 完全な帳票 layout と page 制御 |
| Debugging | E2E、NIST `DB` | Partially verified | full debug module conformance |
| Communication | runtime tests、E2E、NIST `CM` | Partially verified | 実運用メッセージングではなく限定 runtime |
| Intrinsic functions | runtime intrinsic tests、E2E、NIST `IF` | Partially verified | 関数一覧の完全網羅 |
| Object-oriented COBOL | parser/HIR/codegen unit、E2E | Partially verified | ISO/IEC 1989:2002 以降の class/interface 全体 |
| XML | runtime XML tests、E2E | Partially verified | 標準本文ベースの完全制約 |
| JSON | runtime JSON tests、E2E | Partially verified | COBOL 2014/2023 差分の完全制約 |
| `VALIDATE` | sema tests、E2E、runtime validation tests | Partially verified | constraint 全体、diagnostic、nested condition |
| COBOL 2023 新規機能 | parser/sema/codegen の一部 | Unverified | ISO/IEC 1989:2023 本文に基づく網羅テストがない |

## NIST CCVS85 の判定規則

NIST CCVS85 を使う場合は、`PASS`、`FAIL`、`COMPILE_ERROR`、
`RUNTIME_ERROR`、`TIMEOUT`、`INSPECT` を明確に区別する。

- `PASS`: CCVS の成功出力を機械的に確認できた状態。
- `FAIL`: CCVS の失敗出力を確認した状態。
- `COMPILE_ERROR`: コンパイルできなかった状態。
- `RUNTIME_ERROR`: 実行時エラーまたは非ゼロ終了。
- `TIMEOUT`: 実行時間超過。
- `INSPECT`: 実行は終わったが、自動判定では成功とも失敗ともいえない状態。

`INSPECT` は未確定であり、成功扱いにしない。

## 標準版ごとの扱い

### COBOL-85

COBOL-85 系は NIST CCVS85 を中心に検証する。最低限、関連 module ごとに
`make nist-run MODULE=<MODULE>` を実行し、`PASS`、`FAIL`、`INSPECT` の
内訳を記録する。

主要 module は次の通り。

| Module | 対象 |
| --- | --- |
| `NC` | Nucleus |
| `SM` | Source Manipulation |
| `IC` | Inter-program Communication |
| `SQ` | Sequential I/O |
| `IF` | Intrinsic Functions |
| `IX` | Indexed I/O |
| `RL` | Relative I/O |
| `ST` | SORT/MERGE |
| `RW` | Report Writer |
| `DB` | Debugging |
| `SG` | Segmentation |
| `OB` | Obsolete Features |

### COBOL 2002 以降

COBOL 2002 以降の機能は NIST CCVS85 では検証できない。
各機能ごとに、仕様根拠、正例、負例、標準モード別の許可/拒否、実行時意味を
テストに分けて追加する。

必要なテスト分類は次の通り。

- parser accepts/rejects tests
- semantic standard-mode tests
- HIR lowering tests
- codegen snapshot or structural tests
- native execution E2E tests
- runtime unit tests
- diagnostic tests

## 完了条件

機能を `Verified` に上げる条件は次の通り。

- 標準または処理系定義の根拠が文書化されている
- 正例と負例がある
- 標準モードごとの許可/拒否が検証されている
- 実行時意味がある機能は native E2E または runtime unit test がある
- NIST 対象領域の場合は該当 module の結果が記録されている
- 未対応範囲が `docs/cobol-standards.md` または関連文書に残っている

## 直近の不足

- ISO/IEC 1989:2023 本文に基づく COBOL 2023 機能の網羅テストがない。
- COBOL 2002 object-oriented 機能は代表経路の検証に留まる。
- `VALIDATE`、XML、JSON は実装テストがあるが、標準本文の全制約との
  トレーサビリティが不足している。
- NIST module の最新通過率が本文書に固定記録されていない。
- 日立 COBOL2002 資料は補助資料であり、標準準拠の oracle にはできない。

## 運用ルール

- 仕様調査を追加しただけでは `Verified` にしない。
- ベンダー資料から得た挙動は `extension` または `compatibility target` として扱う。
- GnuCOBOL と一致しただけでは標準準拠とは呼ばない。
- NIST の `INSPECT` は未確定として扱う。
- 実装変更時は、直接影響するテストを先に実行し、その後に affected crate または
  workspace の検証へ進む。
