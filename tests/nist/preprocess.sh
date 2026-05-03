#!/usr/bin/env bash
# preprocess.sh — Replace XXXXX placeholders in NIST test programs

set -euo pipefail

INPUT="$1"
OUTPUT="$2"
PROG_NAME="$(basename "$INPUT" .cob)"
TMPDIR="${NIST_TMPDIR:-/tmp/nist/default}"
mkdir -p "$TMPDIR"

# First pass: replace string literals that are exactly XXXXX placeholders
# without adding another layer of quotes. Do not rewrite embedded data
# literals such as "GGGGHXXXXX052ALTKEY1".
awk '{ print substr($0, 1, 72) }' "$INPUT" | sed \
    -e "s|\"XXXXX052\"|\"${TMPDIR}/O52\"|g" \
    -e "s|\"XXXXX051\"|\"${TMPDIR}/O51\"|g" \
    -e "s|\"XXXXX053\"|\"${TMPDIR}/O53\"|g" \
    -e "s|\"XXXXX054\"|\"${TMPDIR}/O54\"|g" \
    -e "s|\"XXXXX055\"|\"${TMPDIR}/P\"|g" \
    | perl -pe '
        s{(?<![A-Z0-9"])\QXXXXX051\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O51"}g;
        s{(?<![A-Z0-9"])\QXXXXX052\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O52"}g;
        s{(?<![A-Z0-9"])\QXXXXX053\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O53"}g;
        s{(?<![A-Z0-9"])\QXXXXX054\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O54"}g;
        s{(?<![A-Z0-9"])\QXXXXX055\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/P"}g;
    ' \
    | perl -pe '
        s{(?<![A-Z0-9"])\QXXXXX082\E(?![A-Z0-9"])}{COMPUTER}g;
        s{(?<![A-Z0-9"])\QXXXXX083\E(?![A-Z0-9"])}{COMPUTER}g;
        s{(?<![A-Z0-9"])\QXXXXX084\E(?![A-Z0-9"])}{00032768}g;
        s{(?<![A-Z0-9"])\QXXXXX001\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D1"}g;
        s{(?<![A-Z0-9"])\QXXXXX002\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D2"}g;
        s{(?<![A-Z0-9"])\QXXXXX003\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D3"}g;
        s{(?<![A-Z0-9"])\QXXXXX004\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D4"}g;
        s{(?<![A-Z0-9"])\QXXXXX005\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D5"}g;
        s{(?<![A-Z0-9"])\QXXXXX006\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D6"}g;
        s{(?<![A-Z0-9"])\QXXXXX007\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D7"}g;
        s{(?<![A-Z0-9"])\QXXXXX008\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D8"}g;
        s{(?<![A-Z0-9"])\QXXXXX009\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D9"}g;
        s{(?<![A-Z0-9"])\QXXXXX014\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D14"}g;
        s{(?<![A-Z0-9"])\QXXXXX015\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D15"}g;
        s{(?<![A-Z0-9"])\QXXXXX016\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D16"}g;
        s{(?<![A-Z0-9"])\QXXXXX017\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D17"}g;
        s{(?<![A-Z0-9"])\QXXXXX018\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D18"}g;
        s{(?<![A-Z0-9"])\QXXXXX019\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D19"}g;
        s{(?<![A-Z0-9"])\QXXXXX020\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D20"}g;
        s{(?<![A-Z0-9"])\QXXXXX027\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/S1"}g;
        s{(?<![A-Z0-9"])\QXXXXX030\E(?![A-Z0-9"])}{"INQUEUE     "}g;
        s{(?<![A-Z0-9"])\QXXXXX031\E(?![A-Z0-9"])}{0001}g;
        s{(?<![A-Z0-9"])\QXXXXX032\E(?![A-Z0-9"])}{"OUTQUEUE    "}g;
        s{(?<![A-Z0-9"])\QXXXXX033\E(?![A-Z0-9"])}{0001}g;
        s{(?<![A-Z0-9"])\QXXXXX034\E(?![A-Z0-9"])}{"INQUEUE-2 MESSAGE PAYLOAD                               "}g;
        s{(?<![A-Z0-9"])\QXXXXX035\E(?![A-Z0-9"])}{"OUTQUEUE-2  "}g;
        s{(?<![A-Z0-9"])\QXXXXX036\E(?![A-Z0-9"])}{0002}g;
        s{(?<![A-Z0-9"])\QXXXXX037\E(?![A-Z0-9"])}{0002}g;
        s{(?<![A-Z0-9"])\QXXXXX038\E(?![A-Z0-9"])}{"QUEUE-SET-1                                      "}g;
        s{(?<![A-Z0-9"])\QXXXXX039\E(?![A-Z0-9"])}{"QUEUE-SET-2                                      "}g;
        s{(?<![A-Z0-9"])\QXXXXX040\E(?![A-Z0-9"])}{"QUEUE-SET-3                                      "}g;
        s{(?<![A-Z0-9"])\QXXXXX041\E(?![A-Z0-9"])}{"QUEUE-SET-4                                      "}g;
        s{(?<![A-Z0-9"])\QXXXXX042\E(?![A-Z0-9"])}{"TERMINAL-0001 "}g;
        s{(?<![A-Z0-9"])\QXXXXX043\E(?![A-Z0-9"])}{"TERMINAL-0002"}g;
        s{(?<![A-Z0-9"])\QXXXXX047\E(?![A-Z0-9"])}{COPYLIB}g;
        s{(?<![A-Z0-9"])\QXXXXX048\E(?![A-Z0-9"])}{COPYLIB}g;
        s{(?<![A-Z0-9"])\QXXXXX056\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O56"}g;
        s{(?<![A-Z0-9"])\QXXXXX057\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O57"}g;
        s{(?<![A-Z0-9"])\QXXXXX058\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O58"}g;
        s{(?<![A-Z0-9"])\QXXXXX059\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O59"}g;
        s{(?<![A-Z0-9"])\QXXXXX060\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O60"}g;
        s{(?<![A-Z0-9"])\QXXXXX063\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D63"}g;
        s{(?<![A-Z0-9"])\QXXXXX064\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/D64"}g;
        s{(?<![A-Z0-9"])\QXXXXX065\E(?![A-Z0-9"])}{00000255}g;
        s{(?<![A-Z0-9"])\QXXXXX066\E(?![A-Z0-9"])}{00000128}g;
        s{(?<![A-Z0-9"])\QXXXXX068\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O68"}g;
        s{(?<![A-Z0-9"])\QXXXXX069\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/O69"}g;
        s{(?<![A-Z0-9"])\QXXXXX081\E(?![A-Z0-9"])}{"COMPUTER"}g;
        s{(?<![A-Z0-9"])\QXXXXX090\E(?![A-Z0-9"])}{COMPUTER}g;
        s{(?<![A-Z0-9"])\QXXXXX091\E(?![A-Z0-9"])}{COMPUTER}g;
        s{(?<![A-Z0-9"])\QXXXXD001\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D1"}g;
        s{(?<![A-Z0-9"])\QXXXXD002\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D2"}g;
        s{(?<![A-Z0-9"])\QXXXXD003\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D3"}g;
        s{(?<![A-Z0-9"])\QXXXXD004\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D4"}g;
        s{(?<![A-Z0-9"])\QXXXXD005\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D5"}g;
        s{(?<![A-Z0-9"])\QXXXXD006\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D6"}g;
        s{(?<![A-Z0-9"])\QXXXXD007\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D7"}g;
        s{(?<![A-Z0-9"])\QXXXXD008\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D8"}g;
        s{(?<![A-Z0-9"])\QXXXXD009\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D9"}g;
        s{(?<![A-Z0-9"])\QXXXXD010\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D10"}g;
        s{(?<![A-Z0-9"])\QXXXXD011\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D11"}g;
        s{(?<![A-Z0-9"])\QXXXXD012\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D12"}g;
        s{(?<![A-Z0-9"])\QXXXXD013\E(?![A-Z0-9"])}{"'"${TMPDIR}"'/../_shared/D13"}g;
        s{(?<![A-Z0-9"])\QXXXXX36\E\s}{00000036}g;
        s{(?<![A-Z0-9"])\QXXXXX38\E\s}{00000038}g;
        s{(?<![A-Z0-9"])\QXXXXX49\E\s}{00000049}g;
        s{(?<![A-Z0-9"])\QXXXXX50\E\s}{00000050}g;
        s{(?<![A-Z0-9"])\QXXXXX0\E\s}{00000000}g;
    ' | \
    sed -e 's/^\(.\{6\}\)P/\1*/' \
        -e 's/^\(.\{6\}\)C/\1*/' \
        -e 's/^\(.\{6\}\)F/\1*/' \
        -e 's/^\(.\{6\}\)I/\1*/' \
        -e 's/^\(.\{6\}\)E/\1 /' \
        -e 's/^\(.\{6\}\)H/\1 /' \
        -e 's/^\(.\{6\}\)Y/\1 /' \
        -e 's/^\(.\{6\}\)S/\1 /' \
        -e 's/^\(.\{6\}\)A/\1 /' \
    | {
        if [ "$PROG_NAME" = "OBNC1M" ]; then
            sed -e '/^004800 /,/^008500 /s/^\(.\{6\}\) /\1*/'
        elif [ "$PROG_NAME" = "NC401M" ]; then
            # Comment out PERFORM...UNTIL paragraphs that create infinite loops
            sed \
                -e '/^029400/s/^\(.\{6\}\) /\1*/' \
                -e '/^029500/s/^\(.\{6\}\) /\1*/' \
                -e '/^030000/s/^\(.\{6\}\) /\1*/' \
                -e '/^030100/s/^\(.\{6\}\) /\1*/' \
                -e '/^030200/s/^\(.\{6\}\) /\1*/'
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
