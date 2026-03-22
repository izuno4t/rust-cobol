#!/usr/bin/env bash
# run_nist.sh — Run NIST CCVS 85 with GnuCOBOL-style judgment rules.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
RESULTS_DIR="$ENV_ROOT/results"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"
PREPROCESS="$SCRIPT_DIR/preprocess.sh"
COBOLC="${COBOLC:-cargo run --release --package cobol-driver --}"
NIST_WORKDIR="$ENV_ROOT/work/run"
NIST_TMPDIR="/tmp/nist/run"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
CURRENT_RUN_PID=""

mkdir -p "$RESULTS_DIR" "$NIST_WORKDIR" "$NIST_TMPDIR"

cleanup_running_job() {
    if [ -n "${CURRENT_RUN_PID:-}" ] && kill -0 "$CURRENT_RUN_PID" 2>/dev/null; then
        kill -TERM -- "-$CURRENT_RUN_PID" 2>/dev/null || true
        sleep 1
        kill -KILL -- "-$CURRENT_RUN_PID" 2>/dev/null || true
        wait "$CURRENT_RUN_PID" 2>/dev/null || true
    fi
    CURRENT_RUN_PID=""
}

handle_interrupt() {
    cleanup_running_job
    exit 130
}

trap handle_interrupt INT TERM
trap cleanup_running_job EXIT

list_modules() {
    find "$PROGRAMS_DIR" -mindepth 1 -maxdepth 1 -type d \
        ! -name COPYLIB -exec basename {} \; | sort
}

inspect_reason_for_program() {
    local src="$1"
    local result_file="$2"
    if [ -f "$src" ] && grep -Eq '^[0-9[:space:]]*PROCEDURE DIVISION USING' "$src"; then
        printf 'subprogram-only\n'
    elif [ -f "$src" ] && grep -Eq 'MOVE "INSPT" TO P-OR-F|INSPECT-COUNTER' "$src"; then
        printf 'manual-report\n'
    elif [ -f "$src" ] && grep -Eq 'DUMMY PROCEDURE|DUMMY PARAGRAPH' "$src"; then
        printf 'dummy-display\n'
    elif [ ! -s "$result_file" ] || ! grep -q '[^[:space:]]' "$result_file" 2>/dev/null; then
        printf 'no-output\n'
    else
        printf 'unclassified\n'
    fi
}

ccvs_summary_count() {
    local file="$1"
    local pattern="$2"
    local value
    value=$(
        awk -v pat="$pattern" '
            $0 ~ pat {
                if ($1 == "NO") {
                    print 0
                } else {
                    print $1
                }
            }
        ' "$file" | tail -n 1
    )
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '0\n'
    fi
}

