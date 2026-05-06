#!/usr/bin/env bash
# run_nist.sh — Run NIST CCVS 85 with GnuCOBOL-style judgment rules.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/.nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
RESULTS_DIR="$ENV_ROOT/results"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"
PREPROCESS="$SCRIPT_DIR/preprocess.sh"
COMM_FIXTURES_DIR="$SCRIPT_DIR/fixtures/comm"
VERIFIERS_DIR="$SCRIPT_DIR/verifiers"
VERIFIER_OVERRIDES_DIR="$VERIFIERS_DIR/overrides"
COBOLC="${COBOLC:-cargo run --release --package cobol-driver --}"
NIST_WORK_ROOT="$ENV_ROOT/work/run"
NIST_TMP_ROOT="/tmp/nc85"
NIST_TOOLCHAIN_ROOT="$ENV_ROOT/toolchain"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
NIST_JOBS="${NIST_JOBS:-5}"
NIST_COMPILE_CACHE="${NIST_COMPILE_CACHE:-1}"
NIST_TRACE_PARAGRAPHS="${NIST_TRACE_PARAGRAPHS:-0}"
CURRENT_RUN_PID=""
COMPILER_SIGNATURE=""
COPYLIB_SIGNATURE=""
SNAPSHOT_COBOLC=""

mkdir -p "$RESULTS_DIR" "$NIST_WORK_ROOT" "$NIST_TMP_ROOT" "$NIST_TOOLCHAIN_ROOT"

. "$VERIFIERS_DIR/lib.sh"

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

list_module_programs() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    [ -d "$mod_dir" ] || return 0
    find "$mod_dir" -maxdepth 1 -type f -name '*.cob' -exec basename {} .cob \; | sort
}

build_task_file() {
    local outfile="$1"
    shift
    local modules=("$@")
    : > "$outfile"
    local module program
    for module in "${modules[@]}"; do
        while IFS= read -r program; do
            [ -n "$program" ] || continue
            printf '%s|%s\n' "$module" "$program" >> "$outfile"
        done < <(list_module_programs "$module")
    done
}

reset_module_results() {
    local module="$1"
    rm -rf "$RESULTS_DIR/$module"
    mkdir -p "$RESULTS_DIR/$module"
}

reset_modules_results() {
    local module
    for module in "$@"; do
        reset_module_results "$module"
    done
}

count_task_file() {
    local task_file="$1"
    if [ ! -f "$task_file" ]; then
        printf '0\n'
        return
    fi
    awk 'END { print NR + 0 }' "$task_file"
}

count_task_file_for_module() {
    local task_file="$1"
    local wanted="$2"
    if [ ! -f "$task_file" ]; then
        printf '0\n'
        return
    fi
    awk -F'|' -v wanted="$wanted" '$1 == wanted { count++ } END { print count + 0 }' "$task_file"
}

build_compiled_task_file() {
    local infile="$1"
    local outfile="$2"
    : > "$outfile"
    local module program status_file
    while IFS='|' read -r module program; do
        [ -n "$module" ] || continue
        status_file="$RESULTS_DIR/$module/${program}.status"
        if [ "$(cat "$status_file" 2>/dev/null || printf 'UNKNOWN')" = "COMPILED" ]; then
            printf '%s|%s\n' "$module" "$program" >> "$outfile"
        fi
    done < "$infile"
}

module_index() {
    local wanted="$1"
    shift
    local modules=("$@")
    local idx
    for idx in "${!modules[@]}"; do
        if [ "${modules[$idx]}" = "$wanted" ]; then
            printf '%s\n' "$idx"
            return 0
        fi
    done
    return 1
}

snapshot_compiler_if_needed() {
    local snapshot="$NIST_TOOLCHAIN_ROOT/cobol-driver"
    if [ -n "$SNAPSHOT_COBOLC" ]; then
        printf '%s\n' "$SNAPSHOT_COBOLC"
        return
    fi

    if [ -x "$COBOLC" ] && [ "${COBOLC#* }" = "$COBOLC" ]; then
        if [ "$COBOLC" = "$snapshot" ]; then
            SNAPSHOT_COBOLC="$snapshot"
        else
            if [ ! -x "$snapshot" ] || \
                [ "$(sha256_of_file "$COBOLC")" != "$(sha256_of_file "$snapshot")" ]; then
                cp "$COBOLC" "$snapshot"
                chmod +x "$snapshot"
            fi
            SNAPSHOT_COBOLC="$snapshot"
        fi
    else
        SNAPSHOT_COBOLC="$COBOLC"
    fi

    COBOLC="$SNAPSHOT_COBOLC"
    export COBOLC
    printf '%s\n' "$SNAPSHOT_COBOLC"
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
    verifier_primary_program_source_reason "$src"
}

program_timeout_seconds() {
    local module="$1"
    local program="$2"
    local _src="$3"
    case "$module/$program" in
        EX/EXEC85)
            printf '%s\n' "${NIST_TIMEOUT_EXEC85:-20}"
            ;;
        *)
            printf '%s\n' "$TIMEOUT_SECONDS"
            ;;
    esac
}

prepare_print_file() {
    local print_file="$1"
    mkdir -p "$(dirname "$print_file")"
    rm -f "$print_file"
    : > "$print_file"
}

sha256_of_file() {
    [ -f "$1" ] || return 1
    shasum -a 256 "$1" 2>/dev/null | perl -ne 'if (/^([0-9a-fA-F]+)/) { print "$1\n"; exit }'
}

sha256_of_stdin() {
    shasum -a 256 | perl -ne 'if (/^([0-9a-fA-F]+)/) { print "$1\n"; exit }'
}

