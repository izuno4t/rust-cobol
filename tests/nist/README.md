# GnuCOBOL-Style NIST Environment

This directory provides the primary NIST CCVS 85 execution environment
for `rust-cobol`.

It keeps generated COBOL sources and test results under `target/nist`,
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
`target/nist/programs`.

## Usage

```bash
make nist-prepare
make nist-run
make nist-run NIST_JOBS=4
make nist-run MODULE=NC
make nist-run MODULE=NC PROGRAM=NC101A
make nist-summary
```

The same `make nist-prepare` and `make nist-run` flow is intended for
both local execution and CI jobs.

`NIST_JOBS` is optional and only affects `--all` execution. It runs
modules in parallel while keeping per-module work directories isolated.

Compilation is cached by default per program. If the preprocessed source,
compiler binary, and `COPYLIB` inputs are unchanged, `run_nist.sh` reuses
the existing executable. Set `NIST_COMPILE_CACHE=0` to force recompiling.
The preprocessed COBOL source and fixture scan results are also reused
when the source file and `preprocess.sh` are unchanged.

## Result Model

- `PASS` — CCVS summary says there are no failures and no inspections
- `FAIL` — CCVS summary or `FAIL*` lines report failures
- `INSPECT` — the run completed, but the report still requires
  inspection or has no decisive summary
- `COMPILE_ERROR` — compilation failed
- `RUNTIME_ERROR` — execution returned non-zero
- `TIMEOUT` — execution exceeded the timeout

`INSPECT` stores an additional `*.reason` file to separate manual-report
cases, subprogram-only cases, dummy outputs, and other unresolved runs.

## Output Location

- extracted programs: `target/nist/programs`
- run results: `target/nist/results`
- compare results: `target/nist/results-compare`
