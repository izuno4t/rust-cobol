#!/usr/bin/env bash
set -euo pipefail

# Program: IF134A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF134A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 13
# Expected Feature: REVERSE Function
# Purpose: It contains tests for the Intrinsic Function REVERSE.
# Purpose: Variables specific to the Intrinsic Function Test IF134A
# Purpose: Intrinsic Function Tests IF134A - REVERSE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
