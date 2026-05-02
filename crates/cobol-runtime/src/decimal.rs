// COBOL Runtime - BCD/Decimal arithmetic engine
//
// COBOL uses fixed-point decimal arithmetic to avoid floating-point rounding
// errors. Numbers are stored as scaled integers: e.g. PIC S9(5)V99 value
// 123.45 is represented as i64 12345 with scale=2.
//
// All public functions use the C ABI so they can be called from generated code.

/// COBOL decimal number stored as a scaled integer.
///
/// For example, PIC S9(5)V99 with value 123.45 is stored as:
///   value = 12345, scale = 2, size = 7, is_signed = true
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CobolDecimal {
    /// Scaled integer value (the "unscaled" representation).
    pub value: i64,
    /// Number of decimal places (digits after the implied decimal point).
    pub scale: i32,
    /// Total number of digit positions in the PICTURE clause.
    pub size: i32,
    /// Whether the field is signed (PIC S9...).
    pub is_signed: bool,
}

const POW10_I64: [i64; 19] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
];

const POW10_I128: [i128; 39] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
    10_000_000_000_000,
    100_000_000_000_000,
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
    1_000_000_000_000_000_000,
    10_000_000_000_000_000_000,
    100_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000_000_000_000_000,
    1_000_000_000_000_000_000_000_000_000_000_000_000,
    10_000_000_000_000_000_000_000_000_000_000_000_000,
    100_000_000_000_000_000_000_000_000_000_000_000_000,
];

const POW10_F64: [f64; 19] = [
    1.0,
    10.0,
    100.0,
    1_000.0,
    10_000.0,
    100_000.0,
    1_000_000.0,
    10_000_000.0,
    100_000_000.0,
    1_000_000_000.0,
    10_000_000_000.0,
    100_000_000_000.0,
    1_000_000_000_000.0,
    10_000_000_000_000.0,
    100_000_000_000_000.0,
    1_000_000_000_000_000.0,
    10_000_000_000_000_000.0,
    100_000_000_000_000_000.0,
    1_000_000_000_000_000_000.0,
];

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Align two decimals to the same scale (the larger of the two) by
/// multiplying the smaller-scale operand by the appropriate power of 10.
/// Returns (aligned_a_value, aligned_b_value, common_scale) using i128
/// to avoid overflow during scale alignment.
fn align_scales(a: &CobolDecimal, b: &CobolDecimal) -> (i128, i128, i32) {
    if a.scale == b.scale {
        (a.value as i128, b.value as i128, a.scale)
    } else if a.scale < b.scale {
        let diff = (b.scale - a.scale) as u32;
        let factor = pow10_i128(diff);
        (a.value as i128 * factor, b.value as i128, b.scale)
    } else {
        let diff = (a.scale - b.scale) as u32;
        let factor = pow10_i128(diff);
        (a.value as i128, b.value as i128 * factor, a.scale)
    }
}

#[inline]
fn pow10_i64(exp: u32) -> i64 {
    POW10_I64
        .get(exp as usize)
        .copied()
        .unwrap_or_else(|| 10_i64.saturating_pow(exp))
}

#[inline]
fn pow10_i128(exp: u32) -> i128 {
    POW10_I128
        .get(exp as usize)
        .copied()
        .unwrap_or_else(|| 10_i128.saturating_pow(exp))
}

#[inline]
fn pow10_f64(exp: i32) -> f64 {
    if exp <= 0 {
        1.0
    } else {
        POW10_F64
            .get(exp as usize)
            .copied()
            .unwrap_or_else(|| 10_f64.powi(exp))
    }
}

/// Truncate or clamp a value so that it fits within `size` digit positions
/// (plus optional sign). If the field is unsigned, negative values become 0.
fn clamp_to_size(value: i64, size: i32, is_signed: bool) -> i64 {
    if size <= 0 {
        return 0;
    }
    if size >= 19 {
        // i64 max is 19 digits; no clamping needed.
        if !is_signed && value < 0 {
            return 0;
        }
        return value;
    }
    if !is_signed && value < 0 {
        return 0;
    }
    let max = 10_i64.pow(size as u32) - 1;
    if is_signed {
        value.clamp(-max, max)
    } else {
        value.clamp(0, max)
    }
}

