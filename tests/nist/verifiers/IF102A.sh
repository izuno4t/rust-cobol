#!/usr/bin/env bash
set -euo pipefail

# Program: IF102A
# Source: .nist/programs/IF/IF102A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 13
# Expected Feature: ANNUITY Function
# Purpose: This program is intended to form part of the CCVS85
# Purpose: Intrinsic Function ANNUITY.
# Purpose: Variables specific to the Intrinsic Function Test IF102A
# Purpose: Intrinsic Function Tests IF102A - ANNUITY

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
