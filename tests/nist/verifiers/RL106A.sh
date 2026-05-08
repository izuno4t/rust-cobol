#!/usr/bin/env bash
set -euo pipefail

# Program: RL106A
# Source: target/nist/programs/RL/RL106A.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 14
# Expected Feature: R/W REL LENGTH RECS
# Purpose: VALIDATION FOR:-

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

verifier_standard_ccvs "$src" "$result_file" "$compile_log"
