// COBOL Runtime - Intrinsic (built-in) functions
//
// Implements COBOL-85 and COBOL-2002 intrinsic functions such as
// CURRENT-DATE, LENGTH, TRIM, UPPER-CASE, LOWER-CASE, REVERSE,
// NUMVAL, MAX, MIN, MOD, INTEGER, ORD, and CHAR.
//
// All public functions use the C ABI for linking with generated code.

/// FUNCTION CURRENT-DATE
///
/// Returns the current date and time in the COBOL standard format:
///   YYYYMMDDHHMMSSFF+HHMM  (21 characters)
///
/// where FF is hundredths of a second and +HHMM is the UTC offset.
///
/// Returns the number of bytes written (up to 21).
///
/// # Safety
/// `buf` must be writable for at least `buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_current_date(buf: *mut u8, buf_len: u32) -> u32 {
    let out = std::slice::from_raw_parts_mut(buf, buf_len as usize);

    // Use libc to get local time and UTC offset portably.
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    libc::gettimeofday(&mut tv, std::ptr::null_mut());

    let mut tm: libc::tm = std::mem::zeroed();
    libc::localtime_r(&tv.tv_sec, &mut tm);

    // Format: YYYYMMDDHHMMSSFF+HHMM
    let year = tm.tm_year + 1900;
    let month = tm.tm_mon + 1;
    let day = tm.tm_mday;
    let hour = tm.tm_hour;
    let min = tm.tm_min;
    let sec = tm.tm_sec;
    let hundredths = tv.tv_usec / 10000;

    // UTC offset from tm_gmtoff (seconds east of UTC).
    let offset_secs = tm.tm_gmtoff;
    let offset_sign = if offset_secs >= 0 { '+' } else { '-' };
    let offset_abs = offset_secs.unsigned_abs() as i64;
    let offset_hours = offset_abs / 3600;
    let offset_mins = (offset_abs % 3600) / 60;

    let formatted = format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}{:02}{}{:02}{:02}",
        year, month, day, hour, min, sec, hundredths, offset_sign, offset_hours, offset_mins,
    );

    let bytes = formatted.as_bytes();
    let copy_len = bytes.len().min(out.len());
    out[..copy_len].copy_from_slice(&bytes[..copy_len]);
    copy_len as u32
}

/// FUNCTION LENGTH -- return the length of a string.
///
/// In COBOL, LENGTH returns the number of character positions.
/// For single-byte character sets this is the byte length.
#[no_mangle]
pub extern "C" fn cobol_func_length(_ptr: *const u8, len: u32) -> u32 {
    len
}

/// FUNCTION TRIM -- trim leading and/or trailing spaces.
///
/// Modes: 0 = both, 1 = leading only, 2 = trailing only.
///
/// Returns the number of bytes written to `dst_ptr`.
///
/// # Safety
/// `src_ptr` must be readable for `src_len` bytes.
/// `dst_ptr` must be writable for `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_trim(
    src_ptr: *const u8,
    src_len: u32,
    dst_ptr: *mut u8,
    dst_len: u32,
    mode: u32,
) -> u32 {
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);

    let trimmed: &[u8] = match mode {
        1 => {
            // Leading only.
            let start = src.iter().position(|&b| b != b' ').unwrap_or(src.len());
            &src[start..]
        }
        2 => {
            // Trailing only.
            let end = src.iter().rposition(|&b| b != b' ').map_or(0, |p| p + 1);
            &src[..end]
        }
        _ => {
            // Both.
            let start = src.iter().position(|&b| b != b' ').unwrap_or(src.len());
            let end = src.iter().rposition(|&b| b != b' ').map_or(0, |p| p + 1);
            if start >= end {
                &[]
            } else {
                &src[start..end]
            }
        }
    };

    let copy_len = trimmed.len().min(dst.len());
    dst[..copy_len].copy_from_slice(&trimmed[..copy_len]);
    copy_len as u32
}

/// FUNCTION UPPER-CASE -- convert to uppercase in-place.
///
/// Only handles ASCII characters (A-Z, a-z).
///
/// # Safety
/// `ptr` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_upper_case(ptr: *mut u8, len: u32) {
    let data = std::slice::from_raw_parts_mut(ptr, len as usize);
    for b in data.iter_mut() {
        *b = b.to_ascii_uppercase();
    }
}

/// FUNCTION LOWER-CASE -- convert to lowercase in-place.
///
/// # Safety
/// `ptr` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_lower_case(ptr: *mut u8, len: u32) {
    let data = std::slice::from_raw_parts_mut(ptr, len as usize);
    for b in data.iter_mut() {
        *b = b.to_ascii_lowercase();
    }
}

/// FUNCTION REVERSE -- reverse a byte string in-place.
///
/// # Safety
/// `ptr` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_reverse(ptr: *mut u8, len: u32) {
    let data = std::slice::from_raw_parts_mut(ptr, len as usize);
    data.reverse();
}

/// FUNCTION NUMVAL -- convert a display string to a numeric value.
///
/// Parses a string like " 123.45 " or " -67 " and returns a scaled
/// integer. The scale is always 0 (integer result). Use
/// `cobol_decimal_from_string` for full decimal parsing.
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_numval(ptr: *const u8, len: u32) -> i64 {
    cobol_func_numval_double(ptr, len) as i64
}

