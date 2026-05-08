#!/usr/bin/env bash
set -euo pipefail

# Program: OBIC1A
# Source: target/nist/programs/OB/OBIC1A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 0
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC220 PRINTS THE RESULTS FOR THE TESTING
# Purpose: CALLED BY THE MAIN PROGRAM IC218 AND THE SUBPROGRAM IC219.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
