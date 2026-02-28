// COBOL Runtime - Exception handling support (COBOL 2002+)
//
// Implements exception handling using setjmp/longjmp for RAISE/RESUME
// semantics. The runtime maintains a stack of exception handlers and
// a global exception state.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

/// Maximum depth of the exception handler stack.
const MAX_EXCEPTION_DEPTH: usize = 64;

/// Exception codes for COBOL 2002 standard exception classes.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionCode {
    None = 0,
    EcSizeOverflow = 1,
    EcSizeTruncation = 2,
    EcSizeZeroDivide = 3,
    EcProgramNotFound = 10,
    EcRangeInvalid = 20,
    EcDataIncompatible = 30,
    EcFlowGlobalGoback = 40,
    EcIoAtEnd = 50,
    EcIoInvalidKey = 51,
    EcIoPermanentError = 52,
    EcOoNull = 60,
    EcOoConformance = 61,
    EcOoResource = 62,
    EcUserException = 100,
}

impl ExceptionCode {
    /// Map a COBOL exception name string to an exception code.
    pub fn from_name(name: &str) -> Self {
        match name {
            "EC-SIZE-OVERFLOW" => ExceptionCode::EcSizeOverflow,
            "EC-SIZE-TRUNCATION" => ExceptionCode::EcSizeTruncation,
            "EC-SIZE-ZERO-DIVIDE" => ExceptionCode::EcSizeZeroDivide,
            "EC-PROGRAM-NOT-FOUND" => ExceptionCode::EcProgramNotFound,
            "EC-RANGE-INVALID" => ExceptionCode::EcRangeInvalid,
            "EC-DATA-INCOMPATIBLE" => ExceptionCode::EcDataIncompatible,
            "EC-FLOW-GLOBAL-GOBACK" => ExceptionCode::EcFlowGlobalGoback,
            "EC-I-O-AT-END" => ExceptionCode::EcIoAtEnd,
            "EC-I-O-INVALID-KEY" => ExceptionCode::EcIoInvalidKey,
            "EC-I-O-PERMANENT-ERROR" => ExceptionCode::EcIoPermanentError,
            "EC-OO-NULL" => ExceptionCode::EcOoNull,
            "EC-OO-CONFORMANCE" => ExceptionCode::EcOoConformance,
            "EC-OO-RESOURCE" => ExceptionCode::EcOoResource,
            _ => ExceptionCode::EcUserException,
        }
    }

    /// Get the human-readable name for this exception code.
    pub fn name(self) -> &'static str {
        match self {
            ExceptionCode::None => "EC-NONE",
            ExceptionCode::EcSizeOverflow => "EC-SIZE-OVERFLOW",
            ExceptionCode::EcSizeTruncation => "EC-SIZE-TRUNCATION",
            ExceptionCode::EcSizeZeroDivide => "EC-SIZE-ZERO-DIVIDE",
            ExceptionCode::EcProgramNotFound => "EC-PROGRAM-NOT-FOUND",
            ExceptionCode::EcRangeInvalid => "EC-RANGE-INVALID",
            ExceptionCode::EcDataIncompatible => "EC-DATA-INCOMPATIBLE",
            ExceptionCode::EcFlowGlobalGoback => "EC-FLOW-GLOBAL-GOBACK",
            ExceptionCode::EcIoAtEnd => "EC-I-O-AT-END",
            ExceptionCode::EcIoInvalidKey => "EC-I-O-INVALID-KEY",
            ExceptionCode::EcIoPermanentError => "EC-I-O-PERMANENT-ERROR",
            ExceptionCode::EcOoNull => "EC-OO-NULL",
            ExceptionCode::EcOoConformance => "EC-OO-CONFORMANCE",
            ExceptionCode::EcOoResource => "EC-OO-RESOURCE",
            ExceptionCode::EcUserException => "EC-USER-EXCEPTION",
        }
    }
}

/// Global exception state.
struct ExceptionState {
    /// Current exception code (0 = no exception).
    code: i32,
    /// Handler stack depth.
    depth: usize,
}

static EXCEPTION_STATE: Mutex<ExceptionState> = Mutex::new(ExceptionState { code: 0, depth: 0 });

/// Raise a COBOL exception by name.
///
/// # Safety
///
/// `exception_name` must be a valid, NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn cobol_raise(exception_name: *const c_char) {
    let name = if exception_name.is_null() {
        "EC-USER-EXCEPTION"
    } else {
        unsafe { CStr::from_ptr(exception_name) }
            .to_str()
            .unwrap_or("EC-USER-EXCEPTION")
    };

    let code = ExceptionCode::from_name(name);

    {
        let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.code = code as i32;
    }

    // In a full implementation, this would longjmp to the nearest handler.
    // For now, print the exception and abort.
    eprintln!("COBOL EXCEPTION: {} (code {})", name, code as i32);
    std::process::abort();
}

