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
tests/nist/prepare.sh
```

By default this reads `tests/nist/newcob.val` and extracts programs into
`target/nist/programs`.

## Usage

```bash
bash tests/nist/run_nist.sh NC
bash tests/nist/run_nist.sh NC NC101A
bash tests/nist/run_nist.sh --all
bash tests/nist/run_nist.sh --summary
```

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
