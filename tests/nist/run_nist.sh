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
NIST_TMP_ROOT="/tmp/nc85"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
NIST_JOBS="${NIST_JOBS:-3}"
NIST_COMPILE_CACHE="${NIST_COMPILE_CACHE:-1}"
CURRENT_RUN_PID=""
COMPILER_SIGNATURE=""
COPYLIB_SIGNATURE=""

mkdir -p "$RESULTS_DIR" "$NIST_WORK_ROOT" "$NIST_TMP_ROOT"

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
    local source_reason
    source_reason="$(source_reason_for_program "$src")"
    if [ -n "$source_reason" ]; then
        printf '%s\n' "$source_reason"
    elif [ ! -s "$result_file" ] || ! grep -q '[^[:space:]]' "$result_file" 2>/dev/null; then
        printf 'no-output\n'
    else
        printf 'unclassified\n'
    fi
}

source_reason_for_program() {
    local src="$1"
    if [ -f "$src" ] && grep -Eq '^[0-9[:space:]]*PROCEDURE DIVISION USING' "$src"; then
        printf 'subprogram-only\n'
    elif [ -f "$src" ] && grep -Eq 'MOVE "INSPT" TO P-OR-F|INSPECT-COUNTER' "$src"; then
        printf 'manual-report\n'
    elif [ -f "$src" ] && grep -Eq 'DUMMY PROCEDURE|DUMMY PARAGRAPH' "$src"; then
        printf 'dummy-display\n'
    else
        printf '%s\n' ""
    fi
}

program_timeout_seconds() {
    local module="$1"
    local program="$2"
    local src="$3"
    local source_reason
    source_reason="$(source_reason_for_program "$src")"
    case "$source_reason" in
        manual-report)
            printf '%s\n' "${NIST_TIMEOUT_MANUAL_REPORT:-5}"
            return
            ;;
    esac
    case "$module/$program" in
        EX/EXEC85)
            printf '%s\n' "${NIST_TIMEOUT_EXEC85:-20}"
            ;;
        *)
            printf '%s\n' "$TIMEOUT_SECONDS"
            ;;
    esac
}

program_parallel_mode() {
    local _module="$1"
    local _program="$2"
    local _src="$3"
    printf 'parallel\n'
}

prepare_print_file() {
    local print_file="$1"
    mkdir -p "$(dirname "$print_file")"
    rm -f "$print_file"
    : > "$print_file"
}

sha256_of_file() {
    shasum -a 256 "$1" | perl -ne 'if (/^([0-9a-fA-F]+)/) { print "$1\n"; exit }'
}

sha256_of_stdin() {
    shasum -a 256 | perl -ne 'if (/^([0-9a-fA-F]+)/) { print "$1\n"; exit }'
}

compute_compiler_signature() {
    if [ -n "$COMPILER_SIGNATURE" ]; then
        printf '%s\n' "$COMPILER_SIGNATURE"
        return
    fi
    if [ -x "$COBOLC" ] && [ "${COBOLC#* }" = "$COBOLC" ]; then
        COMPILER_SIGNATURE="bin:$(sha256_of_file "$COBOLC")"
    else
        COMPILER_SIGNATURE="cmd:$(printf '%s' "$COBOLC" | sha256_of_stdin)"
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
        done | sha256_of_stdin | perl -ne 'chomp; print "copylib:$_\n"; exit'
    )"
    printf '%s\n' "$COPYLIB_SIGNATURE"
}

compute_preprocess_signature() {
    local src="$1"
    printf 'source:%s\npreprocess:%s\n' \
        "$(sha256_of_file "$src")" \
        "$(sha256_of_file "$PREPROCESS")"
}

ccvs_summary_count() {
    local file="$1"
    local pattern="$2"
    local value
    value=$(
        perl -ne '
            our $pat;
            BEGIN { $pat = shift @ARGV; }
            if (/$pat/) {
                my @fields = split " ", $_;
                if (@fields) {
                    print(($fields[0] eq "NO" ? 0 : $fields[0]) . "\n");
                }
            }
        ' "$pattern" "$file" | tail -n 1
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
        perl -ne '
            next unless /ERRORS ENCOUNTERED/;
            my @fields = split " ", $_;
            for my $field (@fields) {
                if ($field eq "NO") {
                    print "0\n";
                    exit;
                }
                if ($field =~ /^[0-9]+$/) {
                    print "$field\n";
                    exit;
                }
            }
            print "1\n";
            exit;
        ' "$file" 2>/dev/null | tail -n 1
    )
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '%s\n' ""
    fi
}

