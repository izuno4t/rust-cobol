#!/usr/bin/env bash
set -euo pipefail

verifier_count_summary() {
    local file="$1"
    local pattern="$2"
    local value
    value="$(
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
    )"
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '0\n'
    fi
}

verifier_footer_errors() {
    local file="$1"
    local value
    value="$(
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
    )"
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '%s\n' ""
    fi
}

verifier_expected_flags() {
    local src="$1"
    local value
    value="$(
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
    )"
    if [ -n "$value" ]; then
        printf '%s\n' "$value"
    else
        printf '0\n'
    fi
}

verifier_compile_warnings() {
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

verifier_expected_case_count() {
    local src="$1"
    local count
    count="$(
        perl -ne '
            if (/MOVE\s+"[^"]+"\s+TO\s+PAR-NAME\./) {
                $count++;
            }
            END {
                print(($count || 0) . "\n");
            }
        ' "$src" 2>/dev/null | tail -n 1
    )"
    if [ -n "$count" ]; then
        printf '%s\n' "$count"
    else
        printf '0\n'
    fi
}

verifier_expected_feature_name() {
    local src="$1"
    local value
    value="$(
        perl -ne '
            if (/MOVE\s+"([^"]+)"\s+TO\s+FEATURE\./) {
                print "$1\n";
                exit;
            }
        ' "$src" 2>/dev/null | head -n 1
    )"
    printf '%s\n' "$value"
}

verifier_count_feature_rows() {
    local file="$1"
    local feature_name="$2"
    local count
    count="$(
        perl -ne '
            our $feature;
            BEGIN { $feature = shift @ARGV; }
            if ($feature ne "" && index($_, $feature) >= 0 && /PASS |FAIL\*|\*{5}/) {
                $count++;
            }
            END {
                print(($count || 0) . "\n");
            }
        ' "$feature_name" "$file" 2>/dev/null | tail -n 1
    )"
    if [ -n "$count" ]; then
        printf '%s\n' "$count"
    else
        printf '0\n'
    fi
}

verifier_has_non_whitespace() {
    local file="$1"
    [ -s "$file" ] || return 1
    grep -q '[^[:space:]]' "$file" 2>/dev/null
}

verifier_standard_ccvs() {
    local src="$1"
    local result_file="$2"
    local compile_log="$3"
    local pass fail ccvs_pass ccvs_failed ccvs_inspect footer_errors
    local expected_flags warning_count expected_cases feature_name feature_rows

    if [ ! -f "$result_file" ] || ! verifier_has_non_whitespace "$result_file"; then
        printf 'FAIL|blank-or-empty-report\n'
        return 0
    fi

    pass=$(grep -ca " PASS " "$result_file" 2>/dev/null) || pass=0
    fail=$(grep -ca "FAIL\*" "$result_file" 2>/dev/null) || fail=0
    ccvs_pass=$(verifier_count_summary "$result_file" 'TESTS WERE EXECUTED SUCCESSFULLY')
    ccvs_failed=$(verifier_count_summary "$result_file" 'TEST\(S\) FAILED')
    ccvs_inspect=$(verifier_count_summary "$result_file" 'TEST\(S\) REQUIRE INSPECTION')
    footer_errors="$(verifier_footer_errors "$result_file")"
    expected_cases="$(verifier_expected_case_count "$src")"
    feature_name="$(verifier_expected_feature_name "$src")"
    feature_rows="$(verifier_count_feature_rows "$result_file" "$feature_name")"

    if [ "$ccvs_failed" -gt 0 ] || [ "$fail" -gt 0 ]; then
        printf 'FAIL|%s passed, %s failed\n' "$ccvs_pass" "$ccvs_failed"
        return 0
    fi

    if [ -n "$footer_errors" ] && [ "$footer_errors" -gt 0 ]; then
        printf 'FAIL|%s error(s) reported in footer\n' "$footer_errors"
        return 0
    fi

    if [ -n "$footer_errors" ] && [ "$footer_errors" -eq 0 ]; then
        printf 'PASS|0 errors reported in footer\n'
        return 0
    fi

    if [ "$ccvs_inspect" -gt 0 ]; then
        printf 'FAIL|%s test(s) require inspection\n' "$ccvs_inspect"
        return 0
    fi

    if [ "$expected_cases" -gt 0 ] && [ "$feature_rows" -gt 0 ] && [ "$feature_rows" -ne "$expected_cases" ]; then
        printf 'FAIL|expected %s detail row(s), got %s\n' "$expected_cases" "$feature_rows"
        return 0
    fi

    if [ "$expected_cases" -gt 0 ] && [ "$ccvs_pass" -gt 0 ] && [ "$ccvs_pass" -ne "$expected_cases" ]; then
        printf 'FAIL|expected %s passed case(s), got %s\n' "$expected_cases" "$ccvs_pass"
        return 0
    fi

    if [ "$ccvs_pass" -gt 0 ]; then
        if [ "$expected_cases" -gt 0 ]; then
            printf 'PASS|%s/%s passed\n' "$ccvs_pass" "$expected_cases"
        else
            printf 'PASS|%s passed\n' "$ccvs_pass"
        fi
        return 0
    fi

    if [ "$pass" -gt 0 ] && [ "$fail" -eq 0 ]; then
        if [ "$expected_cases" -gt 0 ] && [ "$pass" -ne "$expected_cases" ]; then
            printf 'FAIL|expected %s passed line(s), got %s\n' "$expected_cases" "$pass"
        else
            printf 'PASS|%s passed\n' "$pass"
        fi
        return 0
    fi

    expected_flags="$(verifier_expected_flags "$src")"
    warning_count="$(verifier_compile_warnings "$compile_log")"
    if [ "$expected_flags" -gt 0 ]; then
        if [ "$warning_count" -eq "$expected_flags" ]; then
            printf 'PASS|%s warning flag(s) matched expected count\n' "$warning_count"
        else
            printf 'FAIL|expected %s warning flag(s), got %s\n' "$expected_flags" "$warning_count"
        fi
        return 0
    fi

    printf 'FAIL|no-decisive-ccvs-summary\n'
}

verifier_subprogram_standalone() {
    local src="$1"
    local result_file="$2"
    local compile_log="$3"
    local _src="$src"
    local _compile_log="$compile_log"
    if [ ! -f "$result_file" ] || ! verifier_has_non_whitespace "$result_file"; then
        printf 'PASS|subprogram standalone produced no report output\n'
        return 0
    fi
    verifier_standard_ccvs "$src" "$result_file" "$compile_log"
}

verifier_intrinsic_function() {
    local src="$1"
    local result_file="$2"
    local compile_log="$3"
    if [ ! -f "$result_file" ] || ! verifier_has_non_whitespace "$result_file"; then
        printf 'FAIL|intrinsic-function-report-missing\n'
        return 0
    fi
    verifier_standard_ccvs "$src" "$result_file" "$compile_log"
}

verifier_dummy_display() {
    local src="$1"
    local result_file="$2"
    local compile_log="$3"
    local expected_flags warning_count

    expected_flags="$(verifier_expected_flags "$src")"
    warning_count="$(verifier_compile_warnings "$compile_log")"

    if [ "$expected_flags" -gt 0 ]; then
        if [ "$warning_count" -eq "$expected_flags" ]; then
            printf 'PASS|%s warning flag(s) matched expected count\n' "$warning_count"
        else
            printf 'FAIL|expected %s warning flag(s), got %s\n' "$expected_flags" "$warning_count"
        fi
        return 0
    fi

    verifier_standard_ccvs "$src" "$result_file" "$compile_log"
}
