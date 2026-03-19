# rust-cobol User Guide

## Overview

rust-cobol (`cobolc`) is a COBOL compiler written in Rust that compiles COBOL source
programs to native binaries via C code generation.

Pipeline:

```
COBOL Source → Lexer → Parser → Sema → HIR → C codegen → clang → Native Binary
```

## Installation

```bash
# Build from source
git clone https://github.com/izuno4t/rust-cobol.git
cd rust-cobol
make install
```

This installs `cobolc` to `~/.cargo/bin/cobolc`.

### Prerequisites

- Rust 1.75+ (stable toolchain)
- clang (C compiler for code generation)

## Usage

### Basic compilation

```bash
cobolc program.cob -o program
./program
```

### Options

| Option | Description |
|---|---|
| `-o <path>` | Output binary path |
| `--source-format <fmt>` | Source format: `fixed` (default) or `free` |
| `--standard <std>` | COBOL standard: `cobol85`, `cobol2002`, `cobol2014`, `cobol2023` |
| `--emit-c` | Emit generated C code only (do not compile) |
| `-W <level>` | Warning level: `all`, `none`, `error` |
| `-I <dir>` | COPYBOOK search path |

### Examples

```bash
# Fixed-format source (default)
cobolc payroll.cob -o payroll

# Free-format source
cobolc modern.cob -o modern --source-format free

# Emit C code for inspection
cobolc program.cob --emit-c -o program.c

# Treat warnings as errors
cobolc program.cob -o program -W error
```

## Supported COBOL Standards

### COBOL-85 (Primary target)

Core language features with high coverage:

| Feature | Status |
|---|---|
| IDENTIFICATION DIVISION | Supported |
| ENVIRONMENT DIVISION | Supported |
| DATA DIVISION | Supported |
| PROCEDURE DIVISION | Supported |
| Numeric data (PIC 9) | Supported |
| Alphanumeric data (PIC X) | Supported |
| Numeric edited (PIC Z, *, etc.) | Supported |
| Alphanumeric edited | Supported |
| MOVE, COMPUTE, ADD, SUBTRACT, MULTIPLY, DIVIDE | Supported |
| IF/ELSE/END-IF | Supported |
| EVALUATE/WHEN | Supported |
| PERFORM (inline, out-of-line, THRU, VARYING) | Supported |
| GO TO | Supported |
| CALL/CANCEL | Supported |
| STRING/UNSTRING/INSPECT | Supported |
| Sequential file I/O | Supported |
| Indexed file I/O | Supported |
| Relative file I/O | Supported |
| SORT/MERGE | Supported |
| COPY | Supported |
| DECLARATIVES | Supported |
| CORRESPONDING | Supported |

### COBOL 2002 Extensions

| Feature | Status |
|---|---|
| Intrinsic functions (40+) | Supported |
| EVALUATE ALSO | Supported |
| Reference modification | Supported |
| Inline PERFORM | Supported |
| CLASS-ID / METHOD-ID | Partial |
| INTERFACE-ID | Partial |
| FUNCTION-ID | Supported |
| TYPEDEF | Supported |
| NATIONAL data type (PIC N) | Supported |

### COBOL 2014/2023 Extensions

| Feature | Status |
|---|---|
| JSON GENERATE / JSON PARSE | Parsed (runtime stub) |
| XML GENERATE / XML PARSE | Parsed (runtime stub) |
| VALIDATE | Parsed (runtime stub) |
| RAISE / RESUME | Supported |
| ALLOCATE / FREE | Supported |

## Intrinsic Functions

### Mathematical

ABS, SQRT, SIN, COS, TAN, ASIN, ACOS, ATAN, EXP, EXP10, LOG, LOG10,
CEILING, FLOOR, FACTORIAL, REM, RANDOM, SIGN, MEAN, SUM, ANNUITY,
PRESENT-VALUE, MOD, INTEGER, INTEGER-PART, FRACTION-PART

### String

