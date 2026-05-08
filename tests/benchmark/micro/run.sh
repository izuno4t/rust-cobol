#!/usr/bin/env bash
# micro/run.sh — Small fixed-format microbenchmarks for rust-cobol
#
# Usage:
#   tests/benchmark/micro/run.sh                    # Run all benchmarks
#   tests/benchmark/micro/run.sh arithmetic         # Run specific benchmark
#   tests/benchmark/micro/run.sh --compare gnucobol # Compare with GnuCOBOL
# cspell:words microbenchmarks GNUCOBC cobc rustcobol

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COBOLC="${COBOLC:-cargo run --release --package cobol-driver --}"
GNUCOBC="${GNUCOBC:-cobc}"
OUT_DIR="${BENCH_OUT_DIR:-$REPO_ROOT/target/benchmarks/micro}"

BENCHMARKS=(arithmetic string_ops fileio)

run_benchmark() {
    local name="$1"
    local compiler="$2"
    local src="$SCRIPT_DIR/${name}.cob"
    local bin="$OUT_DIR/bench_${name}_${compiler}"

    if [ ! -f "$src" ]; then
        echo "  $name: SKIP (source not found)"
        return
    fi

    mkdir -p "$OUT_DIR"

    # Compile
    local compile_start compile_end compile_time
    compile_start=$(perl -MTime::HiRes=time -e 'print time')

    if [ "$compiler" = "rustcobol" ]; then
        if ! $COBOLC "$src" -o "$bin" --source-format fixed 2>/dev/null; then
            echo "  $name ($compiler): COMPILE ERROR"
            return
        fi
    elif [ "$compiler" = "gnucobol" ]; then
        if ! command -v "$GNUCOBC" &>/dev/null; then
            echo "  $name ($compiler): SKIP (cobc not found)"
            return
        fi
        if ! "$GNUCOBC" -x -o "$bin" "$src" 2>/dev/null; then
            echo "  $name ($compiler): COMPILE ERROR"
            return
        fi
    fi

    compile_end=$(perl -MTime::HiRes=time -e 'print time')
    compile_time=$(perl -e "printf '%.3f', $compile_end - $compile_start")

    # Run
    local run_start run_end run_time
    run_start=$(perl -MTime::HiRes=time -e 'print time')

    if (cd "$OUT_DIR" && timeout 60 "$bin" > /dev/null 2>&1); then
        run_end=$(perl -MTime::HiRes=time -e 'print time')
        run_time=$(perl -e "printf '%.3f', $run_end - $run_start")
        printf "  %-15s %-12s compile: %7ss  run: %7ss\n" \
            "$name" "($compiler)" "$compile_time" "$run_time"
    else
        echo "  $name ($compiler): RUNTIME ERROR or TIMEOUT"
    fi

    rm -f "$bin"
}

echo "=== rust-cobol Performance Benchmark ==="
echo ""

if [ "${1:-}" = "--compare" ] && [ "${2:-}" = "gnucobol" ]; then
    echo "--- rust-cobol ---"
    for bench in "${BENCHMARKS[@]}"; do
        run_benchmark "$bench" "rustcobol"
    done
    echo ""
    echo "--- GnuCOBOL ---"
    for bench in "${BENCHMARKS[@]}"; do
        run_benchmark "$bench" "gnucobol"
    done
elif [ -n "${1:-}" ]; then
    run_benchmark "$1" "rustcobol"
else
    for bench in "${BENCHMARKS[@]}"; do
        run_benchmark "$bench" "rustcobol"
    done
fi

echo ""
echo "Done."
