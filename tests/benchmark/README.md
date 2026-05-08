# Benchmark Suites

This directory contains two independent benchmark suites.

## N-Queens

- Path: `tests/benchmark/nqueens`
- Runner: `tests/benchmark/nqueens/run.sh`
- Workload: a larger free-format COBOL N-Queens program that runs
  `n=1..13`
- Output: `target/benchmarks/nqueens`

## Microbenchmarks

- Path: `tests/benchmark/micro`
- Runner: `tests/benchmark/micro/run.sh`
- Workloads: small fixed-format COBOL programs for arithmetic, string
  operations, and file I/O
- Output: `target/benchmarks/micro`
