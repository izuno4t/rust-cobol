#!/usr/bin/env bash
set -euo pipefail

# Program: IC235A
# Source: .nist/programs/IC/IC235A.cob
# Verifier: verifier_subprogram_standalone
# Expected Cases: 9
# Expected Feature: MULTIPLE EXIT PROGRM
# Purpose: VALIDATION FOR:-
# Purpose: THE SAME AS IN THE SUBPROGRAM BUT THE NUMBER OF CHARACTERS
# Purpose: TAKEN FOR EACH CALL TO THE SUBPROGRAM.
# Purpose: IF THE SUBPROGRAM WITH MULTIPLE EXIT PROGRAM

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
