#!/usr/bin/env bash
set -euo pipefail

# Program: IF118A
# Source: .nist/programs/IF/IF118A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 13
# Expected Feature: LOWER-CASE Function
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF118A
# Purpose: Intrinsic Function Tests IF118A - LOWCASE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