/// FUNCTION NUMVAL -- floating-point variant used when the receiving context
/// can preserve fractional digits.
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_numval_double(ptr: *const u8, len: u32) -> f64 {
    let data = std::slice::from_raw_parts(ptr, len as usize);
    let s = match std::str::from_utf8(data) {
        Ok(s) => s.trim(),
        Err(_) => return 0.0,
    };

    let mut cleaned: String = s
        .chars()
        .filter(|&c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))
        .collect();
    let negative = cleaned.starts_with('-') || cleaned.ends_with('-');
    let positive = cleaned.starts_with('+') || cleaned.ends_with('+');
    if negative || positive {
        cleaned = cleaned
            .trim_start_matches(['-', '+'])
            .trim_end_matches(['-', '+'])
            .to_string();
    }
    let value = cleaned.parse::<f64>().unwrap_or(0.0);
    if negative {
        -value
    } else {
        value
    }
}

/// FUNCTION MAX (integer variant) -- return the larger of two values.
#[no_mangle]
pub extern "C" fn cobol_func_max_int(a: i64, b: i64) -> i64 {
    a.max(b)
}

/// FUNCTION MIN (integer variant) -- return the smaller of two values.
#[no_mangle]
pub extern "C" fn cobol_func_min_int(a: i64, b: i64) -> i64 {
    a.min(b)
}

/// FUNCTION MAX (N-arg integer variant) -- return the largest of N values.
///
/// # Safety
/// `values` must point to a valid array of at least `count` `i64` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_max_int_n(values: *const i64, count: i32) -> i64 {
    if count <= 0 || values.is_null() {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts(values, count as usize) };
    slice.iter().copied().max().unwrap_or(0)
}

/// FUNCTION MIN (N-arg integer variant) -- return the smallest of N values.
///
/// # Safety
/// `values` must point to a valid array of at least `count` `i64` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_min_int_n(values: *const i64, count: i32) -> i64 {
    if count <= 0 || values.is_null() {
        return 0;
    }
    let slice = unsafe { core::slice::from_raw_parts(values, count as usize) };
    slice.iter().copied().min().unwrap_or(0)
}

/// FUNCTION MOD -- remainder (COBOL MOD semantics: result has the sign
/// of the divisor).
///
/// MOD(a, b) = a - b * FUNCTION INTEGER(a / b)
///
/// Returns 0 if b is 0.
#[no_mangle]
pub extern "C" fn cobol_func_mod(a: i64, b: i64) -> i64 {
    if b == 0 {
        return 0;
    }
    // Euclidean-like modulus: result has the sign of b.
    let r = a % b;
    if (r > 0 && b < 0) || (r < 0 && b > 0) {
        r + b
    } else {
        r
    }
}

/// FUNCTION INTEGER -- truncate a scaled value to its integer part.
///
/// Given a value with `scale` decimal places, return the integer
/// portion (truncated toward zero).
#[no_mangle]
pub extern "C" fn cobol_func_integer(value: i64, scale: i32) -> i64 {
    if scale <= 0 {
        return value;
    }
    let factor = 10_i64.pow(scale as u32);
    value / factor
}

/// FUNCTION ORD -- ordinal position of a character (1-based).
///
/// Returns the ordinal value (character code + 1) so that
/// ORD(CHAR(n)) == n.
#[no_mangle]
pub extern "C" fn cobol_func_ord(c: u8) -> u32 {
    c as u32 + 1
}

/// FUNCTION CHAR -- character from ordinal position (1-based).
///
/// Returns the character whose ordinal is `ord`. Inverse of ORD.
#[no_mangle]
pub extern "C" fn cobol_func_char(ord: u32) -> u8 {
    if ord == 0 {
        0
    } else {
        (ord - 1) as u8
    }
}

// ---------------------------------------------------------------------------
// Mathematical intrinsic functions
// ---------------------------------------------------------------------------

/// FUNCTION ABS -- absolute value (integer variant).
#[no_mangle]
pub extern "C" fn cobol_func_abs(value: i64) -> i64 {
    value.abs()
}

/// FUNCTION ABS -- absolute value (float variant).
#[no_mangle]
pub extern "C" fn cobol_func_abs_float(value: f64) -> f64 {
    value.abs()
}

/// FUNCTION SQRT -- square root.
#[no_mangle]
pub extern "C" fn cobol_func_sqrt(value: f64) -> f64 {
    value.sqrt()
}

/// FUNCTION EXP -- natural exponential (e^x).
#[no_mangle]
pub extern "C" fn cobol_func_exp(value: f64) -> f64 {
    value.exp()
}

/// FUNCTION EXP10 -- base-10 exponential (10^x).
#[no_mangle]
pub extern "C" fn cobol_func_exp10(value: f64) -> f64 {
    (10.0_f64).powf(value)
}

/// FUNCTION LOG -- natural logarithm.
#[no_mangle]
pub extern "C" fn cobol_func_log(value: f64) -> f64 {
    value.ln()
}

/// FUNCTION LOG10 -- base-10 logarithm.
#[no_mangle]
pub extern "C" fn cobol_func_log10(value: f64) -> f64 {
    value.log10()
}

/// FUNCTION SIN -- sine (radians).
#[no_mangle]
pub extern "C" fn cobol_func_sin(value: f64) -> f64 {
    value.sin()
}

/// FUNCTION COS -- cosine (radians).
#[no_mangle]
pub extern "C" fn cobol_func_cos(value: f64) -> f64 {
    value.cos()
}

/// FUNCTION TAN -- tangent (radians).
#[no_mangle]
pub extern "C" fn cobol_func_tan(value: f64) -> f64 {
    value.tan()
}

