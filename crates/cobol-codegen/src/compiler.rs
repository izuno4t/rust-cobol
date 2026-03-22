pub fn compile_c_to_executable(
    c_source_path: &std::path::Path,
    output_path: &std::path::Path,
    runtime_lib_path: &std::path::Path,
) -> Result<(), String> {
    // Try clang first, then cc
    let compiler = find_c_compiler()?;

    let status = std::process::Command::new(&compiler)
        .arg(c_source_path)
        .arg("-o")
        .arg(output_path)
        .arg(format!("-L{}", runtime_lib_path.display()))
        .arg("-lcobol_runtime")
        .arg("-lpthread")
        .arg("-ldl")
        .arg("-lm")
        .status()
        .map_err(|e| format!("Failed to run C compiler '{}': {}", compiler, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "C compiler '{}' exited with status: {}",
            compiler, status
        ))
    }
}

fn find_c_compiler() -> Result<String, String> {
    // Check CC environment variable
    if let Ok(cc) = std::env::var("CC") {
        return Ok(cc);
    }

    // Try clang, then gcc, then cc
    for compiler in &["clang", "gcc", "cc"] {
        if std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok(compiler.to_string());
        }
    }

    Err("No C compiler found. Install clang or gcc.".to_string())
}

#[cfg(test)]
mod tests {
    use crate::codegen::{escape_c_string, hir_type_to_c, sanitize_name};
    use crate::generate_c;
    use cobol_common::{FileId, SourceFormat};
    use cobol_hir::{lower_to_hir, HirDataItem, HirProgram, HirStatement, HirType};
    use cobol_lexer::Lexer;
    use cobol_parser::Parser;

    fn parse_lower_generate(source: &str) -> String {
        let mut lexer = Lexer::new(source, FileId(0), SourceFormat::Free);
        let tokens = lexer.lex_all();
        let mut parser = Parser::new(tokens, FileId(0));
        let program = parser.parse_program().unwrap();
        let hir = lower_to_hir(&program);
        generate_c(&hir)
    }

    #[test]
    fn test_generate_hello_world() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. HELLO-WORLD.
PROCEDURE DIVISION.
    DISPLAY \"Hello, World!\".
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("cobol_display_string"));
        assert!(c_code.contains("Hello, World!"));
        assert!(c_code.contains("cobol_display_newline"));
        assert!(c_code.contains("cobol_stop_run"));
        assert!(c_code.contains("int main"));
    }

    #[test]
    fn test_generate_with_data_items() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DATA.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-NAME PIC X(20).
01  WS-COUNT PIC 9(5).
PROCEDURE DIVISION.
    DISPLAY WS-COUNT.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("static char WS_NAME"));
        assert!(c_code.contains("static int64_t WS_COUNT"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("WS-NAME"), "WS_NAME");
        assert_eq!(sanitize_name("HELLO-WORLD"), "HELLO_WORLD");
        assert_eq!(sanitize_name("SIMPLE"), "SIMPLE");
        // C reserved words are prefixed with cob_
        assert_eq!(sanitize_name("int"), "cob_int");
        assert_eq!(sanitize_name("main"), "cob_main");
        assert_eq!(sanitize_name("return"), "cob_return");
        // Names starting with a digit are prefixed with cob_
        assert_eq!(sanitize_name("1ST-FIELD"), "cob_1ST_FIELD");
    }

    #[test]
    fn test_escape_c_string() {
        assert_eq!(escape_c_string("hello"), "hello");
        assert_eq!(escape_c_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_c_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_generate_if_statement() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-IF.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-A PIC 9(3).
PROCEDURE DIVISION.
    IF WS-A > 100
        DISPLAY \"BIG\"
    ELSE
        DISPLAY \"SMALL\"
    END-IF.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(c_code.contains("if ("));
        assert!(c_code.contains("} else {"));
    }

    // -----------------------------------------------------------------------
    // COBOL 2002+ codegen tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_raise() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RAISE.
PROCEDURE DIVISION.
    RAISE EXCEPTION \"EC-SIZE-OVERFLOW\".
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_raise"),
            "Generated C should contain cobol_raise call"
        );
    }

    #[test]
    fn test_generate_resume() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-RESUME.