// ---------------------------------------------------------------------------
// C ABI functions
// ---------------------------------------------------------------------------

/// Add two COBOL decimals. Scales are aligned before addition.
///
/// # Safety
/// All pointers must be valid and properly aligned.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_add(
    a: *const CobolDecimal,
    b: *const CobolDecimal,
    result: *mut CobolDecimal,
) {
    let a = *a;
    let b = *b;
    let r = &mut *result;

    let (av, bv, scale) = align_scales(&a, &b);
    let raw = av + bv;

    r.scale = scale;
    // When scales are aligned upward, the size must grow to accommodate
    // the extra fractional digits.
    let a_adjusted = a.size + (scale - a.scale).max(0);
    let b_adjusted = b.size + (scale - b.scale).max(0);
    r.size = a_adjusted.max(b_adjusted);
    r.is_signed = a.is_signed || b.is_signed;
    r.value = clamp_to_size(raw as i64, r.size, r.is_signed);
}

/// Subtract b from a. Scales are aligned before subtraction.
///
/// # Safety
/// All pointers must be valid and properly aligned.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_sub(
    a: *const CobolDecimal,
    b: *const CobolDecimal,
    result: *mut CobolDecimal,
) {
    let a = *a;
    let b = *b;
    let r = &mut *result;

    let (av, bv, scale) = align_scales(&a, &b);
    let raw = av - bv;

    r.scale = scale;
    let a_adjusted = a.size + (scale - a.scale).max(0);
    let b_adjusted = b.size + (scale - b.scale).max(0);
    r.size = a_adjusted.max(b_adjusted);
    r.is_signed = a.is_signed || b.is_signed;
    r.value = clamp_to_size(raw as i64, r.size, r.is_signed);
}

/// Multiply two COBOL decimals. The resulting scale is the sum of both
/// operand scales.
///
/// # Safety
/// All pointers must be valid and properly aligned.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_mul(
    a: *const CobolDecimal,
    b: *const CobolDecimal,
    result: *mut CobolDecimal,
) {
    let a = *a;
    let b = *b;
    let r = &mut *result;

    let raw = a.value as i128 * b.value as i128;
    let combined_scale = a.scale + b.scale;

    // If the combined scale exceeds the desired result scale (a.scale is the
    // default target), truncate the excess fractional digits.
    let target_scale = a.scale.max(b.scale);
    let excess = combined_scale - target_scale;
    let truncated = if excess > 0 {
        let divisor = pow10_i128(excess as u32);
        (raw / divisor) as i64
    } else {
        raw as i64
    };

    r.scale = target_scale;
    r.size = a.size.max(b.size);
    r.is_signed = a.is_signed || b.is_signed;
    r.value = clamp_to_size(truncated, r.size, r.is_signed);
}

/// Divide a by b. The result scale matches operand a's scale.
/// Division by zero produces a value of 0.
///
/// # Safety
/// All pointers must be valid and properly aligned.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_div(
    a: *const CobolDecimal,
    b: *const CobolDecimal,
    result: *mut CobolDecimal,
) {
    let a = *a;
    let b = *b;
    let r = &mut *result;

    if b.value == 0 {
        // COBOL SIZE ERROR condition -- caller should handle; we return 0.
        r.value = 0;
        r.scale = a.scale;
        r.size = a.size;
        r.is_signed = a.is_signed;
        return;
    }

    // To preserve precision, scale the dividend up before dividing.
    // Target scale = a.scale (the receiver's scale).
    let (av, bv, common_scale) = align_scales(&a, &b);
    // After aligning, both are at common_scale. Dividing two numbers at the
    // same scale would give an integer result (scale 0). To get the answer at
    // common_scale, multiply the numerator by 10^common_scale first.
    let factor = pow10_i128(common_scale as u32);
    let numerator = av * factor;
    let raw = (numerator / bv) as i64;

    r.scale = common_scale;
    r.size = a.size.max(b.size);
    r.is_signed = a.is_signed || b.is_signed;
    r.value = clamp_to_size(raw, r.size, r.is_signed);
}

