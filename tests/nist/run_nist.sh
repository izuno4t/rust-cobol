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
COMM_FIXTURES_DIR="$SCRIPT_DIR/fixtures/comm"
JUDGES_DIR="$SCRIPT_DIR/judges"
COBOLC="${COBOLC:-cargo run --release --package cobol-driver --}"
NIST_WORK_ROOT="$ENV_ROOT/work/run"
NIST_TMPROOT="/tmp/nist/run"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
NIST_JOBS="${NIST_JOBS:-1}"
NIST_COMPILE_CACHE="${NIST_COMPILE_CACHE:-1}"
CURRENT_RUN_PID=""
COMPILER_SIGNATURE=""
COPYLIB_SIGNATURE=""

mkdir -p "$RESULTS_DIR" "$NIST_WORK_ROOT" "$NIST_TMPROOT"

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

prepare_print_file() {
    local print_file="$1"
    mkdir -p "$(dirname "$print_file")"
    rm -f "$print_file"
    : > "$print_file"
}

sha256_of_file() {
    shasum -a 256 "$1" | awk '{print $1}'
}

compute_compiler_signature() {
    if [ -n "$COMPILER_SIGNATURE" ]; then
        printf '%s\n' "$COMPILER_SIGNATURE"
        return
    fi
    if [ -x "$COBOLC" ] && [ "${COBOLC#* }" = "$COBOLC" ]; then
        COMPILER_SIGNATURE="bin:$(sha256_of_file "$COBOLC")"
    else
        COMPILER_SIGNATURE="cmd:$(printf '%s' "$COBOLC" | shasum -a 256 | awk '{print $1}')"
    fi
    printf '%s\n' "$COMPILER_SIGNATURE"
}

compute_copylib_signature() {
    if [ -n "$COPYLIB_SIGNATURE" ]; then
        printf '%s\n' "$COPYLIB_SIGNATURE"
        return
    fi
    if [ ! -d "$COPYLIB_DIR" ]; then
        COPYLIB_SIGNATURE="copylib:none"
        printf '%s\n' "$COPYLIB_SIGNATURE"
        return
    fi
    COPYLIB_SIGNATURE="$(
        find "$COPYLIB_DIR" -type f | LC_ALL=C sort | while IFS= read -r file; do
            printf '%s  %s\n' "$(sha256_of_file "$file")" "${file#$COPYLIB_DIR/}"
        done | shasum -a 256 | awk '{print "copylib:" $1}'
    )"
    printf '%s\n' "$COPYLIB_SIGNATURE"
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

ccvs_footer_error_count() {
    local file="$1"
    local value
    value=$(
        awk '
            /ERRORS ENCOUNTERED/ {
                for (i = 1; i <= NF; i++) {
                    if ($i == "NO") {
                        print 0
                        exit
                    }
                    if ($i ~ /^[0-9]+$/) {
                        print $i + 0
                        exit
                    }
                }
                print 1
                exit
            }
        ' "$file" 2>/dev/null | tail -n 1
    )
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '%s\n' ""
    fi
}

