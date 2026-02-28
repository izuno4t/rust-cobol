// COBOL Runtime - JSON support (COBOL 2014+)
//
// Provides JSON GENERATE and JSON PARSE runtime functions for COBOL programs.
// These functions handle serialization and deserialization of COBOL group items
// to/from JSON format.

/// Describes a single COBOL field for JSON serialization/deserialization.
#[repr(C)]
pub struct CobolJsonField {
    pub name_ptr: *const u8,
    pub name_len: u32,
    pub value_ptr: *const u8,
    pub value_len: u32,
    /// Value type: 0 = string, 1 = number, 2 = bool.
    pub value_type: u32,
}

/// JSON GENERATE -- serialize COBOL group item fields to JSON.
///
/// Writes a JSON object to `output_ptr` using the field descriptors.
/// Returns the number of bytes actually written.
///
/// # Safety
/// - `fields` must point to a valid array of `field_count` `CobolJsonField` items.
/// - `output_ptr` must point to a writable region of `output_len` bytes.
/// - All pointers within the `CobolJsonField` items must be valid.
#[no_mangle]
pub unsafe extern "C" fn cobol_json_generate(
    fields: *const CobolJsonField,
    field_count: u32,
    output_ptr: *mut u8,
    output_len: u32,
) -> u32 {
    if fields.is_null() || output_ptr.is_null() || output_len == 0 {
        return 0;
    }

    let field_slice = std::slice::from_raw_parts(fields, field_count as usize);

    let mut json = String::with_capacity(output_len as usize);
    json.push('{');

    for (i, field) in field_slice.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }

        // Write field name
        let name = if !field.name_ptr.is_null() && field.name_len > 0 {
            let name_bytes = std::slice::from_raw_parts(field.name_ptr, field.name_len as usize);
            std::str::from_utf8(name_bytes).unwrap_or("unknown")
        } else {
            "unknown"
        };
        json.push('"');
        json_escape_string(&mut json, name);
        json.push('"');
        json.push(':');

        // Write field value
        let value = if !field.value_ptr.is_null() && field.value_len > 0 {
            let val_bytes = std::slice::from_raw_parts(field.value_ptr, field.value_len as usize);
            std::str::from_utf8(val_bytes)
                .unwrap_or("")
                .trim_end()
                .to_string()
        } else {
            String::new()
        };

        match field.value_type {
            0 => {
                // String
                json.push('"');
                json_escape_string(&mut json, &value);
                json.push('"');
            }
            1 => {
                // Number
                if value.is_empty() {
                    json.push('0');
                } else {
                    json.push_str(&value);
                }
            }
            2 => {
                // Boolean
                if value == "1" || value.eq_ignore_ascii_case("true") {
                    json.push_str("true");
                } else {
                    json.push_str("false");
                }
            }
            _ => {
                json.push_str("null");
            }
        }
    }

    json.push('}');

    let json_bytes = json.as_bytes();
    let copy_len = json_bytes.len().min(output_len as usize);
    std::ptr::copy_nonoverlapping(json_bytes.as_ptr(), output_ptr, copy_len);

    copy_len as u32
}

