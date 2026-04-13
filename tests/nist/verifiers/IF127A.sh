#!/usr/bin/env bash
set -euo pipefail

# Program: IF127A
# Source: .nist/programs/IF/IF127A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 9
# Expected Feature: ORD Function
# Purpose: It contains tests for the Intrinsic Function ORD.
# Purpose: Variables specific to the Intrinsic Function Test IF127A
# Purpose: Intrinsic Function Tests IF127A - ORD

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
