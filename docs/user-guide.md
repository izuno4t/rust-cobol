# rust-cobol User Guide

## Overview

`rust-cobol` provides the `cobolc` command, a Rust-based COBOL compiler that
parses COBOL source, lowers it through semantic and HIR stages, generates C,
and then invokes a native C toolchain to produce an executable.

```text
COBOL source
  -> preprocess
  -> lexer
  -> parser
  -> semantic analysis
  -> HIR
  -> C
  -> native binary
```

The current compiler is strongest on COBOL-85 style programs, while also
supporting selected features from COBOL 2002, COBOL 2014, and COBOL 2023.

## Installation

### Prerequisites

- Rust 1.75+ (stable)
- `clang` or `gcc`

### Build and install from source

```bash
git clone https://github.com/izuno4t/rust-cobol.git
cd rust-cobol
make install
```

This installs `cobolc` into `~/.cargo/bin/cobolc`.

## Quick start

```bash
make build
cobolc examples/hello.cob -o hello --source-format free
./hello
```

You can also use the project shortcut:

```bash
make example
```

## Command-line interface

Current `cobolc --help` output is summarized below.

| Option | Description |
| --- | --- |
| `<FILES>...` | One or more COBOL source files |
| `-o, --output <OUTPUT>` | Output executable path |
| `--source-format <fmt>` | Source format: `fixed`, `free`, or `variable` |
| `--dump-tokens` | Print lexer output |
| `--dump-ast` | Print parsed AST |
| `--dump-hir` | Print lowered HIR |
| `--emit-c` | Print generated C |
| `--c-only` | Stop after generating C and write the C file |
| `-I, --copy-path <DIR>` | Add a `COPY` search path |
| `-W, --warning <level>` | Warning control: `default`, `all`, `none`, `error` |
| `-v, --verbose` | Verbose build output |

### Common examples

```bash
# Compile free-format COBOL (current default)
cobolc app.cob -o app

# Compile fixed-format COBOL
cobolc legacy.cob -o legacy --source-format fixed

# Compile variable-format COBOL
cobolc variable.cob -o variable --source-format variable

# Add COPY search paths
cobolc main.cob -I copybooks -I vendor/copybooks -o main

# Keep generated C instead of compiling it
cobolc main.cob --emit-c --c-only -o main

# Show all diagnostics
cobolc main.cob -W all -o main
```

## Compilation behavior

- `cobolc` preprocesses `COPY` and `REPLACE` before lexing and parsing
- Native binaries are produced by compiling generated C with the workspace
  runtime library
- If native compilation fails, the generated C file is left on disk for
  inspection
- `COBOL_RUNTIME_LIB` can be used to point the driver at a custom runtime
  library directory

## Source formats

### Free format

This is the current CLI default.

- No fixed column rules
- `*>` starts an inline comment
- Best fit for new source files and most tests in this repository

### Fixed format

Classic COBOL column handling is supported.

- Columns 1-6: sequence area
- Column 7: indicator area
- Columns 8-11: Area A
- Columns 12-72: Area B
- Columns 73-80: identification area

### Variable format

Variable-format input is also accepted by the driver and preprocessor. Use it
explicitly with `--source-format variable`.

## COPY and preprocessing

The preprocessor handles:

- `COPY`
- `REPLACE`
- `COPY ... REPLACING`

Use `-I/--copy-path` to add additional copybook search directories.

## Feature snapshot

The project is moving quickly, so this is intentionally high-level.

### Well-covered today

- Core COBOL-85 data descriptions and procedure logic
- Arithmetic, conditionals, `EVALUATE`, `PERFORM`, `GO TO`, and `CALL`
- Sequential, indexed, and relative file handling
- `GOBACK`, `FILE STATUS`, and many intrinsic functions
- `JSON GENERATE` / `JSON PARSE`
- `XML GENERATE` / `XML PARSE`
- `PIC N` / national data handling
- `RENAMES` lowering/code generation

### Available but still partial

- `SCREEN SECTION`
- `SORT ... INPUT PROCEDURE` / `OUTPUT PROCEDURE`
- Object-oriented syntax such as `CLASS-ID`, `METHOD-ID`, and `INTERFACE-ID`
- `FUNCTION-ID` and `TYPEDEF`
- `COMMUNICATION SECTION`

### Present but not production-complete

- `VALIDATE` currently lowers and generates runtime calls, but the runtime
  implementation is still a no-op
- Report writer statements such as `INITIATE`, `GENERATE`, and `TERMINATE`
  are parsed/code-generated as placeholders rather than full runtime behavior

See [docs/cobol-standards.md](./cobol-standards.md) for a standards-oriented
view and [docs/production-gaps.md](./production-gaps.md) for the remaining
known gaps.

## Diagnostics and debugging

The compiler reports source-annotated diagnostics with line/column context.

```text
[COBC-E001] Error: unexpected token
```

Useful debugging switches:

- `--dump-tokens`
- `--dump-ast`
- `--dump-hir`
- `--emit-c`
- `-v`

You can also enable timing output with:

```bash
COBOL_DEBUG_TIMING=1 cobolc app.cob -o app
```

## Build and test commands

Use the repository `Makefile` for common workflows.

```bash
make build
make release
make test
make test-e2e
make lint
make check
make example
```

For standards-compliance work, see the NIST helpers:

```bash
make nist-prepare
make nist-run MODULE=NC
make nist-summary
```

## Current limitations

- The CLI does not currently expose a `--standard` switch even though the code
  base contains internal COBOL standard enums
- Advanced later-standard features are not enforced by a strict standards mode
- Some advanced features are covered at parse/codegen level but still need more
  native end-to-end execution coverage
