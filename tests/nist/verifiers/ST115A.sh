#!/usr/bin/env bash
set -euo pipefail

# Program: ST115A
# Source: target/nist/programs/ST/ST115A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 1
# Expected Feature: CREATE FILE SQ-FS1
# Purpose: VALIDATION FOR:-
# Purpose: VALIDATION FOR:-
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
