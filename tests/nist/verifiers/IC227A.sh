#!/usr/bin/env bash
set -euo pipefail

# Program: IC227A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC227A.cob
# Verifier: verifier_subprogram_standalone
# Expected Cases: 9
# Expected Feature: EXTERNAL FILE RECORD
# Purpose: VALIDATION FOR:-
# Purpose: * CLOSE THE FILE THROUGH THE SUBPROGRAM
# Purpose: * THE SUBPROGRAM
# Purpose: * FILE THROUGH THE SUBPROGRAM. THIS SHOULD

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
