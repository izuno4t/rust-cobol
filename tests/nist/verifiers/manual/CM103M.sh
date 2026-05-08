#!/usr/bin/env bash
set -euo pipefail

result_file="$3"

if grep -q 'FAIL\*' "$result_file"; then
    printf 'FAIL|FAIL markers found\n'
    exit 0
fi

if grep -q 'MESSAGE LOG' "$result_file" && grep -q 'KILL' "$result_file"; then
    printf 'PASS|communication echo log completed\n'
    exit 0
fi

exit 1