/// FUNCTION ASIN -- arc sine.
#[no_mangle]
pub extern "C" fn cobol_func_asin(value: f64) -> f64 {
    value.asin()
}

/// FUNCTION ACOS -- arc cosine.
#[no_mangle]
pub extern "C" fn cobol_func_acos(value: f64) -> f64 {
    value.acos()
}

/// FUNCTION ATAN -- arc tangent.
#[no_mangle]
pub extern "C" fn cobol_func_atan(value: f64) -> f64 {
    value.atan()
}

/// FUNCTION CEILING -- smallest integer >= value.
#[no_mangle]
pub extern "C" fn cobol_func_ceiling(value: f64) -> i64 {
    value.ceil() as i64
}

/// FUNCTION FLOOR -- largest integer <= value.
#[no_mangle]
pub extern "C" fn cobol_func_floor(value: f64) -> i64 {
    value.floor() as i64
}

/// FUNCTION FACTORIAL -- factorial of a non-negative integer.
#[no_mangle]
pub extern "C" fn cobol_func_factorial(n: i64) -> i64 {
    if n <= 1 {
        return 1;
    }
    let mut result: i64 = 1;
    for i in 2..=n {
        result = result.saturating_mul(i);
    }
    result
}

/// FUNCTION REM / REMAINDER -- truncated remainder.
#[no_mangle]
pub extern "C" fn cobol_func_rem(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return 0.0;
    }
    a - (a / b).trunc() * b
}

/// FUNCTION RANDOM -- pseudo-random number in [0, 1).
///
/// Uses a simple LCG. Pass seed > 0 to reseed.
#[no_mangle]
pub extern "C" fn cobol_func_random(seed: i64) -> f64 {
    static mut STATE: u64 = 12345;
    unsafe {
        if seed > 0 {
            STATE = seed as u64;
        }
        STATE = STATE
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (STATE >> 33) as f64 / (1u64 << 31) as f64
    }
}

/// FUNCTION SIGN -- sign of a value (-1, 0, or 1).
#[no_mangle]
pub extern "C" fn cobol_func_sign(value: i64) -> i64 {
    match value.cmp(&0) {
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Less => -1,
    }
}

/// FUNCTION MEAN -- arithmetic mean of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_mean(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    slice.iter().sum::<f64>() / count as f64
}

/// FUNCTION MEDIAN -- median of an array of f64 values.
///
/// Sorts a copy of the values and returns the middle element (or the
/// average of the two middle elements for even counts).
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_median(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut sorted: Vec<f64> = slice.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// FUNCTION RANGE -- max - min of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_range(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut min = slice[0];
    let mut max = slice[0];
    for &v in &slice[1..] {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    max - min
}

/// FUNCTION MIDRANGE -- (min + max) / 2 of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_midrange(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut min = slice[0];
    let mut max = slice[0];
    for &v in &slice[1..] {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    (min + max) / 2.0
}

/// FUNCTION VARIANCE -- population variance of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_variance(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mean = slice.iter().sum::<f64>() / count as f64;
    let sum_sq: f64 = slice.iter().map(|&v| (v - mean) * (v - mean)).sum();
    sum_sq / count as f64
}

/// FUNCTION STANDARD-DEVIATION -- population standard deviation of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_standard_deviation(values: *const f64, count: i32) -> f64 {
    cobol_func_variance(values, count).sqrt()
}

/// FUNCTION SUM (float variant) -- sum of an array of f64 values.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_sum_float(values: *const f64, count: i32) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    slice.iter().sum()
}

/// FUNCTION ANNUITY -- annuity factor.
///
/// If rate == 0, returns 1/periods. Otherwise rate / (1 - (1+rate)^(-periods)).
#[no_mangle]
pub extern "C" fn cobol_func_annuity(rate: f64, periods: i64) -> f64 {
    if periods <= 0 {
        return 0.0;
    }
    if rate == 0.0 {
        return 1.0 / periods as f64;
    }
    rate / (1.0 - (1.0 + rate).powf(-(periods as f64)))
}

/// FUNCTION PRESENT-VALUE -- present value of a series of future amounts.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_present_value(
    rate: f64,
    values: *const f64,
    count: i32,
) -> f64 {
    if count <= 0 {
        return 0.0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut sum = 0.0;
    for (i, &val) in slice.iter().enumerate() {
        sum += val / (1.0 + rate).powf((i + 1) as f64);
    }
    sum
}

// ---------------------------------------------------------------------------
// String intrinsic functions
// ---------------------------------------------------------------------------

/// FUNCTION CONCATENATE -- concatenate multiple strings.
///
/// Takes arrays of source pointers and their lengths. Writes the concatenated
/// result to `dst`, padding remaining space with spaces.
///
/// Returns the number of bytes written (always `dst_len`).
///
/// # Safety
/// `dst` must be writable for `dst_len` bytes.
/// `sources` and `lengths` must each be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_concatenate(
    dst: *mut u8,
    dst_len: u32,
    sources: *const *const u8,
    lengths: *const u32,
    count: i32,
) -> u32 {
    if dst.is_null() || count <= 0 {
        return 0;
    }
    let srcs = std::slice::from_raw_parts(sources, count as usize);
    let lens = std::slice::from_raw_parts(lengths, count as usize);
    let mut offset = 0u32;
    for i in 0..count as usize {
        if srcs[i].is_null() {
            continue;
        }
        let copy_len = lens[i].min(dst_len.saturating_sub(offset));
        if copy_len == 0 {
            break;
        }
        std::ptr::copy_nonoverlapping(srcs[i], dst.add(offset as usize), copy_len as usize);
        offset += copy_len;
    }
    // Pad remainder with spaces
    while offset < dst_len {
        *dst.add(offset as usize) = b' ';
        offset += 1;
    }
    offset
}