run_custom_judge() {
    local module="$1"
    local program="$2"
    local result_file="$3"
    local judge="$JUDGES_DIR/${program}.sh"
    [ -x "$judge" ] || return 1
    "$judge" "$module" "$program" "$result_file"
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
    for reason in subprogram-only manual-report dummy-display missing-fixture no-output unclassified; do
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

stage_nist_aliases() {
    local dst_dir="$1"
    mkdir -p "$dst_dir"
    local code src dst
    for code in \
        001 002 003 004 005 006 007 008 009 014 015 016 017 018 019 020 027 \
        051 052 053 054 055 056 057 058 059 060 063 064 068 069
    do
        src="$ENV_ROOT/XXXXX${code}"
        case "$code" in
            001) dst="$dst_dir/D1" ;;
            002) dst="$dst_dir/D2" ;;
            003) dst="$dst_dir/D3" ;;
            004) dst="$dst_dir/D4" ;;
            005) dst="$dst_dir/D5" ;;
            006) dst="$dst_dir/D6" ;;
            007) dst="$dst_dir/D7" ;;
            008) dst="$dst_dir/D8" ;;
            009) dst="$dst_dir/D9" ;;
            014) dst="$dst_dir/D14" ;;
            015) dst="$dst_dir/D15" ;;
            016) dst="$dst_dir/D16" ;;
            017) dst="$dst_dir/D17" ;;
            018) dst="$dst_dir/D18" ;;
            019) dst="$dst_dir/D19" ;;
            020) dst="$dst_dir/D20" ;;
            027) dst="$dst_dir/S1" ;;
            051) dst="$dst_dir/O51" ;;
            052) dst="$dst_dir/O52" ;;
            053) dst="$dst_dir/O53" ;;
            054) dst="$dst_dir/O54" ;;
            055) dst="$dst_dir/P" ;;
            056) dst="$dst_dir/O56" ;;
            057) dst="$dst_dir/O57" ;;
            058) dst="$dst_dir/O58" ;;
            059) dst="$dst_dir/O59" ;;
            060) dst="$dst_dir/O60" ;;
            063) dst="$dst_dir/D63" ;;
            064) dst="$dst_dir/D64" ;;
            068) dst="$dst_dir/O68" ;;
            069) dst="$dst_dir/O69" ;;
            *) continue ;;
        esac
        rm -f "$dst"
        if [ -e "$src" ]; then
            ln -s "$src" "$dst"
        fi
    done
}

