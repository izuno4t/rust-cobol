// COBOL Runtime - DISPLAY statement support
//
// These functions are called from generated code via C ABI to implement
// the COBOL DISPLAY statement. Each variant handles a different operand
// type.

use std::io::{self, Write};

/// Display a UTF-8 string (for alphanumeric operands).
///
/// # Safety
/// `ptr` must point to a valid, readable region of `len` bytes.
/// This function is called from generated C code via the C ABI, so
/// the caller is responsible for providing valid arguments.
#[no_mangle]
pub unsafe extern "C" fn cobol_display_string(ptr: *const u8, len: u32) {
    let slice = std::slice::from_raw_parts(ptr, len as usize);
    let s = std::str::from_utf8(slice).unwrap_or("<invalid utf8>");
    print!("{}", s);
}

/// Display a 64-bit signed integer (for numeric operands).
#[no_mangle]
pub extern "C" fn cobol_display_int(value: i64) {
    print!("{}", value);
}

/// Output a newline (default behaviour after DISPLAY unless NO ADVANCING).
#[no_mangle]
pub extern "C" fn cobol_display_newline() {
    println!();
}

/// Output a single space (separator between DISPLAY operands).
#[no_mangle]
pub extern "C" fn cobol_display_space() {
    print!(" ");
}

/// Flush stdout to ensure all output has been written.
#[no_mangle]
pub extern "C" fn cobol_display_flush() {
    let _ = io::stdout().flush();
}
