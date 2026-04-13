#!/usr/bin/env bash
set -euo pipefail

# Program: IF107A
# Source: .nist/programs/IF/IF107A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 2
# Expected Feature: CURRENT-DATE
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF107A
# Purpose: Intrinsic Function Tests IF107A - CURRENT-DATE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
