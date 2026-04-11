#!/usr/bin/env bash
set -euo pipefail

# Program: DB304M
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/DB/DB304M.cob
# Verifier: verifier_dummy_display

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_dummy_display "$src" "$result_file" "$compile_log"