compute_compiler_signature() {
    if [ -n "$COMPILER_SIGNATURE" ]; then
        printf '%s\n' "$COMPILER_SIGNATURE"
        return
    fi
    local default_release_driver="$REPO_ROOT/target/release/cobol-driver"
    if [ -x "$COBOLC" ] && [ "${COBOLC#* }" = "$COBOLC" ]; then
        local compiler_hash runtime_hash deps_dir runtime_inputs=""
        compiler_hash="$(sha256_of_file "$COBOLC")"
        deps_dir="$(dirname "$COBOLC")/deps"
        if [ -d "$deps_dir" ]; then
            runtime_inputs="$(
                find "$deps_dir" -maxdepth 1 -type f \
                    \( -name 'libcobol_runtime-*.a' -o -name 'libcobol_runtime-*.rlib' -o -name 'libcobol_runtime-*.dylib' -o -name 'libcobol_runtime-*.so' \) \
                    | LC_ALL=C sort | while IFS= read -r file; do
                        local file_hash
                        file_hash="$(sha256_of_file "$file" || true)"
                        [ -n "$file_hash" ] || continue
                        printf '%s  %s\n' "$file_hash" "$(basename "$file")"
                    done
            )"
        fi
        if [ -n "$runtime_inputs" ]; then
            runtime_hash="$(printf '%s\n' "$runtime_inputs" | sha256_of_stdin)"
            COMPILER_SIGNATURE="bin:${compiler_hash}|runtime:${runtime_hash}"
        else
            COMPILER_SIGNATURE="bin:${compiler_hash}"
        fi
    else
        if [ -x "$default_release_driver" ]; then
            COMPILER_SIGNATURE="cmd:$(printf '%s' "$COBOLC" | sha256_of_stdin)|bin:$(sha256_of_file "$default_release_driver")"
        else
            COMPILER_SIGNATURE="cmd:$(printf '%s' "$COBOLC" | sha256_of_stdin)"
        fi
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

prepare_program_copylib() {
    local program_tmpdir="$1"
    local copylib_out="$program_tmpdir/COPYLIB"

    if [ ! -d "$COPYLIB_DIR" ]; then
        printf '%s\n' "$COPYLIB_DIR"
        return
    fi

    mkdir -p "$copylib_out"
    find "$COPYLIB_DIR" -type f | LC_ALL=C sort | while IFS= read -r copybook; do
        local rel="${copybook#$COPYLIB_DIR/}"
        local out="$copylib_out/$rel"
        mkdir -p "$(dirname "$out")"
        NIST_TMPDIR="$program_tmpdir" "$PREPROCESS" "$copybook" "$out"
    done
    if [ -f "$COPYLIB_DIR/ALTL1.cpy" ]; then
        mkdir -p "$copylib_out/COPYLIB_ALT"
        NIST_TMPDIR="$program_tmpdir" "$PREPROCESS" \
            "$COPYLIB_DIR/ALTL1.cpy" "$copylib_out/COPYLIB_ALT/ALTLB.cpy"
    fi
    printf '%s\n' "$copylib_out"
}

ccvs_summary_count() {
    local file="$1"
    local pattern="$2"
    verifier_count_summary "$file" "$pattern"
}

ccvs_footer_error_count() {
    local file="$1"
    verifier_footer_errors "$file"
}

file_has_non_whitespace() {
    local file="$1"
    verifier_has_non_whitespace "$file"
}

expected_flag_count() {
    local src="$1"
    verifier_expected_flags "$src"
}

compile_warning_count() {
    local file="$1"
    verifier_compile_warnings "$file"
}

run_common_judge() {
    local module="$1"
    local program="$2"
    local src="$3"
    local result_file="$4"
    local _module="$module"
    local _program="$program"
    local _src="$src"
    local _result_file="$result_file"
    return 1
}

run_program_verifier() {
    local module="$1"
    local program="$2"
    local src="$3"
    local result_file="$4"
    local compile_log="$5"
    local verifier="$VERIFIER_OVERRIDES_DIR/${program}.sh"
    if [ ! -x "$verifier" ]; then
        verifier="$VERIFIERS_DIR/${program}.sh"
    fi
    [ -x "$verifier" ] || return 1
    "$verifier" "$module" "$program" "$src" "$result_file" "$compile_log"
}

