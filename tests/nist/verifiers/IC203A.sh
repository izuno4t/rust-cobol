#!/usr/bin/env bash
set -euo pipefail

# Program: IC203A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC203A.cob
# Verifier: verifier_subprogram_standalone
# Expected Cases: 0
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC206 IS CALLED BY THE SUBPROGRAM
# Purpose: IC205. THE SUBPROGRAM IS THEN CANCELED AND CALLED

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
