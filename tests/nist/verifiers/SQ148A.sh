#!/usr/bin/env bash
set -euo pipefail

# Program: SQ148A
# Source: target/nist/programs/SQ/SQ148A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 2
# Expected Feature: OPEN, CREATE FILE
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
