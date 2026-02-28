// COBOL Runtime - Program lifecycle support
//
// Implements STOP RUN and GOBACK, which terminate the running COBOL
// program. These functions are called from generated code via C ABI.

/// STOP RUN -- terminate the program with exit code 0.
#[no_mangle]
pub extern "C" fn cobol_stop_run() -> ! {
    std::process::exit(0);
}

/// GOBACK -- identical to STOP RUN for the main program.
///
/// In a sub-program context GOBACK would return control to the caller,
/// but for now we treat it as program termination.
#[no_mangle]
pub extern "C" fn cobol_goback() -> ! {
    std::process::exit(0);
}
