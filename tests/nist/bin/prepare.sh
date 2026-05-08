#!/usr/bin/env bash
# cspell:words newcob
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIST_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$NIST_ROOT/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
SOURCE_ROOT="$ENV_ROOT/source"
DEFAULT_SOURCE_ARCHIVE="$NIST_ROOT/assets/newcob.val.tar.gz"
SOURCE_VAL="${1:-$SOURCE_ROOT/newcob.val}"
EXTRACTOR="$NIST_ROOT/lib/extract-programs.pl"
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

case "$SOURCE_VAL" in
    *.tar.gz)
        if [ -f "$SOURCE_VAL" ]; then
            mkdir -p "$SOURCE_ROOT"
            tar -xzf "$SOURCE_VAL" -C "$SOURCE_ROOT"
            SOURCE_VAL="$SOURCE_ROOT/newcob.val"
        fi
        ;;
esac

if [ ! -f "$SOURCE_VAL" ]; then
    if [ "$SOURCE_VAL" = "$SOURCE_ROOT/newcob.val" ] && [ -f "$DEFAULT_SOURCE_ARCHIVE" ]; then
        mkdir -p "$SOURCE_ROOT"
        tar -xzf "$DEFAULT_SOURCE_ARCHIVE" -C "$SOURCE_ROOT"
    else
        echo "Source archive not found: $SOURCE_VAL" >&2
        exit 1
    fi
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