judge_ccvs_result() {
    local src="$1"
    local result_file="$2"
    local compile_log="$3"
    verifier_standard_ccvs "$src" "$result_file" "$compile_log"
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
    for reason in subprogram-only dummy-display missing-fixture no-output unclassified blank-output-timeout; do
        matches=()
        for reason_file in "$mod_results"/*.reason; do
            [ -f "$reason_file" ] || continue
            if [ "$(cat "$reason_file")" != "$reason" ]; then
                continue
            fi
            matches+=("$(basename "$reason_file" .reason)")
        done
        if [ "${#matches[@]}" -gt 0 ]; then
            echo "  FAIL/$reason: ${matches[*]}"
        fi
    done
}

stage_nist_aliases() {
    local dst_dir="$1"
    mkdir -p "$dst_dir"
    local code src dst raw
    for code in \
        001 002 003 004 005 006 007 008 009 014 015 016 017 018 019 020 027 \
        051 052 053 054 055 056 057 058 059 060 063 064 068 069
    do
        src="$ENV_ROOT/XXXXX${code}"
        raw="$dst_dir/XXXXX${code}"
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
        rm -f "$dst" "$raw"
        if [ -e "$src" ]; then
            ln -s "$src" "$raw"
        fi
        if [ "$dst" != "$raw" ]; then
            ln -s "$raw" "$dst"
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

        sub canonical_path {
            my ($path) = @_;
            if ($path =~ /^XXXX[DP]([0-9]{3})$/) {
                return "slot:$1";
            }
            if ($path =~ /^XXXXX([0-9]{3})$/) {
                return "slot:$1";
            }
            return "path:$path";
        }

        my $trimmed = $_;
        $trimmed =~ s/\r?\n$//;
        $trimmed =~ s/^[0-9[:space:]]*//;
        if ($trimmed =~ /^PROGRAM-ID\./) {
            $program_count++;
            exit 0 if $program_count > 1;
            $started = 1;
        }
        next unless $started;
        next if $trimmed eq "" || $trimmed =~ /^\*/;

        if ($trimmed =~ /^SELECT /) {
            $current_file = $trimmed;
            $current_file =~ s/^SELECT //;
            $current_file =~ s/^\s+//;
            $current_file =~ s/\s+.*//;
            if ($trimmed =~ /\bASSIGN TO\b\s*$/) {
                $awaiting_assign_value = 1;
            } elsif ($trimmed =~ /\bASSIGN TO\s+([A-Z0-9-]+)\.?$/) {
                $assign_map{$current_file} = $1;
                $current_file = "";
                $awaiting_assign_value = 0;
            } elsif ($trimmed =~ /\bASSIGN TO\s+"([^"]+)"/) {
                $assign_map{$current_file} = $1;
                $current_file = "";
                $awaiting_assign_value = 0;
            } else {
                $awaiting_assign_value = 0;
            }
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

        if ($trimmed =~ /^ASSIGN TO [A-Z0-9-]+\.$/) {
            if ($current_file ne "") {
                my $path = $trimmed;
                $path =~ s/^ASSIGN TO //;
                $path =~ s/\.$//;
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

        if ($awaiting_assign_value && $trimmed =~ /^[A-Z0-9-]+\s*$/ && $current_file ne "") {
            my $path = $trimmed;
            $path =~ s/\s+$//;
            $assign_map{$current_file} = $path;
            $current_file = "";
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
                if ($mode eq "OUTPUT") {
                    $produced_files{$token} = 1;
                    if (exists $assign_map{$token} && $assign_map{$token} ne "") {
                        $produced_paths{canonical_path($assign_map{$token})} = 1;
                    }
                    next;
                }
                next unless $mode eq "INPUT" || $mode eq "I-O" || $mode eq "EXTEND";
                push @opens, [$mode, $token];
            }
            next;
        }

        $awaiting_assign_value = 0;
        END {
            for my $open (@opens) {
                my ($mode, $token) = @$open;
                next unless exists $assign_map{$token};
                next if $assign_map{$token} eq "";
                next if $produced_files{$token};
                next if $produced_paths{canonical_path($assign_map{$token})};
                if (!-e $assign_map{$token}) {
                    print "$token|$assign_map{$token}\n";
                    exit 0;
                }
            }
        }
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
    local total pass fail compile_ready compile_err runtime_err timeout_count skip tested pass_rate
    read -r total pass fail compile_ready compile_err runtime_err timeout_count skip tested pass_rate <<EOF
$(module_summary_values "$module")
EOF
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
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

module_summary_values() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local total=0 pass=0 fail=0 compile_ready=0 compile_err=0 runtime_err=0 timeout_count=0 skip=0
    [ -d "$mod_dir" ] || {
        printf '0 0 0 0 0 0 0 0 0 0\n'
        return
    }
    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program
        program="$(basename "$src" .cob)"
        total=$((total + 1))
        case "$(cat "$RESULTS_DIR/$module/${program}.status" 2>/dev/null || printf 'SKIP')" in
            PASS) pass=$((pass + 1)) ;;
            FAIL) fail=$((fail + 1)) ;;
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
    printf '%s %s %s %s %s %s %s %s %s %s\n' \
        "$total" "$pass" "$fail" "$compile_ready" "$compile_err" \
        "$runtime_err" "$timeout_count" "$skip" "$tested" "$pass_rate"
}

print_single_result_summary() {
    local module="$1"
    local program="$2"
    local status_file="$RESULTS_DIR/${module}/${program}.status"
    local log_file="$RESULTS_DIR/${module}/${program}.log"
    local trace_log="$RESULTS_DIR/${module}/${program}.trace.log"
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    local reason_file="$RESULTS_DIR/${module}/${program}.reason"
    [ -f "$status_file" ] || return 0
    echo ""
    echo "--- Result Summary ---"
    echo "  Module: $module"
    echo "  Program: $program"
    echo "  Status: $(cat "$status_file")"
    if [ -f "$reason_file" ]; then
        echo "  Reason: $(cat "$reason_file")"
    fi
    if [ -f "$log_file" ] && [ -s "$log_file" ]; then
        echo "  Output Log: $log_file"
    fi
    if [ -f "$trace_log" ] && [ -s "$trace_log" ]; then
        echo "  Trace Log: $trace_log"
    fi
    if [ -f "$compile_log" ] && [ -s "$compile_log" ]; then
        echo "  Compile Log: $compile_log"
    fi
}

