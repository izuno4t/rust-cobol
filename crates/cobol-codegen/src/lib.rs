// COBOL Compiler - Code generation
//
// This crate translates COBOL HIR into C source code. Invoking the C
// compiler and linking the runtime library are handled by the driver
// toolchain layer.
//
// This approach was chosen because the system's LLVM version (22.x) is
// not supported by the inkwell bindings (which support up to LLVM 18).

pub mod codegen;

pub use codegen::generate_c;
