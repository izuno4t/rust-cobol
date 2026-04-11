#!/usr/bin/env bash
set -euo pipefail

# Program: SQ206A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/SQ/SQ206A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 7
# Expected Feature: CREATE SQ-FS1,SQ-FS3
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