PROCEDURE DIVISION.
    RESUME.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_resume"),
            "Generated C should contain cobol_resume call"
        );
    }

    #[test]
    fn test_generate_invoke() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-INVOKE.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  MY-OBJ USAGE POINTER.
01  MY-RESULT PIC 9(5).
PROCEDURE DIVISION.
    INVOKE MY-OBJ \"DO-SOMETHING\" RETURNING MY-RESULT.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_invoke"),
            "Generated C should contain cobol_invoke call"
        );
        assert!(
            c_code.contains("DO-SOMETHING"),
            "Generated C should reference the method name"
        );
    }

    #[test]
    fn test_generate_allocate_and_free() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-ALLOC.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  MY-PTR USAGE POINTER.
PROCEDURE DIVISION.
    ALLOCATE MY-PTR.
    FREE MY-PTR.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("malloc"),
            "Generated C should contain malloc for ALLOCATE"
        );
        assert!(
            c_code.contains("free("),
            "Generated C should contain free for FREE"
        );
    }

    #[test]
    fn test_generate_setjmp_header() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-HDR.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("#include <setjmp.h>"),
            "Generated C should include setjmp.h"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2002() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_raise"),
            "Runtime declarations should include cobol_raise"
        );
        assert!(
            c_code.contains("cobol_resume"),
            "Runtime declarations should include cobol_resume"
        );
        assert!(
            c_code.contains("cobol_invoke"),
            "Runtime declarations should include cobol_invoke"
        );
    }

    #[test]
    fn test_generate_class_struct() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-CLASS".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: vec![cobol_hir::HirClass {
                name: "MY-CLASS".into(),
                parent: None,
                factory_methods: Vec::new(),
                instance_methods: vec![cobol_hir::HirMethod {
                    name: "DO-WORK".into(),
                    params: Vec::new(),
                    returning: None,
                    data_items: Vec::new(),
                    body: Vec::new(),
                    span: Span::dummy(),
                }],
                factory_data: Vec::new(),
                instance_data: vec![HirDataItem {
                    name: "MY-FIELD".into(),
                    data_type: HirType::Numeric {
                        size: 5,
                        decimal_places: 0,
                        is_signed: false,
                    },
                    initial_value: None,
                    occurs: None,
                    indexed_by: Vec::new(),
                    redefines: None,
                    renames: None,
                    screen_info: None,
                    span: Span::dummy(),
                }],
                span: Span::dummy(),
            }],
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("typedef struct MY_CLASS_s"),
            "Should generate struct for class"
        );
        assert!(c_code.contains("_vtable"), "Should generate vtable");
        assert!(
            c_code.contains("MY_CLASS_new"),
            "Should generate constructor"
        );
        assert!(
            c_code.contains("MY_CLASS_DO_WORK"),
            "Should generate method implementation"
        );
    }

    #[test]
    fn test_generate_function() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-FUNC".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: vec![cobol_hir::HirFunction {
                name: "ADD-NUMBERS".into(),
                params: vec![
                    cobol_hir::HirParam {
                        name: "A".into(),
                        mode: cobol_hir::HirParamMode::ByValue,
                        data_type: HirType::Numeric {
                            size: 5,
                            decimal_places: 0,
                            is_signed: false,
                        },
                    },
                    cobol_hir::HirParam {
                        name: "B".into(),
                        mode: cobol_hir::HirParamMode::ByValue,
                        data_type: HirType::Numeric {
                            size: 5,
                            decimal_places: 0,
                            is_signed: false,
                        },
                    },
                ],
                returning: HirType::Numeric {
                    size: 5,
                    decimal_places: 0,
                    is_signed: false,
                },
                data_items: Vec::new(),
                body: Vec::new(),
                span: Span::dummy(),
            }],
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_func_add_numbers"),
            "Should generate function with cobol_func_ prefix (lowercase)"
        );
        assert!(c_code.contains("int64_t A"), "Should generate parameter A");
        assert!(c_code.contains("int64_t B"), "Should generate parameter B");
    }

    #[test]
    fn test_hir_type_to_c_boolean() {
        assert_eq!(hir_type_to_c(&HirType::Boolean), "int8_t");
    }

    #[test]
    fn test_generate_local_storage() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-LOCAL.
