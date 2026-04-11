#!/usr/bin/env bash
set -euo pipefail

# Program: IC116M
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC116M.cob
# Verifier: verifier_standard_ccvs
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC118 IS CALLED BY THE SUBPROGRAM IC117.
# Purpose: THE SUBPROGRAM IC118 DOES NOT CONTAIN A LINKAGE SECTION OR

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