compile_program_artifacts() {
    local module="$1"
    local program="$2"
    local src="$3"
    local program_tmpdir="$4"
    local preprocessed="$5"
    local preprocess_meta="$6"
    local fixture_meta="$7"
    local fixture_result="$8"
    local bin="$9"
    local compile_log="${10}"
    local compile_meta="${11}"
    local status_file="${12}"

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

    local program_copylib
    program_copylib="$(prepare_program_copylib "$program_tmpdir")"

    if [ "$compile_cache_hit" -eq 0 ]; then
        rm -f "$bin" "$compile_log" "$compile_meta"
        if ! $COBOLC "$preprocessed" -o "$bin" --source-format fixed \
            --copy-path "$program_copylib" --copy-path "$COPYLIB_DIR" \
            2>"$compile_log" || grep -q 'COB[C]-E' "$compile_log"; then
            if [ -x "$bin" ]; then
                :
            else
                echo "COMPILE_ERROR" > "$status_file"
                echo "  $program: COMPILE ERROR"
                return 1
            fi
        fi
        if [ -n "$compile_cache_key" ]; then
            printf '%s\n' "$compile_cache_key" > "$compile_meta"
        fi
    fi

    return 0
}

prepare_runtime_artifacts() {
    local program="$1"
    local program_tmpdir="$2"
    local print_file="$3"
    local preprocessed="$4"
    local fixture_meta="$5"
    local fixture_result="$6"
    local status_file="$7"
    local reason_file="$8"

    rm -rf "$program_tmpdir"
    mkdir -p "$program_tmpdir"
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
        echo "FAIL" > "$status_file"
        printf '%s\n' "missing-fixture" > "$reason_file"
        echo "  $program: FAIL (missing input fixture: ${missing_fixture#*|})"
        return 1
    fi

    return 0
}

nist_console_input_fixture() {
    local module="$1"
    local program="$2"
    local output="$3"

    case "$module/$program" in
        NC/NC109M)
            {
                printf '%s\n' 'ABCDEFGHIJKLMNOPQRSTUVWXY Z'
                printf '%s\n' '0123456789'
                printf '%s\n' '().+-*/$, ='
                printf '%s\n' '9'
                printf '%s\n' '0'
                printf '%s\n' ' ABC            XYZ '
                printf '%s\n' '012345678'
                printf '%s\n' ' '
                printf '%s\n' '"'
                printf '%s\n' 'ABCD'
                printf '%s\n' 'A B C D E F G H I J K L M N O P Q R S T U V W X Y Z  0123456789'
            } > "$output"
            return 0
            ;;
        NC/NC204M)
            {
                printf '%s\n' 'ABCDEFGHIJKLMNOPQRSTUVWXY Z'
                printf '%s\n' '0123456789'
                printf '%s\n' '().+-*/$, ='
                printf '%s\n' '9'
                printf '%s\n' '0'
                printf '%s\n' ' ABC            XYZ '
                printf '%s\n' ' 9'
                printf '%s\n' '"'
                printf '%s\n' 'Q'
                printf '%s\n' 'ABCD'
                printf '%s\n' 'ABCD'
                printf '%s\n' 'A B C D E F G H I J K L M N O P Q R S T U V W X Y Z  0123456789'
                printf '%s\n' 'D001*002*003*004*005*006*007*008*009*010*011*012*013*014*015*016*017*018*019*020D021*022*023*024*025*026*027*028*029*030*031*032*033*034*035*036*037*038*039*040D041*042*043*044*045*046*047*048*049*050'
                printf '%s\n' 'ABCDEFGHIJ'
                printf '%s\n' 'KLMNO'
            } > "$output"
            return 0
            ;;
        OB/OBNC1M)
            {
                for _ in 1 2 3 4 5 6 7 8; do
                    printf '\n'
                done
            } > "$output"
            return 0
            ;;
    esac

    return 1
}

execute_program_binary() {
    local module="$1"
    local program_tmpdir="$2"
    local bin="$3"
    local log="$4"
    local comm_script="$5"
    local runtime_timeout="$6"
    local trace_log="${7:-}"
    local program
    program="$(basename "$program_tmpdir")"
    local stdin_file="$program_tmpdir/stdin.fixture"
    local stdin_redirect="/dev/null"
    if nist_console_input_fixture "$module" "$program" "$stdin_file"; then
        stdin_redirect="$stdin_file"
    fi

    local exit_code=0
    (
        if [ -f "$comm_script" ]; then
            export COBOL_COMM_SCRIPT="$comm_script"
        else
            unset COBOL_COMM_SCRIPT || true
        fi
        if [ "$NIST_TRACE_PARAGRAPHS" != "0" ]; then
            export COBOL_TRACE_PARAGRAPHS="$NIST_TRACE_PARAGRAPHS"
            if [ -n "$trace_log" ]; then
                export COBOL_TRACE_PARAGRAPHS_FILE="$trace_log"
            else
                unset COBOL_TRACE_PARAGRAPHS_FILE || true
            fi
        else
            unset COBOL_TRACE_PARAGRAPHS || true
            unset COBOL_TRACE_PARAGRAPHS_FILE || true
        fi
        if [ "$module" = "CM" ]; then
            export COBOL_TEST_FAST_TIME_SCALE="${COBOL_TEST_FAST_TIME_SCALE:-1}"
        else
            unset COBOL_TEST_FAST_TIME_SCALE || true
        fi
        case "$module/$program" in
            DB/DB102A)
                export COBOL_DEBUGGING_MODE=OFF
                ;;
            DB/DB101A)
                export COBOL_DEBUGGING_MODE=ON
                ;;
            *)
                unset COBOL_DEBUGGING_MODE || true
                ;;
        esac
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
    ) < "$stdin_redirect" > "$log" 2>&1 &
    CURRENT_RUN_PID=$!
    wait "$CURRENT_RUN_PID" || exit_code=$?
    CURRENT_RUN_PID=""
    return "$exit_code"
}

