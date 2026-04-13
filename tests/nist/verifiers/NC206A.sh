#!/usr/bin/env bash
set -euo pipefail

# Program: NC206A
# Source: .nist/programs/NC/NC206A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 53
# Expected Feature: QUAL LIMITS 
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
