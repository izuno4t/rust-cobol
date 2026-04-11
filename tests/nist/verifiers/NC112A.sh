#!/usr/bin/env bash
set -euo pipefail

# Program: NC112A
# Source: /Users/izuno/Documents/GitHub/izuno4t/rust-cobol/target/nist/programs/NC/NC112A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 32
# Expected Feature: ADD LIMITS TESTS
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
