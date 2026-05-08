// COBOL Runtime - SCREEN SECTION support
//
// Provides basic terminal positioning and attribute control using ANSI escape
// sequences. This is a simplified implementation that avoids ncurses dependency
// while supporting the most common SCREEN SECTION features.

use std::io::{self, Write};

/// Position the cursor at the given line and column (1-based).
///
/// Emits an ANSI CSI sequence: ESC[line;colH
#[no_mangle]
pub extern "C" fn cobol_screen_position(line: i32, col: i32) {
    print!("\x1b[{};{}H", line, col);
    let _ = io::stdout().flush();
}

/// Clear the entire screen and move cursor to home position.
#[no_mangle]
pub extern "C" fn cobol_screen_clear() {
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

/// Clear the current line from cursor to end.
#[no_mangle]
pub extern "C" fn cobol_screen_clear_line() {
    print!("\x1b[2K\r");
    let _ = io::stdout().flush();
}

/// Enable bold/highlight text attribute.
#[no_mangle]
pub extern "C" fn cobol_screen_highlight_on() {
    print!("\x1b[1m");
    let _ = io::stdout().flush();
}

/// Disable bold/highlight text attribute (reset to normal).
#[no_mangle]
pub extern "C" fn cobol_screen_highlight_off() {
    print!("\x1b[0m");
    let _ = io::stdout().flush();
}

/// Enable reverse-video text attribute.
#[no_mangle]
pub extern "C" fn cobol_screen_reverse_on() {
    print!("\x1b[7m");
    let _ = io::stdout().flush();
}

/// Disable reverse-video text attribute.
#[no_mangle]
pub extern "C" fn cobol_screen_reverse_off() {
    print!("\x1b[27m");
    let _ = io::stdout().flush();
}

/// Reset all text attributes to default.
#[no_mangle]
pub extern "C" fn cobol_screen_reset_attrs() {
    print!("\x1b[0m");
    let _ = io::stdout().flush();
}

/// Read one terminal input line into a fixed-width SCREEN SECTION field.
///
/// The destination is space-filled before the input bytes are copied so COBOL
/// fixed-length display storage keeps the usual padding semantics.
///
/// # Safety
///
/// `dst` must be valid for writes of `len` bytes. The pointer may not alias
/// immutable memory while this function fills and copies into the destination.
#[no_mangle]
pub unsafe extern "C" fn cobol_screen_accept(dst: *mut u8, len: u32) -> u32 {
    if dst.is_null() || len == 0 {
        return 0;
    }

    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        std::ptr::write_bytes(dst, b' ', len as usize);
        return 0;
    }

    let trimmed = input.trim_end_matches(['\r', '\n']);
    let bytes = trimmed.as_bytes();
    let copy_len = bytes.len().min(len as usize);

    std::ptr::write_bytes(dst, b' ', len as usize);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, copy_len);

    copy_len as u32
}

/// Read one terminal input line with a PICTURE-derived input mask.
///
/// `PIC 9` fields accept only ASCII digits and ignore other characters before
/// copying into the fixed-width destination. Other pictures use the normal
/// fixed-width line input behavior.
///
/// # Safety
///
/// `dst` must be valid for writes of `len` bytes. `pic_ptr` must be null or
/// valid for reads of `pic_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_screen_accept_pic(
    dst: *mut u8,
    len: u32,
    pic_ptr: *const u8,
    pic_len: u32,
) -> u32 {
    if dst.is_null() || len == 0 {
        return 0;
    }
    if pic_ptr.is_null() || pic_len == 0 {
        return cobol_screen_accept(dst, len);
    }

    let picture = std::slice::from_raw_parts(pic_ptr, pic_len as usize);
    let numeric_mask = picture.iter().any(|byte| byte.eq_ignore_ascii_case(&b'9'));
    if !numeric_mask {
        return cobol_screen_accept(dst, len);
    }

    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        std::ptr::write_bytes(dst, b' ', len as usize);
        return 0;
    }

    std::ptr::write_bytes(dst, b' ', len as usize);
    let mut copied = 0usize;
    for byte in input.bytes().filter(u8::is_ascii_digit) {
        if copied >= len as usize {
            break;
        }
        *dst.add(copied) = byte;
        copied += 1;
    }

    copied as u32
}