print_status_group() {
    local module="$1"
    local status_name="$2"
    local label="$3"
    local mod_results="$RESULTS_DIR/$module"
    local matches=()
    local status_file
    [ -d "$mod_results" ] || return 0
    for status_file in "$mod_results"/*.status; do
        [ -f "$status_file" ] || continue
        if [ "$(cat "$status_file")" != "$status_name" ]; then
            continue
        fi
        matches+=("$(basename "$status_file" .status)")
    done
    if [ "${#matches[@]}" -gt 0 ]; then
        echo "  $label: ${matches[*]}"
    fi
}

print_inspect_groups() {
    local module="$1"
    local mod_results="$RESULTS_DIR/$module"
    local reason_file reason
    local matches=()
    [ -d "$mod_results" ] || return 0
    for reason in subprogram-only manual-report dummy-display no-output unclassified; do
        matches=()
        for reason_file in "$mod_results"/*.reason; do
            [ -f "$reason_file" ] || continue
            if [ "$(cat "$reason_file")" != "$reason" ]; then
                continue
            fi
            matches+=("$(basename "$reason_file" .reason)")
        done
        if [ "${#matches[@]}" -gt 0 ]; then
            echo "  INSPECT/$reason: ${matches[*]}"
        fi
    done
}

print_module_diagnostics() {
    local module="$1"
    echo "  Detailed Results:"
    print_status_group "$module" "FAIL" "FAIL"
    print_status_group "$module" "COMPILE_ERROR" "COMPILE_ERROR"
    print_status_group "$module" "RUNTIME_ERROR" "RUNTIME_ERROR"
    print_status_group "$module" "TIMEOUT" "TIMEOUT"
    print_status_group "$module" "INSPECT" "INSPECT"
    print_inspect_groups "$module"
}

print_single_result_summary() {
    local module="$1"
    local program="$2"
    local status_file="$RESULTS_DIR/${module}/${program}.status"
    local log_file="$RESULTS_DIR/${module}/${program}.log"
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    local reason_file="$RESULTS_DIR/${module}/${program}.reason"
    [ -f "$status_file" ] || return 0
    echo ""
    echo "--- Result Summary ---"
    echo "  Module: $module"
    echo "  Program: $program"
    echo "  Status: $(cat "$status_file")"
    if [ -f "$reason_file" ]; then
        echo "  Inspect Reason: $(cat "$reason_file")"
    fi
    if [ -f "$log_file" ] && [ -s "$log_file" ]; then
        echo "  Output Log: $log_file"
    fi
    if [ -f "$compile_log" ] && [ -s "$compile_log" ]; then
        echo "  Compile Log: $compile_log"
    fi
}

run_program() {
    local module="$1"
    local program="$2"
    local src="$PROGRAMS_DIR/$module/$program.cob"
    local bin="$NIST_WORKDIR/nist_${program}"
    local log="$RESULTS_DIR/${module}/${program}.log"
    local status_file="$RESULTS_DIR/${module}/${program}.status"
    local reason_file="$RESULTS_DIR/${module}/${program}.reason"
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    local print_file="$NIST_TMPDIR/P"
    local preprocessed="$NIST_WORKDIR/nist_preproc_${program}.cob"

    mkdir -p "$RESULTS_DIR/$module"
    rm -f "$status_file" "$reason_file" "$log" "$compile_log" "$bin" "$preprocessed"

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        echo "  $program: SKIP (source not found)"
        return
    fi

    rm -rf "$NIST_TMPDIR"/*
    NIST_TMPDIR="$NIST_TMPDIR" "$PREPROCESS" "$src" "$preprocessed"

    if ! $COBOLC "$preprocessed" -o "$bin" --source-format fixed --copy-path "$COPYLIB_DIR" \
        2>"$compile_log"; then
        echo "COMPILE_ERROR" > "$status_file"
        echo "  $program: COMPILE ERROR"
        return
    fi

    local exit_code=0
    setsid timeout -k 5s "$TIMEOUT_SECONDS" perl -e '
        chdir $ARGV[0] or die "chdir failed: $!";
        exec { $ARGV[1] } $ARGV[1] or die "exec failed: $!";
    ' "$NIST_WORKDIR" "$bin" < /dev/null > "$log" 2>&1 &
    CURRENT_RUN_PID=$!
    wait "$CURRENT_RUN_PID" || exit_code=$?
    CURRENT_RUN_PID=""

    if [ "$exit_code" -eq 124 ]; then
        echo "TIMEOUT" > "$status_file"
        echo "  $program: TIMEOUT (exceeded ${TIMEOUT_SECONDS}s)"
        return
    elif [ "$exit_code" -ne 0 ]; then
        echo "RUNTIME_ERROR" > "$status_file"
        echo "  $program: RUNTIME ERROR (exit $exit_code)"
        return
    fi

    local result_file="$log"
    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        result_file="$print_file"
        cp "$print_file" "$log" || true
    fi

    local pass fail ccvs_pass ccvs_failed ccvs_inspect
    pass=$(grep -ca " PASS " "$result_file" 2>/dev/null) || pass=0
    fail=$(grep -ca "FAIL\*" "$result_file" 2>/dev/null) || fail=0
    ccvs_pass=$(ccvs_summary_count "$result_file" 'TESTS WERE EXECUTED SUCCESSFULLY')
    ccvs_failed=$(ccvs_summary_count "$result_file" 'TEST\(S\) FAILED')
    ccvs_inspect=$(ccvs_summary_count "$result_file" 'TEST\(S\) REQUIRE INSPECTION')

    if [ "$ccvs_failed" -gt 0 ] || [ "$fail" -gt 0 ]; then
        echo "FAIL" > "$status_file"
        echo "  $program: FAIL ($ccvs_pass passed, $ccvs_failed failed)"
    elif [ "$ccvs_inspect" -gt 0 ]; then
        echo "INSPECT" > "$status_file"
        inspect_reason_for_program "$src" "$result_file" > "$reason_file"
        echo "  $program: INSPECT ($ccvs_inspect test(s) require inspection)"
    elif [ "$ccvs_pass" -gt 0 ]; then
        echo "PASS" > "$status_file"
        echo "  $program: PASS ($ccvs_pass passed)"
    elif [ "$pass" -gt 0 ] && [ "$fail" -eq 0 ]; then
        echo "PASS" > "$status_file"
        echo "  $program: PASS ($pass passed)"
    else
        echo "INSPECT" > "$status_file"
        inspect_reason_for_program "$src" "$result_file" > "$reason_file"
        echo "  $program: INSPECT (no decisive CCVS summary)"
    fi
}

run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local total=0 pass=0 fail=0 compile_err=0 runtime_err=0 timeout=0 skip=0
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
    echo "=== Module: $module ==="
    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program
        program="$(basename "$src" .cob)"
        total=$((total + 1))
        run_program "$module" "$program"
        case "$(cat "$RESULTS_DIR/$module/${program}.status")" in
            PASS) pass=$((pass + 1)) ;;
            FAIL) fail=$((fail + 1)) ;;
            COMPILE_ERROR) compile_err=$((compile_err + 1)) ;;
            RUNTIME_ERROR) runtime_err=$((runtime_err + 1)) ;;
            TIMEOUT) timeout=$((timeout + 1)) ;;
            SKIP) skip=$((skip + 1)) ;;
        esac
    done
    local tested=$((total - skip))
    local pass_rate=0
    if [ "$tested" -gt 0 ]; then
        pass_rate=$((pass * 100 / tested))
    fi
    echo ""
    echo "--- $module Summary ---"
    echo "  Total: $total | Tested: $tested | Pass: $pass | Fail: $fail"
    echo "  Compile Error: $compile_err | Runtime Error: $runtime_err | Timeout: $timeout"
    echo "  Pass Rate: ${pass_rate}%"
    print_module_diagnostics "$module"
    echo ""
    cat > "$RESULTS_DIR/${module}/summary.txt" <<EOF
Module: $module
Total: $total
Tested: $tested
Pass: $pass
Fail: $fail
Compile Error: $compile_err
Runtime Error: $runtime_err
Timeout: $timeout
Pass Rate: ${pass_rate}%
EOF
}

show_summary() {
    echo "=== NIST CCVS 85 — GnuCOBOL-style Summary ==="
    echo ""
    printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
        "Module" "Total" "Pass" "Fail" "CErr" "RErr" "Rate"
    printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
        "------" "------" "------" "------" "------" "------" "--------"
    local grand_total=0 grand_pass=0 grand_fail=0 grand_cerr=0 grand_rerr=0
    local module
    while IFS= read -r module; do
        local summary="$RESULTS_DIR/$module/summary.txt"
        [ -f "$summary" ] || continue
        local total pass fail cerr rerr rate
        total="$(awk '/^Total:/ {print $2}' "$summary")"
        pass="$(awk '/^Pass:/ {print $2}' "$summary")"
        fail="$(awk '/^Fail:/ {print $2}' "$summary")"
        cerr="$(awk '/^Compile Error:/ {print $3}' "$summary")"
        rerr="$(awk '/^Runtime Error:/ {print $3}' "$summary")"
        rate="$(awk '/^Pass Rate:/ {print $3}' "$summary")"
        printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
            "$module" "$total" "$pass" "$fail" "$cerr" "$rerr" "$rate"
        grand_total=$((grand_total + total))
        grand_pass=$((grand_pass + pass))
        grand_fail=$((grand_fail + fail))
        grand_cerr=$((grand_cerr + cerr))
        grand_rerr=$((grand_rerr + rerr))
    done < <(list_modules)
    printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
        "------" "------" "------" "------" "------" "------" "--------"
    local grand_rate=0
    if [ "$grand_total" -gt 0 ]; then
        grand_rate=$((grand_pass * 100 / grand_total))
    fi
    printf "%-6s %6d %6d %6d %6d %6d %7d%%\n" \
        "TOTAL" "$grand_total" "$grand_pass" "$grand_fail" \
        "$grand_cerr" "$grand_rerr" "$grand_rate"
}

if [ ! -d "$PROGRAMS_DIR" ]; then
    echo "Programs not prepared: $PROGRAMS_DIR" >&2
    echo "Run tests/nist/prepare.sh first." >&2
    exit 1
fi

case "${1:-}" in
    --all)
        while IFS= read -r module; do
            run_module "$module"
        done < <(list_modules)
        show_summary
        ;;
    --summary)
        show_summary
        ;;
    "")
        echo "Usage:"
        echo "  $0 <MODULE>"
        echo "  $0 <MODULE> <PROGRAM>"
        echo "  $0 --all"
        echo "  $0 --summary"
        ;;
    *)
        if [ -n "${2:-}" ]; then
            run_program "$1" "$2"
            print_single_result_summary "$1" "$2"
        else
            run_module "$1"
        fi
        ;;
esac
