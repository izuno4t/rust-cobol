# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust workspace for a COBOL compiler. Core crates
live under `crates/`: `cobol-lexer`, `cobol-parser`, `cobol-sema`,
`cobol-hir`, `cobol-codegen`, `cobol-runtime`, and the CLI entry point
`cobol-driver`. Keep implementation code in each crate's `src/`
directory. End-to-end tests live in `crates/cobol-driver/tests/`,
broader regression assets live in `tests/`, example programs live in
`examples/`, and design notes or progress docs belong in `docs/`.

## Build, Test, and Development Commands

Use the `Makefile` as the primary entry point:

- `make build`: debug build for local development.
- `make release`: optimized build; also the default `make` target.
- `make test`: runs `cargo test --workspace`.
- `make test-e2e`: runs the CLI integration suite in `cobol-driver`.
- `make lint`: runs `clippy`, `rustfmt --check`, and `cspell`.
- `make fmt`: formats all Rust code.
- `make check`: fast workspace type-check without full codegen.
- `make example`: builds the compiler and runs `examples/hello.cob`.
- `make nist-prepare`: extracts NIST CCVS assets from
  `tests/nist/newcob.val` into `target/nist/programs`.
- `make nist-run`: runs the primary GnuCOBOL-style NIST suite; pass
  `MODULE=NC` or `PROGRAM=NC101A` to narrow scope.
- `make nist-summary`: prints the latest aggregated NIST results from
  `target/nist/results`.

## Coding Style & Naming Conventions

Follow Rust 2021 idioms and keep formatting compatible with
`rustfmt.toml` (`max_width = 100`). Use 4-space indentation,
`snake_case` for functions/modules, `PascalCase` for types, and
`SCREAMING_SNAKE_CASE` for constants. Prefer small crates with explicit
responsibilities over cross-cutting utility code. Treat
`cargo clippy --workspace --all-targets -- -D warnings` as the lint
baseline.

## Testing Guidelines

Add unit tests near the crate they validate, and put pipeline or CLI
scenarios in `crates/cobol-driver/tests/e2e_test.rs` or adjacent
integration files named `*_test.rs`. Use focused test names such as
`test_perform_varying` that describe the COBOL feature under test. Run
`make test` before opening a PR; when touching standards-compliance
behavior, run `make nist-prepare` once and then `make nist-run
MODULE=NC` or a narrower module/program target. NIST artifacts are
generated under `target/nist`, not under `tests/nist`.

## Work Process

After any implementation change or test expectation change, run the
directly affected test target immediately, then run the full test suite
for the affected crate before reporting completion unless that is
impossible.
After applying a fix, always run `make clean test lint` before reporting
completion.

## Reporting Discipline

Keep progress updates and final reports short, factual, and directly
relevant to the user's requested completion criteria.

- Do not pad reports with self-justification, excuses, or progress that
  does not materially advance the requested goal.
- Do not present partial symptom movement, such as `COMPILE_ERROR ->
  FAIL`, as meaningful completion when the user asked for full
  completion.
- When work is incomplete, state the exact unmet condition plainly and
  continue from the highest-leverage remaining blocker.

## GitHub Actions Discipline

For Linux-based GitHub Actions jobs, inspect the actual job result on
GitHub before concluding CI status. Use the GitHub Actions UI or `gh`
to review the job conclusion, step logs, and uploaded artifacts for the
relevant Linux run.

- Treat GitHub-hosted Linux job logs and artifacts as the source of
  truth when reporting CI failures or recoveries.
- When a workflow uploads diagnostic artifacts, review those artifacts
  in addition to the live job log before proposing or declaring a fix.

## Commit & Pull Request Guidelines

Recent history favors short, imperative subjects with conventional
prefixes such as `fix:`, `feat:`, `docs:`, `refactor:`, `test:`, and
`revert:`. Keep each commit scoped to one logical change. PRs should
describe affected compiler stages, list verification steps run
(for example `make lint`, `make test`), link related issues, and include
sample COBOL input/output when behavior changes are user-visible.
