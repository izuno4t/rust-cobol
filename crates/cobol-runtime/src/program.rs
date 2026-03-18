// COBOL Runtime - Program lifecycle support
//
// Implements STOP RUN and GOBACK, which terminate the running COBOL
// program. These functions are called from generated code via C ABI.
//
// For sub-programs called via CALL, GOBACK should return to the caller.
// This is implemented using setjmp/longjmp: the CALL site pushes a
// jmp_buf via cobol_call_enter(), and GOBACK does longjmp to return.
// Normal return from a sub-program calls cobol_call_leave() to pop
// the jmp_buf without jumping.

use std::sync::Mutex;

/// Stack of jmp_buf pointers for nested CALL/GOBACK.
/// Each entry is a pointer to a C jmp_buf set up by the caller.
static CALL_STACK: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// STOP RUN -- terminate the program with exit code 0.
///
/// Flushes stdout and closes all open files before exiting.
#[no_mangle]
pub extern "C" fn cobol_stop_run() -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    crate::file_io::close_all_files();
    std::process::exit(0);
}

/// GOBACK -- return to the caller if in a sub-program, or exit if in main.
///
/// If a jmp_buf has been pushed (by cobol_call_enter), longjmp back to it.
/// Otherwise, behave like STOP RUN.
///
/// # Safety
///
/// The jmp_buf pointer on the call stack must be valid and its corresponding
/// setjmp frame must still be on the C call stack.
#[no_mangle]
pub unsafe extern "C" fn cobol_goback() {
    use std::io::Write;
    let _ = std::io::stdout().flush();

    let buf = {
        let mut stack = CALL_STACK.lock().unwrap();
        stack.pop()
    };

    if let Some(jmp_buf_addr) = buf {
        // longjmp back to the CALL site
        extern "C" {
            fn longjmp(env: *mut u8, val: i32) -> !;
        }
        longjmp(jmp_buf_addr as *mut u8, 1);
    } else {
        // Main program -- terminate
        crate::file_io::close_all_files();
        std::process::exit(0);
    }
}

/// Push a jmp_buf address onto the call stack before calling a sub-program.
/// Called from generated C code: `cobol_call_enter(&_jbuf)`.
#[no_mangle]
pub extern "C" fn cobol_call_enter(jmp_buf_ptr: usize) {
    let mut stack = CALL_STACK.lock().unwrap();
    stack.push(jmp_buf_ptr);
}

/// Pop the jmp_buf from the call stack after a sub-program returns normally
/// (without GOBACK). Called from generated C code after the CALL returns.
#[no_mangle]
pub extern "C" fn cobol_call_leave() {
    let mut stack = CALL_STACK.lock().unwrap();
    stack.pop();
}
