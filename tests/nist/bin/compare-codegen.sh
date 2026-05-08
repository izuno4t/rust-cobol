#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIST_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$NIST_ROOT/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
AUDIT_RESULTS_DIR="$ENV_ROOT/audit/codegen"
COMPARE_RESULTS_DIR="$ENV_ROOT/audit/compare"
NIST_JOBS="${NIST_JOBS:-3}"
COBOLC="${COBOLC:-$REPO_ROOT/target/release/cobol-driver}"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"

mkdir -p "$COMPARE_RESULTS_DIR"

list_modules() {
    find "$PROGRAMS_DIR" -mindepth 1 -maxdepth 1 -type d \
        ! -name COPYLIB -exec basename {} \; | sort
}

sanitize_name() {
    printf '%s\n' "$1" | tr '-' '_'
}

sanitize_stream() {
    perl -ne '
        chomp;
        my $name = $_;
        $name =~ s/::/__/g;
        $name =~ s/-/_/g;
        if ($name =~ /^[0-9]/) {
            $name = "cob_" . $name;
        }
        if ($name =~ /^(auto|break|case|char|const|continue|default|do|double|else|enum|extern|float|for|goto|if|int|long|register|return|short|signed|sizeof|static|struct|switch|typedef|union|unsigned|void|volatile|while|inline|restrict|_Bool|_Complex|_Imaginary|main)$/) {
            $name = "cob_" . $name;
        }
        print "$name\n";
    '
}

extract_cobol_symbols() {
    local src="$1"
    local out_dir="$2"
    local ast_file="$out_dir/cobol.ast"

    "$COBOLC" --source-format fixed --copy-path "$COPYLIB_DIR" --dump-ast \
        "$src" > "$ast_file" 2> /dev/null

    perl -ne '
        if (/^\s*ProcSection \{$/) {
            $pending = "section";
            next;
        }
        if (/^\s*DeclarativeSection \{$/) {
            $pending = "section";
            next;
        }
        if (/^\s*Paragraph \{$/) {
            $pending = "paragraph";
            next;
        }
        if ($pending ne "" && /^\s*name: "([^"]+)",/) {
            print "$pending:$1\n";
            $pending = "";
            next;
        }

        if (/^\s*kind: ProcedureName \{$/) {
            $in_perform = 1;
            $procedure = "";
            $through = "";
            $await_through = 0;
            next;
        }
        if ($in_perform && /^\s*procedure: "([^"]+)",/) {
            $procedure = $1;
            next;
        }
        if ($in_perform && /^\s*through: Some\($/) {
            $await_through = 1;
            next;
        }
        if ($await_through && /^\s*"([^"]+)",/) {
            $through = $1;
            $await_through = 0;
            next;
        }
        if ($in_perform && /^\s*\},$/) {
            if ($procedure ne "" && $through ne "") {
                print "perform:$procedure:$through\n";
            }
            $in_perform = 0;
            $procedure = "";
            $through = "";
            $await_through = 0;
            next;
        }
    ' "$ast_file" > "$out_dir/cobol.raw"

    awk -F: '$1 == "section" { print $2 } $1 == "paragraph" { print $2 }' \
        "$out_dir/cobol.raw" | sort -u > "$out_dir/cobol.ast.symbols"

    awk -F: '$1 == "perform" { print $2 ":" $3 }' "$out_dir/cobol.raw" \
        | sort -u > "$out_dir/cobol.perform_thru"
}

extract_c_symbols() {
    local c_file="$1"
    local out_dir="$2"
    perl -ne '
        if (/^static void para_([A-Za-z0-9_]+)\(void\)/) {
            print "symbol:$1\n";
        }
        if (/\/\* PERFORM ([A-Za-z0-9_]+) THRU ([A-Za-z0-9_]+) \*\//) {
            print "perform:$1:$2\n";
        }
    ' "$c_file" > "$out_dir/c.raw"

    awk -F: '$1 == "symbol" { print $2 }' "$out_dir/c.raw" | sort -u > "$out_dir/c.symbols"
    awk -F: '$1 == "perform" { print $2 ":" $3 }' "$out_dir/c.raw" \
        | sort -u > "$out_dir/c.perform_thru"
}

