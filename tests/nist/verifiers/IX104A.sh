#!/usr/bin/env bash
set -euo pipefail

# Program: IX104A
# Source: target/nist/programs/IX/IX104A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 13
# Expected Feature: OPEN: 00 EXP.    
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
