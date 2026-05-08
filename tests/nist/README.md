# GnuCOBOL-Style NIST Environment

This directory provides the primary NIST CCVS 85 execution environment
for `rust-cobol`.

It keeps generated COBOL sources and test results under `target/nist`,
not in this directory. Runtime files from `XXXXX`/`XXXX*` NIST
placeholders use short paths under `/tmp` so fixed-format COBOL source
lines are not truncated.

## Goal

This environment applies GnuCOBOL-style CCVS judgment rules to
`rust-cobol`. The primary decision source is the CCVS report summary:
summary counts first, report layout second.

## Files

- `extract.pl` — extracts the NIST programs from `newcob.val`
- `prepare.sh` — extracts a dedicated `programs/` tree under `target/`
- `preprocess.sh` — applies the NIST placeholder replacements with an
  isolated temporary root
- `run_nist.sh` — compiles, runs, and classifies programs with
  GnuCOBOL-style CCVS judgment rules
- `run_compare.sh` — optional audit tool for direct comparison with
  GnuCOBOL

## Setup

```bash
make nist-prepare
```

By default this extracts `tests/nist/newcob.val.tar.gz` into
`target/nist/source/newcob.val`, then extracts programs into
`target/nist/programs`.

## Usage

```bash
make nist-prepare
make nist-compile
make nist-compile-errors
make nist-run
make nist-run MODULE=NC
make nist-run MODULE=NC PROGRAM=NC101A
make nist-summary
```

The same `make nist-prepare`, `make nist-compile`, and `make nist-run`
flow is intended for both local execution and CI jobs.

`make nist-run` executes each NIST module as its own sequential run and
prints the full cross-module summary after all modules finish. A module
run uses one active process at a time so programs from the same module
do not execute in parallel.

`make nist-compile` runs only the compile phase and then collects the
module summaries. This is the primary gate for detecting NIST
`COMPILE_ERROR` regressions before the full execution phase.

`make nist-compile-errors` groups the current `COMPILE_ERROR` programs by
root-cause-like compiler message classes. Use it after `make nist-compile`
to decide which failures should become shared Rust regression tests.

`NIST_JOBS` is still used by compile-only and audit-oriented workflows.
The primary `make nist-run` path intentionally ignores parallelism for
module runs so local behavior matches CI module jobs.

Compilation is cached by default per program. If the preprocessed source,
compiler binary, and `COPYLIB` inputs are unchanged, `run_nist.sh` reuses
the existing executable. Set `NIST_COMPILE_CACHE=0` to force recompiling.
The preprocessed COBOL source and fixture scan results are also reused
when the source file and `preprocess.sh` are unchanged.

## Result Model

- `PASS` — the program-specific verifier and CCVS report both satisfy
  the expected result
- `FAIL` — compilation succeeded, but the verifier found a mismatch,
  missing report, or reported failures
- `COMPILE_ERROR` — compilation failed
- `RUNTIME_ERROR` — execution returned non-zero
- `TIMEOUT` — execution exceeded the timeout

`FAIL` stores an additional `*.reason` file when the verifier can attach
a more specific machine-readable cause.

## Output Location

- extracted programs: `target/nist/programs`
- extracted source archive contents: `target/nist/source`
- runtime temporary files: `/tmp/nc85`, `/tmp/nist`, or `/tmp/na`
- run results: `target/nist/results`
- compare results: `target/nist/results-compare`