/// JSON PARSE -- deserialize JSON into COBOL data item fields.
///
/// Returns 0 on success, 1 on parse error.
///
/// # Safety
/// - `json_ptr` must point to a valid, readable region of `json_len` bytes
///   containing valid UTF-8 JSON data.
/// - `fields` must point to a writable array of `field_count` `CobolJsonField` items.
/// - The `value_ptr` within each field must point to a writable region of `value_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_json_parse(
    json_ptr: *const u8,
    json_len: u32,
    fields: *mut CobolJsonField,
    field_count: u32,
) -> u32 {
    if json_ptr.is_null() || fields.is_null() || json_len == 0 {
        return 1;
    }

    let json_slice = std::slice::from_raw_parts(json_ptr, json_len as usize);
    let json_str = match std::str::from_utf8(json_slice) {
        Ok(s) => s,
        Err(_) => return 1,
    };

    // Simple JSON object parser -- handles flat objects only.
    // A production implementation would use a proper JSON parser.
    let trimmed = json_str.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return 1;
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let field_slice = std::slice::from_raw_parts_mut(fields, field_count as usize);

    // Parse key-value pairs and match them to fields
    let mut field_idx = 0;
    let mut chars = inner.chars().peekable();

    while chars.peek().is_some() && field_idx < field_slice.len() {
        // Skip whitespace and commas
        while matches!(chars.peek(), Some(' ' | '\t' | '\n' | '\r' | ',')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        // Parse key
        if chars.peek() != Some(&'"') {
            return 1;
        }
        chars.next(); // consume opening quote
        let mut key = String::new();
        loop {
            match chars.next() {
                Some('"') => break,
                Some('\\') => {
                    if let Some(c) = chars.next() {
                        key.push(c);
                    }
                }
                Some(c) => key.push(c),
                None => return 1,
            }
        }

        // Skip whitespace and colon
        while matches!(chars.peek(), Some(' ' | '\t' | '\n' | '\r' | ':')) {
            chars.next();
        }

        // Parse value (simplified: read until comma or closing brace)
        let mut value = String::new();
        let in_string = chars.peek() == Some(&'"');
        if in_string {
            chars.next(); // consume opening quote
            loop {
                match chars.next() {
                    Some('"') => break,
                    Some('\\') => {
                        if let Some(c) = chars.next() {
                            value.push(c);
                        }
                    }
                    Some(c) => value.push(c),
                    None => break,
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c == ',' || c == '}' {
                    break;
                }
                value.push(c);
                chars.next();
            }
            value = value.trim().to_string();
        }

        // Find matching field by name and copy value
        for field in field_slice.iter_mut() {
            if !field.name_ptr.is_null() && field.name_len > 0 {
                let name_bytes =
                    std::slice::from_raw_parts(field.name_ptr, field.name_len as usize);
                if let Ok(name) = std::str::from_utf8(name_bytes) {
                    if name == key {
                        if !field.value_ptr.is_null() && field.value_len > 0 {
                            let dst = std::slice::from_raw_parts_mut(
                                field.value_ptr as *mut u8,
                                field.value_len as usize,
                            );
                            let value_bytes = value.as_bytes();
                            let copy_len = value_bytes.len().min(dst.len());
                            dst[..copy_len].copy_from_slice(&value_bytes[..copy_len]);
                            // Pad remaining with spaces (COBOL convention)
                            for byte in dst.iter_mut().skip(copy_len) {
                                *byte = b' ';
                            }
                        }
                        break;
                    }
                }
            }
        }

        field_idx += 1;
    }

    0
}

/// Escape a string for JSON output.
fn json_escape_string(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// Validate a data item (stub for COBOL 2014 VALIDATE statement).
///
/// Currently a no-op stub. A full implementation would check
/// PICTURE/VALUE constraints.
///
/// # Safety
/// `target_name` must be a valid, null-terminated C string pointer, or null.
#[no_mangle]
pub unsafe extern "C" fn cobol_validate(target_name: *const std::os::raw::c_char) {
    if target_name.is_null() {
        return;
    }
    // Stub: validation always succeeds for now
    let _name = std::ffi::CStr::from_ptr(target_name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_generate_single_field() {
        let name = b"name";
        let value = b"John";

        let field = CobolJsonField {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u32,
            value_ptr: value.as_ptr(),
            value_len: value.len() as u32,
            value_type: 0, // string
        };

        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_json_generate(
                &field as *const CobolJsonField,
                1,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert_eq!(result, r#"{"name":"John"}"#);
    }

    #[test]
    fn test_json_generate_number_field() {
        let name = b"age";
        let value = b"42";

        let field = CobolJsonField {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u32,
            value_ptr: value.as_ptr(),
            value_len: value.len() as u32,
            value_type: 1, // number
        };

        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_json_generate(
                &field as *const CobolJsonField,
                1,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert_eq!(result, r#"{"age":42}"#);
    }

    #[test]
    fn test_json_generate_multiple_fields() {
        let name1 = b"name";
        let value1 = b"John";
        let name2 = b"age";
        let value2 = b"30";

        let fields = [
            CobolJsonField {
                name_ptr: name1.as_ptr(),
                name_len: name1.len() as u32,
                value_ptr: value1.as_ptr(),
                value_len: value1.len() as u32,
                value_type: 0,
            },
            CobolJsonField {
                name_ptr: name2.as_ptr(),
                name_len: name2.len() as u32,
                value_ptr: value2.as_ptr(),
                value_len: value2.len() as u32,
                value_type: 1,
            },
        ];

        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_json_generate(fields.as_ptr(), 2, output.as_mut_ptr(), output.len() as u32)
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert_eq!(result, r#"{"name":"John","age":30}"#);
    }

    #[test]
    fn test_json_generate_null_input() {
        let result = unsafe { cobol_json_generate(std::ptr::null(), 0, std::ptr::null_mut(), 0) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_json_parse_simple() {
        let json = r#"{"name":"Alice","age":"25"}"#;
        let json_bytes = json.as_bytes();

        let name_key = b"name";
        let age_key = b"age";
        let mut name_val = [b' '; 20];
        let mut age_val = [b' '; 10];

        let mut fields = [
            CobolJsonField {
                name_ptr: name_key.as_ptr(),
                name_len: name_key.len() as u32,
                value_ptr: name_val.as_mut_ptr(),
                value_len: name_val.len() as u32,
                value_type: 0,
            },
            CobolJsonField {
                name_ptr: age_key.as_ptr(),
                name_len: age_key.len() as u32,
                value_ptr: age_val.as_mut_ptr(),
                value_len: age_val.len() as u32,
                value_type: 1,
            },
        ];

        let result = unsafe {
            cobol_json_parse(
                json_bytes.as_ptr(),
                json_bytes.len() as u32,
                fields.as_mut_ptr(),
                2,
            )
        };

        assert_eq!(result, 0);
        // Check that "Alice" was written (padded with spaces)
        assert!(name_val.starts_with(b"Alice"));
        // Check that "25" was written
        assert!(age_val.starts_with(b"25"));
    }

    #[test]
    fn test_json_parse_invalid() {
        let json = b"not json";
        let result =
            unsafe { cobol_json_parse(json.as_ptr(), json.len() as u32, std::ptr::null_mut(), 0) };
        assert_eq!(result, 1);
    }

    #[test]
    fn test_json_escape_string() {
        let mut out = String::new();
        json_escape_string(&mut out, "hello \"world\"");
        assert_eq!(out, r#"hello \"world\""#);
    }

    #[test]
    fn test_json_escape_newlines() {
        let mut out = String::new();
        json_escape_string(&mut out, "line1\nline2");
        assert_eq!(out, r#"line1\nline2"#);
    }

    #[test]
    fn test_validate_null() {
        unsafe {
            cobol_validate(std::ptr::null());
        }
        // Should not panic
    }

    #[test]
    fn test_validate_valid_name() {
        let name = std::ffi::CString::new("WS-NAME").unwrap();
        unsafe {
            cobol_validate(name.as_ptr());
        }
        // Should not panic
    }
}
