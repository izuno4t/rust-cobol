#!/usr/bin/env bash
set -euo pipefail

# Program: IF101A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF101A.cob
# Verifier: verifier_intrinsic_function
# Purpose: This program is intended to form part of the CCVS85
# Purpose: Intrinsic Function ACOS.
# Purpose: Variables specific to the Intrinsic Function Test IF101A
# Purpose: Intrinsic Function Tests IF101A - ACOS

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
