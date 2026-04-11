#!/usr/bin/env bash
set -euo pipefail

# Program: IF135A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF135A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 32
# Expected Feature: SIN Function
# Purpose: It contains tests for the Intrinsic Function SIN.
# Purpose: Variables specific to the Intrinsic Function Test IF135A
# Purpose: Intrinsic Function Tests IF135A - SIN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
