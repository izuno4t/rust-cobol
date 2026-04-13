# GnuCOBOL-Style NIST Environment

This directory provides the primary NIST CCVS 85 execution environment
for `rust-cobol`.

It keeps generated COBOL sources and test results under `.nist`,
not in this directory.

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

By default this reads `tests/nist/newcob.val` and extracts programs into
`.nist/programs`.

## Usage

```bash
make nist-prepare
make nist-compile
make nist-compile-errors
make nist-run
make nist-run NIST_JOBS=4
make nist-run MODULE=NC
make nist-run MODULE=NC PROGRAM=NC101A
make nist-summary
```

The same `make nist-prepare`, `make nist-compile`, and `make nist-run` flow is intended for
both local execution and CI jobs.

`make nist-compile` runs only the compile phase and then collects the
module summaries. This is the primary gate for detecting NIST
`COMPILE_ERROR` regressions before the full execution phase.

`make nist-compile-errors` groups the current `COMPILE_ERROR` programs by
root-cause-like compiler message classes. Use it after `make nist-compile`
to decide which failures should become shared Rust regression tests.

`NIST_JOBS` is optional and controls the global parallelism for all
three phases: compile, execute, and collect. Each program runs in its
own isolated work directory.

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

- extracted programs: `.nist/programs`
- run results: `.nist/results`
- compare results: `.nist/results-compare`
