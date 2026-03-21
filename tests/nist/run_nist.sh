#!/usr/bin/env bash
# run_nist.sh — Compile and run NIST CCVS 85 test programs
#
# Usage:
#   ./run_nist.sh [module]           # Run all programs in a module (e.g., NC)
#   ./run_nist.sh [module] [program] # Run a single program (e.g., NC NC101A)
#   ./run_nist.sh --all              # Run all modules
#   ./run_nist.sh --summary          # Show summary of previous runs
#
# Prerequisites:
#   - cobolc must be in PATH (or built via cargo)
#   - Test programs extracted to programs/ via extract.pl

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROGRAMS_DIR="$SCRIPT_DIR/programs"
RESULTS_DIR="$SCRIPT_DIR/results"
COBOLC="${COBOLC:-cargo run --release --package cobol-driver --}"
COPYLIB_DIR="$PROGRAMS_DIR/COPYLIB"
# Working directory for test execution — keeps output files out of the repo root
export NIST_WORKDIR="$REPO_ROOT/target/nist"

# Module execution order (by priority)
ALL_MODULES=(NC SM IC SQ IF IX RL ST RW DB SG OB)

mkdir -p "$RESULTS_DIR"
mkdir -p "$NIST_WORKDIR"

# Compile and run a single test program
run_program() {
    local module="$1"
    local program="$2"
    local src="$PROGRAMS_DIR/$module/$program.cob"
    local bin="$NIST_WORKDIR/nist_${program}"
    local log="$RESULTS_DIR/${module}/${program}.log"
    local status_file="$RESULTS_DIR/${module}/${program}.status"

    mkdir -p "$RESULTS_DIR/$module"

    if [ ! -f "$src" ]; then
        echo "SKIP" > "$status_file"
        echo "  $program: SKIP (source not found)"
        return
    fi

    # Preprocess (replace XXXXX placeholders)
    local preprocessed="$NIST_WORKDIR/nist_preproc_${program}.cob"
    "$SCRIPT_DIR/preprocess.sh" "$src" "$preprocessed"

    # Compile
    local compile_log="$RESULTS_DIR/${module}/${program}.compile.log"
    if $COBOLC "$preprocessed" -o "$bin" --source-format fixed --copy-path "$COPYLIB_DIR" 2>"$compile_log"; then
        # Run with timeout (30 seconds)
        # Execute in target/nist so output files don't pollute the repo
        if timeout 30 bash -c "cd \"$NIST_WORKDIR\" && \"$bin\"" < /dev/null > "$log" 2>&1; then
            # NIST programs write to PRINT-FILE, also check stdout
            local print_file="/tmp/nist/P"
            local result_file="$log"
            if [ -f "$print_file" ] && [ -s "$print_file" ]; then
                result_file="$print_file"
                cp "$print_file" "$log"
            fi

            # Parse results — count PASS/FAIL in output
            local pass
            pass=$(grep -ca " PASS " "$result_file" 2>/dev/null) || pass=0
            local fail
            fail=$(grep -ca "FAIL\*" "$result_file" 2>/dev/null) || fail=0

            # Clean up print file for next test
            rm -f "$print_file"

            if [ "$fail" -eq 0 ] && [ "$pass" -gt 0 ]; then
                echo "PASS" > "$status_file"
                echo "  $program: PASS ($pass passed)"
            elif [ "$fail" -gt 0 ]; then
                echo "FAIL" > "$status_file"
                echo "  $program: FAIL ($pass passed, $fail failed)"
            else
                echo "INSPECT" > "$status_file"
                echo "  $program: INSPECT (manual review needed)"
            fi
        else
            local exit_code=$?
            if [ "$exit_code" -eq 124 ]; then
                echo "TIMEOUT" > "$status_file"
                echo "  $program: TIMEOUT (exceeded 30s)"
            else
                echo "RUNTIME_ERROR" > "$status_file"
                echo "  $program: RUNTIME ERROR (exit $exit_code)"
            fi
        fi
    else
        echo "COMPILE_ERROR" > "$status_file"
        echo "  $program: COMPILE ERROR"
    fi

    # Cleanup
    rm -f "$bin" "$preprocessed"
}

# Run all programs in a module
run_module() {
    local module="$1"
    local mod_dir="$PROGRAMS_DIR/$module"

    if [ ! -d "$mod_dir" ]; then
        echo "Module $module: no programs found in $mod_dir"
        return
    fi

    local total=0
    local pass=0
    local fail=0
    local compile_err=0
    local runtime_err=0
    local timeout=0
    local skip=0

    echo "=== Module: $module ==="

    for src in "$mod_dir"/*.cob; do
        [ -f "$src" ] || continue
        local program=$(basename "$src" .cob)
        total=$((total + 1))
        run_program "$module" "$program"

        # Read status
        local status_file="$RESULTS_DIR/${module}/${program}.status"
        if [ -f "$status_file" ]; then
            case "$(cat "$status_file")" in
                PASS) pass=$((pass + 1)) ;;
                FAIL) fail=$((fail + 1)) ;;
                COMPILE_ERROR) compile_err=$((compile_err + 1)) ;;
                RUNTIME_ERROR) runtime_err=$((runtime_err + 1)) ;;
                TIMEOUT) timeout=$((timeout + 1)) ;;
                SKIP) skip=$((skip + 1)) ;;
            esac
        fi
    done

    # Module summary
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
    echo ""

    # Save module summary
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

# Show summary of all modules
show_summary() {
    echo "=== NIST CCVS 85 — Results Summary ==="
    echo ""

    local grand_total=0
    local grand_pass=0
    local grand_fail=0
    local grand_cerr=0
    local grand_rerr=0

    printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
        "Module" "Total" "Pass" "Fail" "CErr" "RErr" "Rate"
    printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
        "------" "------" "------" "------" "------" "------" "--------"

    for module in "${ALL_MODULES[@]}"; do
        local summary="$RESULTS_DIR/$module/summary.txt"
        if [ -f "$summary" ]; then
            local total=$(grep "^Total:" "$summary" | awk '{print $2}')
            local pass=$(grep "^Pass:" "$summary" | awk '{print $2}')
            local fail=$(grep "^Fail:" "$summary" | awk '{print $2}')
            local cerr=$(grep "^Compile Error:" "$summary" | awk '{print $3}')
            local rerr=$(grep "^Runtime Error:" "$summary" | awk '{print $3}')
            local rate=$(grep "^Pass Rate:" "$summary" | awk '{print $3}')

            printf "%-6s %6s %6s %6s %6s %6s %8s\n" \
                "$module" "$total" "$pass" "$fail" "$cerr" "$rerr" "$rate"

            grand_total=$((grand_total + total))
            grand_pass=$((grand_pass + pass))
            grand_fail=$((grand_fail + fail))
            grand_cerr=$((grand_cerr + cerr))
            grand_rerr=$((grand_rerr + rerr))
        fi
    done

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

# Main
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
        echo "  $0 <MODULE>              Run a module (e.g., NC)"
        echo "  $0 <MODULE> <PROGRAM>    Run one program (e.g., NC NC101A)"
        echo "  $0 --all                 Run all modules"
        echo "  $0 --summary             Show results summary"
        ;;
    *)
        module="$1"
        if [ -n "${2:-}" ]; then
            run_program "$module" "$2"
        else
            run_module "$module"
        fi
        ;;
esac
