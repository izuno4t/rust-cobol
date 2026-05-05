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
    let upper_trimmed: Vec<u8> = trimmed.iter().map(|b| b.to_ascii_uppercase()).collect();
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
    if trimmed.last().is_some_and(|b| *b == b'-') || upper_trimmed.ends_with(b"CR") {
        negative = true;
        is_signed = true;
    }
    if upper_trimmed.ends_with(b"DB") {
        is_signed = true;
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
            b' ' | b'\t' | b'\r' | b'\n' | b'+' | b'-' | b',' | b'$' | b'/' => {}
            _ if b.to_ascii_uppercase().is_ascii_uppercase() => {}
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
    let pic_upper = expand_picture_repeats(pic).to_uppercase();
    let chars: Vec<char> = pic_upper.chars().collect();
    let display_abs_value = apply_picture_p_scaling(abs_value, &chars);
    if chars.iter().all(|&c| c == '$' || c == 'W') && !chars.is_empty() {
        let symbol = chars[0];
        let digits = display_abs_value.to_string();
        let mut result = String::with_capacity(chars.len());
        let occupied = (digits.len() + 1).min(chars.len());
        result.push_str(&" ".repeat(chars.len().saturating_sub(occupied)));
        if digits.len() < chars.len() {
            result.push(symbol);
            result.push_str(&digits);
        } else {
            result.push_str(&digits[digits.len().saturating_sub(chars.len())..]);
        }
        return result;
    }

    // When both period and comma appear, the rightmost separator is the actual
    // decimal point and the other separator is an insertion character.
    let actual_decimal_char = numeric_edited_actual_decimal_char(&chars);
    let decimal_pos = chars
        .iter()
        .position(|&c| c == 'V' || Some(c) == actual_decimal_char);
    let has_actual_point = actual_decimal_char.is_some();
    let has_mandatory_integer_digit = chars
        .iter()
        .take_while(|&&c| c != 'V' && c != '.')
        .any(|&c| c == '9');
    if display_abs_value == 0
        && chars.contains(&'Z')
        && !chars.iter().any(|&c| c == '9' || c == '*')
    {
        return " ".repeat(chars.len());
    }
    let zero_asterisk_fill =
        display_abs_value == 0 && chars.contains(&'*') && !has_mandatory_integer_digit;
    let zero_asterisk_preserves_currency = zero_asterisk_fill
        && dec.scale > 0
        && actual_decimal_char.is_some()
        && chars
            .iter()
            .skip_while(|&&c| Some(c) != actual_decimal_char)
            .any(|&c| c == '9');

    // Count integer and fractional digit positions.
    let floating_symbol = numeric_edited_floating_symbol(&chars, actual_decimal_char);
    let mut int_digits = 0usize;
    let mut frac_digits = 0usize;
    let mut found_decimal = false;
    for &c in &chars {
        if c == 'V' || Some(c) == actual_decimal_char {
            found_decimal = true;
            continue;
        }
        if c == '9' || c == 'Z' || c == '*' || Some(c) == floating_symbol {
            if found_decimal {
                frac_digits += 1;
            } else {
                int_digits += 1;
            }
        }
    }

    // Extract integer and fractional parts from the value.
    let frac_factor = pow10_i64(dec.scale.max(0) as u32) as u64;
    let int_val = display_abs_value / frac_factor;
    let frac_val = display_abs_value % frac_factor;

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

    if let Some(formatted) =
        format_floating_numeric_edited(&chars, actual_decimal_char, int_val, &frac_str, negative)
    {
        return formatted;
    }

    // Now walk the PICTURE and build the output.
    let mut int_idx = 0usize;
    let mut frac_idx = 0usize;
    let mut result = String::with_capacity(chars.len());
    let mut in_frac = false;
    let mut suppress_zeros = true; // for Z suppression
    let mut forced_next = None;
    let mut pending_floating_symbol = None;

    for (idx, &c) in chars.iter().enumerate() {
        if let Some(ch) = forced_next.take() {
            result.push(ch);
            continue;
        }
        match c {
            'C' if !zero_asterisk_fill && chars.get(idx + 1).is_some_and(|next| *next == 'R') => {
                result.push(if negative { 'C' } else { ' ' });
                forced_next = Some(if negative { 'R' } else { ' ' });
            }
            'D' if !zero_asterisk_fill && chars.get(idx + 1).is_some_and(|next| *next == 'B') => {
                result.push(if negative { 'D' } else { ' ' });
                forced_next = Some(if negative { 'B' } else { ' ' });
            }
            '*' | 'C' | 'R' if zero_asterisk_fill => {
                result.push('*');
                if c == '*' && !in_frac {
                    int_idx += 1;
                } else if c == '*' {
                    frac_idx += 1;
                }
            }
            '$' | 'W' if zero_asterisk_fill && zero_asterisk_preserves_currency => {
                result.push(c);
            }
            '$' | 'W' if zero_asterisk_fill => {
                result.push('*');
            }
            'S' => {
                // Implicit sign — not emitted.
            }
            '+' => {
                if chars.get(idx + 1).is_some_and(|next| *next == '+') {
                    result.push(' ');
                } else if suppress_zeros
                    && chars
                        .get(idx + 1)
                        .is_some_and(|next| *next == ',' || *next == 'B')
                {
                    result.push(' ');
                    pending_floating_symbol = Some(if negative { '-' } else { '+' });
                } else if negative {
                    result.push('-');
                } else {
                    result.push('+');
                }
            }
            '-' => {
                if chars.get(idx + 1).is_some_and(|next| *next == '-') {
                    result.push(' ');
                } else if suppress_zeros
                    && chars
                        .get(idx + 1)
                        .is_some_and(|next| *next == ',' || *next == 'B')
                {
                    result.push(' ');
                    pending_floating_symbol = Some(if negative { '-' } else { ' ' });
                } else if negative {
                    result.push('-');
                } else {
                    result.push(' ');
                }
            }
            '$' | 'W' => {
                if suppress_zeros && chars.get(idx + 1).is_some_and(|next| *next == c) {
                    result.push(' ');
                } else if suppress_zeros && chars.get(idx + 1).is_some_and(|next| *next == '*') {
                    result.push(c);
                } else if suppress_zeros
                    && chars
                        .get(idx + 1)
                        .is_some_and(|next| *next == ',' || *next == 'B')
                {
                    result.push(' ');
                    pending_floating_symbol = Some(c);
                } else {
                    result.push(c);
                    suppress_zeros = false;
                }
            }
            'V' => {
                in_frac = true;
                suppress_zeros = false;
            }
            '.' if Some(c) == actual_decimal_char => {
                in_frac = true;
                suppress_zeros = false;
                result.push('.');
            }
            ',' if Some(c) == actual_decimal_char => {
                in_frac = true;
                suppress_zeros = false;
                result.push(',');
            }
            ',' => {
                if let Some(symbol) = pending_floating_symbol.take() {
                    result.push(symbol);
                    suppress_zeros = false;
                } else if suppress_zeros && chars[..idx].contains(&'*') {
                    result.push('*');
                } else if suppress_zeros {
                    result.push(' ');
                } else {
                    result.push(',');
                }
            }
            '.' => {
                if suppress_zeros {
                    result.push(' ');
                } else {
                    result.push('.');
                }
            }
            'B' => {
                if let Some(symbol) = pending_floating_symbol.take() {
                    result.push(symbol);
                    suppress_zeros = false;
                } else if suppress_zeros && chars[..idx].contains(&'*') {
                    result.push('*');
                } else {
                    result.push(' ');
                }
            }
            'P' => {
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
                    if suppress_zeros && digit == b'0' {
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

fn numeric_edited_actual_decimal_char(chars: &[char]) -> Option<char> {
    let dot_pos = chars.iter().rposition(|&c| c == '.');
    let comma_pos = chars.iter().rposition(|&c| c == ',');
    let dot_count = chars.iter().filter(|&&c| c == '.').count();
    let comma_count = chars.iter().filter(|&&c| c == ',').count();
    match (dot_pos, comma_pos) {
        (Some(_), Some(_)) if dot_count == 1 && comma_count > 1 => Some('.'),
        (Some(_), Some(_)) if comma_count == 1 && dot_count > 1 => Some(','),
        (Some(dot), Some(comma)) => Some(if comma > dot { ',' } else { '.' }),
        (Some(_), None) => Some('.'),
        _ => None,
    }
}

fn format_floating_numeric_edited(
    chars: &[char],
    actual_decimal_char: Option<char>,
    int_val: u64,
    frac_str: &str,
    negative: bool,
) -> Option<String> {
    let decimal_idx = chars
        .iter()
        .position(|&c| c == 'V' || Some(c) == actual_decimal_char)
        .unwrap_or(chars.len());
    let int_pic = &chars[..decimal_idx];
    let floating_symbol = ['$', '+', '-', '<', '>']
        .into_iter()
        .find(|symbol| int_pic.iter().filter(|&&c| c == *symbol).count() > 1)?;

    let emitted_symbol = match floating_symbol {
        '$' => Some('$'),
        '+' => Some(if negative { '-' } else { '+' }),
        '-' if negative => Some('-'),
        '-' => None,
        other => Some(other),
    };

    let frac_pic = if decimal_idx < chars.len() {
        &chars[decimal_idx + 1..]
    } else {
        &[]
    };
    let mandatory_int_digits = int_pic
        .iter()
        .filter(|&&c| c == '9' || c == 'Z' || c == '*')
        .count();
    let floating_frac_digits = frac_pic.contains(&floating_symbol);
    let mandatory_frac_digits = frac_pic.iter().any(|&c| c == '9' || c == 'Z' || c == '*');
    let all_zero = int_val == 0 && frac_str.chars().all(|ch| ch == '0');
    if all_zero && mandatory_int_digits == 0 && !mandatory_frac_digits {
        return Some(" ".repeat(chars.len()));
    }

    let digit_text = floating_integer_text(int_val, mandatory_int_digits, int_pic.contains(&','));
    let mut integer_text = String::with_capacity(digit_text.len() + 1);
    if let Some(symbol) = emitted_symbol {
        integer_text.push(symbol);
    }
    integer_text.push_str(&digit_text);
    let last_int_slot = int_pic
        .iter()
        .rposition(|&c| c == floating_symbol || c == '9' || c == 'Z' || c == '*');
    let (int_core_width, trailing_int_insertions) = if let Some(last_slot) = last_int_slot {
        (last_slot + 1, &int_pic[last_slot + 1..])
    } else {
        (int_pic.len(), &[][..])
    };
    let mut result = right_align_floating_text(&integer_text, int_core_width, emitted_symbol);
    for &c in trailing_int_insertions {
        result.push(match c {
            'B' => ' ',
            other => other,
        });
    }
    if decimal_idx < chars.len() {
        match chars[decimal_idx] {
            'V' => {}
            c if Some(c) == actual_decimal_char => result.push(c),
            _ => {}
        }
        let mut frac_idx = 0usize;
        let mut idx = decimal_idx + 1;
        while idx < chars.len() {
            let c = chars[idx];
            match c {
                '9' | 'Z' | '*' => {
                    let digit = frac_str.as_bytes().get(frac_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    frac_idx += 1;
                }
                c if c == floating_symbol && floating_frac_digits => {
                    let digit = frac_str.as_bytes().get(frac_idx).copied().unwrap_or(b'0');
                    result.push(digit as char);
                    frac_idx += 1;
                }
                '+' => result.push(if negative { '-' } else { '+' }),
                '-' => result.push(if negative { '-' } else { ' ' }),
                'C' if chars.get(idx + 1).is_some_and(|next| *next == 'R') => {
                    result.push(if negative { 'C' } else { ' ' });
                    result.push(if negative { 'R' } else { ' ' });
                    idx += 1;
                }
                'D' if chars.get(idx + 1).is_some_and(|next| *next == 'B') => {
                    result.push(if negative { 'D' } else { ' ' });
                    result.push(if negative { 'B' } else { ' ' });
                    idx += 1;
                }
                'B' => result.push(' '),
                'S' => {}
                other => result.push(other),
            }
            idx += 1;
        }
    }
    Some(result)
}

fn numeric_edited_floating_symbol(
    chars: &[char],
    actual_decimal_char: Option<char>,
) -> Option<char> {
    let decimal_idx = chars
        .iter()
        .position(|&c| c == 'V' || Some(c) == actual_decimal_char)
        .unwrap_or(chars.len());
    let int_pic = &chars[..decimal_idx];
    ['$', '+', '-', '<', '>']
        .into_iter()
        .find(|symbol| int_pic.iter().filter(|&&c| c == *symbol).count() > 1)
}

fn floating_integer_text(value: u64, mandatory_digits: usize, grouped: bool) -> String {
    let digits = if mandatory_digits > 0 {
        format!("{value:0>mandatory_digits$}")
    } else {
        plain_integer_text(value)
    };
    if grouped && digits.len() > 3 {
        grouped_digits(&digits)
    } else {
        digits
    }
}

fn grouped_digits(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (idx, ch) in digits.chars().enumerate() {
        if idx > 0 && (digits.len() - idx).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn plain_integer_text(value: u64) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}

fn right_align_text(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.chars().skip(len - width).collect();
    }
    let mut out = String::with_capacity(width);
    out.push_str(&" ".repeat(width - len));
    out.push_str(text);
    out
}

fn right_align_floating_text(text: &str, width: usize, symbol: Option<char>) -> String {
    let len = text.chars().count();
    if len <= width {
        return right_align_text(text, width);
    }
    if let Some(symbol) = symbol {
        let mut out = String::with_capacity(width);
        out.push(symbol);
        out.extend(
            text.chars()
                .rev()
                .take(width.saturating_sub(1))
                .collect::<Vec<_>>()
                .into_iter()
                .rev(),
        );
        return out;
    }
    right_align_text(text, width)
}

fn apply_picture_p_scaling(value: u64, chars: &[char]) -> u64 {
    let trailing_p = chars.iter().rev().take_while(|&&c| c == 'P').count() as u32;
    if trailing_p == 0 {
        value
    } else {
        value / 10u64.pow(trailing_p)
    }
}

fn expand_picture_repeats(pic: &str) -> String {
    let chars: Vec<char> = pic.chars().collect();
    let mut result = String::with_capacity(pic.len());
    let mut i = 0usize;

    while i < chars.len() {
        let ch = chars[i];
        if i + 1 < chars.len() && chars[i + 1] == '(' {
            let mut j = i + 2;
            let mut count = String::new();
            while j < chars.len() && chars[j].is_ascii_digit() {
                count.push(chars[j]);
                j += 1;
            }
            if j < chars.len() && chars[j] == ')' {
                let repeat = count.parse::<usize>().unwrap_or(1);
                for _ in 0..repeat {
                    result.push(ch);
                }
                i = j + 1;
                continue;
            }
        }

        result.push(ch);
        i += 1;
    }

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
    fn test_zero_star_fill_suppresses_single_floating_currency() {
        let zero = make_dec(0, 2, 4, true);
        let mut out = [0u8; 16];
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$**.**CR".as_ptr(),
                8,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "***.****"
        );
    }

    #[test]
    fn test_floating_numeric_edited_long_pictures() {
        let pic = b"$$,$$$,$$$,$$$,$$$,$$$.99";
        let mut out = [0u8; 32];
        let zero = make_dec(0, 2, 18, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                     $.00"
        );

        let one = make_dec(100, 2, 18, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &one,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                    $1.00"
        );

        let hundreds = make_dec(11111, 2, 18, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &hundreds,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                  $111.11"
        );

        let amount = make_dec(999911, 2, 18, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &amount,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                $9,999.11"
        );

        let overflow = make_dec(1234, 0, 4, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &overflow,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$$99".as_ptr(),
                4,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "$234");

        let plus_pic = b"++,+++,+++,+++,+++,+++.99";
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                plus_pic.as_ptr(),
                plus_pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                     +.00"
        );

        let minus_pic = b"--,---,---,---,---,---.99";
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                minus_pic.as_ptr(),
                minus_pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                      .00"
        );

        let negative = make_dec(-100, 2, 18, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &negative,
                out.as_mut_ptr(),
                out.len() as u32,
                minus_pic.as_ptr(),
                minus_pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "                    -1.00"
        );
    }

    #[test]
    fn test_currency_asterisk_zero_preserves_currency_with_mandatory_fraction() {
        let pic = b"$**.99";
        let mut out = [0u8; 16];
        let zero = make_dec(0, 2, 4, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "$**.00");
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
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "***.00");

        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$.**".as_ptr(),
                4,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "*.**");

        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$**.**CR".as_ptr(),
                8,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "***.****"
        );

        let negative = make_dec(-42, 0, 6, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &negative,
                out.as_mut_ptr(),
                out.len() as u32,
                b"-*B*99".as_ptr(),
                6,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "-***42");

        let positive = make_dec(55, 2, 5, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &positive,
                out.as_mut_ptr(),
                out.len() as u32,
                b"$$$.99CR".as_ptr(),
                8,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "  $.55  "
        );
    }

    #[test]
    fn test_trailing_comma_is_numeric_edited_insertion() {
        let mut out = [0u8; 32];
        let d = make_dec(123456789012, 0, 12, true);
        let pic = b"9,9,9,9,9,9,9,9,9,9,9,9,";
        let len = unsafe {
            cobol_decimal_to_display(
                &d,
                out.as_mut_ptr(),
                out.len() as u32,
                pic.as_ptr(),
                pic.len() as u32,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "1,2,3,4,5,6,7,8,9,0,1,2,"
        );
    }

    #[test]
    fn test_z_only_picture_zero_suppresses_decimal() {
        let mut out = [0u8; 8];
        let zero = make_dec(0, 0, 4, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"ZZ.ZZ".as_ptr(),
                5,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "     ");
    }

    #[test]
    fn test_p_positions_are_not_literal_output() {
        let mut out = [0u8; 8];
        let d = make_dec(900, 0, 5, true);
        let len = unsafe {
            cobol_decimal_to_display(&d, out.as_mut_ptr(), out.len() as u32, b"ZZZPP".as_ptr(), 5)
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "  9  ");
    }

    #[test]
    fn test_floating_sign_fraction_positions() {
        let mut out = [0u8; 16];
        let zero = make_dec(0, 0, 5, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"++++9".as_ptr(),
                5,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "   +0");

        let twelve = make_dec(12, 0, 5, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &twelve,
                out.as_mut_ptr(),
                out.len() as u32,
                b"++++9".as_ptr(),
                5,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "  +12");

        let large = make_dec(1234, 0, 5, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &large,
                out.as_mut_ptr(),
                out.len() as u32,
                b"++++9".as_ptr(),
                5,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "+1234");

        let len = unsafe {
            cobol_decimal_to_display(
                &zero,
                out.as_mut_ptr(),
                out.len() as u32,
                b"+++++.".as_ptr(),
                6,
            )
        };
        assert_eq!(std::str::from_utf8(&out[..len as usize]).unwrap(), "      ");

        let d = make_dec(12, 0, 7, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &d,
                out.as_mut_ptr(),
                out.len() as u32,
                b"+++++.++".as_ptr(),
                8,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "  +12.00"
        );

        let len = unsafe {
            cobol_decimal_to_display(
                &d,
                out.as_mut_ptr(),
                out.len() as u32,
                b"--,---.--".as_ptr(),
                9,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "    12.00"
        );

        let negative = make_dec(-1298, 2, 8, true);
        let len = unsafe {
            cobol_decimal_to_display(
                &negative,
                out.as_mut_ptr(),
                out.len() as u32,
                b"---,999.99".as_ptr(),
                10,
            )
        };
        assert_eq!(
            std::str::from_utf8(&out[..len as usize]).unwrap(),
            "   -012.98"
        );
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

    #[test]
    fn test_decimal_to_display_decimal_point_comma_picture() {
        let d = make_dec(123456789, 2, 9, false);
        let pic = b"9.999.999,99";
        let mut buf = [0u8; 32];
        let written = unsafe {
            cobol_decimal_to_display(&d, buf.as_mut_ptr(), 32, pic.as_ptr(), pic.len() as u32)
        };
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s, "1.234.567,89");
    }

    #[test]
    fn test_decimal_to_display_mixed_insertion_after_decimal_point() {
        let d = make_dec(123456789, 4, 10, false);
        let pic = b"ZZZ,999.999,9";
        let mut buf = [0u8; 32];
        let written = unsafe {
            cobol_decimal_to_display(&d, buf.as_mut_ptr(), 32, pic.as_ptr(), pic.len() as u32)
        };
        let s = std::str::from_utf8(&buf[..written as usize]).unwrap();
        assert_eq!(s, " 12,345.678,9");
    }
}