find_missing_input_fixture() {
    local preprocessed="$1"
    awk '
        function clean_token(token) {
            sub(/\.$/, "", token)
            return token
        }

        {
            trimmed = $0
            sub(/^[0-9[:space:]]*/, "", trimmed)
            if (trimmed == "" || trimmed ~ /^\*/) {
                next
            }

            if (trimmed ~ /^SELECT /) {
                current_file = trimmed
                sub(/^SELECT /, "", current_file)
                sub(/ .*/, "", current_file)
                awaiting_assign_value = 0
                next
            }

            if (trimmed == "ASSIGN TO") {
                awaiting_assign_value = 1
                next
            }

            if (trimmed ~ /^ASSIGN TO ".*"\.$/) {
                if (current_file != "") {
                    path = trimmed
                    sub(/^ASSIGN TO "/, "", path)
                    sub(/".*$/, "", path)
                    assign_map[current_file] = path
                    current_file = ""
                }
                awaiting_assign_value = 0
                next
            }

            if (trimmed ~ /^".*"\.$/) {
                if (awaiting_assign_value && current_file != "") {
                    path = trimmed
                    sub(/^"/, "", path)
                    sub(/".*$/, "", path)
                    assign_map[current_file] = path
                    current_file = ""
                }
                awaiting_assign_value = 0
                next
            }

            if (trimmed ~ /^OPEN /) {
                awaiting_assign_value = 0
                n = split(trimmed, parts, /[[:space:]]+/)
                mode = ""
                for (i = 1; i <= n; i++) {
                    token = parts[i]
                    if (token == "OPEN") {
                        continue
                    }
                    if (token == "INPUT" || token == "I-O" || token == "EXTEND" || token == "OUTPUT") {
                        mode = token
                        continue
                    }
                    token = clean_token(token)
                    if (mode == "INPUT" || mode == "I-O" || mode == "EXTEND") {
                        path = assign_map[token]
                        if (path != "" && system("[ -e \"" path "\" ]") != 0) {
                            print token "|" path
                            exit 0
                        }
                    }
                }
                next
            }

            awaiting_assign_value = 0
        }
    ' "$preprocessed"
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
    local module_workdir="$NIST_WORK_ROOT/$module"
    local bin="$module_workdir/nist_${program}"
    local log="$RESULTS_DIR/${module}/${program}.log"
    local status_file="$RESULTS_DIR/${module}/${program}.status"
    local reason_file="$RESULTS_DIR/${module}/${program}.reason"
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    local compile_meta="$RESULTS_DIR/${module}/${program}.compile.meta"
    local program_tmpdir="$NIST_TMPROOT/${module}_${program}"
    local print_file="$program_tmpdir/P"
    local preprocessed="$module_workdir/nist_preproc_${program}.cob"
    local comm_script="$COMM_FIXTURES_DIR/${program}.comm"

    mkdir -p "$RESULTS_DIR/$module"
    mkdir -p "$module_workdir"
    rm -f "$status_file" "$reason_file" "$log" "$preprocessed"

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        echo "  $program: SKIP (source not found)"
        return
    fi

    rm -rf "$program_tmpdir"
    mkdir -p "$program_tmpdir"
    NIST_TMPDIR="$program_tmpdir" "$PREPROCESS" "$src" "$preprocessed"
    stage_nist_aliases "$program_tmpdir"
    prepare_print_file "$print_file"

    local missing_fixture=""
    missing_fixture="$(find_missing_input_fixture "$preprocessed" || true)"
    if [ -n "$missing_fixture" ]; then
        echo "INSPECT" > "$status_file"
        printf '%s\n' "missing-fixture" > "$reason_file"
        echo "  $program: INSPECT (missing input fixture: ${missing_fixture#*|})"
        return
    fi

    local compile_cache_key=""
    local compile_cache_hit=0
    if [ "$NIST_COMPILE_CACHE" != "0" ]; then
        compile_cache_key="$(
            printf 'source:%s\ncompiler:%s\n%s\nformat:fixed\n' \
                "$(sha256_of_file "$preprocessed")" \
                "$(compute_compiler_signature)" \
                "$(compute_copylib_signature)"
        )"
        if [ -x "$bin" ] && [ -f "$compile_meta" ] && \
            [ "$(cat "$compile_meta")" = "$compile_cache_key" ]; then
            compile_cache_hit=1
            if [ ! -f "$compile_log" ]; then
                printf 'compile cache hit\n' > "$compile_log"
            fi
        fi
    fi

    if [ "$compile_cache_hit" -eq 0 ]; then
        rm -f "$bin" "$compile_log" "$compile_meta"
        if ! $COBOLC "$preprocessed" -o "$bin" --source-format fixed --copy-path "$COPYLIB_DIR" \
            2>"$compile_log"; then
            echo "COMPILE_ERROR" > "$status_file"
            echo "  $program: COMPILE ERROR"
            return
        fi
        if [ -n "$compile_cache_key" ]; then
            printf '%s\n' "$compile_cache_key" > "$compile_meta"
        fi
    fi

    local exit_code=0
    (
        if [ -f "$comm_script" ]; then
            export COBOL_COMM_SCRIPT="$comm_script"
        else
            unset COBOL_COMM_SCRIPT || true
        fi
        exec setsid timeout -k 5s "$TIMEOUT_SECONDS" perl -e '
            chdir $ARGV[0] or die "chdir failed: $!";
            exec { $ARGV[1] } $ARGV[1] or die "exec failed: $!";
        ' "$module_workdir" "$bin"
    ) < /dev/null > "$log" 2>&1 &
    CURRENT_RUN_PID=$!
    wait "$CURRENT_RUN_PID" || exit_code=$?
    CURRENT_RUN_PID=""

    if [ "$exit_code" -eq 124 ]; then
        local inspect_reason
        inspect_reason="$(inspect_reason_for_program "$src" "$log")"
        if [ "$inspect_reason" = "manual-report" ]; then
            echo "FAIL" > "$status_file"
            echo "  $program: FAIL (manual-report timed out waiting for external interaction)"
            return
        fi
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

    local pass fail ccvs_pass ccvs_failed ccvs_inspect footer_errors judge_output judge_status
    pass=$(grep -ca " PASS " "$result_file" 2>/dev/null) || pass=0
    fail=$(grep -ca "FAIL\*" "$result_file" 2>/dev/null) || fail=0
    ccvs_pass=$(ccvs_summary_count "$result_file" 'TESTS WERE EXECUTED SUCCESSFULLY')
    ccvs_failed=$(ccvs_summary_count "$result_file" 'TEST\(S\) FAILED')
    ccvs_inspect=$(ccvs_summary_count "$result_file" 'TEST\(S\) REQUIRE INSPECTION')
    footer_errors="$(ccvs_footer_error_count "$result_file")"

    if judge_output="$(run_custom_judge "$module" "$program" "$result_file" 2>/dev/null)"; then
        judge_status="${judge_output%%|*}"
        case "$judge_status" in
            PASS|FAIL|INSPECT)
                echo "$judge_status" > "$status_file"
                if [ "$judge_status" = "INSPECT" ]; then
                    printf '%s\n' "${judge_output#*|}" > "$reason_file"
                    echo "  $program: INSPECT (${judge_output#*|})"
                elif [ "$judge_output" = "$judge_status" ]; then
                    echo "  $program: $judge_status"
                else
                    echo "  $program: $judge_status (${judge_output#*|})"
                fi
                return
                ;;
        esac
    fi

    if [ "$ccvs_failed" -gt 0 ] || [ "$fail" -gt 0 ]; then
        echo "FAIL" > "$status_file"
        echo "  $program: FAIL ($ccvs_pass passed, $ccvs_failed failed)"
    elif [ -n "$footer_errors" ] && [ "$footer_errors" -gt 0 ]; then
        echo "FAIL" > "$status_file"
        echo "  $program: FAIL ($footer_errors error(s) reported in footer)"
    elif [ -n "$footer_errors" ] && [ "$footer_errors" -eq 0 ]; then
        echo "PASS" > "$status_file"
        echo "  $program: PASS (0 errors reported in footer)"
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
        local inspect_reason
        inspect_reason="$(inspect_reason_for_program "$src" "$result_file")"
        case "$inspect_reason" in
            manual-report)
                echo "FAIL" > "$status_file"
                echo "  $program: FAIL (manual-report produced no decisive summary)"
                ;;
            no-output|subprogram-only)
                echo "PASS" > "$status_file"
                echo "  $program: PASS (completed without report output)"
                ;;
            *)
                echo "INSPECT" > "$status_file"
                printf '%s\n' "$inspect_reason" > "$reason_file"
                echo "  $program: INSPECT (no decisive CCVS summary)"
                ;;
        esac
    fi
}

