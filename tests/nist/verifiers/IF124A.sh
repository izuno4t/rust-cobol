#!/usr/bin/env bash
set -euo pipefail

# Program: IF124A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IF/IF124A.cob
# Verifier: verifier_intrinsic_function
# Purpose: This program is intended to form part of the CCVS85
# Purpose: Intrinsic Function MOD.
# Purpose: Variables specific to the Intrinsic Function Test IF124A
# Purpose: Intrinsic Function Tests IF124A - MOD

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_intrinsic_function "$src" "$result_file" "$compile_log"
