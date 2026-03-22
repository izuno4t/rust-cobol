#!/usr/bin/env bash
# run_compare.sh — Compare rust-cobol NIST output with GnuCOBOL output.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ENV_ROOT="${NIST_ENV_ROOT:-$REPO_ROOT/target/nist}"
PROGRAMS_DIR="$ENV_ROOT/programs"
RESULTS_DIR="$ENV_ROOT/results-compare"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"
PREPROCESS="$SCRIPT_DIR/preprocess.sh"
RUST_COBOLC="${RUST_COBOLC:-cargo run --release --package cobol-driver --}"
GNU_COBOLC="${GNU_COBOLC:-cobc}"
RUST_WORKDIR="$ENV_ROOT/work/compare-rust"
GNU_WORKDIR="$ENV_ROOT/work/compare-gnu"
RUST_TMPDIR="/tmp/nist/rust"
GNU_TMPDIR="/tmp/nist/gnu"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
ALL_MODULES=(NC SM IC SQ IF IX RL ST RW DB SG OB)

mkdir -p "$RESULTS_DIR" "$RUST_WORKDIR" "$GNU_WORKDIR" "$RUST_TMPDIR" "$GNU_TMPDIR"

require_programs() {
    if [ ! -d "$PROGRAMS_DIR" ]; then
        echo "Programs not prepared: $PROGRAMS_DIR" >&2
        echo "Run tests/nist/prepare.sh first." >&2
        exit 1
    fi
}

normalize_report() {
    local input="$1"
    local output="$2"
    perl -pe 's/\r$//; s/[ \t]+$//' "$input" > "$output"
}

extract_ccvs_summary() {
    local input="$1"
    local output="$2"
    awk '
        /TESTS WERE EXECUTED SUCCESSFULLY/ {
            value = ($1 == "NO") ? 0 : $1
            print "executed_successfully=" value
        }
        /TEST\(S\) FAILED/ {
            value = ($1 == "NO") ? 0 : $1
            print "failed=" value
        }
        /TEST\(S\) REQUIRE INSPECTION/ {
            value = ($1 == "NO") ? 0 : $1
            print "inspection=" value
        }
        /TEST\(S\) DELETED/ {
            value = ($1 == "NO") ? 0 : $1
            print "deleted=" value
        }
    ' "$input" > "$output"
}

extract_ccvs_fail_items() {
    local input="$1"
    local output="$2"
    awk '
        /FAIL\*/ {
            line = $0
            sub(/^.*FAIL\*[[:space:]]*/, "", line)
            gsub(/[[:space:]]+$/, "", line)
            if (line != "") {
                print line
            }
        }
    ' "$input" | sort -u > "$output"
}

write_comparison_diff() {
    local gnu_file="$1"
    local rust_file="$2"
    local diff_file="$3"
    if diff -u "$gnu_file" "$rust_file" > "$diff_file"; then
        rm -f "$diff_file"
        return 0
    fi
    return 1
}

run_binary() {
    local workdir="$1"
    local bin="$2"
    local stdout_log="$3"
    local exit_code=0
    {
        timeout "$TIMEOUT_SECONDS" perl -e '
            chdir $ARGV[0] or die "chdir failed: $!";
            exec { $ARGV[1] } $ARGV[1] or die "exec failed: $!";
        ' "$workdir" "$bin"
    } < /dev/null > "$stdout_log" 2>&1 || exit_code=$?
    printf '%s\n' "$exit_code"
}

compile_rust() {
    local src="$1"
    local bin="$2"
    local compile_log="$3"
    $RUST_COBOLC "$src" -o "$bin" --source-format fixed --copy-path "$COPYLIB_DIR" \
        > /dev/null 2>"$compile_log"
}

compile_gnu() {
    local src="$1"
    local bin="$2"
    local compile_log="$3"
    "$GNU_COBOLC" -x -fixed -I "$COPYLIB_DIR" -o "$bin" "$src" \
        > /dev/null 2>"$compile_log"
}