run_all_modules() {
    local jobs="$1"
    local module
    local batch_modules=()
    local batch_pids=()
    local batch_logs=()
    local module_log

    flush_batch() {
        local i
        for i in "${!batch_pids[@]}"; do
            wait "${batch_pids[$i]}"
        done
        for i in "${!batch_logs[@]}"; do
            cat "${batch_logs[$i]}"
            rm -f "${batch_logs[$i]}"
        done
        batch_modules=()
        batch_pids=()
        batch_logs=()
    }

    while IFS= read -r module; do
        if [ "$jobs" -le 1 ]; then
            run_module "$module"
            continue
        fi
        module_log="$RESULTS_DIR/.module_${module}.out"
        rm -f "$module_log"
        run_module "$module" >"$module_log" 2>&1 &
        batch_modules+=("$module")
        batch_pids+=("$!")
        batch_logs+=("$module_log")
        if [ "${#batch_pids[@]}" -ge "$jobs" ]; then
            flush_batch
        fi
    done < <(list_modules)

    if [ "${#batch_pids[@]}" -gt 0 ]; then
        flush_batch
    fi
}

run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local total=0 pass=0 fail=0 compile_err=0 runtime_err=0 timeout_count=0 skip=0
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
            TIMEOUT) timeout_count=$((timeout_count + 1)) ;;
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
    echo "  Compile Error: $compile_err | Runtime Error: $runtime_err | Timeout: $timeout_count"
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
Timeout: $timeout_count
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
        run_all_modules "$NIST_JOBS"
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
