#!/usr/bin/env bash
set -euo pipefail

# Program: IF105A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF105A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 8
# Expected Feature: CHAR Function
# Purpose: It contains tests for the Intrinsic Function CHAR.
# Purpose: Variables specific to the Intrinsic Function Test IF105A
# Purpose: Intrinsic Function Tests IF105A - CHAR

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
