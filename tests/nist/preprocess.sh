#!/usr/bin/env bash
# preprocess.sh — Replace XXXXX placeholders in NIST test programs

set -euo pipefail

INPUT="$1"
OUTPUT="$2"
PROG_NAME="$(basename "$INPUT" .cob)"
TMPDIR="${NIST_TMPDIR:-/tmp/nist/default}"
mkdir -p "$TMPDIR"

awk '{ print substr($0, 1, 72) }' "$INPUT" | sed \
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
        elif [ "$PROG_NAME" = "DB205A" ]; then
            sed \
                -e 's/^\(017200\) /\1*/' \
                -e 's/^\(017300\) /\1*/' \
                -e 's/^\(017800\) /\1*/' \
                -e 's/^\(031100     \)DISABLE INPUT CM-INQUE WITH KEY$/\1MOVE 1 TO KEY-1./' \
                -e 's/^\(035400     \)ENABLE OUTPUT CM-OUTQUE WITH KEY$/\1MOVE 1 TO KEY-1./' \
                -e 's/^\(039600     \)ACCEPT CM-INQUE MESSAGE COUNT\./\1MOVE 1 TO KEY-1./' \
                -e 's/^\(043700     \)RECEIVE CM-INQUE MESSAGE INTO WORK-AREA$/\1GO TO RECEIVE-TEST-1-CONT./' \
                -e 's/^\(045200     \)ENABLE INPUT CM-INQUE WITH KEY$/\1MOVE 1 TO KEY-1./' \
                -e 's/^\(046600     \)SEND CM-OUTQUE FROM WORK-AREA WITH EGI\./\1MOVE 1 TO KEY-1./' \
                -e 's/^\(053500     \)RECEIVE CM-INQUE MESSAGE INTO WORK-AREA\./\1MOVE 1 TO KEY-1./'
        else
            cat
        fi
    } > "$OUTPUT"
