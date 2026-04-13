#!/usr/bin/env bash
set -euo pipefail

# Program: CM105M
# Source: .nist/programs/CM/CM105M.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 11
# Expected Feature: QUEUE SERIES PPPP

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
