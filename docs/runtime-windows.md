# Windows Runtime Environment

## Overview

This project can support Windows, but Windows support needs to be validated in
an actual Windows runtime environment rather than inferred from macOS builds.

The current recommended validation path is:

- develop normally on macOS or Linux
- validate x86 Linux runtime behavior with [runtime-x86.md](./runtime-x86.md)
- validate Windows compiler and runtime behavior in CI on `windows-latest`

## What is validated on Windows

The Windows smoke path should prove at least these points:

- the Rust crates build on Windows
- generated C can be compiled on Windows with a supported C toolchain
- the runtime library links correctly
- a compiled COBOL executable actually runs on Windows

## Current approach

The repository uses a GitHub Actions Windows job based on MSYS2 UCRT64 for:

- Rust toolchain access
- GCC/Clang-style C compilation
- `make`
- optional future additions such as `GnuCOBOL` and `hyperfine`

## Smoke test

The Windows smoke test compiles and runs:

- `tests/windows_smoke.cob`

via:

```bash
bash scripts/windows-smoke.sh
```

The expected output is:

```text
WINDOWS SMOKE OK
```

## Scope

This is intentionally a minimal runtime proof first.

Not yet covered in the Windows path:

- full NIST execution
- benchmark runs
- Unix-specific E2E tests that embed `/tmp` paths

Those can be expanded after the base Windows build and runtime path is stable.
