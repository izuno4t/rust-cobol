#!/usr/bin/env bash
set -euo pipefail

# Program: IF136A
# Source: target/nist/programs/IF/IF136A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 26
# Expected Feature: SQRT Function
# Purpose: It contains tests for the Intrinsic Function SQRT.
# Purpose: Variables specific to the Intrinsic Function Test IF136A
# Purpose: Intrinsic Function Tests IF136A - SQRT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
