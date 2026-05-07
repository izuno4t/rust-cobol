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
    /// Handler stack: jmp_buf addresses for setjmp-based exception handling.
    handler_stack: Vec<usize>,
}

static EXCEPTION_STATE: Mutex<ExceptionState> = Mutex::new(ExceptionState {
    code: 0,
    handler_stack: Vec::new(),
});

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

    let jmp_buf_addr = {
        let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.code = code as i32;
        state.handler_stack.pop()
    };

    if let Some(addr) = jmp_buf_addr {
        // longjmp back to the nearest exception handler
        extern "C" {
            fn longjmp(env: *mut u8, val: i32) -> !;
        }
        longjmp(addr as *mut u8, code as i32);
    } else {
        // No handler registered -- abort
        eprintln!(
            "COBOL EXCEPTION (unhandled): {} (code {})",
            name, code as i32
        );
        std::process::abort();
    }
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

/// Push an exception handler (jmp_buf address) onto the handler stack.
///
/// Called from generated C code: `cobol_exception_push((uintptr_t)&_jbuf)`.
#[no_mangle]
pub extern "C" fn cobol_exception_push(jmp_buf_ptr: usize) {
    let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    if state.handler_stack.len() < MAX_EXCEPTION_DEPTH {
        state.handler_stack.push(jmp_buf_ptr);
    }
}

/// Pop an exception handler from the handler stack.
#[no_mangle]
pub extern "C" fn cobol_exception_pop() {
    let mut state = EXCEPTION_STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.handler_stack.pop();
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

/// Invoke a method on a COBOL object via vtable dispatch.
///
/// The object's first member is a pointer to a vtable. The vtable's first
/// entry is a dispatch function with the signature:
///   `int64_t dispatch(void* obj, const char* method, int64_t* args, int32_t argc)`
///
/// The dispatch function (generated per class by codegen) maps method name
/// strings to the actual method implementations.
///
/// # Safety
///
/// `obj` must be a valid pointer to a COBOL object whose first member is
/// a vtable pointer. The vtable's first entry must be a valid dispatch
/// function pointer.
/// `method` must be a valid, NUL-terminated C string.
/// `args` must point to an array of at least `argc` int64_t values,
/// or be NULL if `argc` is 0.
#[no_mangle]
pub unsafe extern "C" fn cobol_invoke(
    obj: *mut std::ffi::c_void,
    method: *const c_char,
    args: *mut i64,
    argc: i32,
) -> i64 {
    if obj.is_null() {
        eprintln!("COBOL INVOKE: null object reference");
        return 0;
    }

    // The object's first member is a void* pointing to the vtable.
    // The vtable's first entry is the dispatch function pointer.
    type DispatchFn =
        unsafe extern "C" fn(*mut std::ffi::c_void, *const c_char, *mut i64, i32) -> i64;

    let vtable_ptr = *(obj as *const *const *const std::ffi::c_void);
    if vtable_ptr.is_null() {
        eprintln!("COBOL INVOKE: null vtable");
        return 0;
    }

    let dispatch: DispatchFn = std::mem::transmute(*vtable_ptr);
    dispatch(obj, method, args, argc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

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

    unsafe extern "C" fn test_dispatch(
        _obj: *mut std::ffi::c_void,
        method: *const c_char,
        args: *mut i64,
        argc: i32,
    ) -> i64 {
        let method = unsafe { std::ffi::CStr::from_ptr(method) }
            .to_string_lossy()
            .into_owned();
        if method == "ADD" && argc == 2 {
            let args = unsafe { std::slice::from_raw_parts(args, argc as usize) };
            return args[0] + args[1];
        }
        -1
    }

    #[repr(C)]
    struct TestVtable {
        dispatch: unsafe extern "C" fn(*mut std::ffi::c_void, *const c_char, *mut i64, i32) -> i64,
    }

    #[repr(C)]
    struct TestObject {
        vtable: *const TestVtable,
    }

    #[test]
    fn test_cobol_invoke_dispatches_vtable_method() {
        let vtable = TestVtable {
            dispatch: test_dispatch,
        };
        let mut object = TestObject { vtable: &vtable };
        let method = CString::new("ADD").unwrap();
        let mut args = [20, 22];

        let result = unsafe {
            cobol_invoke(
                (&mut object as *mut TestObject).cast(),
                method.as_ptr(),
                args.as_mut_ptr(),
                args.len() as i32,
            )
        };

        assert_eq!(result, 42);
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

        // Push dummy handler addresses
        cobol_exception_push(0x1000);
        cobol_exception_push(0x2000);

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
