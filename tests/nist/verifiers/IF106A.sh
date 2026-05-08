#!/usr/bin/env bash
set -euo pipefail

# Program: IF106A
# Source: target/nist/programs/IF/IF106A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 30
# Expected Feature: COS Function
# Purpose: It contains tests for the Intrinsic Function COS.
# Purpose: Variables specific to the Intrinsic Function Test IF106A
# Purpose: Intrinsic Function Tests IF106A - COS

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