/// Compare two COBOL decimals.
/// Returns: -1 if a < b, 0 if a == b, 1 if a > b.
///
/// # Safety
/// All pointers must be valid and properly aligned.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_cmp(a: *const CobolDecimal, b: *const CobolDecimal) -> i32 {
    let a = &*a;
    let b = &*b;
    let (av, bv, _) = align_scales(a, b);
    match (av).cmp(&bv) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Count the number of decimal digits in a non-negative integer.
fn count_digits(n: u64) -> i32 {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut v = n;
    while v > 0 {
        count += 1;
        v /= 10;
    }
    count
}

/// Create a CobolDecimal from a raw integer and scale.
///
/// # Safety
/// `result` must be a valid, writable pointer.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_from_int(value: i64, scale: i32, result: *mut CobolDecimal) {
    let r = &mut *result;
    r.value = value;
    r.scale = scale;
    // Compute the number of digit positions using integer arithmetic.
    let abs = value.unsigned_abs();
    r.size = count_digits(abs);
    r.is_signed = value < 0;
}

/// Convert a COBOL decimal to an integer by truncating fractional digits.
///
/// # Safety
/// `d` must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_to_int64(d: *const CobolDecimal) -> i64 {
    let d = &*d;
    if d.scale <= 0 {
        d.value
    } else {
        d.value / pow10_i64(d.scale as u32)
    }
}

/// Convert a `CobolDecimal` to a C `double`.
///
/// This preserves the fractional part, unlike `cobol_decimal_to_int64` which
/// truncates.  Used when passing decimal values to math intrinsic functions
/// (ACOS, ASIN, COS, SIN, TAN, LOG, SQRT, etc.).
///
/// # Safety
/// `d` must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_to_double(d: *const CobolDecimal) -> f64 {
    let d = &*d;
    if d.scale <= 0 {
        d.value as f64
    } else {
        d.value as f64 / pow10_f64(d.scale)
    }
}

/// Create a `CobolDecimal` from a C `double`, using the existing target's scale.
///
/// The value is multiplied by 10^scale to convert from floating point to the
/// scaled-integer representation.  Used when assigning the result of a
/// floating-point expression (e.g. `COMPUTE ARG1 = ARG1 + 0.25`) back to a
/// `CobolDecimal` field.
///
/// # Safety
/// `result` must be a valid, writable pointer.  The target's `scale`, `size`,
/// and `is_signed` fields must already be initialised (they are preserved).
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_from_double(val: f64, result: *mut CobolDecimal) {
    let r = &mut *result;
    let factor = pow10_f64(r.scale);
    let scaled = (val * factor).round() as i64;
    r.value = clamp_to_size(scaled, r.size, r.is_signed);
}

