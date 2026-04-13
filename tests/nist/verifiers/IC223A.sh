#!/usr/bin/env bash
set -euo pipefail

# Program: IC223A
# Source: .nist/programs/IC/IC223A.cob
# Verifier: verifier_subprogram_standalone
# Expected Cases: 3
# Expected Feature: LEV 2 CALL STATEMENT
# Purpose: VALIDATION FOR:-
# Purpose: CONTAINING THE NAME OF THE SUBPROGRAM TO BE CALLED.
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_subprogram_standalone "$src" "$result_file" "$compile_log"
