#!/usr/bin/env bash
set -euo pipefail

# Program: ST146A                                                    TES00010
# Source: target/nist/programs/ST/ST146A                                                    TES00010.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 4
# Expected Feature: OCCURS DEPENDING ON
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
