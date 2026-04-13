#!/usr/bin/env bash
set -euo pipefail

# Program: ST132A
# Source: .nist/programs/ST/ST132A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 8
# Expected Feature: SORT, OUTPUT PROC
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
