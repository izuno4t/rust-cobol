#!/usr/bin/env bash
set -euo pipefail

# Program: IF117A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF117A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 32
# Expected Feature: LOG10 Function
# Purpose: It contains tests for the Intrinsic Function LOG10.
# Purpose: Variables specific to the Intrinsic Function Test IF117A
# Purpose: Intrinsic Function Tests IF117A - LOG10

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
