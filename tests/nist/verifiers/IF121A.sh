#!/usr/bin/env bash
set -euo pipefail

# Program: IF121A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF121A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 17
# Expected Feature: MEDIAN Function
# Purpose: It contains tests for the Intrinsic Function MEDIAN.
# Purpose: Variables specific to the Intrinsic Function Test IF121A
# Purpose: Intrinsic Function Tests IF121A - MEDIAN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