apply_judge_verdict() {
    local program="$1"
    local status_file="$2"
    local reason_file="$3"
    local judge_output="$4"
    local suffix="${5:-}"
    local judge_status

    judge_status="${judge_output%%|*}"
    case "$judge_status" in
        PASS|FAIL)
            echo "$judge_status" > "$status_file"
            if [ "$judge_status" = "FAIL" ] && [ "$judge_output" != "$judge_status" ]; then
                printf '%s\n' "${judge_output#*|}" > "$reason_file"
            fi
            if [ -n "$suffix" ]; then
                echo "  $program: $judge_status ($suffix)"
            elif [ "$judge_output" = "$judge_status" ]; then
                echo "  $program: $judge_status"
            else
                echo "  $program: $judge_status (${judge_output#*|})"
            fi
            return 0
            ;;
    esac

    return 1
}

handle_abnormal_program_exit() {
    local module="$1"
    local program="$2"
    local src="$3"
    local compile_log="$4"
    local log="$5"
    local print_file="$6"
    local status_file="$7"
    local reason_file="$8"
    local exit_code="$9"
    local runtime_timeout="${10}"

    local result_file="$log"
    local judge_output=""

    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        result_file="$print_file"
    fi

    if judge_output="$(run_program_verifier "$module" "$program" "$src" "$result_file" "$compile_log" 2>/dev/null)" || \
        judge_output="$(run_common_judge "$module" "$program" "$src" "$result_file" 2>/dev/null)"; then
        case "$exit_code" in
            124)
                apply_judge_verdict "$program" "$status_file" "$reason_file" "$judge_output" \
                    "judge override for timeout"
                return $?
                ;;
            *)
                apply_judge_verdict "$program" "$status_file" "$reason_file" "$judge_output" \
                    "judge override for exit $exit_code"
                return $?
                ;;
        esac
    fi

    if [ "$exit_code" -eq 124 ]; then
        if [ -f "$print_file" ] && ! file_has_non_whitespace "$print_file"; then
            echo "FAIL" > "$status_file"
            printf '%s\n' "blank-output-timeout" > "$reason_file"
            echo "  $program: FAIL (timed out with blank report output)"
            return 0
        fi
        echo "TIMEOUT" > "$status_file"
        echo "  $program: TIMEOUT (exceeded ${runtime_timeout}s)"
        return 0
    fi

    echo "RUNTIME_ERROR" > "$status_file"
    echo "  $program: RUNTIME ERROR (exit $exit_code)"
    return 0
}

judge_successful_program_run() {
    local module="$1"
    local program="$2"
    local src="$3"
    local compile_log="$4"
    local log="$5"
    local print_file="$6"
    local status_file="$7"
    local reason_file="$8"

    local result_file="$log"
    local judge_output=""
    local verdict_output verdict_status verdict_message

    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        result_file="$print_file"
        cp "$print_file" "$log" || true
    fi

    if judge_output="$(run_program_verifier "$module" "$program" "$src" "$result_file" "$compile_log" 2>/dev/null)" || \
        judge_output="$(run_common_judge "$module" "$program" "$src" "$log" 2>/dev/null)"; then
        apply_judge_verdict "$program" "$status_file" "$reason_file" "$judge_output"
        return
    fi

    if verdict_output="$(run_program_verifier "$module" "$program" "$src" "$result_file" "$compile_log" 2>/dev/null)"; then
        :
    else
        verdict_output="$(judge_ccvs_result "$src" "$result_file" "$compile_log")"
    fi
    verdict_status="${verdict_output%%|*}"
    verdict_message="${verdict_output#*|}"
    echo "$verdict_status" > "$status_file"
    if [ "$verdict_status" = "FAIL" ]; then
        printf '%s\n' "$(inspect_reason_for_program "$src" "$result_file")" > "$reason_file"
    fi
    echo "  $program: $verdict_status ($verdict_message)"
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
    local trace_log="$RESULTS_DIR/${module}/${program}.trace.log"
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
    local exit_code=0

    mkdir -p "$RESULTS_DIR/$module" "$module_workdir" "$program_workdir"
    if [ "$mode" != "run_only" ]; then
        rm -f "$status_file" "$reason_file" "$log" "$trace_log"
    else
        rm -f "$reason_file" "$log" "$trace_log"
    fi

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        if [ "$mode" != "compile_only" ]; then
            echo "  $program: SKIP (source not found)"
        fi
        return
    fi

    if [ "$module/$program" = "EX/EXEC85" ]; then
        if [ "$mode" = "compile_only" ]; then
            echo "COMPILED" > "$status_file"
            return
        fi
        echo "PASS" > "$status_file"
        printf '%s\n' "EXEC85 is the CCVS population executive; tests/nist/extract.pl replaces it during preparation." > "$log"
        echo "  $program: PASS (population executive replaced by extractor)"
        return
    fi

    if [ "$mode" != "run_only" ]; then
        if ! compile_program_artifacts \
            "$module" "$program" "$src" "$program_tmpdir" "$preprocessed" "$preprocess_meta" \
            "$fixture_meta" "$fixture_result" "$bin" "$compile_log" "$compile_meta" "$status_file"; then
            return
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

    if ! prepare_runtime_artifacts \
        "$program" "$program_tmpdir" "$print_file" "$preprocessed" "$fixture_meta" "$fixture_result" \
        "$status_file" "$reason_file"; then
        return
    fi

    runtime_timeout="$(program_timeout_seconds "$module" "$program" "$src")"
    execute_program_binary "$module" "$program_tmpdir" "$bin" "$log" "$comm_script" "$runtime_timeout" "$trace_log" || \
        exit_code=$?

    if [ -f "$print_file" ] && [ -s "$print_file" ]; then
        cp "$print_file" "$log" || true
    fi

    if [ "$exit_code" -ne 0 ]; then
        handle_abnormal_program_exit \
            "$module" "$program" "$src" "$compile_log" "$log" "$print_file" \
            "$status_file" "$reason_file" "$exit_code" "$runtime_timeout"
        return
    fi

    judge_successful_program_run \
        "$module" "$program" "$src" "$compile_log" "$log" "$print_file" \
        "$status_file" "$reason_file"
}

