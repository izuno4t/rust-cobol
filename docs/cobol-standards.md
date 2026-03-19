# Supported COBOL Standards

## Overview

rust-cobol targets COBOL-85 as the primary standard, with extensions from
COBOL 2002, COBOL 2014, and COBOL 2023.

## COBOL-85 (ANSI X3.23-1985)

### Nucleus Module

| Feature | Support |
|---|---|
| Data descriptions (levels 01-49, 66, 77, 88) | Full |
| PICTURE clause (9, X, A, V, S, P) | Full |
| USAGE clause (DISPLAY, COMP, COMP-3, INDEX, POINTER) | Full |
| REDEFINES | Full |
| RENAMES (level 66) | Full |
| VALUE clause | Full |
| OCCURS clause (fixed, DEPENDING ON) | Full |
| MOVE statement | Full |
| COMPUTE statement | Full |
| ADD/SUBTRACT/MULTIPLY/DIVIDE | Full |
| IF/ELSE/END-IF | Full |
| EVALUATE/WHEN/WHEN OTHER | Full |
| PERFORM (inline, out-of-line, THRU, VARYING, UNTIL) | Full |
| GO TO / GO TO DEPENDING ON | Full |
| STOP RUN | Full |
| EXIT (PROGRAM, PARAGRAPH, SECTION) | Full |
| CONTINUE | Full |
| STRING/UNSTRING | Full |
| INSPECT (TALLYING, REPLACING, CONVERTING) | Full |
| ACCEPT/DISPLAY | Full |
| SET | Full |
| INITIALIZE | Full |
| CORRESPONDING (ADD, SUBTRACT, MOVE) | Full |
| ON SIZE ERROR / NOT ON SIZE ERROR | Full |
| Reference modification | Full |
| Figurative constants (SPACES, ZEROS, HIGH-VALUES, LOW-VALUES, ALL) | Full |

### Sequential I/O Module

| Feature | Support |
|---|---|
| OPEN (INPUT, OUTPUT, EXTEND, I-O) | Full |
| CLOSE | Full |
| READ (AT END, NOT AT END) | Full |
| WRITE (BEFORE/AFTER ADVANCING) | Full |
| REWRITE | Full |
| FILE STATUS | Full |

### Relative I/O Module

| Feature | Support |
|---|---|
| ORGANIZATION RELATIVE | Full |
| ACCESS MODE (SEQUENTIAL, RANDOM, DYNAMIC) | Full |
| READ (NEXT, key-based) | Full |
| WRITE/REWRITE/DELETE | Full |
| START | Full |

### Indexed I/O Module

| Feature | Support |
|---|---|
| ORGANIZATION INDEXED | Full |
| RECORD KEY / ALTERNATE RECORD KEY | Full |
| ACCESS MODE (SEQUENTIAL, RANDOM, DYNAMIC) | Full |
| READ (NEXT, key-based) | Full |
| WRITE/REWRITE/DELETE | Full |
| START (EQUAL, GREATER, NOT LESS) | Full |
| INVALID KEY / NOT INVALID KEY | Full |

### Inter-Program Communication Module

| Feature | Support |
|---|---|
| CALL (BY REFERENCE, BY CONTENT, BY VALUE) | Full |
| CANCEL | Full |
| GOBACK | Full |
| EXIT PROGRAM | Full |

### Sort-Merge Module

| Feature | Support |
|---|---|
| SORT (USING/GIVING) | Full |
| SORT (INPUT/OUTPUT PROCEDURE) | Partial |
| MERGE | Full |
| RELEASE | Partial |
| RETURN | Partial |
| ASCENDING/DESCENDING KEY | Full |

### Source Manipulation Module

| Feature | Support |
|---|---|
| COPY | Full |
| REPLACE | Partial |
| COPY REPLACING | Partial |

### Report Writer Module

| Feature | Support |
|---|---|
| REPORT SECTION | Parsed |
| INITIATE/GENERATE/TERMINATE | Parsed (stub runtime) |

### Segmentation Module

Not implemented (obsolete in modern COBOL).

### Debug Module

Not implemented (replaced by modern debugging tools).

## COBOL 2002 (ISO/IEC 1989:2002)

| Feature | Support |
|---|---|
| Intrinsic functions (40+) | Full |
| Object-oriented syntax (CLASS-ID, METHOD-ID) | Partial |
| INTERFACE-ID | Partial |
| FUNCTION-ID (user-defined functions) | Full |
| Free-format source | Full |
| TYPEDEF | Full |
| NATIONAL data type (PIC N) | Full |
| RAISE statement | Full |
| XML GENERATE / XML PARSE | Parsed |

## COBOL 2014 (ISO/IEC 1989:2014)

| Feature | Support |
|---|---|
| VALIDATE statement | Parsed |
| JSON GENERATE / JSON PARSE | Parsed |
| ALLOCATE / FREE | Full |

## COBOL 2023 (ISO/IEC 1989:2023)

Limited support. The compiler accepts `--standard cobol2023` but does not
implement 2023-specific features beyond those already in COBOL 2014.

## GnuCOBOL Extensions

| Feature | Support |
|---|---|
| SCREEN SECTION | Partial (ANSI escape) |
| COMP-5 (native binary) | Not implemented |
| Extended ACCEPT/DISPLAY | Partial |
