#!/usr/bin/env bash
set -euo pipefail

# Program: IC114A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC114A.cob
# Verifier: verifier_subprogram_standalone
# Purpose: VALIDATION FOR:-
# Purpose: THE IDENTIFIER CALL-FLAG CONTROLS THE SUBPROGRAM
# Purpose: THE MAIN PROGRAM IC114 REPEATLY CALLS THE SUBPROGRAM

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