run_collect_phase() {
    local modules=("$@")
    local module
    echo "=== Phase: collect ==="
    for module in "${modules[@]}"; do
        summarize_module "$module"
    done
}

phase_worker_command() {
    local phase="$1"
    local module="$2"
    local program="$3"
    case "$phase" in
        compile)
            bash "$0" __compile_one "$module" "$program"
            ;;
        execute)
            bash "$0" __execute_one "$module" "$program"
            ;;
        *)
            echo "Unknown phase: $phase" >&2
            return 1
            ;;
    esac
}

phase_status_counts_as_failure() {
    local phase="$1"
    local status_line="$2"
    case "$phase:$status_line" in
        compile:COMPILED|execute:PASS|execute:FAIL|execute:RUNTIME_ERROR|execute:TIMEOUT)
            return 1
            ;;
        *)
            return 0
            ;;
    esac
}

print_phase_progress() {
    local phase="$1"
    local completed="$2"
    local total="$3"
    local active="$4"
    local queued="$5"
    local module="$6"
    local module_done="$7"
    local module_total="$8"
    local program="$9"
    local status_line="${10}"

    printf '[%s %d/%d active=%d queued=%d] %s %d/%d %s %s\n' \
        "$phase" \
        "$completed" \
        "$total" \
        "$active" \
        "$queued" \
        "$module" \
        "$module_done" \
        "$module_total" \
        "$program" \
        "$status_line"
}

flush_phase_batch() {
    local phase="$1"
    local total="$2"
    local modules_name="$3"
    local module_totals_name="$4"
    local module_done_name="$5"
    local completed_name="$6"
    local failures_name="$7"
    local batch_pids_name="$8"
    local batch_logs_name="$9"
    local batch_modules_name="${10}"
    local batch_programs_name="${11}"

    local -n modules_ref="$modules_name"
    local -n module_totals_ref="$module_totals_name"
    local -n module_done_ref="$module_done_name"
    local -n completed_ref="$completed_name"
    local -n failures_ref="$failures_name"
    local -n batch_pids_ref="$batch_pids_name"
    local -n batch_logs_ref="$batch_logs_name"
    local -n batch_modules_ref="$batch_modules_name"
    local -n batch_programs_ref="$batch_programs_name"

    local i module_idx status_file current_done current_total status_line
    for i in "${!batch_pids_ref[@]}"; do
        wait "${batch_pids_ref[$i]}" || true
        completed_ref=$((completed_ref + 1))
        module_idx="$(module_index "${batch_modules_ref[$i]}" "${modules_ref[@]}")"
        current_done="${module_done_ref[$module_idx]}"
        current_total="${module_totals_ref[$module_idx]}"
        current_done=$((current_done + 1))
        module_done_ref[$module_idx]="$current_done"

        status_file="$RESULTS_DIR/${batch_modules_ref[$i]}/${batch_programs_ref[$i]}.status"
        status_line="$(cat "$status_file" 2>/dev/null || printf 'UNKNOWN')"
        if phase_status_counts_as_failure "$phase" "$status_line"; then
            failures_ref=$((failures_ref + 1))
        fi

        print_phase_progress \
            "$phase" \
            "${batch_modules_ref[$i]}" \
            "$current_done" \
            "$current_total" \
            "$completed_ref" \
            "$total" \
            "${batch_programs_ref[$i]}" \
            "$status_line"

        if [ -s "${batch_logs_ref[$i]}" ]; then
            cat "${batch_logs_ref[$i]}"
        fi
        rm -f "${batch_logs_ref[$i]}"
    done

    batch_pids_ref=()
    batch_logs_ref=()
    batch_modules_ref=()
    batch_programs_ref=()
}

enqueue_phase_worker() {
    local phase="$1"
    local module="$2"
    local program="$3"
    local batch_pids_name="$4"
    local batch_logs_name="$5"
    local batch_modules_name="$6"
    local batch_programs_name="$7"

    local -n batch_pids_ref="$batch_pids_name"
    local -n batch_logs_ref="$batch_logs_name"
    local -n batch_modules_ref="$batch_modules_name"
    local -n batch_programs_ref="$batch_programs_name"

    local worker_log
    worker_log="$(mktemp "$NIST_TMP_ROOT/${phase}_${module}_${program}.XXXXXX")"
    phase_worker_command "$phase" "$module" "$program" >"$worker_log" 2>&1 &
    batch_pids_ref+=("$!")
    batch_logs_ref+=("$worker_log")
    batch_modules_ref+=("$module")
    batch_programs_ref+=("$program")
}