/// FUNCTION SUBSTITUTE -- replace all occurrences of a pattern in a string.
///
/// Simple single-pattern replacement. Writes the result to `dst`, padding
/// remaining space with spaces.
///
/// Returns the number of bytes written (always `dst_len`).
///
/// # Safety
/// `src` must be readable for `src_len` bytes.
/// `pattern` must be readable for `pat_len` bytes.
/// `replacement` must be readable for `rep_len` bytes.
/// `dst` must be writable for `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_substitute(
    src: *const u8,
    src_len: u32,
    pattern: *const u8,
    pat_len: u32,
    replacement: *const u8,
    rep_len: u32,
    dst: *mut u8,
    dst_len: u32,
) -> u32 {
    if src.is_null() || dst.is_null() || pat_len == 0 {
        if !src.is_null() && !dst.is_null() {
            let copy_len = src_len.min(dst_len);
            std::ptr::copy_nonoverlapping(src, dst, copy_len as usize);
            return copy_len;
        }
        return 0;
    }

    let src_slice = std::slice::from_raw_parts(src, src_len as usize);
    let pat_slice = std::slice::from_raw_parts(pattern, pat_len as usize);
    let rep_slice = std::slice::from_raw_parts(replacement, rep_len as usize);

    let mut offset = 0usize;
    let mut dst_offset = 0usize;
    let dst_max = dst_len as usize;

    while offset + pat_len as usize <= src_len as usize {
        if &src_slice[offset..offset + pat_len as usize] == pat_slice {
            // Match found: copy replacement
            let copy_len = rep_len as usize;
            if dst_offset + copy_len > dst_max {
                break;
            }
            std::ptr::copy_nonoverlapping(rep_slice.as_ptr(), dst.add(dst_offset), copy_len);
            dst_offset += copy_len;
            offset += pat_len as usize;
        } else {
            if dst_offset >= dst_max {
                break;
            }
            *dst.add(dst_offset) = src_slice[offset];
            dst_offset += 1;
            offset += 1;
        }
    }

    // Copy remaining source bytes
    while offset < src_len as usize && dst_offset < dst_max {
        *dst.add(dst_offset) = src_slice[offset];
        dst_offset += 1;
        offset += 1;
    }

    // Pad with spaces
    while dst_offset < dst_max {
        *dst.add(dst_offset) = b' ';
        dst_offset += 1;
    }

    dst_offset as u32
}

/// FUNCTION ORD-MAX -- ordinal position of the argument with the highest value.
///
/// Returns a 1-based index.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_ord_max(values: *const i64, count: i32) -> i64 {
    if count <= 0 {
        return 0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut max_idx = 0usize;
    for (i, &val) in slice.iter().enumerate() {
        if val > slice[max_idx] {
            max_idx = i;
        }
    }
    (max_idx + 1) as i64 // 1-based
}

/// FUNCTION ORD-MIN -- ordinal position of the argument with the lowest value.
///
/// Returns a 1-based index.
///
/// # Safety
/// `values` must be readable for `count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_ord_min(values: *const i64, count: i32) -> i64 {
    if count <= 0 {
        return 0;
    }
    let slice = std::slice::from_raw_parts(values, count as usize);
    let mut min_idx = 0usize;
    for (i, &val) in slice.iter().enumerate() {
        if val < slice[min_idx] {
            min_idx = i;
        }
    }
    (min_idx + 1) as i64 // 1-based
}

/// FUNCTION MAX (alphanumeric variant) -- return buffer index of the greatest value.
/// `ptrs` is an array of (ptr, len) pairs packed as (ptr: *const u8, len: u32) structs.
/// Returns the index (0-based) of the maximum element.
///
/// # Safety
/// All pointers in the array must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_max_alpha(
    ptrs: *const *const u8,
    lens: *const u32,
    count: i32,
) -> i32 {
    if count <= 0 {
        return 0;
    }
    let ptr_slice = core::slice::from_raw_parts(ptrs, count as usize);
    let len_slice = core::slice::from_raw_parts(lens, count as usize);
    let mut max_idx = 0i32;
    for i in 1..count as usize {
        let a = core::slice::from_raw_parts(
            ptr_slice[max_idx as usize],
            len_slice[max_idx as usize] as usize,
        );
        let b = core::slice::from_raw_parts(ptr_slice[i], len_slice[i] as usize);
        if b > a {
            max_idx = i as i32;
        }
    }
    max_idx
}

/// FUNCTION MIN (alphanumeric variant) -- return buffer index of the smallest value.
///
/// # Safety
/// All pointers in the array must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_min_alpha(
    ptrs: *const *const u8,
    lens: *const u32,
    count: i32,
) -> i32 {
    if count <= 0 {
        return 0;
    }
    let ptr_slice = core::slice::from_raw_parts(ptrs, count as usize);
    let len_slice = core::slice::from_raw_parts(lens, count as usize);
    let mut min_idx = 0i32;
    for i in 1..count as usize {
        let a = core::slice::from_raw_parts(
            ptr_slice[min_idx as usize],
            len_slice[min_idx as usize] as usize,
        );
        let b = core::slice::from_raw_parts(ptr_slice[i], len_slice[i] as usize);
        if b < a {
            min_idx = i as i32;
        }
    }
    min_idx
}

