#!/usr/bin/env bash
set -euo pipefail

# Program: IF103A
# Source: target/nist/programs/IF/IF103A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 23
# Expected Feature: ASIN Function
# Purpose: This program is intended to form part of the CCVS85
# Purpose: Intrinsic Function ASIN.
# Purpose: Variables specific to the Intrinsic Function Test IF103A
# Purpose: Intrinsic Function Tests IF103A - ASIN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
