// COBOL Runtime - XML support (COBOL 2014+)
//
// Provides XML GENERATE and XML PARSE runtime functions for COBOL programs.
// XML GENERATE serializes COBOL group items to XML format.
// XML PARSE provides SAX-style parsing with event callbacks.

/// Describes a single COBOL field for XML serialization.
#[repr(C)]
pub struct CobolXmlField {
    pub name_ptr: *const u8,
    pub name_len: u32,
    pub value_ptr: *const u8,
    pub value_len: u32,
    /// If non-zero, emit as an XML attribute instead of an element.
    pub is_attribute: u8,
}

/// XML event types for SAX-style parsing.
pub const XML_EVENT_START_ELEMENT: u32 = 1;
pub const XML_EVENT_END_ELEMENT: u32 = 2;
pub const XML_EVENT_CONTENT: u32 = 3;
pub const XML_EVENT_ATTRIBUTE: u32 = 4;

/// XML GENERATE -- serialize COBOL group item fields to XML.
///
/// Writes an XML document to `output_ptr` with the given root element name
/// and field descriptors. Returns the number of bytes actually written.
///
/// # Safety
/// - `fields` must point to a valid array of `field_count` `CobolXmlField` items.
/// - `root_name_ptr` must point to a valid, readable region of `root_name_len` bytes.
/// - `output_ptr` must point to a writable region of `output_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_xml_generate(
    fields: *const CobolXmlField,
    field_count: u32,
    root_name_ptr: *const u8,
    root_name_len: u32,
    output_ptr: *mut u8,
    output_len: u32,
) -> u32 {
    if output_ptr.is_null() || output_len == 0 {
        return 0;
    }

    let root_name = if !root_name_ptr.is_null() && root_name_len > 0 {
        let name_bytes = std::slice::from_raw_parts(root_name_ptr, root_name_len as usize);
        std::str::from_utf8(name_bytes).unwrap_or("root")
    } else {
        "root"
    };

    let mut xml = String::with_capacity(output_len as usize);

    // Collect attributes
    let field_slice = if !fields.is_null() && field_count > 0 {
        std::slice::from_raw_parts(fields, field_count as usize)
    } else {
        &[]
    };

    // Start root element
    xml.push('<');
    xml_escape_name(&mut xml, root_name);

    // Emit attributes first
    for field in field_slice {
        if field.is_attribute != 0 {
            let name = read_field_name(field);
            let value = read_field_value(field);
            xml.push(' ');
            xml_escape_name(&mut xml, &name);
            xml.push_str("=\"");
            xml_escape_value(&mut xml, &value);
            xml.push('"');
        }
    }
    xml.push('>');

    // Emit child elements
    for field in field_slice {
        if field.is_attribute == 0 {
            let name = read_field_name(field);
            let value = read_field_value(field);
            xml.push('<');
            xml_escape_name(&mut xml, &name);
            xml.push('>');
            xml_escape_value(&mut xml, &value);
            xml.push_str("</");
            xml_escape_name(&mut xml, &name);
            xml.push('>');
        }
    }

    // Close root element
    xml.push_str("</");
    xml_escape_name(&mut xml, root_name);
    xml.push('>');

    let xml_bytes = xml.as_bytes();
    let copy_len = xml_bytes.len().min(output_len as usize);
    std::ptr::copy_nonoverlapping(xml_bytes.as_ptr(), output_ptr, copy_len);

    copy_len as u32
}

