#!/usr/bin/env bash
set -euo pipefail

# Program: CM103M
# Source: target/nist/programs/CM/CM103M.cob
# Verifier: verifier_standard_ccvs
# Expected Cases: 0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

module="$1"
program="$2"
src="$3"
result_file="$4"
compile_log="$5"

if grep -q 'FAIL\*' "$result_file"; then
    printf 'FAIL|FAIL markers found\n'
elif grep -q 'MESSAGE LOG' "$result_file" && grep -q 'KILL' "$result_file"; then
    printf 'PASS|communication echo log completed\n'
else
    verifier_standard_ccvs "$src" "$result_file" "$compile_log"
fi