compare_program() {
    local module="$1"
    local program="$2"
    local src="$PROGRAMS_DIR/$module/$program.cob"
    local out_dir="$RESULTS_DIR/$module/$program"
    local rust_src="$RUST_WORKDIR/${program}.cob"
    local gnu_src="$GNU_WORKDIR/${program}.cob"
    local rust_bin="$RUST_WORKDIR/${program}.bin"
    local gnu_bin="$GNU_WORKDIR/${program}.bin"
    local status_file="$RESULTS_DIR/$module/${program}.status"

    mkdir -p "$RESULTS_DIR/$module" "$out_dir"
    rm -f "$status_file"
    rm -f "$out_dir"/*

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        echo "  $program: SKIP (source not found)"
        return
    fi

    rm -rf "$RUST_TMPDIR"/* "$GNU_TMPDIR"/*
    NIST_TMPDIR="$RUST_TMPDIR" "$PREPROCESS" "$src" "$rust_src"
    NIST_TMPDIR="$GNU_TMPDIR" "$PREPROCESS" "$src" "$gnu_src"

    if ! compile_rust "$rust_src" "$rust_bin" "$out_dir/rust.compile.log"; then
        echo "RUST_COMPILE_ERROR" > "$status_file"
        echo "  $program: RUST COMPILE ERROR"
        return
    fi

    if ! compile_gnu "$gnu_src" "$gnu_bin" "$out_dir/gnu.compile.log"; then
        echo "GNU_COMPILE_ERROR" > "$status_file"
        echo "  $program: GNU COMPILE ERROR"
        return
    fi

    local rust_exit
    rust_exit="$(run_binary "$RUST_WORKDIR" "$rust_bin" "$out_dir/rust.stdout.log")"
    if [ "$rust_exit" -eq 124 ]; then
        echo "RUST_TIMEOUT" > "$status_file"
        echo "  $program: RUST TIMEOUT"
        return
    elif [ "$rust_exit" -ne 0 ]; then
        echo "RUST_RUNTIME_ERROR" > "$status_file"
        echo "  $program: RUST RUNTIME ERROR (exit $rust_exit)"
        return
    fi

    local gnu_exit
    gnu_exit="$(run_binary "$GNU_WORKDIR" "$gnu_bin" "$out_dir/gnu.stdout.log")"
    if [ "$gnu_exit" -eq 124 ]; then
        echo "GNU_TIMEOUT" > "$status_file"
        echo "  $program: GNU TIMEOUT"
        return
    elif [ "$gnu_exit" -ne 0 ]; then
        echo "GNU_RUNTIME_ERROR" > "$status_file"
        echo "  $program: GNU RUNTIME ERROR (exit $gnu_exit)"
        return
    fi

    local rust_report="$out_dir/rust.report"
    local gnu_report="$out_dir/gnu.report"
    if [ -s "$RUST_TMPDIR/P" ]; then
        cp "$RUST_TMPDIR/P" "$rust_report"
    else
        cp "$out_dir/rust.stdout.log" "$rust_report"
    fi
    if [ -s "$GNU_TMPDIR/P" ]; then
        cp "$GNU_TMPDIR/P" "$gnu_report"
    else
        cp "$out_dir/gnu.stdout.log" "$gnu_report"
    fi

    normalize_report "$rust_report" "$out_dir/rust.normalized.report"
    normalize_report "$gnu_report" "$out_dir/gnu.normalized.report"
    extract_ccvs_summary "$out_dir/rust.normalized.report" "$out_dir/rust.summary"
    extract_ccvs_summary "$out_dir/gnu.normalized.report" "$out_dir/gnu.summary"
    extract_ccvs_fail_items "$out_dir/rust.normalized.report" "$out_dir/rust.fail-items"
    extract_ccvs_fail_items "$out_dir/gnu.normalized.report" "$out_dir/gnu.fail-items"

    local summary_match=0
    local fail_items_match=0
    if write_comparison_diff "$out_dir/gnu.summary" "$out_dir/rust.summary" \
        "$out_dir/summary.diff"; then
        summary_match=1
    fi
    if write_comparison_diff "$out_dir/gnu.fail-items" "$out_dir/rust.fail-items" \
        "$out_dir/fail-items.diff"; then
        fail_items_match=1
    fi

    if [ "$summary_match" -eq 1 ] && [ "$fail_items_match" -eq 1 ]; then
        rm -f "$out_dir/report.diff"
        echo "MATCH" > "$status_file"
        echo "  $program: MATCH"
    else
        write_comparison_diff "$out_dir/gnu.normalized.report" \
            "$out_dir/rust.normalized.report" "$out_dir/report.diff" || true
        echo "MISMATCH" > "$status_file"
        echo "  $program: MISMATCH"
    fi
}

run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"
    local total=0 match=0 mismatch=0 rust_compile=0 gnu_compile=0
    local rust_runtime=0 gnu_runtime=0 rust_timeout=0 gnu_timeout=0
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
        compare_program "$module" "$program"
        case "$(cat "$RESULTS_DIR/$module/${program}.status")" in
            MATCH) match=$((match + 1)) ;;
            MISMATCH) mismatch=$((mismatch + 1)) ;;
            RUST_COMPILE_ERROR) rust_compile=$((rust_compile + 1)) ;;
            GNU_COMPILE_ERROR) gnu_compile=$((gnu_compile + 1)) ;;
            RUST_RUNTIME_ERROR) rust_runtime=$((rust_runtime + 1)) ;;
            GNU_RUNTIME_ERROR) gnu_runtime=$((gnu_runtime + 1)) ;;
            RUST_TIMEOUT) rust_timeout=$((rust_timeout + 1)) ;;
            GNU_TIMEOUT) gnu_timeout=$((gnu_timeout + 1)) ;;
        esac
    done
    echo ""
    echo "--- $module Summary ---"
    echo "  Total: $total | Match: $match | Mismatch: $mismatch"
    echo "  Rust CE: $rust_compile | GNU CE: $gnu_compile"
    echo "  Rust RE: $rust_runtime | GNU RE: $gnu_runtime"
    echo "  Rust TO: $rust_timeout | GNU TO: $gnu_timeout"
    cat > "$RESULTS_DIR/$module/summary.txt" <<EOF
Module: $module
Total: $total
Match: $match
Mismatch: $mismatch
Rust Compile Error: $rust_compile
GNU Compile Error: $gnu_compile
Rust Runtime Error: $rust_runtime
GNU Runtime Error: $gnu_runtime
Rust Timeout: $rust_timeout
GNU Timeout: $gnu_timeout
EOF
    echo ""
}

show_summary() {
    echo "=== NIST vs GnuCOBOL Summary ==="
    echo ""
    printf "%-6s %6s %7s %8s %7s %7s %7s %7s %7s %7s\n" \
        "Module" "Total" "Match" "Mismatch" "RustCE" "GNUCE" \
        "RustRE" "GNURE" "RustTO" "GNUTO"
    printf "%-6s %6s %7s %8s %7s %7s %7s %7s %7s %7s\n" \
        "------" "------" "-------" "--------" "-------" "-------" \
        "-------" "-------" "-------" "-------"
    local grand_total=0 grand_match=0 grand_mismatch=0 grand_rust_ce=0
    local grand_gnu_ce=0 grand_rust_re=0 grand_gnu_re=0 grand_rust_to=0 grand_gnu_to=0
    for module in "${ALL_MODULES[@]}"; do
        local summary="$RESULTS_DIR/$module/summary.txt"
        [ -f "$summary" ] || continue
        local total match mismatch rust_ce gnu_ce rust_re gnu_re rust_to gnu_to
        total="$(awk '/^Total:/ {print $2}' "$summary")"
        match="$(awk '/^Match:/ {print $2}' "$summary")"
        mismatch="$(awk '/^Mismatch:/ {print $2}' "$summary")"
        rust_ce="$(awk '/^Rust Compile Error:/ {print $4}' "$summary")"
        gnu_ce="$(awk '/^GNU Compile Error:/ {print $4}' "$summary")"
        rust_re="$(awk '/^Rust Runtime Error:/ {print $4}' "$summary")"
        gnu_re="$(awk '/^GNU Runtime Error:/ {print $4}' "$summary")"
        rust_to="$(awk '/^Rust Timeout:/ {print $3}' "$summary")"
        gnu_to="$(awk '/^GNU Timeout:/ {print $3}' "$summary")"
        printf "%-6s %6s %7s %8s %7s %7s %7s %7s %7s %7s\n" \
            "$module" "$total" "$match" "$mismatch" "$rust_ce" "$gnu_ce" \
            "$rust_re" "$gnu_re" "$rust_to" "$gnu_to"
        grand_total=$((grand_total + total))
        grand_match=$((grand_match + match))
        grand_mismatch=$((grand_mismatch + mismatch))
        grand_rust_ce=$((grand_rust_ce + rust_ce))
        grand_gnu_ce=$((grand_gnu_ce + gnu_ce))
        grand_rust_re=$((grand_rust_re + rust_re))
        grand_gnu_re=$((grand_gnu_re + gnu_re))
        grand_rust_to=$((grand_rust_to + rust_to))
        grand_gnu_to=$((grand_gnu_to + gnu_to))
    done
    printf "%-6s %6s %7s %8s %7s %7s %7s %7s %7s %7s\n" \
        "------" "------" "-------" "--------" "-------" "-------" \
        "-------" "-------" "-------" "-------"
    printf "%-6s %6d %7d %8d %7d %7d %7d %7d %7d %7d\n" \
        "TOTAL" "$grand_total" "$grand_match" "$grand_mismatch" \
        "$grand_rust_ce" "$grand_gnu_ce" "$grand_rust_re" "$grand_gnu_re" \
        "$grand_rust_to" "$grand_gnu_to"
}

require_programs

case "${1:-}" in
    --all)
        for module in "${ALL_MODULES[@]}"; do
            run_module "$module"
        done
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
            compare_program "$1" "$2"
        else
            run_module "$1"
        fi
        ;;
esac
