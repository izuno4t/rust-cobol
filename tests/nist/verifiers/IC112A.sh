#!/usr/bin/env bash
set -euo pipefail

# Program: IC112A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC112A.cob
# Verifier: verifier_subprogram_standalone
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC113 IS CALLED BY THE MAIN PROGRAM
# Purpose: SECTION OF THE SUBPROGRAM. IF ANY ERRORS ARE ENCOUNTERED

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