DATA DIVISION.
LOCAL-STORAGE SECTION.
01  LS-COUNTER PIC 9(5) VALUE 0.
WORKING-STORAGE SECTION.
01  WS-COUNTER PIC 9(5) VALUE 0.
PROCEDURE DIVISION.
    ADD 1 TO LS-COUNTER.
    ADD 1 TO WS-COUNTER.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("LS_COUNTER"),
            "Should emit LOCAL-STORAGE variable"
        );
        assert!(
            c_code.contains("WS_COUNTER"),
            "Should emit WORKING-STORAGE variable"
        );
    }

    // -----------------------------------------------------------------------
    // COBOL 2014+ codegen tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_float_short() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static float WS_FLOAT"),
            "FLOAT-SHORT should generate C float type"
        );
    }

    #[test]
    fn test_generate_float_long() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-L.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-L USAGE FLOAT-LONG.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static double WS_FLOAT_L"),
            "FLOAT-LONG should generate C double type"
        );
    }

    #[test]
    fn test_generate_float_extended() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-E.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-FLOAT-E USAGE FLOAT-EXTENDED.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("static long double WS_FLOAT_E"),
            "FLOAT-EXTENDED should generate C long double type"
        );
    }

    #[test]
    fn test_generate_float_init() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-FLOAT-INIT.
