# NIST CCVS 85 Test Guide

COBOL-85 Compiler Validation System (CCVS 85) によるコンパイラ適合性検証。

## セットアップ

### 1. newcob.val の入手

GnuCOBOL SourceForge から `newcob.val` をダウンロード:

```bash
# GnuCOBOL SourceForge の nist ディレクトリから取得
curl -L -o newcob.val.Z \
  "https://sourceforge.net/projects/gnucobol/files/nist/newcob.val.Z/download"
uncompress newcob.val.Z
# または gzip 版
# gunzip newcob.val.gz
```

### 2. テストプログラムの抽出

```bash
perl tests/nist/lib/extract-programs.pl newcob.val programs/
```

抽出結果:

```text
programs/
  NC/    — Nucleus (核文法)
  SM/    — Source Manipulation (COPY文)
  IC/    — Inter-program Communication (CALL文)
  SQ/    — Sequential I/O
  IF/    — Intrinsic Functions
  IX/    — Indexed I/O
  RL/    — Relative I/O
  ST/    — SORT/MERGE
  RW/    — Report Writer
  DB/    — Debugging
  SG/    — Segmentation
  OB/    — Obsolete Features
```

### 3. テスト実行

```bash
# 単一モジュール実行
tests/nist/bin/run.sh NC

# 単一プログラム実行
tests/nist/bin/run.sh NC NC101A

# 全モジュール実行
tests/nist/bin/run.sh --all

# 結果サマリー表示
tests/nist/bin/run.sh --summary
```

各実行の終了時に、モジュール別の失敗一覧や単一プログラムの結果要約も標準出力へ表示される。

## 目標通過率

| モジュール | 目標 | 備考 |
| --- | --- | --- |
| NC (Nucleus) | 95%+ | 最優先。COBOL核文法の網羅テスト |
| IF (Intrinsic Functions) | 95%+ | 組み込み関数テスト |
| SQ (Sequential I/O) | 95%+ | 順編成ファイルI/O |
| IC (Inter-program Comm) | 90%+ | CALL文テスト |
| SM (Source Manipulation) | 80%+ | COPY文テスト |

参考: GnuCOBOL v2.2 は全体で 99.79% (9,688/9,708) を通過。

## 結果の読み方

`results/` ディレクトリに各プログラムの実行結果が保存される:

- `*.status` — PASS / FAIL / COMPILE_ERROR / RUNTIME_ERROR / TIMEOUT
- `*.log` — プログラムの標準出力
- `*.compile.log` — コンパイルエラー出力
- `*.reason` — `INSPECT` の分類理由
- `summary.txt` — モジュール別の集計

## Module Meanings

The extracted suite is organized by the original CCVS 85 module groups.

| Module | Meaning | Primary focus |
| --- | --- | --- |
| `NC` | Nucleus | Core COBOL syntax and semantics |
| `SM` | Source Manipulation | `COPY`, replacement, source handling |
| `IC` | Inter-program Communication | `CALL`, linkage, subprogram behavior |
| `SQ` | Sequential I/O | Sequential file handling |
| `IF` | Intrinsic Functions | COBOL intrinsic functions |
| `IX` | Indexed I/O | Indexed file access |
| `RL` | Relative I/O | Relative file access |
| `ST` | SORT/MERGE | Sort and merge statements |
| `RW` | Report Writer | Report section behavior |
| `DB` | Debugging | Debugging and monitoring features |
| `SG` | Segmentation | Segments and control flow |
| `OB` | Obsolete Features | Legacy COBOL features |

## Result Classification

`tests/nist/bin/run.sh` uses conservative result classes.
Successful process exit alone is never treated as `PASS`.

- `PASS`: Output contains `PASS` markers or a valid CCVS success footer.
- `FAIL`: Output contains `FAIL*` markers or a CCVS failure footer.
- `COMPILE_ERROR`: The program failed to compile.
- `RUNTIME_ERROR`: The compiled binary crashed or returned a non-zero
  exit status.
- `TIMEOUT`: Execution exceeded 60 seconds.
- `INSPECT`: The run finished, but the harness could not prove `PASS`
  or `FAIL` automatically.

This policy is intentional: product-quality validation must prefer
unknown over false positive.

## INSPECT Reason Codes

When a program ends as `INSPECT`, `tests/nist/bin/run.sh` writes a `*.reason`
file and prints the reason in the summary.

- `manual-report`: The source contains CCVS inspection/report patterns
  such as `INSPECT-COUNTER` or `MOVE "INSPT" TO P-OR-F`.
- `subprogram-only`: The source has `PROCEDURE DIVISION USING` and is
  likely meant to be called by another program.
- `dummy-display`: The source only prints a placeholder such as
  `DUMMY PROCEDURE` or `DUMMY PARAGRAPH`.
- `no-output`: The process ended without a usable report file or
  visible output.
- `unclassified`: No known pattern matched; manual investigation is
  still required.

These reasons are diagnostic metadata, not verdicts.
They help separate harness gaps from actual compiler/runtime bugs.

## Notes on CCVS Output

Many NIST programs do not report only through stdout.
The current harness checks both:

- stdout captured in `results/<MODULE>/<PROGRAM>.log`
- `/tmp/nc85`, which is the common CCVS print file used by generated programs

Before each run, `tests/nist/bin/run.sh` removes stale `/tmp/nc85`
to avoid cross-test contamination.

## GnuCOBOL-Style Environment

The primary NIST environment lives in
[`tests/nist`](../tests/nist/README.md).

That environment:

- extracts its own `programs/` tree under `target/`
- preprocesses sources with isolated temporary roots
- runs `rust-cobol` in isolation
- classifies results from CCVS summary counts and `FAIL*` lines
- keeps `INSPECT` only for unresolved or manual-review cases
- provides an optional comparison tool against GnuCOBOL when needed

Use it when the goal is not only local `PASS` or `FAIL` classification,
but a runner that follows the same style of NIST judgment as GnuCOBOL.

## 環境変数

- `COBOLC` — コンパイラのパス (デフォルト: `cargo run --release --package cobol-driver --`)
