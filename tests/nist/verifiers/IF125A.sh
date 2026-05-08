#!/usr/bin/env bash
set -euo pipefail

# Program: IF125A
# Source: target/nist/programs/IF/IF125A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 20
# Expected Feature: NUMVAL Function
# Purpose: It contains tests for the Intrinsic Function NUMVAL.
# Purpose: Variables specific to the Intrinsic Function Test IF125A
# Purpose: Intrinsic Function Tests IF125A - NUMVAL

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
