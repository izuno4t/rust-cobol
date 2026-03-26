#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

OUTPUT_DIR="$ROOT/target/windows-smoke"
mkdir -p "$OUTPUT_DIR"

OUT="$OUTPUT_DIR/windows-smoke.exe"

cargo build --package cobol-runtime --package cobol-driver
cargo run --package cobol-driver -- \
  tests/windows_smoke.cob \
  -o "$OUT" \
  --source-format free

output="$("$OUT")"
printf '%s\n' "$output"

case "$output" in
  *"WINDOWS SMOKE OK"*) ;;
  *)
    echo "unexpected smoke output"
    exit 1
    ;;
esac