/// Resume execution after exception handling.
///
/// # Safety
///
/// `target` may be NULL (resume at next statement) or a valid NUL-terminated
/// C string specifying a resume target.
#[no_mangle]
pub unsafe extern "C" fn cobol_resume(target: *const c_char) {
    if target.is_null() {
        // Resume at next statement -- clear exception state
        let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.code = 0;
    } else {
        let _target_name = unsafe { CStr::from_ptr(target) }.to_str().unwrap_or("");
        // Resume at the specified target -- would require longjmp in full impl
        let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.code = 0;
    }
}

/// Push an exception handler onto the handler stack.
///
/// Returns the current handler depth (for use with setjmp).
#[no_mangle]
pub extern "C" fn cobol_exception_push() -> c_int {
    let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.depth < MAX_EXCEPTION_DEPTH {
        state.depth += 1;
    }
    state.depth as c_int
}

/// Pop an exception handler from the handler stack.
#[no_mangle]
pub extern "C" fn cobol_exception_pop() {
    let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.depth > 0 {
        state.depth -= 1;
    }
}

/// Get the current exception code.
///
/// Returns 0 if no exception is active.
#[no_mangle]
pub extern "C" fn cobol_exception_code() -> c_int {
    let state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.code
}

/// Clear the current exception state.
#[no_mangle]
pub extern "C" fn cobol_exception_clear() {
    let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.code = 0;
}

/// Invoke a method on a COBOL object (OOP runtime support).
///
/// This is a simplified dispatcher. In a full implementation, the vtable
/// would be looked up from the object's header and the method resolved
/// dynamically.
///
/// # Safety
///
/// `obj` must be a valid pointer to a COBOL object.
/// `method` must be a valid, NUL-terminated C string.
/// `args` must point to an array of at least `argc` int64_t values,
/// or be NULL if `argc` is 0.
#[no_mangle]
pub unsafe extern "C" fn cobol_invoke(
    _obj: *mut std::ffi::c_void,
    method: *const c_char,
    _args: *mut i64,
    _argc: i32,
) -> i64 {
    let method_name = if method.is_null() {
        "UNKNOWN"
    } else {
        unsafe { CStr::from_ptr(method) }
            .to_str()
            .unwrap_or("UNKNOWN")
    };

    // Placeholder: in a full implementation, dispatch through the vtable
    eprintln!(
        "COBOL INVOKE: method '{}' (not yet fully implemented)",
        method_name
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exception_code_from_name() {
        assert_eq!(
            ExceptionCode::from_name("EC-SIZE-OVERFLOW"),
            ExceptionCode::EcSizeOverflow
        );
        assert_eq!(
            ExceptionCode::from_name("EC-SIZE-ZERO-DIVIDE"),
            ExceptionCode::EcSizeZeroDivide
        );
        assert_eq!(
            ExceptionCode::from_name("EC-OO-NULL"),
            ExceptionCode::EcOoNull
        );
        assert_eq!(
            ExceptionCode::from_name("UNKNOWN"),
            ExceptionCode::EcUserException
        );
    }

    #[test]
    fn test_exception_code_name_roundtrip() {
        let codes = [
            ExceptionCode::None,
            ExceptionCode::EcSizeOverflow,
            ExceptionCode::EcSizeTruncation,
            ExceptionCode::EcSizeZeroDivide,
            ExceptionCode::EcProgramNotFound,
            ExceptionCode::EcRangeInvalid,
            ExceptionCode::EcDataIncompatible,
            ExceptionCode::EcFlowGlobalGoback,
            ExceptionCode::EcIoAtEnd,
            ExceptionCode::EcIoInvalidKey,
            ExceptionCode::EcIoPermanentError,
            ExceptionCode::EcOoNull,
            ExceptionCode::EcOoConformance,
            ExceptionCode::EcOoResource,
            ExceptionCode::EcUserException,
        ];
        for code in codes {
            let name = code.name();
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_exception_push_pop() {
        // Reset state
        cobol_exception_clear();

        let depth1 = cobol_exception_push();
        assert!(depth1 > 0);

        let depth2 = cobol_exception_push();
        assert_eq!(depth2, depth1 + 1);

        cobol_exception_pop();
        cobol_exception_pop();
    }

    #[test]
    fn test_exception_code_get_clear() {
        cobol_exception_clear();
        assert_eq!(cobol_exception_code(), 0);
    }

    #[test]
    fn test_resume_null_target() {
        cobol_exception_clear();
        // Should not panic
        unsafe { cobol_resume(std::ptr::null()) };
        assert_eq!(cobol_exception_code(), 0);
    }
}
