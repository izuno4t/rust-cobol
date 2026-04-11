#!/usr/bin/env bash
set -euo pipefail

# Program: NC302M
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/NC/NC302M.cob
# Verifier: verifier_dummy_display
# Expected Cases: 0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_dummy_display "$src" "$result_file" "$compile_log"
