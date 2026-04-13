#!/usr/bin/env bash
set -euo pipefail

# Program: IF141A
# Source: .nist/programs/IF/IF141A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 16
# Expected Feature: VARIANCE Function
# Purpose: It contains tests for the Intrinsic Function VARIANCE
# Purpose: Variables specific to the Intrinsic Function Test IF141A
# Purpose: Intrinsic Function Tests IF141A - VARIANCE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
