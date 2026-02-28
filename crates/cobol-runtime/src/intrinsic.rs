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
    let data = std::slice::from_raw_parts(ptr, len as usize);
    let s = match std::str::from_utf8(data) {
        Ok(s) => s.trim(),
        Err(_) => return 0,
    };

    // Remove commas and parse.
    let cleaned: String = s.chars().filter(|&c| c != ',').collect();
    cleaned.parse::<f64>().unwrap_or(0.0) as i64
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
}