/// FUNCTION ORD-MAX (alphanumeric variant) -- 1-based ordinal position of the max.
///
/// # Safety
/// All pointers in the array must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_ord_max_alpha(
    ptrs: *const *const u8,
    lens: *const u32,
    count: i32,
) -> i64 {
    (cobol_func_max_alpha(ptrs, lens, count) + 1) as i64
}

/// FUNCTION ORD-MIN (alphanumeric variant) -- 1-based ordinal position of the min.
///
/// # Safety
/// All pointers in the array must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_ord_min_alpha(
    ptrs: *const *const u8,
    lens: *const u32,
    count: i32,
) -> i64 {
    (cobol_func_min_alpha(ptrs, lens, count) + 1) as i64
}

/// FUNCTION STORED-CHAR-LENGTH -- length of a string excluding trailing spaces.
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_stored_char_length(ptr: *const u8, len: u32) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let mut end = len as usize;
    while end > 0 && slice[end - 1] == b' ' {
        end -= 1;
    }
    end as u32
}

// ---------------------------------------------------------------------------
// Date/time intrinsic functions
// ---------------------------------------------------------------------------

/// INTEGER-OF-DATE: Converts YYYYMMDD to integer day count from day 1 of COBOL epoch.
/// COBOL epoch: January 1, 1601 = day 1.
#[no_mangle]
pub extern "C" fn cobol_func_integer_of_date(yyyymmdd: i64) -> i64 {
    let year = (yyyymmdd / 10000) as i32;
    let month = ((yyyymmdd % 10000) / 100) as u32;
    let day = (yyyymmdd % 100) as u32;

    let mut total_days: i64 = 0;
    for y in 1601..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }
    let days_in_months = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += days_in_months[m as usize] as i64;
        if m == 2 && is_leap_year(year) {
            total_days += 1;
        }
    }
    total_days += day as i64;
    total_days
}

