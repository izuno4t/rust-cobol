#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SOURCE_VAL="${1:-$REPO_ROOT/tests/nist/newcob.val}"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
EXTRACTOR="$REPO_ROOT/tests/nist/extract.pl"
TMP_PROGRAMS_DIR=""
BACKUP_PROGRAMS_DIR=""

cleanup() {
    if [ -n "$TMP_PROGRAMS_DIR" ] && [ -d "$TMP_PROGRAMS_DIR" ]; then
        rm -rf "$TMP_PROGRAMS_DIR"
    fi
    if [ -n "$BACKUP_PROGRAMS_DIR" ] && [ -d "$BACKUP_PROGRAMS_DIR" ] && [ ! -d "$PROGRAMS_DIR" ]; then
        mv "$BACKUP_PROGRAMS_DIR" "$PROGRAMS_DIR"
    fi
}

trap cleanup EXIT

if [ ! -f "$SOURCE_VAL" ]; then
    echo "Source archive not found: $SOURCE_VAL" >&2
    exit 1
fi

if [ ! -f "$EXTRACTOR" ]; then
    echo "Extractor not found: $EXTRACTOR" >&2
    exit 1
fi

mkdir -p "$ENV_ROOT"
TMP_PROGRAMS_DIR="$(mktemp -d "$ENV_ROOT/programs.tmp.XXXXXX")"
perl "$EXTRACTOR" "$SOURCE_VAL" "$TMP_PROGRAMS_DIR"

if [ -d "$PROGRAMS_DIR" ]; then
    BACKUP_PROGRAMS_DIR="${PROGRAMS_DIR}.bak.$$"
    rm -rf "$BACKUP_PROGRAMS_DIR"
    mv "$PROGRAMS_DIR" "$BACKUP_PROGRAMS_DIR"
fi

mv "$TMP_PROGRAMS_DIR" "$PROGRAMS_DIR"
TMP_PROGRAMS_DIR=""

if [ -n "$BACKUP_PROGRAMS_DIR" ] && [ -d "$BACKUP_PROGRAMS_DIR" ]; then
    rm -rf "$BACKUP_PROGRAMS_DIR"
    BACKUP_PROGRAMS_DIR=""
fi

echo "Prepared NIST programs in: $PROGRAMS_DIR"
