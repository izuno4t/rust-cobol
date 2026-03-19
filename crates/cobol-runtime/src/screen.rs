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
