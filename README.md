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

On a local Apple Silicon macOS environment, `rust-cobol` showed a
clear throughput advantage on the included `N-Queens` benchmark:
`704.5 ms` mean runtime versus `20.806 s` for `GnuCOBOL`
(`hyperfine`, 3 runs), or about `29.5x` faster.

The smaller microbenchmarks in `tests/benchmark` were much closer:
arithmetic was about `1.32x` slower than `GnuCOBOL`, string
operations about `1.12x` slower, and file I/O about `1.09x`
faster. In practice, the current runtime looks strongest on
CPU-bound loop-heavy workloads while remaining broadly competitive
on simpler tasks.

## Requirements

- Rust 1.75+ (stable toolchain)
- clang or gcc (for C compilation and linking)

## Quick Start

```bash
# Build the compiler
make build

# Compile and run the example
make example
```

This compiles `examples/hello.cob` and runs the resulting binary.

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

# Compile a COBOL program
cobolc myprogram.cob -o myprogram --source-format free
```

## Build Commands

| Command | Description |
| --- | --- |
| `make build` | Debug build |
| `make release` | Release build (default) |
| `make test` | All tests (unit + E2E) |
| `make test-unit` | Unit tests only |
| `make test-e2e` | E2E tests only |
| `make lint` | clippy + rustfmt check |
| `make fmt` | Apply rustfmt |
| `make check` | Type-check without codegen |
| `make install` | Install cobolc to ~/.cargo/bin |

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
  VALIDATE
- **COBOL 2023** - UTF-8, JSON, XML, threading extensions

## License

MIT OR Apache-2.0
