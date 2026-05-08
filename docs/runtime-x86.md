# x86 Runtime Test Environment

## Overview

This repository includes an x86_64 Linux runtime test environment for
NIST validation and benchmark runs on non-x86 hosts such as Apple Silicon.

The container is pinned to `linux/amd64`, so NIST and benchmark runs execute in
an x86 userspace even when the host machine is ARM-based.

## Files

- `docker/runtime-x86.Dockerfile`

## Requirements

- Docker Desktop or another Docker engine with Compose support
- x86_64 emulation support on the host when running from Apple Silicon

## Included tools

- Rust 1.75 toolchain
- `clang` and `gcc`
- `GnuCOBOL` (`cobc`)
- `hyperfine`
- `make`, `perl`, and `python3`

## Build the environment

```bash
docker build -f docker/runtime-x86.Dockerfile -t rust-cobol-runtime-x86 .
```

## Open a shell

```bash
docker run --rm -it \
  -v "$PWD:/workspace" \
  -e CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64 \
  rust-cobol-runtime-x86 \
  bash
```

## Typical NIST commands

```bash
docker run --rm \
  -v "$PWD:/workspace" \
  -e CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64 \
  rust-cobol-runtime-x86 \
  bash -lc "make nist-prepare && make nist MODULE=NC && NIST_ENV_ROOT=target/nist bash tests/nist/bin/run.sh --summary"
```

## Typical benchmark commands

```bash
docker run --rm \
  -v "$PWD:/workspace" \
  -e CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64 \
  rust-cobol-runtime-x86 \
  bash -lc "bash tests/benchmark/nqueens/run.sh && bash tests/benchmark/micro/run.sh --compare gnucobol"
```

## Notes

- The container uses `CARGO_TARGET_DIR=/workspace/target/runtime-x86-linux-amd64`
  so x86 artifacts do not mix with host-native build outputs
- The repository is bind-mounted into `/workspace`
- Cargo registry and git caches are stored in Docker named volumes
- This environment is intended for runtime-oriented validation rather than
  general day-to-day development
