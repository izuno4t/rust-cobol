// COBOL Runtime - UTF-8/Unicode support (COBOL 2023)
//
// Provides character-level operations on UTF-8 strings. COBOL 2023 introduces
// native Unicode support, and these functions bridge the gap between COBOL's
// character-oriented model and UTF-8's variable-width encoding.

/// Get character count (not byte count) of a UTF-8 string.
///
/// # Safety
/// `ptr` must point to a valid, readable region of `byte_len` bytes
/// containing valid UTF-8 data.
#[no_mangle]
pub unsafe extern "C" fn cobol_utf8_char_count(ptr: *const u8, byte_len: u32) -> u32 {
    if ptr.is_null() || byte_len == 0 {
        return 0;
    }
    let slice = std::slice::from_raw_parts(ptr, byte_len as usize);
    match std::str::from_utf8(slice) {
        Ok(s) => s.chars().count() as u32,
        Err(_) => 0,
    }
}

/// Extract a substring by character position (not byte position).
///
/// Returns the number of bytes actually written to `dst_ptr`.
///
/// # Safety
/// - `src_ptr` must point to a valid, readable region of `src_byte_len` bytes.
/// - `dst_ptr` must point to a valid, writable region of `dst_byte_len` bytes.
/// - `start_char` is 1-based (COBOL convention).
#[no_mangle]
pub unsafe extern "C" fn cobol_utf8_substring(
    src_ptr: *const u8,
    src_byte_len: u32,
    start_char: u32,
    char_count: u32,
    dst_ptr: *mut u8,
    dst_byte_len: u32,
) -> u32 {
    if src_ptr.is_null() || dst_ptr.is_null() || src_byte_len == 0 || dst_byte_len == 0 {
        return 0;
    }

    let src_slice = std::slice::from_raw_parts(src_ptr, src_byte_len as usize);
    let s = match std::str::from_utf8(src_slice) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    // Convert from 1-based to 0-based
    let start_idx = if start_char > 0 {
        (start_char - 1) as usize
    } else {
        0
    };

    let substring: String = s
        .chars()
        .skip(start_idx)
        .take(char_count as usize)
        .collect();

    let bytes = substring.as_bytes();
    let copy_len = bytes.len().min(dst_byte_len as usize);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst_ptr, copy_len);

    copy_len as u32
}

/// Convert a UTF-8 string to uppercase (Unicode-aware).
///
/// The conversion is done in-place. Returns the number of bytes after
/// conversion (may differ from input for some Unicode characters).
///
/// # Safety
/// `ptr` must point to a valid, readable and writable region of `byte_len` bytes
/// containing valid UTF-8 data.
#[no_mangle]
pub unsafe extern "C" fn cobol_utf8_upper(ptr: *mut u8, byte_len: u32) -> u32 {
    if ptr.is_null() || byte_len == 0 {
        return 0;
    }

    let slice = std::slice::from_raw_parts(ptr, byte_len as usize);
    let s = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return byte_len,
    };

    let upper = s.to_uppercase();
    let upper_bytes = upper.as_bytes();
    // Find a safe truncation point at a character boundary.
    let mut copy_len = upper_bytes.len().min(byte_len as usize);
    while copy_len > 0 && !upper.is_char_boundary(copy_len) {
        copy_len -= 1;
    }
    std::ptr::copy_nonoverlapping(upper_bytes.as_ptr(), ptr, copy_len);

    copy_len as u32
}

/// Convert a UTF-8 string to lowercase (Unicode-aware).
///
/// The conversion is done in-place. Returns the number of bytes after
/// conversion (may differ from input for some Unicode characters).
///
/// # Safety
/// `ptr` must point to a valid, readable and writable region of `byte_len` bytes
/// containing valid UTF-8 data.
#[no_mangle]
pub unsafe extern "C" fn cobol_utf8_lower(ptr: *mut u8, byte_len: u32) -> u32 {
    if ptr.is_null() || byte_len == 0 {
        return 0;
    }

    let slice = std::slice::from_raw_parts(ptr, byte_len as usize);
    let s = match std::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return byte_len,
    };

    let lower = s.to_lowercase();
    let lower_bytes = lower.as_bytes();
    // Find a safe truncation point at a character boundary.
    let mut copy_len = lower_bytes.len().min(byte_len as usize);
    while copy_len > 0 && !lower.is_char_boundary(copy_len) {
        copy_len -= 1;
    }
    std::ptr::copy_nonoverlapping(lower_bytes.as_ptr(), ptr, copy_len);

    copy_len as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_char_count_ascii() {
        let s = b"Hello";
        let count = unsafe { cobol_utf8_char_count(s.as_ptr(), s.len() as u32) };
        assert_eq!(count, 5);
    }

    #[test]
    fn test_utf8_char_count_multibyte() {
        let s = "こんにちは"; // 5 Japanese characters, 15 bytes
        let bytes = s.as_bytes();
        let count = unsafe { cobol_utf8_char_count(bytes.as_ptr(), bytes.len() as u32) };
        assert_eq!(count, 5);
    }

    #[test]
    fn test_utf8_char_count_empty() {
        let count = unsafe { cobol_utf8_char_count(std::ptr::null(), 0) };
        assert_eq!(count, 0);
    }

    #[test]
    fn test_utf8_char_count_mixed() {
        let s = "Hello世界"; // 7 characters
        let bytes = s.as_bytes();
        let count = unsafe { cobol_utf8_char_count(bytes.as_ptr(), bytes.len() as u32) };
        assert_eq!(count, 7);
    }

    #[test]
    fn test_utf8_substring() {
        let s = "Hello, World!";
        let bytes = s.as_bytes();
        let mut dst = [0u8; 64];
        let written = unsafe {
            cobol_utf8_substring(
                bytes.as_ptr(),
                bytes.len() as u32,
                1, // 1-based start
                5,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        assert_eq!(written, 5);
        assert_eq!(&dst[..written as usize], b"Hello");
    }

    #[test]
    fn test_utf8_substring_multibyte() {
        let s = "こんにちは世界";
        let bytes = s.as_bytes();
        let mut dst = [0u8; 64];
        let written = unsafe {
            cobol_utf8_substring(
                bytes.as_ptr(),
                bytes.len() as u32,
                1,
                3,
                dst.as_mut_ptr(),
                dst.len() as u32,
            )
        };
        let result = std::str::from_utf8(&dst[..written as usize]).unwrap();
        assert_eq!(result, "こんに");
    }

    #[test]
    fn test_utf8_upper() {
        let mut buf = *b"hello world";
        let len = unsafe { cobol_utf8_upper(buf.as_mut_ptr(), buf.len() as u32) };
        assert_eq!(&buf[..len as usize], b"HELLO WORLD");
    }

    #[test]
    fn test_utf8_lower() {
        let mut buf = *b"HELLO WORLD";
        let len = unsafe { cobol_utf8_lower(buf.as_mut_ptr(), buf.len() as u32) };
        assert_eq!(&buf[..len as usize], b"hello world");
    }

    #[test]
    fn test_utf8_upper_null() {
        let result = unsafe { cobol_utf8_upper(std::ptr::null_mut(), 0) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_utf8_lower_null() {
        let result = unsafe { cobol_utf8_lower(std::ptr::null_mut(), 0) };
        assert_eq!(result, 0);
    }
}
