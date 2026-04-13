#!/usr/bin/env bash
set -euo pipefail

# Program: IF119A
# Source: .nist/programs/IF/IF119A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 23
# Expected Feature: MAX Function
# Purpose: It contains tests for the Intrinsic Function MAX.
# Purpose: Variables specific to the Intrinsic Function Test IF119A
# Purpose: Intrinsic Function Tests IF119A - MAX

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
