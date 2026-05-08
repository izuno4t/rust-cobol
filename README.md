# rust-cobol

[![Build](https://github.com/izuno4t/rust-cobol/actions/workflows/rust.yml/badge.svg)](https://github.com/izuno4t/rust-cobol/actions/workflows/rust.yml)

A COBOL compiler written in Rust, targeting COBOL-85 through
COBOL 2023. It compiles COBOL source code to native binaries
via C code generation.

## Overview

```text
Source → Lexer → Parser → Sema → HIR → C codegen → clang → native binary
```

The compiler reads COBOL source files in Fixed or Free format,
parses them into an AST, performs semantic analysis (name
resolution, type checking, PICTURE analysis), lowers to HIR,
generates C code, and invokes clang/gcc to produce a native
executable.

## Runtime Characteristics and Benchmarks

`rust-cobol` showed a clear throughput advantage on the included
`N-Queens` benchmark:
`704.5 ms` mean runtime versus `20.806 s` for `GnuCOBOL`
(`hyperfine`, 3 runs), or about `29.5x` faster.

The smaller microbenchmarks in `tests/benchmark` were much closer:
arithmetic was about `1.32x` slower than `GnuCOBOL`, string
operations about `1.12x` slower, and file I/O about `1.09x`
faster. In practice, the current runtime looks strongest on
CPU-bound loop-heavy workloads while remaining broadly competitive
on simpler tasks.

Measurement environment: macOS 26.3.1 on Apple Silicon, Apple clang
17.0.0, cargo 1.93.1.

## Requirements

- Rust 1.75+ (stable toolchain)
- clang or gcc (for C compilation and linking)

## Quick Start

Typical user flow:

1. Build the compiler.
2. Compile a COBOL source file into a native executable.
3. Run the generated executable.

Try the bundled `examples/hello.cob` program:

```bash
# 1. Build the compiler
make build

# 2. Compile the COBOL source into a native executable
./target/debug/cobol-driver examples/hello.cob -o /tmp/hello --source-format free

# 3. Run the generated executable
/tmp/hello
```

`examples/hello.cob` is the input COBOL program. The `-o` path is the
output executable path. After compilation finishes, run that generated
file directly. For example, if you compile with `-o ./hello`, run it
with `./hello`.

If you just want to try the bundled example end-to-end, `make example`
performs the same compile step and then runs the generated binary for you.

```cobol
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY "Hello, World!".
    STOP RUN.
```

## Installation

```bash
# Install cobolc to ~/.cargo/bin
make install

# Compile a COBOL program into a native executable
cobolc myprogram.cob -o myprogram --source-format free

# Run the generated executable
./myprogram
```

## Build Commands

| Command | Description |
| --- | --- |
| `make build` | Debug build |
| `make release` | Release build (default) |
| `make test` | Run the full workspace test suite |
| `make lint` | Run clippy, rustfmt check, and cspell |
| `make clean` | Remove build artifacts |
| `make nist` | Run NIST CCVS 85; pass `MODULE` or `PROGRAM` to narrow scope |
| `make benchmark` | Run the bundled local benchmarks |
| `make install` | Install cobolc to ~/.cargo/bin |

NIST commands:

Use `MODULE` and `PROGRAM` only with commands that actually run or inspect a
selected module/program.

| Command | Description |
| --- | --- |
| `make nist [MODULE=NC] [PROGRAM=NC101A]` | Individual or scoped run: compile, execute, judge, and summarize |
| `make nist-compile [MODULE=NC] [PROGRAM=NC101A]` | Individual or scoped compile-only check |
| `make nist-audit [MODULE=NC] [PROGRAM=NC101A]` | Individual or scoped generated-code audit |
| `make nist-compare [MODULE=NC] [PROGRAM=NC101A]` | Individual or scoped symbol comparison after audit |

Whole-environment setup:

| Command | Description |
| --- | --- |
| `make nist-prepare` | Extract CCVS inputs into `target/nist/programs`; do not pass `MODULE` or `PROGRAM` |

NIST result inspection is not exposed as Make targets. Use the underlying
scripts directly when needed:

```bash
NIST_ENV_ROOT=target/nist bash tests/nist/bin/run.sh --summary
python3 tests/nist/bin/classify-compile-errors.py target/nist
```

x86 runtime actions:

| Command | Description |
| --- | --- |
| `make runtime-x86 RUNTIME_X86_ACTION=build` | Build the x86_64 Linux Docker image |
| `make runtime-x86 RUNTIME_X86_ACTION=shell` | Open a shell inside the x86_64 Linux container |
| `make runtime-x86 MODULE=NC` | Run NIST inside the x86_64 Linux container |
| `make runtime-x86 RUNTIME_X86_ACTION=bench` | Run benchmarks inside the x86_64 Linux container |

## Crate Structure

The compiler is organized as a Cargo workspace with 12 crates:

| Crate | Role |
| --- | --- |
| `cobol-common` | Span, FileId, SourceMap, CobolStandard |
| `cobol-diagnostics` | Diagnostic reporting and accumulation |
| `cobol-lexer` | Source reader and tokenizer |
| `cobol-preprocessor` | COPY and REPLACE directives |
| `cobol-ast` | Untyped AST nodes |
| `cobol-parser` | Recursive-descent parser |
| `cobol-sema` | Name resolution, type checking, PICTURE |
| `cobol-hir` | Desugared intermediate representation |
| `cobol-mir` | Mid-level IR (placeholder) |
| `cobol-codegen` | C emission and clang/gcc invocation |
| `cobol-runtime` | Staticlib: BCD, file I/O, string ops |
| `cobol-driver` | CLI (`cobolc`) for the full pipeline |

## Supported Standards

- **COBOL-85** - Core language features
- **COBOL 2002** - OOP, user-defined functions, exceptions,
  LOCAL-STORAGE
- **COBOL 2014** - FLOAT-BINARY/DECIMAL/EXTENDED, TYPEDEF,
  VALIDATE (partial runtime validation)
- **COBOL 2023** - UTF-8, JSON, XML, threading extensions

This list describes standards areas represented in the compiler, not a claim
that every feature in each standard is production-complete. See
[docs/cobol-standards.md](docs/cobol-standards.md) and
[docs/production-gaps.md](docs/production-gaps.md) for feature-level status.
Use `--standard cobol85`, `--standard cobol2002`, `--standard cobol2014`, or
`--standard cobol2023` to select a standard mode.

## License

MIT OR Apache-2.0
