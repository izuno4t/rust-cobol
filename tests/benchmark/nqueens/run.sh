#!/usr/bin/env bash
# N-Queens Benchmark: rust-cobol vs GnuCOBOL
#
# Usage: bash tests/benchmark/nqueens/run.sh
#
# Requires: cobc (GnuCOBOL), cargo (rust-cobol), hyperfine (optional)
# cspell:words cobc rustcobol

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BENCH_SRC="$SCRIPT_DIR/nqueens.cob"
OUT_DIR="$REPO_ROOT/target/benchmarks/nqueens"
mkdir -p "$OUT_DIR"

now_ms() {
    perl -MTime::HiRes=time -e 'printf "%.0f\n", time * 1000'
}

echo "=== N-Queens Benchmark ==="
echo ""

# --- Compile with rust-cobol ---
echo "[1/4] Compiling with rust-cobol..."
time_start="$(now_ms)"
cargo run --release --package cobol-driver -- \
    --source-format free "$BENCH_SRC" -o "$OUT_DIR/nqueens_rustcobol" 2>/dev/null
time_end="$(now_ms)"
echo "  rust-cobol compile: $(( time_end - time_start )) ms"

# --- Compile with GnuCOBOL ---
if command -v cobc &>/dev/null; then
    echo "[2/4] Compiling with GnuCOBOL (cobc -O2)..."
    time_start="$(now_ms)"
    cobc -O2 -x -o "$OUT_DIR/nqueens_gnucobol" "$BENCH_SRC" 2>/dev/null
    time_end="$(now_ms)"
    echo "  GnuCOBOL compile: $(( time_end - time_start )) ms"
else
    echo "[2/4] GnuCOBOL not found (skipping)"
fi

echo ""

# --- Run rust-cobol ---
echo "[3/4] Running rust-cobol binary (N=1..13)..."
echo "---"
time "$OUT_DIR/nqueens_rustcobol" 2>&1
echo "---"
echo ""

# --- Run GnuCOBOL ---
if [ -x "$OUT_DIR/nqueens_gnucobol" ]; then
    echo "[4/4] Running GnuCOBOL binary (N=1..13)..."
    echo "---"
    time "$OUT_DIR/nqueens_gnucobol" 2>&1
    echo "---"
    echo ""
fi

# --- hyperfine comparison (if available) ---
if command -v hyperfine &>/dev/null; then
    echo "=== hyperfine comparison ==="
    if [ -x "$OUT_DIR/nqueens_gnucobol" ]; then
        hyperfine --warmup 1 --runs 3 \
            "$OUT_DIR/nqueens_rustcobol" \
            "$OUT_DIR/nqueens_gnucobol" \
            --export-markdown "$OUT_DIR/benchmark_results.md"
        echo ""
        echo "Results saved to $OUT_DIR/benchmark_results.md"
    else
        hyperfine --warmup 1 --runs 3 "$OUT_DIR/nqueens_rustcobol"
    fi
else
    echo "(Install 'hyperfine' for detailed comparison: brew install hyperfine)"
fi
