#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROGRAMS_DIR="${1:-$REPO_ROOT/.nist/programs}"
VERIFIERS_DIR="$SCRIPT_DIR/verifiers"
VERIFIER_OVERRIDES_DIR="$VERIFIERS_DIR/overrides"

mkdir -p "$VERIFIERS_DIR"
mkdir -p "$VERIFIER_OVERRIDES_DIR"

extract_program_purpose() {
    local src="$1"
    perl -ne '
        our $carry = "";
        if (/^\d{6}\*\s*(.+?)\s*\*?\s*$/) {
            my $line = $1;
            $line =~ s/\s+[A-Z]{2,}[0-9A-Z]{2,}\.?[0-9]*\s*$//;
            $line =~ s/\s+\*\s*$//;
            $line =~ s/\*+$//;
            $line =~ s/\s+/ /g;
            $line =~ s/^\s+|\s+$//g;
            next if $line eq "" || $line =~ /^[*]+$/;
            if ($carry ne "") {
                print "$carry $line\n";
                $carry = "";
                next;
            }
            if ($line =~ /contains tests for the$/i) {
                $carry = $line;
                next;
            }
            if ($line =~ /This program is intended|Intrinsic Function|VALIDATION FOR|THE SUBPROGRAM|called by the main program/i) {
                print "$line\n";
            }
        }
    ' "$src" | head -n 4
}

select_verifier_fn() {
    local src="$1"
    if rg -q '^[0-9[:space:]]*PROCEDURE DIVISION USING' "$src"; then
        printf 'verifier_subprogram_standalone\n'
    elif rg -q 'DUMMY PROCEDURE|DUMMY PARAGRAPH' "$src"; then
        printf 'verifier_dummy_display\n'
    elif rg -q 'Intrinsic Function' "$src"; then
        printf 'verifier_intrinsic_function\n'
    else
        printf 'verifier_standard_ccvs\n'
    fi
}

extract_expected_case_count() {
    local src="$1"
    perl -ne '
        if (/MOVE\s+"[^"]+"\s+TO\s+PAR-NAME\./) {
            $count++;
        }
        END {
            print(($count || 0) . "\n");
        }
    ' "$src" | tail -n 1
}

extract_expected_feature_name() {
    local src="$1"
    perl -ne '
        if (/MOVE\s+"([^"]+)"\s+TO\s+FEATURE\./) {
            print "$1\n";
            exit;
        }
    ' "$src" | head -n 1
}

find "$PROGRAMS_DIR" -path '*/COPYLIB' -prune -o -name '*.cob' -print | while IFS= read -r src; do
    program="$(basename "$src" .cob)"
    verifier="$VERIFIERS_DIR/${program}.sh"
    override_verifier="$VERIFIER_OVERRIDES_DIR/${program}.sh"
    if [ -e "$override_verifier" ]; then
        continue
    fi
    verifier_fn="$(select_verifier_fn "$src")"
    purpose="$(extract_program_purpose "$src")"
    expected_cases="$(extract_expected_case_count "$src")"
    expected_feature="$(extract_expected_feature_name "$src")"
    cat > "$verifier" <<EOF
#!/usr/bin/env bash
set -euo pipefail

# Program: ${program}
# Source: ${src}
# Verifier: ${verifier_fn}
# Expected Cases: ${expected_cases}
EOF
    if [ -n "$expected_feature" ]; then
        printf '# Expected Feature: %s\n' "$expected_feature" >> "$verifier"
    fi
    if [ -n "$purpose" ]; then
        while IFS= read -r line; do
            printf '# Purpose: %s\n' "$line" >> "$verifier"
        done <<EOF
${purpose}
EOF
    fi
    cat >> "$verifier" <<EOF

SCRIPT_DIR="\$(cd "\$(dirname "\$0")" && pwd)"
. "\$SCRIPT_DIR/lib.sh"

module="\$1"
program="\$2"
src="\$3"
result_file="\$4"
compile_log="\$5"

${verifier_fn} "\$src" "\$result_file" "\$compile_log"
EOF
    chmod +x "$verifier"
done