expected_flag_count() {
    local src="$1"
    local value
    value=$(
        perl -ne '
            next unless /TOTAL NUMBER OF FLAGS EXPECTED\s*=/;
            my @fields = split " ", $_;
            for my $field (@fields) {
                if ($field =~ /^[0-9]+\.?$/) {
                    $field =~ s/\.//g;
                    print "$field\n";
                    exit;
                }
            }
        ' "$src" 2>/dev/null | tail -n 1
    )
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '0\n'
    fi
}

compile_warning_count() {
    local file="$1"
    if [ ! -f "$file" ]; then
        printf '0\n'
        return
    fi
    local count
    count="$(grep -c 'COB[C]-W' "$file" 2>/dev/null || true)"
    if [ -n "$count" ]; then
        printf '%s\n' "$count"
    else
        printf '0\n'
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
    perl -ne '
        sub clean_token {
            my ($token) = @_;
            $token =~ s/\.$//;
            return $token;
        }

        my $trimmed = $_;
        $trimmed =~ s/^[0-9[:space:]]*//;
        next if $trimmed eq "" || $trimmed =~ /^\*/;

        if ($trimmed =~ /^SELECT /) {
            $current_file = $trimmed;
            $current_file =~ s/^SELECT //;
            $current_file =~ s/ .*//;
            $awaiting_assign_value = 0;
            next;
        }

        if ($trimmed eq "ASSIGN TO") {
            $awaiting_assign_value = 1;
            next;
        }

        if ($trimmed =~ /^ASSIGN TO ".*"\.$/) {
            if ($current_file ne "") {
                my $path = $trimmed;
                $path =~ s/^ASSIGN TO "//;
                $path =~ s/".*$//;
                $assign_map{$current_file} = $path;
                $current_file = "";
            }
            $awaiting_assign_value = 0;
            next;
        }

        if ($trimmed =~ /^".*"\.$/) {
            if ($awaiting_assign_value && $current_file ne "") {
                my $path = $trimmed;
                $path =~ s/^"//;
                $path =~ s/".*$//;
                $assign_map{$current_file} = $path;
                $current_file = "";
            }
            $awaiting_assign_value = 0;
            next;
        }

        if ($trimmed =~ /^OPEN /) {
            $awaiting_assign_value = 0;
            my @parts = split /\s+/, $trimmed;
            my $mode = "";
            for my $token (@parts) {
                next if $token eq "OPEN";
                if ($token eq "INPUT" || $token eq "I-O" || $token eq "EXTEND" || $token eq "OUTPUT") {
                    $mode = $token;
                    next;
                }
                $token = clean_token($token);
                if (($mode eq "INPUT" || $mode eq "I-O" || $mode eq "EXTEND")
                    && exists $assign_map{$token}
                    && $assign_map{$token} ne ""
                    && !-e $assign_map{$token}) {
                    print "$token|$assign_map{$token}\n";
                    exit 0;
                }
            }
            next;
        }

        $awaiting_assign_value = 0;
    ' "$preprocessed"
}