extract_hir_symbols() {
    local hir_file="$1"
    local out_dir="$2"
    perl -ne '
        if (/^\s{4}([A-Za-z0-9-]+)[\.:]$/) {
            print "$1\n";
        }
    ' "$hir_file" | sort -u > "$out_dir/cobol.hir.symbols"
}

compare_one() {
    local module="$1"
    local program="$2"
    local src="$ENV_ROOT/work/audit-codegen/$module/$program/nist_preproc_${program}.cob"
    local c_file="$AUDIT_RESULTS_DIR/$module/$program/$program.c"
    local hir_file="$AUDIT_RESULTS_DIR/$module/$program/$program.hir"
    local status_file="$AUDIT_RESULTS_DIR/$module/$program/$program.status"
    local out_dir="$COMPARE_RESULTS_DIR/$module/$program"
    local summary="$out_dir/summary.txt"

    mkdir -p "$out_dir"
    rm -f "$out_dir"/*.raw "$out_dir"/*.symbols "$out_dir"/*.perform_thru "$summary"

    if [ ! -f "$status_file" ] || [ "$(cat "$status_file")" != "OK" ]; then
        cat > "$summary" <<EOF
program=$module/$program
status=SKIP
reason=audit-not-ok
EOF
        printf '%s/%s: SKIP\n' "$module" "$program"
        return
    fi

    extract_cobol_symbols "$src" "$out_dir"
    extract_hir_symbols "$hir_file" "$out_dir"
    extract_c_symbols "$c_file" "$out_dir"

    cat "$out_dir/cobol.ast.symbols" "$out_dir/cobol.hir.symbols" \
        | sort -u > "$out_dir/cobol.symbols"
    sanitize_stream < "$out_dir/cobol.symbols" | sort -u > "$out_dir/cobol.symbols.sanitized"
    sanitize_stream < "$out_dir/c.symbols" | sort -u > "$out_dir/c.symbols.sanitized"
    awk -F: '{ print $1 "\n" $2 }' "$out_dir/cobol.perform_thru" \
        | sanitize_stream \
        | paste -d: - - \
        | sort -u > "$out_dir/cobol.perform_thru.sanitized"
    awk -F: '{ print $1 "\n" $2 }' "$out_dir/c.perform_thru" \
        | sanitize_stream \
        | paste -d: - - \
        | sort -u > "$out_dir/c.perform_thru.sanitized"

    comm -23 "$out_dir/cobol.symbols.sanitized" "$out_dir/c.symbols.sanitized" > "$out_dir/missing_symbols"
    comm -13 "$out_dir/cobol.symbols.sanitized" "$out_dir/c.symbols.sanitized" > "$out_dir/extra_symbols"
    comm -23 "$out_dir/cobol.perform_thru.sanitized" "$out_dir/c.perform_thru.sanitized" > "$out_dir/missing_perform_thru"
    comm -13 "$out_dir/cobol.perform_thru.sanitized" "$out_dir/c.perform_thru.sanitized" > "$out_dir/extra_perform_thru"

    local missing_symbols_count extra_symbols_count missing_perform_count extra_perform_count
    missing_symbols_count="$(wc -l < "$out_dir/missing_symbols" | tr -d ' ')"
    extra_symbols_count="$(wc -l < "$out_dir/extra_symbols" | tr -d ' ')"
    missing_perform_count="$(wc -l < "$out_dir/missing_perform_thru" | tr -d ' ')"
    extra_perform_count="$(wc -l < "$out_dir/extra_perform_thru" | tr -d ' ')"

    if [ "$missing_symbols_count" -eq 0 ] \
        && [ "$extra_symbols_count" -eq 0 ] \
        && [ "$missing_perform_count" -eq 0 ] \
        && [ "$extra_perform_count" -eq 0 ]; then
        cat > "$summary" <<EOF
program=$module/$program
status=OK
missing_symbols=$missing_symbols_count
extra_symbols=$extra_symbols_count
missing_perform_thru=$missing_perform_count
extra_perform_thru=$extra_perform_count
EOF
        printf '%s/%s: OK\n' "$module" "$program"
    else
        cat > "$summary" <<EOF
program=$module/$program
status=DIFF
missing_symbols=$missing_symbols_count
extra_symbols=$extra_symbols_count
missing_perform_thru=$missing_perform_count
extra_perform_thru=$extra_perform_count
EOF
        printf '%s/%s: DIFF symbols=%s perform=%s\n' \
            "$module" "$program" \
            "$((missing_symbols_count + extra_symbols_count))" \
            "$((missing_perform_count + extra_perform_count))"
    fi
}

flush_jobs() {
    local -n pids_ref=$1
    local -n logs_ref=$2
    local i
    for i in "${!pids_ref[@]}"; do
        wait "${pids_ref[$i]}"
    done
    for i in "${!logs_ref[@]}"; do
        if [ -s "${logs_ref[$i]}" ]; then
            cat "${logs_ref[$i]}"
        fi
        rm -f "${logs_ref[$i]}"
    done
    pids_ref=()
    logs_ref=()
}

run_many() {
    local modules=("$@")
    local module src program
    local pids=()
    local logs=()

    for module in "${modules[@]}"; do
        [ -d "$PROGRAMS_DIR/$module" ] || continue
        rm -rf "$COMPARE_RESULTS_DIR/$module"
        for src in "$PROGRAMS_DIR/$module"/*.cob; do
            [ -f "$src" ] || continue
            program="$(basename "$src" .cob)"
            local job_log="$COMPARE_RESULTS_DIR/.${module}_${program}.out"
            rm -f "$job_log"
            (
                compare_one "$module" "$program"
            ) >"$job_log" 2>&1 &
            pids+=("$!")
            logs+=("$job_log")
            if [ "${#pids[@]}" -ge "$NIST_JOBS" ]; then
                flush_jobs pids logs
            fi
        done
    done

    if [ "${#pids[@]}" -gt 0 ]; then
        flush_jobs pids logs
    fi
}

print_summary() {
    local total=0 exact=0 diff=0 skip=0
    local summary_file missing_symbols missing_perform
    while IFS= read -r -d '' summary_file; do
        total=$((total + 1))
        if grep -q '^status=SKIP$' "$summary_file"; then
            skip=$((skip + 1))
            continue
        fi
        missing_symbols="$(awk -F= '$1 == "missing_symbols" { print $2 }' "$summary_file")"
        missing_perform="$(awk -F= '$1 == "missing_perform_thru" { print $2 }' "$summary_file")"
        local extra_symbols extra_perform
        extra_symbols="$(awk -F= '$1 == "extra_symbols" { print $2 }' "$summary_file")"
        extra_perform="$(awk -F= '$1 == "extra_perform_thru" { print $2 }' "$summary_file")"
        if [ "${missing_symbols:-0}" -eq 0 ] \
            && [ "${extra_symbols:-0}" -eq 0 ] \
            && [ "${missing_perform:-0}" -eq 0 ] \
            && [ "${extra_perform:-0}" -eq 0 ]; then
            exact=$((exact + 1))
        else
            diff=$((diff + 1))
        fi
    done < <(find "$COMPARE_RESULTS_DIR" -name summary.txt -print0 | sort -z)

    cat <<EOF
=== NIST COBOL/C Compare Summary ===
Total: $total
Exact: $exact
Diff: $diff
Skip: $skip
Results: $COMPARE_RESULTS_DIR
EOF
}

if [ ! -d "$PROGRAMS_DIR" ]; then
    echo "NIST programs are not prepared in $PROGRAMS_DIR" >&2
    echo "Run 'make nist-prepare' first." >&2
    exit 1
fi

if [ ! -d "$AUDIT_RESULTS_DIR" ]; then
    echo "Audit results not found in $AUDIT_RESULTS_DIR" >&2
    echo "Run 'make nist-audit-codegen' first." >&2
    exit 1
fi

if [ "${1:-}" = "--summary" ]; then
    print_summary
    exit 0
fi

if [ "${1:-}" = "--all" ] || [ $# -eq 0 ]; then
    mapfile -t modules < <(list_modules)
    run_many "${modules[@]}"
    print_summary
    exit 0
fi

if [ $# -eq 2 ]; then
    rm -rf "$COMPARE_RESULTS_DIR/$1/$2"
    compare_one "$1" "$2"
    print_summary
    exit 0
fi

run_many "$1"
print_summary
