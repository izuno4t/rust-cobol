#!/usr/bin/env bash
# preprocess.sh — Replace XXXXX placeholders in NIST test programs
#
# NIST CCVS 85 test programs use XXXXX placeholders for environment-specific
# values. This script replaces them with values suitable for rust-cobol.
#
# Usage: ./preprocess.sh <input.cob> <output.cob>

set -euo pipefail

INPUT="$1"
OUTPUT="$2"
PROG_NAME=$(basename "$INPUT" .cob)
TMPDIR="/tmp/nist"
mkdir -p "$TMPDIR"

# Replacement table (based on GnuCOBOL's test configuration)
# XXXXX082 = SOURCE-COMPUTER name
# XXXXX083 = OBJECT-COMPUTER name
# XXXXX084 = OBJECT-COMPUTER MEMORY SIZE
# XXXXX055 = PRINT-FILE (main report output)
# XXXXX001-XXXXX004 = data files
# XXXXX051-XXXXX057 = additional output files
# XXXXX081 = line sequential marker (not needed)
# XXXXX090/091 = implementor names

sed \
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
    -e "s|XXXXX36[[:space:]]|00000036|g" \
    -e "s|XXXXX38[[:space:]]|00000038|g" \
    -e "s|XXXXX49[[:space:]]|00000049|g" \
    -e "s|XXXXX50[[:space:]]|00000050|g" \
    -e "s|XXXXX0[[:space:]]|00000000|g" \
    "$INPUT" | \
    sed -e 's/^\(.\{6\}\)P/\1*/' \
        -e 's/^\(.\{6\}\)C/\1*/' \
        -e 's/^\(.\{6\}\)Y/\1 /' \
        -e 's/^\(.\{6\}\)S/\1 /' \
        -e 's/^\(.\{6\}\)A/\1 /' \
    > "$OUTPUT"