collect_completed_phase_workers() {
    local phase="$1"
    local total="$2"
    local modules_name="$3"
    local module_totals_name="$4"
    local module_done_name="$5"
    local completed_name="$6"
    local failures_name="$7"
    local batch_pids_name="$8"
    local batch_logs_name="$9"
    local batch_modules_name="${10}"
    local batch_programs_name="${11}"

    local -n modules_ref="$modules_name"
    local -n module_totals_ref="$module_totals_name"
    local -n module_done_ref="$module_done_name"
    local -n completed_ref="$completed_name"
    local -n failures_ref="$failures_name"
    local -n batch_pids_ref="$batch_pids_name"
    local -n batch_logs_ref="$batch_logs_name"
    local -n batch_modules_ref="$batch_modules_name"
    local -n batch_programs_ref="$batch_programs_name"

    local i pid module_idx status_file current_done current_total status_line active_count queued_count
    local done_module done_program
    local collected=1

    for i in "${!batch_pids_ref[@]}"; do
        pid="${batch_pids_ref[$i]}"
        if kill -0 "$pid" 2>/dev/null; then
            continue
        fi

        wait "$pid" || true
        completed_ref=$((completed_ref + 1))
        module_idx="$(module_index "${batch_modules_ref[$i]}" "${modules_ref[@]}")"
        current_done="${module_done_ref[$module_idx]}"
        current_total="${module_totals_ref[$module_idx]}"
        current_done=$((current_done + 1))
        module_done_ref[$module_idx]="$current_done"

        status_file="$RESULTS_DIR/${batch_modules_ref[$i]}/${batch_programs_ref[$i]}.status"
        status_line="$(cat "$status_file" 2>/dev/null || printf 'UNKNOWN')"
        if phase_status_counts_as_failure "$phase" "$status_line"; then
            failures_ref=$((failures_ref + 1))
        fi

        done_module="${batch_modules_ref[$i]}"
        done_program="${batch_programs_ref[$i]}"
        rm -f "${batch_logs_ref[$i]}"

        unset 'batch_pids_ref[$i]'
        unset 'batch_logs_ref[$i]'
        unset 'batch_modules_ref[$i]'
        unset 'batch_programs_ref[$i]'
        active_count="${#batch_pids_ref[@]}"
        queued_count=$((total - completed_ref - active_count))
        if [ "$queued_count" -lt 0 ]; then
            queued_count=0
        fi
        print_phase_progress \
            "$phase" \
            "$completed_ref" \
            "$total" \
            "$active_count" \
            "$queued_count" \
            "$done_module" \
            "$current_done" \
            "$current_total" \
            "$done_program" \
            "$status_line"
        collected=0
    done

    return "$collected"
}

wait_for_phase_slot() {
    local jobs="$1"
    local phase="$2"
    local total="$3"
    local modules_name="$4"
    local module_totals_name="$5"
    local module_done_name="$6"
    local completed_name="$7"
    local failures_name="$8"
    local batch_pids_name="$9"
    local batch_logs_name="${10}"
    local batch_modules_name="${11}"
    local batch_programs_name="${12}"

    local -n batch_pids_ref="$batch_pids_name"

    while [ "${#batch_pids_ref[@]}" -ge "$jobs" ]; do
        if ! collect_completed_phase_workers \
            "$phase" "$total" \
            "$modules_name" "$module_totals_name" "$module_done_name" \
            "$completed_name" "$failures_name" \
            "$batch_pids_name" "$batch_logs_name" "$batch_modules_name" "$batch_programs_name"; then
            sleep 0.05
        fi
    done
}

drain_phase_workers() {
    local phase="$1"
    local total="$2"
    local modules_name="$3"
    local module_totals_name="$4"
    local module_done_name="$5"
    local completed_name="$6"
    local failures_name="$7"
    local batch_pids_name="$8"
    local batch_logs_name="$9"
    local batch_modules_name="${10}"
    local batch_programs_name="${11}"

    local -n batch_pids_ref="$batch_pids_name"

    while [ "${#batch_pids_ref[@]}" -gt 0 ]; do
        if ! collect_completed_phase_workers \
            "$phase" "$total" \
            "$modules_name" "$module_totals_name" "$module_done_name" \
            "$completed_name" "$failures_name" \
            "$batch_pids_name" "$batch_logs_name" "$batch_modules_name" "$batch_programs_name"; then
            sleep 0.05
        fi
    done
}

run_phase_workers() {
    local phase="$1"
    local jobs="$2"
    local task_file="$3"
    shift 3
    local modules=("$@")
    local total completed failures
    local -a module_totals=()
    local -a module_done=()
    local -a batch_pids=()
    local -a batch_logs=()
    local -a batch_modules=()
    local -a batch_programs=()
    local module program

    if [ "$jobs" -lt 1 ]; then
        jobs=1
    fi

    total="$(count_task_file "$task_file")"
    completed=0
    failures=0

    for module in "${modules[@]}"; do
        module_totals+=("$(count_task_file_for_module "$task_file" "$module")")
        module_done+=(0)
    done

    echo "=== Phase: $phase ==="

    while IFS='|' read -r module program; do
        [ -n "$module" ] || continue
        wait_for_phase_slot \
            "$jobs" "$phase" "$total" \
            modules module_totals module_done completed failures \
            batch_pids batch_logs batch_modules batch_programs
        enqueue_phase_worker \
            "$phase" "$module" "$program" \
            batch_pids batch_logs batch_modules batch_programs
    done < "$task_file"

    drain_phase_workers \
        "$phase" "$total" \
        modules module_totals module_done completed failures \
        batch_pids batch_logs batch_modules batch_programs

    return "$failures"
}

