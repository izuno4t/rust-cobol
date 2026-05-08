#!/usr/bin/env bash
set -euo pipefail

# Program: IC114A
# Source: target/nist/programs/IC/IC114A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 0
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

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
