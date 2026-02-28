// COBOL Compiler - Code generation
//
// This crate translates COBOL HIR into executable code. The current
// backend generates C source code which is then compiled with a system
// C compiler (clang/gcc) and linked against the COBOL runtime library.
//
// This approach was chosen because the system's LLVM version (22.x) is
// not supported by the inkwell bindings (which support up to LLVM 18).

pub mod codegen;

pub use codegen::{compile_c_to_executable, generate_c};