LENGTH, UPPER-CASE, LOWER-CASE, REVERSE, TRIM, CONCATENATE, SUBSTITUTE,
ORD, CHAR, ORD-MAX, ORD-MIN, STORED-CHAR-LENGTH, NATIONAL-OF, DISPLAY-OF

### Date/Time

CURRENT-DATE, INTEGER-OF-DATE, DATE-OF-INTEGER, INTEGER-OF-DAY,
DAY-OF-INTEGER, DATE-TO-YYYYMMDD, YEAR-TO-YYYY, DAY-TO-YYYYDDD,
TEST-DATE-YYYYMMDD, TEST-DAY-YYYYDDD, WHEN-COMPILED

### Numeric

NUMVAL, NUMVAL-C, MAX, MIN

## Source Formats

### Fixed format (default)

Standard COBOL-85 column layout:

- Columns 1-6: Sequence number area (ignored)
- Column 7: Indicator area (`*` = comment, `-` = continuation, `D` = debug)
- Columns 8-11: Area A (division/section/paragraph headers, level numbers)
- Columns 12-72: Area B (statements)
- Columns 73-80: Identification area (ignored)

### Free format

Modern format with no column restrictions:

- `*>` starts a comment
- No sequence number or identification areas
- Statements can start at any column

## File I/O

### Supported organizations

| Organization | Access Modes |
|---|---|
| SEQUENTIAL | Sequential |
| INDEXED | Sequential, Random, Dynamic |
| RELATIVE | Sequential, Random, Dynamic |

### File status codes

| Code | Meaning |
|---|---|
| 00 | Successful completion |
| 10 | End of file (AT END) |
| 21 | Sequence error |
| 22 | Duplicate key |
| 23 | Record not found |
| 30 | Permanent I/O error |
| 35 | File not found (OPEN) |
| 41 | File already open |
| 42 | File not open |
| 46 | No valid next record |
| 47 | READ on file not opened for input |
| 48 | WRITE on file not opened for output |

## Runtime Library

The compiler links with a Rust-based runtime library (`libcobolrt`) that provides:

- BCD (packed decimal) arithmetic
- File I/O operations
- String manipulation
- Intrinsic function implementations
- Exception handling (setjmp/longjmp based)
- SORT/MERGE operations
- Screen handling (ANSI escape sequences)

## Known Limitations

1. **COMMUNICATION SECTION** — Obsolete in COBOL 2002, not implemented
2. **REPORT SECTION** — INITIATE/GENERATE/TERMINATE parsed but runtime is stub only
3. **JSON/XML runtime** — Parsed but runtime operations are stubs
4. **Nested reference modification** — `WS-FIELD(1:3)(1:2)` not supported
5. **SCREEN SECTION** — Basic ANSI positioning only, no ncurses support
6. **OOP features** — CLASS-ID/INTERFACE-ID codegen exists but limited E2E coverage
7. **ADD/SUBTRACT with literal first** — Use `ADD WS-A TO WS-B` not `ADD 1 TO WS-B`
   (workaround: use COMPUTE)
8. **Multi-program compilation** — Each COBOL program compiled separately
9. **ACCEPT FROM CONSOLE** — Terminal raw mode not fully implemented

## Diagnostics

Error messages include source location with line highlighting:

```
[COBC-E001] Error: unexpected token: Equals
    ╭─[program.cob:10:20]
    │
 10 │ COMPUTE WS-RESULT = WS-A + WS-B
    ·                    ^
────╯
```

Use `-W all` to see all diagnostics including informational messages.
Use `-W error` to treat warnings as errors.

## Performance

The generated native binaries run at near-C speed since the compiler generates
optimized C code compiled by clang. Typical performance characteristics:

- Arithmetic operations: comparable to C
- String operations: overhead from runtime library calls
- File I/O: overhead from runtime abstraction layer

## Building from Source

```bash
make build       # Debug build
make release     # Release build (optimized)
make test        # Run all tests
make lint        # Run clippy + format check + spellcheck
make example     # Compile and run examples/hello.cob
```