/// Parse a decimal number from a UTF-8 string (e.g. "123.45" or "-0.5").
///
/// # Safety
/// `ptr` must point to a readable region of at least `len` bytes.
/// `result` must be a valid, writable pointer.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_from_string(
    ptr: *const u8,
    len: u32,
    result: *mut CobolDecimal,
) {
    let r = &mut *result;
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let mut start = 0usize;
    let mut end = slice.len();
    while start < end && slice[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && slice[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    if start == end {
        *r = CobolDecimal {
            value: 0,
            scale: 0,
            size: 1,
            is_signed: false,
        };
        return;
    }

    let trimmed = &slice[start..end];
    let mut idx = 0usize;
    let mut negative = false;
    let mut is_signed = false;
    match trimmed.first().copied() {
        Some(b'-') => {
            negative = true;
            is_signed = true;
            idx = 1;
        }
        Some(b'+') => {
            is_signed = true;
            idx = 1;
        }
        _ => {}
    }

    let mut value: i64 = 0;
    let mut scale: i32 = 0;
    let mut total_digits: i32 = 0;
    let mut seen_dot = false;

    for &b in &trimmed[idx..] {
        match b {
            b'0'..=b'9' => {
                value = value.saturating_mul(10).saturating_add((b - b'0') as i64);
                total_digits += 1;
                if seen_dot {
                    scale += 1;
                }
            }
            b'.' if !seen_dot => {
                seen_dot = true;
            }
            _ => {
                *r = CobolDecimal {
                    value: 0,
                    scale: 0,
                    size: 1,
                    is_signed: false,
                };
                return;
            }
        }
    }

    r.value = if negative { -value } else { value };
    r.scale = scale;
    r.size = total_digits.max(1);
    r.is_signed = is_signed;
}

/// Format a CobolDecimal as a display string according to a PICTURE clause.
///
/// Supported PICTURE characters:
///   9 — digit position
///   V — implied decimal point (not emitted)
///   . — actual decimal point
///   Z — zero-suppressed digit (space if leading zero)
///   - — sign (negative only)
///   + — sign (always)
///     S — sign (implicit, not emitted but value retains sign)
///     , — comma insertion
///     $ — currency symbol
///
/// Returns the number of bytes written.
///
/// # Safety
/// All pointers must be valid; `buf` must have room for `buf_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_decimal_to_display(
    dec: *const CobolDecimal,
    buf: *mut u8,
    buf_len: u32,
    pic_ptr: *const u8,
    pic_len: u32,
) -> u32 {
    let d = &*dec;
    let pic_slice = std::slice::from_raw_parts(pic_ptr, pic_len as usize);
    let pic = match std::str::from_utf8(pic_slice) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let out_slice = std::slice::from_raw_parts_mut(buf, buf_len as usize);

    let formatted = format_picture(d, pic);
    let bytes = formatted.as_bytes();
    let copy_len = bytes.len().min(buf_len as usize);
    out_slice[..copy_len].copy_from_slice(&bytes[..copy_len]);
    copy_len as u32
}

/// Internal PICTURE formatting.
fn format_picture(dec: &CobolDecimal, pic: &str) -> String {
    let negative = dec.value < 0;
    let abs_value = dec.value.unsigned_abs();

    // Count digit positions before and after the decimal point in the PICTURE.
    let pic_upper = pic.to_uppercase();
    let chars: Vec<char> = pic_upper.chars().collect();

    // Find the decimal point position (V or .).
    let decimal_pos = chars.iter().position(|&c| c == 'V' || c == '.');
    let has_actual_point = chars.contains(&'.');

    // Count integer and fractional digit positions.
    let mut int_digits = 0usize;
    let mut frac_digits = 0usize;
    let mut found_decimal = false;
    for &c in &chars {
        if c == 'V' || c == '.' {
            found_decimal = true;
            continue;
        }
        if c == '9' || c == 'Z' || c == '*' {
            if found_decimal {
                frac_digits += 1;
            } else {
                int_digits += 1;
            }
        }
    }

    // Extract integer and fractional parts from the value.
    let frac_factor = pow10_i64(dec.scale.max(0) as u32) as u64;
    let int_val = abs_value / frac_factor;
    let frac_val = abs_value % frac_factor;

    // Pad to the expected number of digits.
    let int_str = format!("{:0>width$}", int_val, width = int_digits);
    let frac_str = if frac_digits > 0 {
        let raw = format!("{:0>width$}", frac_val, width = dec.scale.max(0) as usize);
        // Truncate or pad to match the PICTURE.
        if raw.len() >= frac_digits {
            raw[..frac_digits].to_string()
        } else {
            format!("{:0<width$}", raw, width = frac_digits)
        }
    } else {
        String::new()
    };

    // Now walk the PICTURE and build the output.
    let mut int_idx = 0usize;
    let mut frac_idx = 0usize;
    let mut result = String::with_capacity(chars.len());
    let mut in_frac = false;
    let mut suppress_zeros = true; // for Z suppression

    for (idx, &c) in chars.iter().enumerate() {
        match c {
            'S' => {
                // Implicit sign — not emitted.
            }
            '+' => {
                if negative {
                    result.push('-');
                } else {
                    result.push('+');
                }
            }
            '-' => {
                if negative {
                    result.push('-');
                } else {
                    result.push(' ');
                }
            }
            '$' => {
                if suppress_zeros && chars.get(idx + 1).is_some_and(|next| *next == '$') {
                    result.push(' ');
                } else if suppress_zeros && chars.get(idx + 1).is_some_and(|next| *next == '*') {
                    result.push('$');
                } else {
                    result.push('$');
                    suppress_zeros = false;
                }
            }
            'V' => {
                in_frac = true;
                suppress_zeros = false;
            }
            '.' => {
                in_frac = true;
                suppress_zeros = false;
                result.push('.');
            }
            ',' => {
                if suppress_zeros {
                    result.push(' ');
                } else {
                    result.push(',');
                }
            }
            'B' => {
                result.push(' ');
            }
            '9' => {
                if !in_frac {
                    let digit = int_str.as_bytes().get(int_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    int_idx += 1;
                    suppress_zeros = false;
                } else {
                    let digit = frac_str.as_bytes().get(frac_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    frac_idx += 1;
                }
            }
            'Z' => {
                if !in_frac {
                    let digit = int_str.as_bytes().get(int_idx).copied().unwrap_or(b'0');
                    if suppress_zeros && digit == b'0' && int_idx < int_str.len() - 1 {
                        result.push(' ');
                    } else {
                        suppress_zeros = false;
                        result.push(digit as char);
                    }
                    int_idx += 1;
                } else {
                    let digit = frac_str.as_bytes().get(frac_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    frac_idx += 1;
                }
            }
            '*' => {
                if !in_frac {
                    let digit = int_str.as_bytes().get(int_idx).copied().unwrap_or(b'0');
                    if suppress_zeros && digit == b'0' {
                        result.push('*');
                    } else {
                        suppress_zeros = false;
                        result.push(digit as char);
                    }
                    int_idx += 1;
                } else {
                    let digit = frac_str.as_bytes().get(frac_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    frac_idx += 1;
                }
            }
            _ => {
                // Pass through unknown characters.
                result.push(c);
            }
        }
    }

    // Handle the case where the PICTURE has no explicit decimal or sign but
    // no digit positions were consumed (edge case).
    let _ = decimal_pos;
    let _ = has_actual_point;

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dec(value: i64, scale: i32, size: i32, is_signed: bool) -> CobolDecimal {
        CobolDecimal {
            value,
            scale,
            size,
            is_signed,
        }
    }

    #[test]
    fn test_decimal_add() {
        let a = make_dec(10050, 2, 5, true); // 100.50
        let b = make_dec(20075, 2, 5, true); // 200.75
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_add(&a, &b, &mut r) };
        assert_eq!(r.value, 30125); // 301.25
        assert_eq!(r.scale, 2);
    }

    #[test]
    fn test_floating_currency_picture_suppresses_leading_symbol() {
        let d = make_dec(7211, 2, 7, true);
        let mut out = [0u8; 16];
        let len = unsafe {
            cobol_decimal_to_display(
                &d,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$$99.99".as_ptr(),
                7,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            " $72.11"
        );
    }

    #[test]
    fn test_asterisk_picture_replaces_suppressed_zeroes() {
        let mut out = [0u8; 16];
        let d = make_dec(1000, 2, 6, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &d,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$**.99".as_ptr(),
                6,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "$10.00");

        let zero = make_dec(0, 0, 6, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$**.99".as_ptr(),
                6,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "$**.00");
    }

    #[test]
    fn test_decimal_sub() {
        let a = make_dec(50000, 2, 5, true); // 500.00
        let b = make_dec(12345, 2, 5, true); // 123.45
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_sub(&a, &b, &mut r) };
        assert_eq!(r.value, 37655); // 376.55
        assert_eq!(r.scale, 2);
    }

    #[test]
    fn test_decimal_mul() {
        let a = make_dec(1250, 2, 4, true); // 12.50
        let b = make_dec(400, 2, 3, false); //  4.00
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_mul(&a, &b, &mut r) };
        assert_eq!(r.value, 5000); // 50.00
        assert_eq!(r.scale, 2);
    }

    #[test]
    fn test_decimal_div() {
        let a = make_dec(10000, 2, 5, true); // 100.00
        let b = make_dec(300, 2, 3, false); //   3.00
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_div(&a, &b, &mut r) };
        assert_eq!(r.value, 3333); // 33.33 (truncated)
        assert_eq!(r.scale, 2);
    }

    #[test]
    fn test_decimal_div_by_zero() {
        let a = make_dec(10000, 2, 5, true);
        let b = make_dec(0, 2, 3, false);
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_div(&a, &b, &mut r) };
        assert_eq!(r.value, 0);
    }

    #[test]
    fn test_decimal_cmp() {
        let a = make_dec(10050, 2, 5, true); // 100.50
        let b = make_dec(20075, 2, 5, true); // 200.75
        let c = make_dec(10050, 2, 5, true); // 100.50

        unsafe {
            assert_eq!(cobol_decimal_cmp(&a, &b), -1);
            assert_eq!(cobol_decimal_cmp(&b, &a), 1);
            assert_eq!(cobol_decimal_cmp(&a, &c), 0);
        }
    }

    #[test]
    fn test_decimal_cmp_negative() {
        let a = make_dec(-500, 2, 3, true); // -5.00
        let b = make_dec(500, 2, 3, true); //   5.00

        unsafe {
            assert_eq!(cobol_decimal_cmp(&a, &b), -1);
            assert_eq!(cobol_decimal_cmp(&b, &a), 1);
        }
    }

    #[test]
    fn test_scale_alignment() {
        // 123.45 (scale=2) + 6.789 (scale=3)
        let a = make_dec(12345, 2, 5, true); // 123.45
        let b = make_dec(6789, 3, 4, true); //    6.789
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_add(&a, &b, &mut r) };
        // 123.450 + 6.789 = 130.239 => value=130239, scale=3
        assert_eq!(r.value, 130239);
        assert_eq!(r.scale, 3);
    }

    #[test]
    fn test_decimal_from_int() {
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_from_int(12345, 2, &mut r) };
        assert_eq!(r.value, 12345);
        assert_eq!(r.scale, 2);
    }

    #[test]
    fn test_decimal_from_string() {
        let s = b"123.45";
        let mut r = make_dec(0, 0, 0, false);
        unsafe { cobol_decimal_from_string(s.as_ptr(), s.len() as u32, &mut r) };
        assert_eq!(r.value, 12345);
        assert_eq!(r.scale, 2);
        assert_eq!(r.size, 5);

        let neg = b"-0.5";
        unsafe { cobol_decimal_from_string(neg.as_ptr(), neg.len() as u32, &mut r) };
        assert_eq!(r.value, -5);
        assert_eq!(r.scale, 1);
        assert!(r.is_signed);
    }

    #[test]
    fn test_decimal_to_display_simple() {
        let d = make_dec(12345, 2, 5, true); // 123.45
        let pic = b"999.99";
        let mut buf = [0u8; 32];
        let written = unsafe {
            cobol_decimal_to_display(&d, buf.as_mut_ptr(), 32, pic.as_ptr(), pic.len() as u32)
        };
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s, "123.45");
    }

    #[test]
    fn test_decimal_to_display_zero_suppress() {
        let d = make_dec(42, 0, 4, false); // 42
        let pic = b"ZZ99";
        let mut buf = [0u8; 32];
        let written = unsafe {
            cobol_decimal_to_display(&d, buf.as_mut_ptr(), 32, pic.as_ptr(), pic.len() as u32)
        };
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s, "  42");
    }

    #[test]
    fn test_decimal_to_display_signed() {
        let d = make_dec(-12345, 2, 5, true); // -123.45
        let pic = b"-999.99";
        let mut buf = [0u8; 32];
        let written = unsafe {
            cobol_decimal_to_display(&d, buf.as_mut_ptr(), 32, pic.as_ptr(), pic.len() as u32)
        };
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s, "-123.45");
    }
}
