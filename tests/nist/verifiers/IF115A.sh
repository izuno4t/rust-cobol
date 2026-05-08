#!/usr/bin/env bash
set -euo pipefail

# Program: IF115A
# Source: target/nist/programs/IF/IF115A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 8
# Expected Feature: LENGTH Function
# Purpose: It contains tests for the Intrinsic Function LENGTH.
# Purpose: Variables specific to the Intrinsic Function Test IF115A
# Purpose: Intrinsic Function Tests IF115A - LENGTH

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
