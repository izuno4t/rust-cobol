#!/usr/bin/env bash
set -euo pipefail

# Program: IC201A
# Source: target/nist/programs/IC/IC201A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 0
# Purpose: VALIDATION FOR:-
# Purpose: THE SUBPROGRAM IC202 IS CALLED BY THE PROGRAM IC201.
# Purpose: THE SUBPROGRAM HAS FOUR OPERANDS IN THE USING PHRASE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
