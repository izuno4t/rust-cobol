#!/usr/bin/env bash
set -euo pipefail

# Program: IF108A
# Source: .nist/programs/IF/IF108A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 10
# Expected Feature: DATE-OF-INTEGER
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF108A
# Purpose: Intrinsic Function Test IF108A - DATE-OF-INTEGER

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
