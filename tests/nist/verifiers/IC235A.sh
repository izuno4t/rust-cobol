#!/usr/bin/env bash
set -euo pipefail

# Program: IC235A
# Source: target/nist/programs/IC/IC235A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 9
# Expected Feature: MULTIPLE EXIT PROGRM
# Purpose: VALIDATION FOR:-
# Purpose: THE SAME AS IN THE SUBPROGRAM BUT THE NUMBER OF CHARACTERS
# Purpose: TAKEN FOR EACH CALL TO THE SUBPROGRAM.
# Purpose: IF THE SUBPROGRAM WITH MULTIPLE EXIT PROGRAM
# Purpose: DESCRIPTIONS ARE DIFFERENT IN THE SUBPROGRAM FROM THE MAIN
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC235A-1 HAS THREE OPERANDS IN THE
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC235A-2 HAS TWO OPERANDS IN THE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
