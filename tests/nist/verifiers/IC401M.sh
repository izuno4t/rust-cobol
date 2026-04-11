#!/usr/bin/env bash
set -euo pipefail

# Program: IC401M
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC401M.cob
# Verifier: verifier_subprogram_standalone
# Expected Cases: 0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
