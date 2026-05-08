#!/usr/bin/env bash
set -euo pipefail

# Program: IF122A
# Source: target/nist/programs/IF/IF122A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 17
# Expected Feature: MIDRANGE Function
# Purpose: It contains tests for the Intrinsic Function MIDRANGE
# Purpose: Variables specific to the Intrinsic Function Test IF122A
# Purpose: Intrinsic Function Tests IF122A - MIDRANGE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
