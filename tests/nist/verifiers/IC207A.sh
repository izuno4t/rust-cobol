#!/usr/bin/env bash
set -euo pipefail

# Program: IC207A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/IC/IC207A.cob
# Verifier: verifier_subprogram_standalone
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC208 CONTAINS TABLES AND AN INDEX

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