DATA DIVISION.
WORKING-STORAGE SECTION.
01  WS-F USAGE FLOAT-SHORT.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("WS_F = 0.0"),
            "Float data items should be initialized to 0.0"
        );
    }

    #[test]
    fn test_hir_type_to_c_float_short() {
        assert_eq!(hir_type_to_c(&HirType::FloatShort), "float");
    }

    #[test]
    fn test_hir_type_to_c_float_long() {
        assert_eq!(hir_type_to_c(&HirType::FloatLong), "double");
    }

    #[test]
    fn test_hir_type_to_c_float_extended() {
        assert_eq!(hir_type_to_c(&HirType::FloatExtended), "long double");
    }

    #[test]
    fn test_generate_validate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-VALIDATE".into(),
            data_items: vec![HirDataItem {
                name: "WS-NAME".into(),
                data_type: HirType::Alphanumeric { size: 20 },
                initial_value: None,
                occurs: None,
                indexed_by: Vec::new(),
                redefines: None,
                renames: None,
                screen_info: None,
                span: Span::dummy(),
            }],
            paragraphs: Vec::new(),
            body: vec![HirStatement::Validate {
                target: "WS-NAME".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_validate"),
            "VALIDATE should generate cobol_validate call"
        );
    }

    #[test]
    fn test_generate_json_generate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-JSON-GEN".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::JsonGenerate {
                source: "WS-DATA".into(),
                target: "WS-JSON".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_json_generate"),
            "JSON GENERATE should emit cobol_json_generate call"
        );
    }

    #[test]
    fn test_generate_json_parse_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-JSON-PARSE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::JsonParse {
                source: "WS-JSON".into(),
                target: "WS-DATA".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_json_parse"),
            "JSON PARSE should emit cobol_json_parse call"
        );
    }

    #[test]
    fn test_generate_xml_generate_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-XML-GEN".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::XmlGenerate {
                source: "WS-DATA".into(),
                target: "WS-XML".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("cobol_xml_generate"),
            "XML GENERATE should emit cobol_xml_generate call"
        );
    }

    #[test]
    fn test_generate_xml_parse_statement() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-XML-PARSE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: vec![HirStatement::XmlParse {
                source: "WS-XML".into(),
                processing_procedure: "XML-HANDLER".into(),
                span: Span::dummy(),
            }],
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("XML PARSE"),
            "XML PARSE should emit XML PARSE comment"
        );
        assert!(
            c_code.contains("XML_HANDLER"),
            "XML PARSE should reference processing procedure"
        );
    }

    #[test]
    fn test_generate_typedef() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-TYPEDEF".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: vec![cobol_hir::HirTypedef {
                name: "MONEY-TYPE".into(),
                base_type: HirType::Numeric {
                    size: 9,
                    decimal_places: 2,
                    is_signed: true,
                },
                span: Span::dummy(),
            }],
            interfaces: Vec::new(),
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("typedef int64_t MONEY_TYPE"),
            "TYPEDEF should generate C typedef"
        );
    }

    #[test]
    fn test_generate_interface() {
        use cobol_common::Span;

        let hir = HirProgram {
            name: "TEST-IFACE".into(),
            data_items: Vec::new(),
            paragraphs: Vec::new(),
            body: Vec::new(),
            classes: Vec::new(),
            functions: Vec::new(),
            typedefs: Vec::new(),
            interfaces: vec![cobol_hir::HirInterface {
                name: "IComparable".into(),
                methods: vec![cobol_hir::HirMethod {
                    name: "CompareTo".into(),
                    params: Vec::new(),
                    returning: None,
                    data_items: Vec::new(),
                    body: Vec::new(),
                    span: Span::dummy(),
                }],
                span: Span::dummy(),
            }],
            using_params: Vec::new(),
            file_organizations: std::collections::HashMap::new(),
            file_assignments: std::collections::HashMap::new(),
            file_status_vars: Vec::new(),
            declaratives: Vec::new(),
            file_records: std::collections::HashMap::new(),
            fd_record_aliases: std::collections::HashMap::new(),
            nested_programs: Vec::new(),
            span: Span::dummy(),
        };

        let c_code = generate_c(&hir);
        assert!(
            c_code.contains("INTERFACE IComparable"),
            "Should generate interface comment"
        );
        assert!(
            c_code.contains("IComparable_vtable"),
            "Should generate vtable for interface"
        );
        assert!(
            c_code.contains("CompareTo"),
            "Should include method in vtable"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2014() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL-2014.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_validate"),
            "Should declare cobol_validate"
        );
        assert!(
            c_code.contains("cobol_json_generate"),
            "Should declare cobol_json_generate"
        );
        assert!(
            c_code.contains("cobol_json_parse"),
            "Should declare cobol_json_parse"
        );
        assert!(
            c_code.contains("cobol_xml_generate"),
            "Should declare cobol_xml_generate"
        );
        assert!(
            c_code.contains("cobol_xml_parse"),
            "Should declare cobol_xml_parse"
        );
    }

    #[test]
    fn test_generate_runtime_declarations_2023() {
        let src = "\
IDENTIFICATION DIVISION.
PROGRAM-ID. TEST-DECL-2023.
PROCEDURE DIVISION.
    STOP RUN.
";
        let c_code = parse_lower_generate(src);
        assert!(
            c_code.contains("cobol_utf8_char_count"),
            "Should declare cobol_utf8_char_count"
        );
        assert!(
            c_code.contains("cobol_utf8_substring"),
            "Should declare cobol_utf8_substring"
        );
        assert!(
            c_code.contains("cobol_thread_create"),
            "Should declare cobol_thread_create"
        );
        assert!(
            c_code.contains("cobol_mutex_create"),
            "Should declare cobol_mutex_create"
        );
    }
}
