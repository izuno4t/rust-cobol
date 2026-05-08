#!/usr/bin/env bash
set -euo pipefail

# Program: IF126A
# Source: target/nist/programs/IF/IF126A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 30
# Expected Feature: NUMVAL-C Function
# Purpose: It contains tests for the Intrinsic Function
# Purpose: Variables specific to the Intrinsic Function Test IF126A
# Purpose: Intrinsic Function Tests IF126A - NUMVAL-C

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
