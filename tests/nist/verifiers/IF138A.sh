#!/usr/bin/env bash
set -euo pipefail

# Program: IF138A
# Source: .nist/programs/IF/IF138A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 16
# Expected Feature: SUM Function
# Purpose: It contains tests for the Intrinsic Function SUM .
# Purpose: Variables specific to the Intrinsic Function Test IF138A
# Purpose: Intrinsic Function Tests IF138A - SUM

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
