#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIST_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$NIST_ROOT/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
RESULTS_DIR="$ENV_ROOT/audit/codegen"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"
PREPROCESS="$NIST_ROOT/lib/preprocess-placeholders.sh"
COBOLC="${COBOLC:-$REPO_ROOT/target/release/cobol-driver}"
NIST_JOBS="${NIST_JOBS:-3}"
TMP_ROOT="$ENV_ROOT/work/audit-codegen"
NIST_AUDIT_TMP_ROOT="${NIST_AUDIT_TMP_ROOT:-/tmp/na}"

mkdir -p "$RESULTS_DIR" "$TMP_ROOT"

list_modules() {
    find "$PROGRAMS_DIR" -mindepth 1 -maxdepth 1 -type d \
        ! -name COPYLIB -exec basename {} \; | sort
}

program_exists() {
    local module="$1"
    local program="$2"
    [ -f "$PROGRAMS_DIR/$module/$program.cob" ]
}

run_one() {
    local module="$1"
    local program="$2"
    local src="$PROGRAMS_DIR/$module/$program.cob"
    local program_dir="$RESULTS_DIR/$module/$program"
    local workdir="$TMP_ROOT/$module/$program"
    local program_tmpdir="$NIST_AUDIT_TMP_ROOT/$module/$program"
    local preprocessed="$workdir/nist_preproc_${program}.cob"
    local hir_file="$program_dir/${program}.hir"
    local c_file="$program_dir/${program}.c"
    local stderr_log="$program_dir/${program}.stderr.log"
    local status_file="$program_dir/${program}.status"

    mkdir -p "$program_dir" "$workdir"
    rm -rf "$program_tmpdir"
    mkdir -p "$program_tmpdir"
    rm -f "$preprocessed" "$hir_file" "$c_file" "$stderr_log" "$status_file"

    NIST_TMPDIR="$program_tmpdir" "$PREPROCESS" "$src" "$preprocessed"

    if ! "$COBOLC" --source-format fixed --copy-path "$COPYLIB_DIR" --dump-hir \
        "$preprocessed" >"$hir_file" 2>"$stderr_log"; then
        printf 'HIR_ERROR\n' > "$status_file"
        printf '%s/%s: HIR_ERROR\n' "$module" "$program"
        return
    fi

    if ! "$COBOLC" --source-format fixed --copy-path "$COPYLIB_DIR" --emit-c --c-only \
        "$preprocessed" >"$c_file" 2>>"$stderr_log"; then
        printf 'C_ERROR\n' > "$status_file"
        printf '%s/%s: C_ERROR\n' "$module" "$program"
        return
    fi

    printf 'OK\n' > "$status_file"
    printf '%s/%s: OK\n' "$module" "$program"
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

run_all() {
    local modules=("$@")
    local module src program
    local pids=()
    local logs=()

    for module in "${modules[@]}"; do
        [ -d "$PROGRAMS_DIR/$module" ] || continue
        rm -rf "$RESULTS_DIR/$module" "$TMP_ROOT/$module"
        for src in "$PROGRAMS_DIR/$module"/*.cob; do
            [ -f "$src" ] || continue
            program="$(basename "$src" .cob)"
            local job_log="$RESULTS_DIR/.${module}_${program}.out"
            rm -f "$job_log"
            (
                run_one "$module" "$program"
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
    local ok=0 hir_error=0 c_error=0 total=0
    local status_file status
    while IFS= read -r -d '' status_file; do
        total=$((total + 1))
        status="$(cat "$status_file")"
        case "$status" in
            OK) ok=$((ok + 1)) ;;
            HIR_ERROR) hir_error=$((hir_error + 1)) ;;
            C_ERROR) c_error=$((c_error + 1)) ;;
        esac
    done < <(find "$RESULTS_DIR" -name '*.status' -print0 | sort -z)

    cat <<EOF
=== NIST Codegen Audit Summary ===
Total: $total
OK: $ok
HIR_ERROR: $hir_error
C_ERROR: $c_error
Results: $RESULTS_DIR
EOF
}

if [ ! -x "$COBOLC" ] && [ "${COBOLC#* }" = "$COBOLC" ]; then
    echo "Compiler not found: $COBOLC" >&2
    exit 1
fi

if [ ! -d "$PROGRAMS_DIR" ]; then
    echo "NIST programs are not prepared in $PROGRAMS_DIR" >&2
    echo "Run 'make nist-prepare' first." >&2
    exit 1
fi

if [ "${1:-}" = "--summary" ]; then
    print_summary
    exit 0
fi

if [ "${1:-}" = "--all" ] || [ $# -eq 0 ]; then
    mapfile -t modules < <(list_modules)
    run_all "${modules[@]}"
    print_summary
    exit 0
fi

if [ $# -eq 2 ]; then
    module="$1"
    program="$2"
    if ! program_exists "$module" "$program"; then
        echo "Program not found: $module/$program" >&2
        exit 1
    fi
    rm -rf "$RESULTS_DIR/$module/$program" "$TMP_ROOT/$module/$program"
    run_one "$module" "$program"
    print_summary
    exit 0
fi

module="$1"
if [ ! -d "$PROGRAMS_DIR/$module" ]; then
    echo "Module not found: $module" >&2
    exit 1
fi

run_all "$module"
print_summary