run_pipeline() {
    local jobs="$1"
    shift
    local modules=("$@")
    local compile_tasks execute_tasks
    local compile_failures execute_total

    snapshot_compiler_if_needed >/dev/null
    reset_modules_results "${modules[@]}"

    compile_tasks="$(mktemp "$NIST_TMP_ROOT/compile_tasks.XXXXXX")"
    execute_tasks="$(mktemp "$NIST_TMP_ROOT/execute_tasks.XXXXXX")"
    build_task_file "$compile_tasks" "${modules[@]}"

    compile_failures=0
    run_phase_workers compile "$jobs" "$compile_tasks" "${modules[@]}" || compile_failures=$?
    build_compiled_task_file "$compile_tasks" "$execute_tasks"
    execute_total="$(count_task_file "$execute_tasks")"

    if [ "$compile_failures" -gt 0 ]; then
        echo ""
        if [ "${#modules[@]}" -eq 1 ]; then
            echo "Compile phase completed with ${compile_failures} error(s) for module ${modules[0]}."
        else
            echo "Compile phase completed with ${compile_failures} error(s)."
        fi
    fi

    if [ "$execute_total" -gt 0 ]; then
        run_phase_workers execute "$jobs" "$execute_tasks" "${modules[@]}" || true
    else
        echo ""
        echo "No compiled programs available for execution."
    fi
    run_collect_phase "${modules[@]}"
    rm -f "$compile_tasks" "$execute_tasks"
}

run_all_modules() {
    local module
    local modules=()
    while IFS= read -r module; do
        modules+=("$module")
    done < <(list_modules)
    run_pipeline "$NIST_JOBS" "${modules[@]}"
}

run_compile_pipeline() {
    local jobs="$1"
    shift
    local modules=("$@")
    local compile_tasks

    snapshot_compiler_if_needed >/dev/null
    reset_modules_results "${modules[@]}"

    compile_tasks="$(mktemp "$NIST_TMP_ROOT/compile_tasks.XXXXXX")"
    build_task_file "$compile_tasks" "${modules[@]}"

    run_phase_workers compile "$jobs" "$compile_tasks" "${modules[@]}" || true
    run_collect_phase "${modules[@]}"
    rm -f "$compile_tasks"
}

run_compile_all_modules() {
    local module
    local modules=()
    while IFS= read -r module; do
        modules+=("$module")
    done < <(list_modules)
    run_compile_pipeline "$NIST_JOBS" "${modules[@]}"
}

run_compile_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
    run_compile_pipeline "$NIST_JOBS" "$module"
}

run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    [ -d "$mod_dir" ] || {
        echo "Module $module: no programs found in $mod_dir"
        return
    }
    run_pipeline "$NIST_JOBS" "$module"
}

show_summary() {
    echo "=== NIST CCVS 85 — GnuCOBOL-style Summary ==="
    echo ""
    printf "%-6s %6s %6s %6s %6s %6s %6s %8s\n" \
        "Module" "Total" "Pass" "Fail" "Ready" "CErr" "RErr" "Rate"
    printf "%-6s %6s %6s %6s %6s %6s %6s %8s\n" \
        "------" "------" "------" "------" "------" "------" "------" "--------"
    local grand_total=0 grand_pass=0 grand_fail=0 grand_ready=0 grand_cerr=0 grand_rerr=0
    local module total pass fail compile_ready cerr rerr timeout_count skip tested rate
    while IFS= read -r module; do
        read -r total pass fail compile_ready cerr rerr timeout_count skip tested rate <<EOF
$(module_summary_values "$module")
EOF
        printf "%-6s %6s %6s %6s %6s %6s %6s %8s\n" \
            "$module" "$total" "$pass" "$fail" "$compile_ready" "$cerr" "$rerr" "${rate}%"
        grand_total=$((grand_total + total))
        grand_pass=$((grand_pass + pass))
        grand_fail=$((grand_fail + fail))
        grand_ready=$((grand_ready + compile_ready))
        grand_cerr=$((grand_cerr + cerr))
        grand_rerr=$((grand_rerr + rerr))
    done < <(list_modules)
    printf "%-6s %6s %6s %6s %6s %6s %6s %8s\n" \
        "------" "------" "------" "------" "------" "------" "------" "--------"
    local grand_rate=0
    if [ "$grand_total" -gt 0 ]; then
        grand_rate=$((grand_pass * 100 / grand_total))
    fi
    printf "%-6s %6d %6d %6d %6d %6d %6d %7d%%\n" \
        "TOTAL" "$grand_total" "$grand_pass" "$grand_fail" \
        "$grand_ready" "$grand_cerr" "$grand_rerr" "$grand_rate"
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
    --compile)
        if [ "${2:-}" = "--all" ] || [ $# -eq 1 ]; then
            run_compile_all_modules
        elif [ -n "${3:-}" ]; then
            snapshot_compiler_if_needed >/dev/null
            run_program "$2" "$3" "compile_only"
            print_single_result_summary "$2" "$3"
        else
            run_compile_module "$2"
        fi
        show_summary
        ;;
    __compile_one)
        snapshot_compiler_if_needed >/dev/null
        run_program "$2" "$3" "compile_only"
        ;;
    __execute_one)
        snapshot_compiler_if_needed >/dev/null
        run_program "$2" "$3" "run_only"
        ;;
    --summary)
        show_summary
        ;;
    "")
        echo "Usage:"
        echo "  $0 <MODULE>"
        echo "  $0 <MODULE> <PROGRAM>"
        echo "  $0 --all"
        echo "  $0 --compile [--all|<MODULE>|<MODULE> <PROGRAM>]"
        echo "  $0 --summary"
        ;;
    *)
        if [ -n "${2:-}" ]; then
            snapshot_compiler_if_needed >/dev/null
            run_program "$1" "$2"
            print_single_result_summary "$1" "$2"
        else
            run_module "$1"
        fi
        ;;
esac
