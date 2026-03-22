#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SOURCE_VAL="${1:-$REPO_ROOT/tests/nist/newcob.val}"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
EXTRACTOR="$REPO_ROOT/tests/nist/extract.pl"

if [ ! -f "$SOURCE_VAL" ]; then
    echo "Source archive not found: $SOURCE_VAL" >&2
    exit 1
fi

if [ ! -f "$EXTRACTOR" ]; then
    echo "Extractor not found: $EXTRACTOR" >&2
    exit 1
fi

if [ -d "$PROGRAMS_DIR" ] && find "$PROGRAMS_DIR" -mindepth 1 -print -quit | grep -q .; then
    echo "Refusing to overwrite non-empty directory: $PROGRAMS_DIR" >&2
    echo "Remove it manually if you want to regenerate this environment." >&2
    exit 1
fi

mkdir -p "$PROGRAMS_DIR"
perl "$EXTRACTOR" "$SOURCE_VAL" "$PROGRAMS_DIR"

echo "Prepared NIST programs in: $PROGRAMS_DIR"
