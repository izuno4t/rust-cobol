#!/usr/bin/env bash
set -euo pipefail

# Program: IF109A
# Source: .nist/programs/IF/IF109A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 8
# Expected Feature: DAY-OF-INTEGER
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF109A
# Purpose: Intrinsic Function Test IF109A - DAY-OF-INTEGER

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
