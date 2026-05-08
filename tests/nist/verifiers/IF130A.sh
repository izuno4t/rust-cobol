#!/usr/bin/env bash
set -euo pipefail

# Program: IF130A
# Source: target/nist/programs/IF/IF130A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 21
# Expected Feature: PRESENT-VALUE Function
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF130A
# Purpose: Intrinsic Function Tests IF130A - PRESENT-VALUE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
