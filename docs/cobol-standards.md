# Supported COBOL Standards

## Overview

`rust-cobol` is best understood as a COBOL-85-first compiler with selected
later-standard features layered on top. Support is not currently enforced by a
strict command-line standards mode, so this document describes practical
implementation status rather than a parser gate.

Status meanings used below:

- `Implemented` - available and used in normal compilation flows
- `Partial` - implemented only for part of the feature, or with limited runtime
- `Experimental` - available in code generation/runtime but not yet
  production-complete

## COBOL-85

### Core language

| Area | Status | Notes |
| --- | --- | --- |
| Divisions and basic program structure | Implemented | `IDENTIFICATION`, `ENVIRONMENT`, `DATA`, `PROCEDURE` |
| Data descriptions (levels 01-49, 66, 77, 88) | Implemented | Includes `RENAMES` support in lowering/codegen |
| PICTURE and numeric/alphanumeric storage | Implemented | Includes display numeric, edited forms, and binary forms |
| Core statements | Implemented | `MOVE`, `COMPUTE`, `ADD`, `SUBTRACT`, `MULTIPLY`, `DIVIDE`, `IF`, `EVALUATE`, `PERFORM`, `GO TO`, `DISPLAY`, `ACCEPT`, `SET`, `INITIALIZE` |
| String and inspection statements | Implemented | `STRING`, `UNSTRING`, `INSPECT` |
| Inter-program control flow | Implemented | `CALL`, `CANCEL`, `GOBACK`, `EXIT PROGRAM` |
| Reference modification | Implemented | Covered in parser/lowering/codegen |
| `CORRESPONDING` operations | Implemented | Supported in codegen |

### File handling

| Area | Status | Notes |
| --- | --- | --- |
| Sequential file I/O | Implemented | Includes `FILE STATUS` |
| Indexed file I/O | Implemented | Basic operations are present |
| Relative file I/O | Implemented | Basic operations are present |
| Declarative file exception lowering | Partial | `DECLARATIVES` / `USE AFTER EXCEPTION` lowering exists; more end-to-end coverage is still desirable |

### Sort/merge and report-related features

| Area | Status | Notes |
| --- | --- | --- |
| `SORT` / `MERGE` basic flow | Partial | Core support exists |
| `SORT ... INPUT PROCEDURE` / `OUTPUT PROCEDURE` | Partial | Not yet documented as production-complete |
| Report writer statements | Experimental | `INITIATE`, `GENERATE`, `TERMINATE` currently emit placeholders rather than full report runtime behavior |
| `REPORT SECTION` | Experimental | Parsed/lowered structure is not yet a full report writer implementation |

### Legacy and niche modules

| Area | Status | Notes |
| --- | --- | --- |
| `SCREEN SECTION` | Partial | ANSI-style display support exists; full screen/forms behavior is limited |
| `COMMUNICATION SECTION` | Partial | Parser/runtime pieces exist, but production readiness is still limited |
| Segmentation module | Experimental | Not a current focus |
| Debug module | Experimental | Not a current focus |

## COBOL 2002

| Feature area | Status | Notes |
| --- | --- | --- |
| Free-format source | Implemented | Also exposed directly by the CLI |
| Intrinsic functions | Partial | Many are implemented, but coverage is not yet exhaustive |
| National data (`PIC N`) | Implemented | Includes codegen/runtime support |
| Object-oriented syntax (`CLASS-ID`, `METHOD-ID`, `INVOKE`) | Partial | Front-end/codegen support exists, but end-to-end coverage is still limited |
| `INTERFACE-ID` | Partial | Present in the IR model, not yet documented as complete |
| `FUNCTION-ID` | Partial | Lowering/codegen support exists; more native execution coverage is needed |
| XML (`XML GENERATE` / `XML PARSE`) | Implemented | Parser, lowering, codegen, and runtime support exist |

## COBOL 2014

| Feature area | Status | Notes |
| --- | --- | --- |
| JSON (`JSON GENERATE` / `JSON PARSE`) | Implemented | Parser, lowering, codegen, and runtime support exist |
| `TYPEDEF` | Partial | Code generation is present, but usage coverage is still limited |
| `VALIDATE` | Partial | Performs generated PICTURE-derived storage validation for basic numeric cases; broader constraint coverage remains limited |
| `ALLOCATE` / `FREE` | Partial | Codegen support exists; not yet treated as fully production-ready |

## COBOL 2023

| Feature area | Status | Notes |
| --- | --- | --- |
| Internal standard enum and parser surface | Implemented | The code base contains `Cobol85`, `Cobol2002`, `Cobol2014`, and `Cobol2023` modes |
| Dedicated conformance mode | Partial | The CLI exposes `--standard`; semantic checks reject known later-standard features in older modes |
| 2023-only feature coverage | Experimental | Current support mainly builds on earlier implemented features |

## GnuCOBOL-style and practical extensions

| Feature area | Status | Notes |
| --- | --- | --- |
| Variable source format | Implemented | Exposed as `--source-format variable` |
| `COPY` preprocessing | Implemented | Includes extra search paths via `--copy-path` |
| `REPLACE` / `COPY ... REPLACING` | Partial | Supported in preprocessing, but still worth treating cautiously in edge cases |
| Native binary storage forms (`COMP`, `COMP-4`, `COMP-5`, `BINARY`) | Implemented | Lowered to binary integer storage in the current backend |

## Notes

- This document reflects the current repository state as of 2026-03-27
- For practical limitations that still block production use, see
  [docs/production-gaps.md](./production-gaps.md)
- For everyday usage and CLI examples, see [docs/user-guide.md](./user-guide.md)