summary_value() {
    local summary="$1"
    local label="$2"
    perl -ne '
        our $label;
        BEGIN { $label = shift @ARGV; }
        if (/^\Q$label\E\s*(\S+)/) {
            print "$1\n";
            exit;
        }
    ' "$label" "$summary"
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

module_has_status() {
    local module="$1"
    local expected="$2"
    local status_file
    for status_file in "$RESULTS_DIR/$module"/*.status; do
        [ -f "$status_file" ] || continue
        if [ "$(cat "$status_file")" = "$expected" ]; then
            return 0
        fi
    done
    return 1
}

summarize_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local total=0 pass=0 fail=0 compile_ready=0 compile_err=0 runtime_err=0 timeout_count=0 skip=0
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program
        program="$(basename "$src" .cob)"
        total=$((total + 1))
        case "$(cat "$RESULTS_DIR/$module/${program}.status" 2>/dev/null || printf 'SKIP')" in
            PASS) pass=$((pass + 1)) ;;
            FAIL|INSPECT) fail=$((fail + 1)) ;;
            COMPILED) compile_ready=$((compile_ready + 1)) ;;
            COMPILE_ERROR) compile_err=$((compile_err + 1)) ;;
            RUNTIME_ERROR) runtime_err=$((runtime_err + 1)) ;;
            TIMEOUT) timeout_count=$((timeout_count + 1)) ;;
            SKIP) skip=$((skip + 1)) ;;
        esac
    done
    local tested=$((total - skip - compile_ready))
    local pass_rate=0
    if [ "$tested" -gt 0 ]; then
        pass_rate=$((pass * 100 / tested))
    fi
    echo ""
    echo "--- $module Summary ---"
    echo "  Total: $total | Tested: $tested | Pass: $pass | Fail: $fail"
    echo "  Compile Ready: $compile_ready | Compile Error: $compile_err | Runtime Error: $runtime_err | Timeout: $timeout_count"
    echo "  Pass Rate: ${pass_rate}%"
    print_module_diagnostics "$module"
    echo ""
    cat > "$RESULTS_DIR/${module}/summary.txt" <<EOF
Module: $module
Total: $total
Tested: $tested
Pass: $pass
Fail: $fail
Compile Ready: $compile_ready
Compile Error: $compile_err
Runtime Error: $runtime_err
Timeout: $timeout_count
Pass Rate: ${pass_rate}%
EOF
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
    local mode="${3:-full}"
    local src="$PROGRAMS_DIR/$module/$program.cob"
    local module_workdir="$NIST_WORK_ROOT/$module"
    local program_workdir="$module_workdir/$program"
    local bin="$program_workdir/nist_${program}"
    local log="$RESULTS_DIR/${module}/${program}.log"
    local status_file="$RESULTS_DIR/${module}/${program}.status"
    local reason_file="$RESULTS_DIR/${module}/${program}.reason"
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    local compile_meta="$RESULTS_DIR/${module}/${program}.compile.meta"
    local preprocess_meta="$RESULTS_DIR/${module}/${program}.preprocess.meta"
    local fixture_meta="$RESULTS_DIR/${module}/${program}.fixture.meta"
    local fixture_result="$RESULTS_DIR/${module}/${program}.fixture.result"
    local program_tmpdir="$NIST_TMP_ROOT/${module}/${program}"
    local print_file="$program_tmpdir/P"
    local preprocessed="$program_workdir/nist_preproc_${program}.cob"
    local comm_script="$COMM_FIXTURES_DIR/${program}.comm"
    local runtime_timeout="$TIMEOUT_SECONDS"

    mkdir -p "$RESULTS_DIR/$module"
    mkdir -p "$module_workdir"
    mkdir -p "$program_workdir"
    if [ "$mode" != "run_only" ]; then
        rm -f "$status_file" "$reason_file" "$log"
    else
        rm -f "$reason_file" "$log"
    fi

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        if [ "$mode" != "compile_only" ]; then
            echo "  $program: SKIP (source not found)"
        fi
        return
    fi

    if [ "$mode" != "run_only" ]; then
        rm -rf "$program_tmpdir"
        mkdir -p "$program_tmpdir"
        local preprocess_key
        preprocess_key="$(compute_preprocess_signature "$src")"
        if [ ! -f "$preprocessed" ] || [ ! -f "$preprocess_meta" ] || \
            [ "$(cat "$preprocess_meta")" != "$preprocess_key" ]; then
            rm -f "$preprocessed" "$preprocess_meta" "$fixture_meta" "$fixture_result"
            NIST_TMPDIR="$program_tmpdir" "$PREPROCESS" "$src" "$preprocessed"
            printf '%s\n' "$preprocess_key" > "$preprocess_meta"
        fi
        stage_nist_aliases "$program_tmpdir"
        prepare_print_file "$print_file"

        local missing_fixture=""
        local fixture_key
        fixture_key="preprocessed:$(sha256_of_file "$preprocessed")"
        if [ -f "$fixture_meta" ] && [ -f "$fixture_result" ] && \
            [ "$(cat "$fixture_meta")" = "$fixture_key" ]; then
            missing_fixture="$(cat "$fixture_result")"
        else
            missing_fixture="$(find_missing_input_fixture "$preprocessed" || true)"
            printf '%s\n' "$fixture_key" > "$fixture_meta"
            printf '%s\n' "$missing_fixture" > "$fixture_result"
        fi
        if [ -n "$missing_fixture" ]; then
            echo "INSPECT" > "$status_file"
            printf '%s\n' "missing-fixture" > "$reason_file"
            if [ "$mode" != "compile_only" ]; then
                echo "  $program: INSPECT (missing input fixture: ${missing_fixture#*|})"
            fi
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
                2>"$compile_log" || grep -q 'COB[C]-E' "$compile_log"; then
                # Check if binary was still created (non-fatal errors) and judge exists
                if [ -x "$bin" ]; then
                    local judge_output judge_status
                    if judge_output="$(run_custom_judge "$module" "$program" "$compile_log" 2>/dev/null)"; then
                        judge_status="${judge_output%%|*}"
                        if [ "$judge_status" != "PASS" ] && [ "$judge_status" != "FAIL" ]; then
                            echo "COMPILE_ERROR" > "$status_file"
                            echo "  $program: COMPILE ERROR"
                            return
                        fi
                    else
                        echo "COMPILE_ERROR" > "$status_file"
                        echo "  $program: COMPILE ERROR"
                        return
                    fi
                else
                    echo "COMPILE_ERROR" > "$status_file"
                    echo "  $program: COMPILE ERROR"
                    return
                fi
            fi
            if [ -n "$compile_cache_key" ]; then
                printf '%s\n' "$compile_cache_key" > "$compile_meta"
            fi
        fi

        if [ "$mode" = "compile_only" ]; then
            echo "COMPILED" > "$status_file"
            return
        fi
    elif [ ! -x "$bin" ]; then
        echo "COMPILE_ERROR" > "$status_file"
        echo "  $program: COMPILE ERROR (missing compiled binary)"
        return
    fi

    if [ "$mode" = "run_only" ]; then
        rm -rf "$program_tmpdir"
        mkdir -p "$program_tmpdir"
        stage_nist_aliases "$program_tmpdir"
        prepare_print_file "$print_file"
    fi

    runtime_timeout="$(program_timeout_seconds "$module" "$program" "$src")"

    local exit_code=0
    (
        if [ -f "$comm_script" ]; then
            export COBOL_COMM_SCRIPT="$comm_script"
        else
            unset COBOL_COMM_SCRIPT || true
        fi
        if [ "$module" = "CM" ]; then
            export COBOL_TEST_FAST_TIME_SCALE="${COBOL_TEST_FAST_TIME_SCALE:-100000}"
        else
            unset COBOL_TEST_FAST_TIME_SCALE || true
        fi
        if command -v setsid >/dev/null 2>&1; then
            exec setsid timeout -k 5s "$runtime_timeout" perl -e '
                chdir $ARGV[0] or die "chdir failed: $!";
                exec { $ARGV[1] } $ARGV[1] or die "exec failed: $!";
            ' "$program_tmpdir" "$bin"
        else
            exec timeout -k 5s "$runtime_timeout" perl -e '
                chdir $ARGV[0] or die "chdir failed: $!";
                exec { $ARGV[1] } $ARGV[1] or die "exec failed: $!";
            ' "$program_tmpdir" "$bin"
        fi
    ) < /dev/null > "$log" 2>&1 &
    CURRENT_RUN_PID=$!
    wait "$CURRENT_RUN_PID" || exit_code=$?
    CURRENT_RUN_PID=""

    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        cp "$print_file" "$log" || true
    fi

    if [ "$exit_code" -eq 124 ]; then
        # Check for custom judge first — some tests legitimately time out
        # (e.g., communication tests, subprogram tests) but should be PASS.
        local judge_output judge_status
        if judge_output="$(run_custom_judge "$module" "$program" "$log" 2>/dev/null)"; then
            judge_status="${judge_output%%|*}"
            case "$judge_status" in
                PASS|FAIL|INSPECT)
                    echo "$judge_status" > "$status_file"
                    echo "  $program: $judge_status (judge override for timeout)"
                    return
                    ;;
            esac
        fi
        local inspect_reason
        inspect_reason="$(inspect_reason_for_program "$src" "$log")"
        if [ "$inspect_reason" = "manual-report" ]; then
            echo "FAIL" > "$status_file"
            echo "  $program: FAIL (manual-report timed out waiting for external interaction)"
            return
        fi
        echo "TIMEOUT" > "$status_file"
        echo "  $program: TIMEOUT (exceeded ${runtime_timeout}s)"
        return
    elif [ "$exit_code" -ne 0 ]; then
        # Check for custom judge first for runtime errors too
        local judge_output judge_status
        if judge_output="$(run_custom_judge "$module" "$program" "$log" 2>/dev/null)"; then
            judge_status="${judge_output%%|*}"
            case "$judge_status" in
                PASS|FAIL|INSPECT)
                    echo "$judge_status" > "$status_file"
                    echo "  $program: $judge_status (judge override for exit $exit_code)"
                    return
                    ;;
            esac
        fi
        echo "RUNTIME_ERROR" > "$status_file"
        echo "  $program: RUNTIME ERROR (exit $exit_code)"
        return
    fi

    # Copy print file to log first so judges and CCVS parsing see the same data.
    local result_file="$log"
    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        result_file="$print_file"
        cp "$print_file" "$log" || true
    fi

    # Check for custom judge.
    local judge_output judge_status
    if judge_output="$(run_custom_judge "$module" "$program" "$log" 2>/dev/null)"; then
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

    local pass fail ccvs_pass ccvs_failed ccvs_inspect footer_errors
    pass=$(grep -ca " PASS " "$result_file" 2>/dev/null) || pass=0
    fail=$(grep -ca "FAIL\*" "$result_file" 2>/dev/null) || fail=0
    ccvs_pass=$(ccvs_summary_count "$result_file" 'TESTS WERE EXECUTED SUCCESSFULLY')
    ccvs_failed=$(ccvs_summary_count "$result_file" 'TEST\(S\) FAILED')
    ccvs_inspect=$(ccvs_summary_count "$result_file" 'TEST\(S\) REQUIRE INSPECTION')
    footer_errors="$(ccvs_footer_error_count "$result_file")"

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
        local inspect_reason expected_flags warning_count
        expected_flags="$(expected_flag_count "$src")"
        warning_count="$(compile_warning_count "$compile_log")"
        if [ "$expected_flags" -gt 0 ]; then
            if [ "$warning_count" -eq "$expected_flags" ]; then
                echo "PASS" > "$status_file"
                echo "  $program: PASS ($warning_count warning flag(s) matched expected count)"
            else
                echo "FAIL" > "$status_file"
                echo "  $program: FAIL (expected $expected_flags warning flag(s), got $warning_count)"
            fi
            return
        fi

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
            dummy-display)
                expected_flags="$(expected_flag_count "$src")"
                warning_count="$(compile_warning_count "$compile_log")"
                if [ "$expected_flags" -gt 0 ] && [ "$warning_count" -eq "$expected_flags" ]; then
                    echo "PASS" > "$status_file"
                    echo "  $program: PASS ($warning_count warning flag(s) matched expected count)"
                else
                    echo "FAIL" > "$status_file"
                    echo "  $program: FAIL (expected $expected_flags warning flag(s), got $warning_count)"
                fi
                ;;
            *)
                echo "INSPECT" > "$status_file"
                printf '%s\n' "$inspect_reason" > "$reason_file"
                echo "  $program: INSPECT (no decisive CCVS summary)"
                ;;
        esac
    fi
}

run_compile_phase() {
    local jobs="$1"
    shift
    local modules=("$@")
    local module
    local batch_pids=()
    local batch_logs=()

    flush_compile_batch() {
        local i
        for i in "${!batch_pids[@]}"; do
            wait "${batch_pids[$i]}"
        done
        for i in "${!batch_logs[@]}"; do
            if [ -s "${batch_logs[$i]}" ]; then
                cat "${batch_logs[$i]}"
            fi
            rm -f "${batch_logs[$i]}"
        done
        batch_pids=()
        batch_logs=()
    }

    for module in "${modules[@]}"; do
        echo "=== Module: $module (compile) ==="
        if [ "$jobs" -le 1 ]; then
            compile_module "$module"
            continue
        fi
        local module_log="$RESULTS_DIR/.compile_${module}.out"
        rm -f "$module_log"
        (
            compile_module "$module"
        ) >"$module_log" 2>&1 &
        batch_pids+=("$!")
        batch_logs+=("$module_log")
        if [ "${#batch_pids[@]}" -ge "$jobs" ]; then
            flush_compile_batch
        fi
    done

    if [ "${#batch_pids[@]}" -gt 0 ]; then
        flush_compile_batch
    fi
}

run_execute_phase() {
    local jobs="$1"
    shift
    local modules=("$@")
    local module
    local execute_pids=()
    local execute_logs=()

    flush_execute_batch() {
        local i
        for i in "${!execute_pids[@]}"; do
            wait "${execute_pids[$i]}"
        done
        for i in "${!execute_logs[@]}"; do
            if [ -s "${execute_logs[$i]}" ]; then
                cat "${execute_logs[$i]}"
            fi
            rm -f "${execute_logs[$i]}"
        done
        execute_pids=()
        execute_logs=()
    }

    for module in "${modules[@]}"; do
        echo "=== Module: $module (execute) ==="
    done

    for module in "${modules[@]}"; do
        local mod_dir="$PROGRAMS_DIR/$module"
        local src program program_log
        [ -d "$mod_dir" ] || continue
        for src in "$mod_dir"/*.cob; do
            [ -f "$src" ] || continue
            program="$(basename "$src" .cob)"
            if [ "$(cat "$RESULTS_DIR/$module/${program}.status" 2>/dev/null || printf 'SKIP')" != "COMPILED" ]; then
                continue
            fi
            if [ "$jobs" -le 1 ]; then
                run_program "$module" "$program" "run_only"
                continue
            fi
            program_log="$(mktemp "$NIST_TMP_ROOT/${module}_${program}.execute.XXXXXX.log")"
            (
                run_program "$module" "$program" "run_only"
            ) >"$program_log" 2>&1 &
            execute_pids+=("$!")
            execute_logs+=("$program_log")
            if [ "${#execute_pids[@]}" -ge "$jobs" ]; then
                flush_execute_batch
            fi
        done
    done

    if [ "${#execute_pids[@]}" -gt 0 ]; then
        flush_execute_batch
    fi
}

run_collect_phase() {
    local modules=("$@")
    local module
    for module in "${modules[@]}"; do
        summarize_module "$module"
    done
}

run_all_modules() {
    local jobs="$1"
    local module
    local modules=()
    local had_compile_error=0
    while IFS= read -r module; do
        modules+=("$module")
    done < <(list_modules)

    run_compile_phase "$jobs" "${modules[@]}"

    for module in "${modules[@]}"; do
        if module_has_status "$module" "COMPILE_ERROR"; then
            had_compile_error=1
        fi
    done

    if [ "$had_compile_error" -ne 0 ]; then
        echo ""
        echo "Compile phase failed. Execution phase skipped."
        run_collect_phase "${modules[@]}"
        return
    fi

    run_execute_phase "$jobs" "${modules[@]}"
    run_collect_phase "${modules[@]}"
}

run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
    run_compile_phase "$NIST_JOBS" "$module"
    if module_has_status "$module" "COMPILE_ERROR"; then
        echo ""
        echo "Compile phase failed. Execution phase skipped for module $module."
        run_collect_phase "$module"
        return
    fi
    run_execute_phase "$NIST_JOBS" "$module"
    run_collect_phase "$module"
}

compile_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    [ -d "$mod_dir" ] || return
    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program
        program="$(basename "$src" .cob)"
        run_program "$module" "$program" "compile_only"
    done
}

execute_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local jobs="$NIST_JOBS"
    local -a batch_pids=()
    local -a batch_logs=()
    [ -d "$mod_dir" ] || return

    flush_execute_batch() {
        local idx pid program_log
        for idx in "${!batch_pids[@]}"; do
            pid="${batch_pids[$idx]}"
            program_log="${batch_logs[$idx]}"
            wait "$pid"
            cat "$program_log"
            rm -f "$program_log"
        done
        batch_pids=()
        batch_logs=()
    }

    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program parallel_mode program_log
        program="$(basename "$src" .cob)"
        if [ "$(cat "$RESULTS_DIR/$module/${program}.status" 2>/dev/null || printf 'SKIP')" != "COMPILED" ]; then
            continue
        fi
        parallel_mode="$(program_parallel_mode "$module" "$program" "$src")"
        if [ "$jobs" -le 1 ] || [ "$parallel_mode" != "parallel" ]; then
            flush_execute_batch
            run_program "$module" "$program" "run_only"
            continue
        fi
        program_log="$(mktemp "$NIST_TMP_ROOT/${module}_${program}.execute.XXXXXX.log")"
        (
            run_program "$module" "$program" "run_only"
        ) >"$program_log" 2>&1 &
        batch_pids+=("$!")
        batch_logs+=("$program_log")
        if [ "${#batch_pids[@]}" -ge "$jobs" ]; then
            flush_execute_batch
        fi
    done
    flush_execute_batch
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
        total="$(summary_value "$summary" 'Total:')"
        pass="$(summary_value "$summary" 'Pass:')"
        fail="$(summary_value "$summary" 'Fail:')"
        cerr="$(summary_value "$summary" 'Compile Error:')"
        rerr="$(summary_value "$summary" 'Runtime Error:')"
        rate="$(summary_value "$summary" 'Pass Rate:')"
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
