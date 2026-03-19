// COBOL Runtime - NATIONAL (PIC N) data type support
//
// Implements UTF-16 based operations for NATIONAL data items.
// NATIONAL fields are stored as uint16_t arrays in generated C code.
//
// All public functions use the C ABI for linking with generated code.

/// FUNCTION NATIONAL-OF -- convert alphanumeric (char*) to national (uint16_t*).
///
/// Performs simple ASCII-to-UTF-16 widening: each byte is zero-extended to uint16_t.
///
/// Returns the number of uint16_t characters written.
///
/// # Safety
/// `src` must be readable for `src_len` bytes.
/// `dst` must be writable for `dst_len` uint16_t values.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_national_of(
    src: *const u8,
    src_len: u32,
    dst: *mut u16,
    dst_len: u32,
) -> u32 {
    let src_slice = std::slice::from_raw_parts(src, src_len as usize);
    let dst_slice = std::slice::from_raw_parts_mut(dst, dst_len as usize);
    let copy_len = src_slice.len().min(dst_slice.len());
    for (d, s) in dst_slice[..copy_len]
        .iter_mut()
        .zip(src_slice[..copy_len].iter())
    {
        *d = *s as u16;
    }
    // Pad remaining positions with U+0020 (space)
    for item in dst_slice.iter_mut().skip(copy_len) {
        *item = 0x0020;
    }
    copy_len as u32
}

/// FUNCTION DISPLAY-OF -- convert national (uint16_t*) to alphanumeric (char*).
///
/// Performs simple UTF-16-to-ASCII narrowing: each uint16_t is truncated to u8.
/// Characters outside ASCII range are replaced with '?'.
///
/// Returns the number of bytes written.
///
/// # Safety
/// `src` must be readable for `src_len` uint16_t values.
/// `dst` must be writable for `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_func_display_of(
    src: *const u16,
    src_len: u32,
    dst: *mut u8,
    dst_len: u32,
) -> u32 {
    let src_slice = std::slice::from_raw_parts(src, src_len as usize);
    let dst_slice = std::slice::from_raw_parts_mut(dst, dst_len as usize);
    let copy_len = src_slice.len().min(dst_slice.len());
    for (d, s) in dst_slice[..copy_len]
        .iter_mut()
        .zip(src_slice[..copy_len].iter())
    {
        *d = if *s <= 0x7F { *s as u8 } else { b'?' };
    }
    // Pad remaining positions with space
    for item in dst_slice.iter_mut().skip(copy_len) {
        *item = b' ';
    }
    copy_len as u32
}

/// Move an alphanumeric (char*) value into a NATIONAL field (uint16_t*).
///
/// Widens each source byte to uint16_t and pads with spaces (U+0020).
///
/// # Safety
/// `src` must be readable for `src_len` bytes.
/// `dst` must be writable for `dst_len` uint16_t values.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_to_national(
    src: *const u8,
    src_len: u32,
    dst: *mut u16,
    dst_len: u32,
) {
    cobol_func_national_of(src, src_len, dst, dst_len);
}

/// Display a NATIONAL field by converting it to ASCII and printing.
///
/// # Safety
/// `ptr` must be readable for `len` uint16_t values.
#[no_mangle]
pub unsafe extern "C" fn cobol_display_national(ptr: *const u16, len: u32) {
    let src = std::slice::from_raw_parts(ptr, len as usize);
    let mut buf = Vec::with_capacity(len as usize);
    for &ch in src {
        if ch <= 0x7F {
            buf.push(ch as u8);
        } else {
            buf.push(b'?');
        }
    }
    // Trim trailing spaces for display
    let s = std::str::from_utf8(&buf).unwrap_or("?");
    let trimmed = s.trim_end();
    print!("{}", trimmed);
}

/// Move a NATIONAL field (uint16_t*) to another NATIONAL field (uint16_t*).
///
/// # Safety
/// `src` must be readable for `src_len` uint16_t values.
/// `dst` must be writable for `dst_len` uint16_t values.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_national_to_national(
    src: *const u16,
    src_len: u32,
    dst: *mut u16,
    dst_len: u32,
) {
    let src_slice = std::slice::from_raw_parts(src, src_len as usize);
    let dst_slice = std::slice::from_raw_parts_mut(dst, dst_len as usize);
    let copy_len = src_slice.len().min(dst_slice.len());
    dst_slice[..copy_len].copy_from_slice(&src_slice[..copy_len]);
    // Pad remaining with spaces
    for item in dst_slice.iter_mut().skip(copy_len) {
        *item = 0x0020;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_national_of() {
        let src = b"HELLO";
        let mut dst = [0u16; 10];
        let written = unsafe { cobol_func_national_of(src.as_ptr(), 5, dst.as_mut_ptr(), 10) };
        assert_eq!(written, 5);
        assert_eq!(dst[0], b'H' as u16);
        assert_eq!(dst[4], b'O' as u16);
        // Padded with spaces
        assert_eq!(dst[5], 0x0020);
        assert_eq!(dst[9], 0x0020);
    }

    #[test]
    fn test_display_of() {
        let src: [u16; 5] = [
            b'H' as u16,
            b'E' as u16,
            b'L' as u16,
            b'L' as u16,
            b'O' as u16,
        ];
        let mut dst = [0u8; 10];
        let written = unsafe { cobol_func_display_of(src.as_ptr(), 5, dst.as_mut_ptr(), 10) };
        assert_eq!(written, 5);
        assert_eq!(&dst[..5], b"HELLO");
        // Padded with spaces
        assert_eq!(dst[5], b' ');
    }

    #[test]
    fn test_move_to_national() {
        let src = b"ABC";
        let mut dst = [0u16; 5];
        unsafe { cobol_move_to_national(src.as_ptr(), 3, dst.as_mut_ptr(), 5) };
        assert_eq!(dst[0], b'A' as u16);
        assert_eq!(dst[2], b'C' as u16);
        assert_eq!(dst[3], 0x0020);
    }

    #[test]
    fn test_national_to_national() {
        let src: [u16; 3] = [b'X' as u16, b'Y' as u16, b'Z' as u16];
        let mut dst = [0u16; 5];
        unsafe {
            cobol_move_national_to_national(src.as_ptr(), 3, dst.as_mut_ptr(), 5);
        }
        assert_eq!(dst[0], b'X' as u16);
        assert_eq!(dst[2], b'Z' as u16);
        assert_eq!(dst[3], 0x0020);
    }
}
