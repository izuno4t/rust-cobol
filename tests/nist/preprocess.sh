#!/usr/bin/env bash
# preprocess.sh — Replace XXXXX placeholders in NIST test programs

set -euo pipefail

INPUT="$1"
OUTPUT="$2"
PROG_NAME="$(basename "$INPUT" .cob)"
TMPDIR="${NIST_TMPDIR:-/tmp/nist/default}"
mkdir -p "$TMPDIR"

# First pass: replace XXXXX placeholders that appear inside string literals
# (between quotes) without adding extra quotes.
awk '{ print substr($0, 1, 72) }' "$INPUT" | sed \
    -e "s|\"\\([^\"]*\\)XXXXX052\\([^\"]*\\)\"|\"\\1${TMPDIR}/O52\\2\"|g" \
    -e "s|\"\\([^\"]*\\)XXXXX051\\([^\"]*\\)\"|\"\\1${TMPDIR}/O51\\2\"|g" \
    -e "s|\"\\([^\"]*\\)XXXXX053\\([^\"]*\\)\"|\"\\1${TMPDIR}/O53\\2\"|g" \
    -e "s|\"\\([^\"]*\\)XXXXX054\\([^\"]*\\)\"|\"\\1${TMPDIR}/O54\\2\"|g" \
    -e "s|\"\\([^\"]*\\)XXXXX055\\([^\"]*\\)\"|\"\\1${TMPDIR}/P\\2\"|g" \
    | sed \
    -e "s|XXXXX082|COMPUTER|g" \
    -e "s|XXXXX083|COMPUTER|g" \
    -e "s|XXXXX084|00032768|g" \
    -e "s|XXXXX055|\"${TMPDIR}/P\"|g" \
    -e "s|XXXXX001|\"${TMPDIR}/D1\"|g" \
    -e "s|XXXXX002|\"${TMPDIR}/D2\"|g" \
    -e "s|XXXXX003|\"${TMPDIR}/D3\"|g" \
    -e "s|XXXXX004|\"${TMPDIR}/D4\"|g" \
    -e "s|XXXXX005|\"${TMPDIR}/D5\"|g" \
    -e "s|XXXXX006|\"${TMPDIR}/D6\"|g" \
    -e "s|XXXXX007|\"${TMPDIR}/D7\"|g" \
    -e "s|XXXXX014|\"${TMPDIR}/D14\"|g" \
    -e "s|XXXXX051|\"${TMPDIR}/O51\"|g" \
    -e "s|XXXXX052|\"${TMPDIR}/O52\"|g" \
    -e "s|XXXXX053|\"${TMPDIR}/O53\"|g" \
    -e "s|XXXXX054|\"${TMPDIR}/O54\"|g" \
    -e "s|XXXXX056|\"${TMPDIR}/O56\"|g" \
    -e "s|XXXXX057|\"${TMPDIR}/O57\"|g" \
    -e "s|XXXXX058|\"${TMPDIR}/O58\"|g" \
    -e "s|XXXXX059|\"${TMPDIR}/O59\"|g" \
    -e "s|XXXXX060|\"${TMPDIR}/O60\"|g" \
    -e "s|XXXXX068|\"${TMPDIR}/O68\"|g" \
    -e "s|XXXXX069|\"${TMPDIR}/O69\"|g" \
    -e "s|XXXXX081|\"COMPUTER\"|g" \
    -e "s|XXXXX090|COMPUTER|g" \
    -e "s|XXXXX091|COMPUTER|g" \
    -e "s|XXXXX027|\"${TMPDIR}/S1\"|g" \
    -e "s|XXXXX047|COPYLIB|g" \
    -e "s|XXXXX048|COPYLIB|g" \
    -e "s|XXXXX008|\"${TMPDIR}/D8\"|g" \
    -e "s|XXXXX009|\"${TMPDIR}/D9\"|g" \
    -e "s|XXXXX015|\"${TMPDIR}/D15\"|g" \
    -e "s|XXXXX016|\"${TMPDIR}/D16\"|g" \
    -e "s|XXXXX017|\"${TMPDIR}/D17\"|g" \
    -e "s|XXXXX018|\"${TMPDIR}/D18\"|g" \
    -e "s|XXXXX019|\"${TMPDIR}/D19\"|g" \
    -e "s|XXXXX020|\"${TMPDIR}/D20\"|g" \
    -e "s|XXXXX063|\"${TMPDIR}/D63\"|g" \
    -e "s|XXXXX064|\"${TMPDIR}/D64\"|g" \
    -e "s|XXXXX065|00000255|g" \
    -e "s|XXXXX066|00000128|g" \
    -e "s|XXXXX030|\"INQUEUE     \"|g" \
    -e "s|XXXXX031|0001|g" \
    -e "s|XXXXX032|\"OUTQUEUE    \"|g" \
    -e "s|XXXXX033|0001|g" \
    -e "s|XXXXX034|\"INQUEUE-2 MESSAGE PAYLOAD                               \"|g" \
    -e "s|XXXXX035|\"OUTQUEUE-2  \"|g" \
    -e "s|XXXXX036|0002|g" \
    -e "s|XXXXX037|0002|g" \
    -e "s|XXXXX038|\"QUEUE-SET-1                                      \"|g" \
    -e "s|XXXXX039|\"QUEUE-SET-2                                      \"|g" \
    -e "s|XXXXX040|\"QUEUE-SET-3                                      \"|g" \
    -e "s|XXXXX041|\"QUEUE-SET-4                                      \"|g" \
    -e "s|XXXXX042|\"TERMINAL-0001 \"|g" \
    -e "s|XXXXX043|\"TERMINAL-0002\"|g" \
    -e "s|XXXXX36[[:space:]]|00000036|g" \
    -e "s|XXXXX38[[:space:]]|00000038|g" \
    -e "s|XXXXX49[[:space:]]|00000049|g" \
    -e "s|XXXXX50[[:space:]]|00000050|g" \
    -e "s|XXXXX0[[:space:]]|00000000|g" | \
    sed -e 's/^\(.\{6\}\)P/\1*/' \
        -e 's/^\(.\{6\}\)C/\1*/' \
        -e 's/^\(.\{6\}\)Y/\1 /' \
        -e 's/^\(.\{6\}\)S/\1 /' \
        -e 's/^\(.\{6\}\)A/\1 /' \
    | {
        if [ "$PROG_NAME" = "OBNC1M" ]; then
            sed -e '/^004800 /,/^008500 /s/^\(.\{6\}\) /\1*/'
        elif [ "$PROG_NAME" = "CM102M" ]; then
            sed \
                -e 's/^\(073700     IF \)(HOURS OF SYSTEM-TIME \* 3600 + MINUTES OF SYSTEM-TIME \* 60$/\1((HOURS OF SYSTEM-TIME * 3600 + MINUTES OF SYSTEM-TIME * 60/' \
                -e 's/^\(073900         \* 60 + COMP-SECS\) IS LESS THAN 30$/\1) IS LESS THAN 30/'
        else
            cat
        fi
    } | awk '{
        line = substr($0, 1, 72)
        # Comment out NIST trailer lines (flags expected counts, etc.)
        # These start with lots of spaces then program-id.section-number
        if (match(line, /^[[:space:]]{18,}[A-Z][A-Z0-9]*\.[0-9]/) > 0) {
            line = substr(line, 1, 6) "*" substr(line, 8)
        }
        print line
    }' > "$OUTPUT"