/// DATE-OF-INTEGER: Converts integer day count to YYYYMMDD.
#[no_mangle]
pub extern "C" fn cobol_func_date_of_integer(day_count: i64) -> i64 {
    let mut remaining = day_count;
    let mut year = 1601i32;

    loop {
        let days_in_year: i64 = if is_leap_year(year) { 366 } else { 365 };
        if remaining <= days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let days_in_months = if is_leap_year(year) {
        [0, 31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    while month <= 12 && remaining > days_in_months[month as usize] as i64 {
        remaining -= days_in_months[month as usize] as i64;
        month += 1;
    }

    year as i64 * 10000 + month as i64 * 100 + remaining
}

/// INTEGER-OF-DAY: Converts YYYYDDD (Julian) to integer day count.
#[no_mangle]
pub extern "C" fn cobol_func_integer_of_day(yyyyddd: i64) -> i64 {
    let year = (yyyyddd / 1000) as i32;
    let day_of_year = yyyyddd % 1000;

    let mut total_days: i64 = 0;
    for y in 1601..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }
    total_days + day_of_year
}

/// DAY-OF-INTEGER: Converts integer day count to YYYYDDD (Julian).
#[no_mangle]
pub extern "C" fn cobol_func_day_of_integer(day_count: i64) -> i64 {
    let mut remaining = day_count;
    let mut year = 1601i32;

    loop {
        let days_in_year: i64 = if is_leap_year(year) { 366 } else { 365 };
        if remaining <= days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    year as i64 * 1000 + remaining
}

/// DATE-TO-YYYYMMDD: Converts 2-digit year date to 4-digit year.
#[no_mangle]
pub extern "C" fn cobol_func_date_to_yyyymmdd(yymmdd: i64, pivot: i64) -> i64 {
    let yy = yymmdd / 10000;
    let mmdd = yymmdd % 10000;
    let pivot_yy = pivot % 100;
    let pivot_century = pivot / 100 * 100;

    let yyyy = if yy <= pivot_yy {
        pivot_century + yy
    } else {
        pivot_century - 100 + yy
    };
    yyyy * 10000 + mmdd
}

/// YEAR-TO-YYYY: Converts 2-digit year to 4-digit year.
#[no_mangle]
pub extern "C" fn cobol_func_year_to_yyyy(yy: i64, pivot: i64) -> i64 {
    let pivot_yy = pivot % 100;
    let pivot_century = pivot / 100 * 100;
    if yy <= pivot_yy {
        pivot_century + yy
    } else {
        pivot_century - 100 + yy
    }
}

/// DAY-TO-YYYYDDD: Converts 2-digit year Julian date to 4-digit year.
#[no_mangle]
pub extern "C" fn cobol_func_day_to_yyyyddd(yyddd: i64, pivot: i64) -> i64 {
    let yy = yyddd / 1000;
    let ddd = yyddd % 1000;
    let yyyy = cobol_func_year_to_yyyy(yy, pivot);
    yyyy * 1000 + ddd
}

/// TEST-DATE-YYYYMMDD: Returns 0 if valid date, non-zero otherwise.
#[no_mangle]
pub extern "C" fn cobol_func_test_date_yyyymmdd(yyyymmdd: i64) -> i64 {
    let year = (yyyymmdd / 10000) as i32;
    let month = ((yyyymmdd % 10000) / 100) as i32;
    let day = (yyyymmdd % 100) as i32;

    if !(1601..=9999).contains(&year) {
        return 1;
    }
    if !(1..=12).contains(&month) {
        return 1;
    }

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => return 1,
    };

    if day < 1 || day > max_day {
        return 1;
    }
    0
}

/// TEST-DAY-YYYYDDD: Returns 0 if valid Julian date, non-zero otherwise.
#[no_mangle]
pub extern "C" fn cobol_func_test_day_yyyyddd(yyyyddd: i64) -> i64 {
    let year = (yyyyddd / 1000) as i32;
    let ddd = (yyyyddd % 1000) as i32;

    if !(1601..=9999).contains(&year) {
        return 1;
    }
    let max_ddd = if is_leap_year(year) { 366 } else { 365 };
    if !(1..=max_ddd).contains(&ddd) {
        return 1;
    }
    0
}

/// WHEN-COMPILED: Returns compilation timestamp.
/// For now, returns current time (we don't have compile-time info at runtime).
///
/// # Safety
/// `buf` must be writable for at least `buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_when_compiled(buf: *mut u8, buf_len: u32) -> u32 {
    cobol_func_current_date(buf, buf_len)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_date() {
        let mut buf = [0u8; 21];
        let written = unsafe { cobol_func_current_date(buf.as_mut_ptr(), 21) };
        assert_eq!(written, 21);
        // Verify format: YYYYMMDDHHMMSSFF+HHMM
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s.len(), 21);
        // Year should start with "20" for current century.
        assert!(
            s.starts_with("20"),
            "Expected date to start with 20, got: {}",
            s
        );
        // Sign character at position 16.
        let sign = s.as_bytes()[16];
        assert!(sign == b'+' || sign == b'-');
    }

    #[test]
    fn test_length() {
        assert_eq!(cobol_func_length(std::ptr::null(), 0), 0);
        assert_eq!(cobol_func_length(std::ptr::null(), 42), 42);
    }

    #[test]
    fn test_trim_both() {
        let src = b"  HELLO  ";
        let mut dst = [0u8; 20];
        let len = unsafe { cobol_func_trim(src.as_ptr(), 9, dst.as_mut_ptr(), 20, 0) };
        assert_eq!(len, 5);
        assert_eq!(&dst[..5], b"HELLO");
    }

    #[test]
    fn test_trim_leading() {
        let src = b"  HELLO  ";
        let mut dst = [0u8; 20];
        let len = unsafe { cobol_func_trim(src.as_ptr(), 9, dst.as_mut_ptr(), 20, 1) };
        assert_eq!(len, 7);
        assert_eq!(&dst[..7], b"HELLO  ");
    }

    #[test]
    fn test_trim_trailing() {
        let src = b"  HELLO  ";
        let mut dst = [0u8; 20];
        let len = unsafe { cobol_func_trim(src.as_ptr(), 9, dst.as_mut_ptr(), 20, 2) };
        assert_eq!(len, 7);
        assert_eq!(&dst[..7], b"  HELLO");
    }

    #[test]
    fn test_upper_case() {
        let mut data = *b"Hello World";
        unsafe { cobol_func_upper_case(data.as_mut_ptr(), 11) };
        assert_eq!(&data, b"HELLO WORLD");
    }

    #[test]
    fn test_lower_case() {
        let mut data = *b"Hello World";
        unsafe { cobol_func_lower_case(data.as_mut_ptr(), 11) };
        assert_eq!(&data, b"hello world");
    }

    #[test]
    fn test_reverse() {
        let mut data = *b"ABCDE";
        unsafe { cobol_func_reverse(data.as_mut_ptr(), 5) };
        assert_eq!(&data, b"EDCBA");
    }

    #[test]
    fn test_numval() {
        let s = b"  123  ";
        let v = unsafe { cobol_func_numval(s.as_ptr(), s.len() as u32) };
        assert_eq!(v, 123);

        let s2 = b" -42 ";
        let v2 = unsafe { cobol_func_numval(s2.as_ptr(), s2.len() as u32) };
        assert_eq!(v2, -42);

        let s3 = b" 1,234 ";
        let v3 = unsafe { cobol_func_numval(s3.as_ptr(), s3.len() as u32) };
        assert_eq!(v3, 1234);
    }

    #[test]
    fn test_max_min() {
        assert_eq!(cobol_func_max_int(10, 20), 20);
        assert_eq!(cobol_func_max_int(-5, -10), -5);
        assert_eq!(cobol_func_min_int(10, 20), 10);
        assert_eq!(cobol_func_min_int(-5, -10), -10);
    }

    #[test]
    fn test_mod() {
        assert_eq!(cobol_func_mod(10, 3), 1);
        assert_eq!(cobol_func_mod(-10, 3), 2); // COBOL MOD: sign of divisor
        assert_eq!(cobol_func_mod(10, -3), -2);
        assert_eq!(cobol_func_mod(10, 0), 0); // division by zero => 0
    }

    #[test]
    fn test_integer() {
        assert_eq!(cobol_func_integer(12345, 2), 123); // 123.45 -> 123
        assert_eq!(cobol_func_integer(-12345, 2), -123); // -123.45 -> -123
        assert_eq!(cobol_func_integer(42, 0), 42);
    }

    #[test]
    fn test_ord_char() {
        assert_eq!(cobol_func_ord(b'A'), 66); // ASCII 65 + 1 = 66
        assert_eq!(cobol_func_char(66), b'A');
        assert_eq!(cobol_func_char(cobol_func_ord(b'Z')), b'Z');
        assert_eq!(cobol_func_ord(cobol_func_char(1)), 1);
    }

    #[test]
    fn test_abs() {
        assert_eq!(cobol_func_abs(-42), 42);
        assert_eq!(cobol_func_abs(42), 42);
        assert_eq!(cobol_func_abs(0), 0);
    }

    #[test]
    fn test_abs_float() {
        assert!((cobol_func_abs_float(-2.75) - 2.75).abs() < 1e-10);
        assert!((cobol_func_abs_float(2.75) - 2.75).abs() < 1e-10);
    }

    #[test]
    fn test_sqrt() {
        assert!((cobol_func_sqrt(4.0) - 2.0).abs() < 1e-10);
        assert!((cobol_func_sqrt(9.0) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_exp_log() {
        assert!((cobol_func_exp(0.0) - 1.0).abs() < 1e-10);
        assert!((cobol_func_log(1.0)).abs() < 1e-10);
        assert!((cobol_func_exp(cobol_func_log(5.0)) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_exp10_log10() {
        assert!((cobol_func_exp10(2.0) - 100.0).abs() < 1e-10);
        assert!((cobol_func_log10(100.0) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_trig() {
        assert!((cobol_func_sin(0.0)).abs() < 1e-10);
        assert!((cobol_func_cos(0.0) - 1.0).abs() < 1e-10);
        assert!((cobol_func_tan(0.0)).abs() < 1e-10);
    }

    #[test]
    fn test_inverse_trig() {
        assert!((cobol_func_asin(0.0)).abs() < 1e-10);
        assert!((cobol_func_acos(1.0)).abs() < 1e-10);
        assert!((cobol_func_atan(0.0)).abs() < 1e-10);
    }

    #[test]
    fn test_ceiling_floor() {
        assert_eq!(cobol_func_ceiling(2.3), 3);
        assert_eq!(cobol_func_ceiling(-2.3), -2);
        assert_eq!(cobol_func_floor(2.7), 2);
        assert_eq!(cobol_func_floor(-2.7), -3);
    }

    #[test]
    fn test_factorial() {
        assert_eq!(cobol_func_factorial(0), 1);
        assert_eq!(cobol_func_factorial(1), 1);
        assert_eq!(cobol_func_factorial(5), 120);
        assert_eq!(cobol_func_factorial(10), 3628800);
    }

    #[test]
    fn test_rem() {
        assert!((cobol_func_rem(10.0, 3.0) - 1.0).abs() < 1e-10);
        assert!((cobol_func_rem(10.0, 0.0)).abs() < 1e-10);
    }

    #[test]
    fn test_random() {
        let r1 = cobol_func_random(42);
        assert!((0.0..1.0).contains(&r1));
        let r2 = cobol_func_random(0);
        assert!((0.0..1.0).contains(&r2));
    }

    #[test]
    fn test_sign() {
        assert_eq!(cobol_func_sign(42), 1);
        assert_eq!(cobol_func_sign(0), 0);
        assert_eq!(cobol_func_sign(-42), -1);
    }

    #[test]
    fn test_mean() {
        let vals = [1.0, 2.0, 3.0, 4.0, 5.0];
        let m = unsafe { cobol_func_mean(vals.as_ptr(), 5) };
        assert!((m - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_sum_float() {
        let vals = [1.0, 2.0, 3.0];
        let s = unsafe { cobol_func_sum_float(vals.as_ptr(), 3) };
        assert!((s - 6.0).abs() < 1e-10);
    }

    #[test]
    fn test_annuity() {
        // Zero rate: 1/periods
        assert!((cobol_func_annuity(0.0, 4) - 0.25).abs() < 1e-10);
        // Non-zero rate
        let a = cobol_func_annuity(0.1, 10);
        assert!(a > 0.0);
    }

    #[test]
    fn test_present_value() {
        let vals = [100.0, 100.0, 100.0];
        let pv = unsafe { cobol_func_present_value(0.1, vals.as_ptr(), 3) };
        assert!(pv > 0.0);
        // PV at 0% rate should equal sum
        let pv0 = unsafe { cobol_func_present_value(0.0, vals.as_ptr(), 3) };
        assert!((pv0 - 300.0).abs() < 1e-10);
    }

    #[test]
    fn test_concatenate() {
        let s1 = b"HELLO";
        let s2 = b" WORLD";
        let sources = [s1.as_ptr(), s2.as_ptr()];
        let lengths = [5u32, 6u32];
        let mut dst = [0u8; 20];
        let written = unsafe {
            cobol_func_concatenate(dst.as_mut_ptr(), 20, sources.as_ptr(), lengths.as_ptr(), 2)
        };
        assert_eq!(written, 20);
        assert_eq!(&dst[..11], b"HELLO WORLD");
        assert!(dst[11..].iter().all(|&b| b == b' '));
    }

    #[test]
    fn test_substitute() {
        let src = b"HELLO WORLD";
        let pat = b"WORLD";
        let rep = b"COBOL";
        let mut dst = [0u8; 20];
        let written = unsafe {
            cobol_func_substitute(
                src.as_ptr(),
                11,
                pat.as_ptr(),
                5,
                rep.as_ptr(),
                5,
                dst.as_mut_ptr(),
                20,
            )
        };
        assert_eq!(written, 20);
        assert_eq!(&dst[..11], b"HELLO COBOL");
        assert!(dst[11..].iter().all(|&b| b == b' '));
    }

    #[test]
    fn test_stored_char_length() {
        let data = b"HELLO     ";
        let result = unsafe { cobol_func_stored_char_length(data.as_ptr(), data.len() as u32) };
        assert_eq!(result, 5);
    }

    #[test]
    fn test_stored_char_length_all_spaces() {
        let data = b"          ";
        let result = unsafe { cobol_func_stored_char_length(data.as_ptr(), data.len() as u32) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_median_odd() {
        let vals = [3.0, 1.0, 2.0];
        let m = unsafe { cobol_func_median(vals.as_ptr(), 3) };
        assert!((m - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_median_even() {
        let vals = [4.0, 1.0, 3.0, 2.0];
        let m = unsafe { cobol_func_median(vals.as_ptr(), 4) };
        assert!((m - 2.5).abs() < 1e-10);
    }

    #[test]
    fn test_range() {
        let vals = [1.0, 5.0, 3.0, 2.0];
        let r = unsafe { cobol_func_range(vals.as_ptr(), 4) };
        assert!((r - 4.0).abs() < 1e-10); // 5 - 1 = 4
    }

    #[test]
    fn test_midrange() {
        let vals = [1.0, 5.0, 3.0, 2.0];
        let m = unsafe { cobol_func_midrange(vals.as_ptr(), 4) };
        assert!((m - 3.0).abs() < 1e-10); // (1+5)/2 = 3
    }

    #[test]
    fn test_variance() {
        let vals = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let v = unsafe { cobol_func_variance(vals.as_ptr(), 8) };
        assert!((v - 4.0).abs() < 1e-10); // population variance = 4.0
    }

    #[test]
    fn test_standard_deviation() {
        let vals = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let sd = unsafe { cobol_func_standard_deviation(vals.as_ptr(), 8) };
        assert!((sd - 2.0).abs() < 1e-10); // sqrt(4) = 2
    }

    #[test]
    fn test_ord_max() {
        let values = [10i64, 30, 20, 5];
        let result = unsafe { cobol_func_ord_max(values.as_ptr(), 4) };
        assert_eq!(result, 2); // 30 is at position 2 (1-based)
    }

    #[test]
    fn test_ord_min() {
        let values = [10i64, 30, 20, 5];
        let result = unsafe { cobol_func_ord_min(values.as_ptr(), 4) };
        assert_eq!(result, 4); // 5 is at position 4 (1-based)
    }

    #[test]
    fn test_integer_of_date() {
        // 1601-01-01 = day 1
        assert_eq!(cobol_func_integer_of_date(16010101), 1);
        // 1601-01-31 = day 31
        assert_eq!(cobol_func_integer_of_date(16010131), 31);
        // 1601-02-01 = day 32
        assert_eq!(cobol_func_integer_of_date(16010201), 32);
    }

    #[test]
    fn test_date_of_integer() {
        assert_eq!(cobol_func_date_of_integer(1), 16010101);
        assert_eq!(cobol_func_date_of_integer(31), 16010131);
        assert_eq!(cobol_func_date_of_integer(32), 16010201);
    }

    #[test]
    fn test_date_roundtrip() {
        let date = 20260319i64;
        let int_val = cobol_func_integer_of_date(date);
        let back = cobol_func_date_of_integer(int_val);
        assert_eq!(back, date);
    }

    #[test]
    fn test_integer_of_day() {
        // 1601-001 = day 1
        assert_eq!(cobol_func_integer_of_day(1601001), 1);
        // 1601-032 = day 32
        assert_eq!(cobol_func_integer_of_day(1601032), 32);
    }

    #[test]
    fn test_day_of_integer() {
        assert_eq!(cobol_func_day_of_integer(1), 1601001);
        assert_eq!(cobol_func_day_of_integer(32), 1601032);
    }

    #[test]
    fn test_test_date_yyyymmdd() {
        assert_eq!(cobol_func_test_date_yyyymmdd(20260319), 0); // valid
        assert_eq!(cobol_func_test_date_yyyymmdd(20260230), 1); // invalid (Feb 30)
        assert_eq!(cobol_func_test_date_yyyymmdd(20260001), 1); // invalid month
        assert_eq!(cobol_func_test_date_yyyymmdd(20261301), 1); // month 13
    }

    #[test]
    fn test_test_day_yyyyddd() {
        assert_eq!(cobol_func_test_day_yyyyddd(2026078), 0); // valid (day 78)
        assert_eq!(cobol_func_test_day_yyyyddd(2026000), 1); // invalid (day 0)
        assert_eq!(cobol_func_test_day_yyyyddd(2026366), 1); // invalid (non-leap 366)
        assert_eq!(cobol_func_test_day_yyyyddd(2024366), 0); // valid (leap year 366)
    }

    #[test]
    fn test_year_to_yyyy() {
        // Pivot 2050: years 00-50 -> 2000-2050, years 51-99 -> 1951-1999
        assert_eq!(cobol_func_year_to_yyyy(26, 2050), 2026);
        assert_eq!(cobol_func_year_to_yyyy(99, 2050), 1999);
        assert_eq!(cobol_func_year_to_yyyy(50, 2050), 2050);
        assert_eq!(cobol_func_year_to_yyyy(51, 2050), 1951);
    }

    #[test]
    fn test_date_to_yyyymmdd() {
        assert_eq!(cobol_func_date_to_yyyymmdd(260319, 2050), 20260319);
        assert_eq!(cobol_func_date_to_yyyymmdd(990101, 2050), 19990101);
    }

    #[test]
    fn test_day_to_yyyyddd() {
        assert_eq!(cobol_func_day_to_yyyyddd(26078, 2050), 2026078);
        assert_eq!(cobol_func_day_to_yyyyddd(99001, 2050), 1999001);
    }
}