/// XML PARSE -- SAX-style parsing with event callback.
///
/// Parses the XML in `xml_ptr` and calls `callback` for each event.
/// Returns 0 on success, 1 on parse error.
///
/// # Safety
/// - `xml_ptr` must point to a valid, readable region of `xml_len` bytes
///   containing valid UTF-8 XML data.
/// - `callback` must be a valid function pointer.
#[no_mangle]
pub unsafe extern "C" fn cobol_xml_parse(
    xml_ptr: *const u8,
    xml_len: u32,
    callback: extern "C" fn(
        event_type: u32,
        name_ptr: *const u8,
        name_len: u32,
        value_ptr: *const u8,
        value_len: u32,
    ),
) -> u32 {
    if xml_ptr.is_null() || xml_len == 0 {
        return 1;
    }

    let xml_slice = std::slice::from_raw_parts(xml_ptr, xml_len as usize);
    let xml_str = match std::str::from_utf8(xml_slice) {
        Ok(s) => s,
        Err(_) => return 1,
    };

    // Simple XML parser -- handles basic element structure.
    // A production implementation would use a proper XML parser.
    let mut chars = xml_str.chars().peekable();

    while chars.peek().is_some() {
        // Skip whitespace
        while matches!(chars.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            chars.next();
        }

        if chars.peek() == Some(&'<') {
            chars.next(); // consume '<'

            if chars.peek() == Some(&'/') {
                // End element
                chars.next(); // consume '/'
                let name = collect_until(&mut chars, '>');
                if chars.peek() == Some(&'>') {
                    chars.next();
                }
                callback(
                    XML_EVENT_END_ELEMENT,
                    name.as_ptr(),
                    name.len() as u32,
                    std::ptr::null(),
                    0,
                );
            } else if chars.peek() == Some(&'?') {
                // Processing instruction -- skip until ?>
                chars.next(); // consume '?'
                loop {
                    match chars.next() {
                        None => break,
                        Some('?') => {
                            if chars.peek() == Some(&'>') {
                                chars.next();
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            } else if chars.peek() == Some(&'!') {
                // Check for XML comment <!-- ... -->
                chars.next(); // consume '!'
                let is_comment = chars.peek() == Some(&'-');
                if is_comment {
                    chars.next(); // consume first '-'
                    if chars.peek() == Some(&'-') {
                        chars.next(); // consume second '-'
                                      // Now skip until -->
                        loop {
                            match chars.next() {
                                None => break,
                                Some('-') => {
                                    if chars.peek() == Some(&'-') {
                                        chars.next(); // consume second '-'
                                        if chars.peek() == Some(&'>') {
                                            chars.next(); // consume '>'
                                            break;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else {
                        // Not a comment (e.g., <!DOCTYPE>) -- skip to '>'
                        while chars.peek().is_some() && chars.peek() != Some(&'>') {
                            chars.next();
                        }
                        if chars.peek() == Some(&'>') {
                            chars.next();
                        }
                    }
                } else {
                    // Other declaration (e.g., <!DOCTYPE>) -- skip to '>'
                    while chars.peek().is_some() && chars.peek() != Some(&'>') {
                        chars.next();
                    }
                    if chars.peek() == Some(&'>') {
                        chars.next();
                    }
                }
            } else {
                // Start element (possibly with attributes)
                let tag_content = collect_until(&mut chars, '>');
                let self_closing = tag_content.ends_with('/');
                let tag_str = if self_closing {
                    &tag_content[..tag_content.len() - 1]
                } else {
                    &tag_content
                };

                // Split into element name and attributes
                let mut parts = tag_str.splitn(2, |c: char| c.is_whitespace());
                let elem_name = parts.next().unwrap_or("");
                let attrs_str = parts.next().unwrap_or("");

                if chars.peek() == Some(&'>') {
                    chars.next();
                }

                // Emit start element event
                let name_bytes = elem_name.as_bytes();
                callback(
                    XML_EVENT_START_ELEMENT,
                    name_bytes.as_ptr(),
                    name_bytes.len() as u32,
                    std::ptr::null(),
                    0,
                );

                // Parse and emit attribute events
                parse_attributes(attrs_str, callback);

                if self_closing {
                    callback(
                        XML_EVENT_END_ELEMENT,
                        name_bytes.as_ptr(),
                        name_bytes.len() as u32,
                        std::ptr::null(),
                        0,
                    );
                }
            }
        } else {
            // Text content
            let content = collect_until(&mut chars, '<');
            let content_trimmed = content.trim();
            if !content_trimmed.is_empty() {
                let content_bytes = content_trimmed.as_bytes();
                callback(
                    XML_EVENT_CONTENT,
                    std::ptr::null(),
                    0,
                    content_bytes.as_ptr(),
                    content_bytes.len() as u32,
                );
            }
            // Don't consume the '<' -- it will be handled by the next iteration
        }
    }

    0
}

/// Collect characters until a delimiter is found (without consuming the delimiter).
fn collect_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, delimiter: char) -> String {
    let mut result = String::new();
    while let Some(&c) = chars.peek() {
        if c == delimiter {
            break;
        }
        result.push(c);
        chars.next();
    }
    result
}

/// Parse simple XML attributes and emit events.
fn parse_attributes(attrs: &str, callback: extern "C" fn(u32, *const u8, u32, *const u8, u32)) {
    let trimmed = attrs.trim();
    if trimmed.is_empty() {
        return;
    }

    // Very simple attribute parser: name="value" pairs
    let mut chars = trimmed.chars().peekable();
    while chars.peek().is_some() {
        // Skip whitespace
        while matches!(chars.peek(), Some(' ' | '\t')) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }

        // Read attribute name
        let name = collect_until(&mut chars, '=');
        let name = name.trim();
        if chars.peek() == Some(&'=') {
            chars.next();
        }

        // Skip quote
        if matches!(chars.peek(), Some('"' | '\'')) {
            let quote = *chars.peek().unwrap();
            chars.next();
            let value = collect_until(&mut chars, quote);
            if chars.peek() == Some(&quote) {
                chars.next();
            }

            let name_bytes = name.as_bytes();
            let value_bytes = value.as_bytes();
            callback(
                XML_EVENT_ATTRIBUTE,
                name_bytes.as_ptr(),
                name_bytes.len() as u32,
                value_bytes.as_ptr(),
                value_bytes.len() as u32,
            );
        }
    }
}

/// Read a field name from a CobolXmlField.
unsafe fn read_field_name(field: &CobolXmlField) -> String {
    if !field.name_ptr.is_null() && field.name_len > 0 {
        let bytes = std::slice::from_raw_parts(field.name_ptr, field.name_len as usize);
        std::str::from_utf8(bytes)
            .unwrap_or("unknown")
            .trim_end()
            .to_string()
    } else {
        "unknown".to_string()
    }
}

/// Read a field value from a CobolXmlField.
unsafe fn read_field_value(field: &CobolXmlField) -> String {
    if !field.value_ptr.is_null() && field.value_len > 0 {
        let bytes = std::slice::from_raw_parts(field.value_ptr, field.value_len as usize);
        std::str::from_utf8(bytes)
            .unwrap_or("")
            .trim_end()
            .to_string()
    } else {
        String::new()
    }
}

/// Escape special characters in an XML element/attribute name.
fn xml_escape_name(out: &mut String, name: &str) {
    // XML names can't contain spaces or special chars -- just pass through
    // valid characters. Replace hyphens (COBOL convention) with hyphens
    // (valid in XML names).
    for ch in name.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        }
    }
}

/// Escape special characters in an XML value.
fn xml_escape_value(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Thread-local event collector for testing the callback-based XML parser
    static EVENTS: Mutex<Vec<(u32, String, String)>> = Mutex::new(Vec::new());

    extern "C" fn test_callback(
        event_type: u32,
        name_ptr: *const u8,
        name_len: u32,
        value_ptr: *const u8,
        value_len: u32,
    ) {
        let name = if !name_ptr.is_null() && name_len > 0 {
            unsafe {
                let bytes = std::slice::from_raw_parts(name_ptr, name_len as usize);
                std::str::from_utf8(bytes).unwrap_or("").to_string()
            }
        } else {
            String::new()
        };
        let value = if !value_ptr.is_null() && value_len > 0 {
            unsafe {
                let bytes = std::slice::from_raw_parts(value_ptr, value_len as usize);
                std::str::from_utf8(bytes).unwrap_or("").to_string()
            }
        } else {
            String::new()
        };
        EVENTS.lock().unwrap().push((event_type, name, value));
    }

    #[test]
    fn test_xml_generate_simple() {
        let name = b"name";
        let value = b"John";

        let field = CobolXmlField {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u32,
            value_ptr: value.as_ptr(),
            value_len: value.len() as u32,
            is_attribute: 0,
        };

        let root = b"person";
        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_xml_generate(
                &field as *const CobolXmlField,
                1,
                root.as_ptr(),
                root.len() as u32,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert_eq!(result, "<person><name>John</name></person>");
    }

    #[test]
    fn test_xml_generate_with_attribute() {
        let attr_name = b"id";
        let attr_value = b"42";
        let elem_name = b"name";
        let elem_value = b"John";

        let fields = [
            CobolXmlField {
                name_ptr: attr_name.as_ptr(),
                name_len: attr_name.len() as u32,
                value_ptr: attr_value.as_ptr(),
                value_len: attr_value.len() as u32,
                is_attribute: 1,
            },
            CobolXmlField {
                name_ptr: elem_name.as_ptr(),
                name_len: elem_name.len() as u32,
                value_ptr: elem_value.as_ptr(),
                value_len: elem_value.len() as u32,
                is_attribute: 0,
            },
        ];

        let root = b"person";
        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_xml_generate(
                fields.as_ptr(),
                2,
                root.as_ptr(),
                root.len() as u32,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert_eq!(result, r#"<person id="42"><name>John</name></person>"#);
    }

    #[test]
    fn test_xml_generate_escape_values() {
        let name = b"msg";
        let value = b"a<b&c";

        let field = CobolXmlField {
            name_ptr: name.as_ptr(),
            name_len: name.len() as u32,
            value_ptr: value.as_ptr(),
            value_len: value.len() as u32,
            is_attribute: 0,
        };

        let root = b"data";
        let mut output = [0u8; 256];
        let written = unsafe {
            cobol_xml_generate(
                &field as *const CobolXmlField,
                1,
                root.as_ptr(),
                root.len() as u32,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        };

        let result = std::str::from_utf8(&output[..written as usize]).unwrap();
        assert!(result.contains("&lt;"));
        assert!(result.contains("&amp;"));
    }

    #[test]
    fn test_xml_generate_null_output() {
        let result = unsafe {
            cobol_xml_generate(
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
            )
        };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_xml_parse_simple() {
        {
            EVENTS.lock().unwrap().clear();
        }

        let xml = b"<root><name>John</name></root>";
        let result = unsafe { cobol_xml_parse(xml.as_ptr(), xml.len() as u32, test_callback) };

        assert_eq!(result, 0);
        let events = EVENTS.lock().unwrap();
        // Should have: start(root), start(name), content(John), end(name), end(root)
        assert!(events.len() >= 3);
        // First event should be start element "root"
        assert_eq!(events[0].0, XML_EVENT_START_ELEMENT);
        assert_eq!(events[0].1, "root");
    }

    #[test]
    fn test_xml_parse_null() {
        let result = unsafe { cobol_xml_parse(std::ptr::null(), 0, test_callback) };
        assert_eq!(result, 1);
    }

    #[test]
    fn test_xml_escape_value() {
        let mut out = String::new();
        xml_escape_value(&mut out, "a<b>c&d\"e");
        assert_eq!(out, "a&lt;b&gt;c&amp;d&quot;e");
    }

    #[test]
    fn test_xml_escape_name() {
        let mut out = String::new();
        xml_escape_name(&mut out, "WS-NAME");
        assert_eq!(out, "WS-NAME");
    }
}
