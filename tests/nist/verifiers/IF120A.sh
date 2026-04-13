#!/usr/bin/env bash
set -euo pipefail

# Program: IF120A
# Source: .nist/programs/IF/IF120A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 17
# Expected Feature: MEAN Function
# Purpose: It contains tests for the Intrinsic Function MEAN.
# Purpose: Variables specific to the Intrinsic Function Test IF120A
# Purpose: Intrinsic Function Tests IF120A - MEAN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
