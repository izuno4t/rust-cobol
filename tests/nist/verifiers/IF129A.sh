#!/usr/bin/env bash
set -euo pipefail

# Program: IF129A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF129A.cob
# Verifier: verifier_intrinsic_function
# Purpose: It contains tests for the Intrinsic Function ORD-MIN.
# Purpose: Variables specific to the Intrinsic Function Test IF129A
# Purpose: Intrinsic Function Tests IF129A - ORD-MIN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
