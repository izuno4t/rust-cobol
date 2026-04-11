#!/usr/bin/env bash
set -euo pipefail

# Program: IF104A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF104A.cob
# Verifier: verifier_intrinsic_function
# Expected Cases: 27
# Expected Feature: ATAN Function
# Purpose: This program is intended to form part of the CCVS85
# Purpose: Intrinsic Function ATAN.
# Purpose: Variables specific to the Intrinsic Function Test IF104A
# Purpose: Intrinsic Function Tests IF104A - ATAN

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
