// COBOL Runtime - String operations
//
// Implements the COBOL STRING, UNSTRING, INSPECT, and MOVE statements.
// COBOL strings are fixed-length byte arrays, left-justified and
// space-padded (for alphanumeric fields) or right-justified and
// zero-padded (for numeric display fields).
//
// All public functions use the C ABI for linking with generated code.

/// Descriptor for a source operand in a STRING statement.
#[repr(C)]
pub struct CobolStringSource {
    /// Pointer to source bytes.
    pub ptr: *const u8,
    /// Length of the source field.
    pub len: u32,
    /// Pointer to delimiter bytes (null if no DELIMITED BY).
    pub delim_ptr: *const u8,
    /// Length of the delimiter (0 if no delimiter / DELIMITED BY SIZE).
    pub delim_len: u32,
}

/// Descriptor for a target operand in an UNSTRING statement.
#[repr(C)]
pub struct CobolUnstringTarget {
    /// Pointer to the target field buffer.
    pub ptr: *mut u8,
    /// Length of the target field.
    pub len: u32,
    /// Pointer to the delimiter buffer (receives the matched delimiter).
    /// May be null if not requested.
    pub delimiter_ptr: *mut u8,
    /// Length of the delimiter buffer.
    pub delimiter_len: u32,
    /// Pointer to the count field (receives the number of characters moved).
    /// May be null if not requested.
    pub count_ptr: *mut u32,
    /// 0 = alphanumeric left, 1 = alphanumeric JUSTIFIED RIGHT,
    /// 2 = native integer numeric.
    pub kind: u32,
}

// ---------------------------------------------------------------------------
// Alphanumeric comparison
// ---------------------------------------------------------------------------

/// Compare two alphanumeric fields with COBOL semantics.
///
/// The shorter field is logically padded with spaces on the right.
/// Returns negative if a < b, 0 if a == b, positive if a > b.
///
/// # Safety
/// `a_ptr` must be readable for `a_len` bytes.
/// `b_ptr` must be readable for `b_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_compare_alphanumeric(
    a_ptr: *const u8,
    a_len: u32,
    b_ptr: *const u8,
    b_len: u32,
) -> i32 {
    if a_ptr.is_null() || b_ptr.is_null() {
        return 0;
    }
    let a = std::slice::from_raw_parts(a_ptr, a_len as usize);
    let b = std::slice::from_raw_parts(b_ptr, b_len as usize);
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let ca = if i < a.len() { a[i] } else { b' ' };
        let cb = if i < b.len() { b[i] } else { b' ' };
        if ca < cb {
            return -1;
        }
        if ca > cb {
            return 1;
        }
    }
    0
}

/// Compare two alphanumeric operands using a 256-entry collating weight table.
///
/// # Safety
/// `a_ptr`, `b_ptr`, and `weights` must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_compare_alphanumeric_collated(
    a_ptr: *const u8,
    a_len: u32,
    b_ptr: *const u8,
    b_len: u32,
    weights: *const u16,
) -> i32 {
    if a_ptr.is_null() || b_ptr.is_null() || weights.is_null() {
        return 0;
    }
    let a = std::slice::from_raw_parts(a_ptr, a_len as usize);
    let b = std::slice::from_raw_parts(b_ptr, b_len as usize);
    let weights = std::slice::from_raw_parts(weights, 256);
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let ca = if i < a.len() { a[i] } else { b' ' };
        let cb = if i < b.len() { b[i] } else { b' ' };
        let wa = weights[ca as usize];
        let wb = weights[cb as usize];
        if wa < wb {
            return -1;
        }
        if wa > wb {
            return 1;
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Class condition checks
// ---------------------------------------------------------------------------

/// Check if the given alphanumeric field is NUMERIC (all bytes are digits,
/// optionally with leading/trailing sign and a decimal point).
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_numeric(ptr: *const u8, len: u32) -> i32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    let mut has_digit = false;
    let mut has_decimal = false;
    for (i, &b) in data.iter().enumerate() {
        match b {
            b'0'..=b'9' => has_digit = true,
            b'+' | b'-' => {
                if i != 0 && i != data.len() - 1 {
                    return 0; // sign only allowed at start or end
                }
            }
            b'.' => {
                if has_decimal {
                    return 0; // only one decimal point
                }
                has_decimal = true;
            }
            b' ' => {} // trailing/leading spaces allowed
            _ => return 0,
        }
    }
    if has_digit {
        1
    } else {
        0
    }
}

/// Check whether an alphanumeric field satisfies the NUMERIC class condition.
///
/// For nonnumeric character data, COBOL's NUMERIC class requires every
/// character position to be a decimal digit; signs, decimal points, and spaces
/// are not numeric characters.
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_numeric_strict(ptr: *const u8, len: u32) -> i32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    if data.iter().all(|b| b.is_ascii_digit()) {
        1
    } else {
        0
    }
}

/// Check if the given field is ALPHABETIC (all bytes are A-Z, a-z, or space).
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_alphabetic(ptr: *const u8, len: u32) -> i32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    for &b in data {
        if !b.is_ascii_alphabetic() && b != b' ' {
            return 0;
        }
    }
    1
}

/// Check if the given field is ALPHABETIC-LOWER (all bytes a-z or space).
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_alphabetic_lower(ptr: *const u8, len: u32) -> i32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    for &b in data {
        if !b.is_ascii_lowercase() && b != b' ' {
            return 0;
        }
    }
    1
}

/// Check if the given field is ALPHABETIC-UPPER (all bytes A-Z or space).
///
/// # Safety
/// `ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_alphabetic_upper(ptr: *const u8, len: u32) -> i32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    for &b in data {
        if !b.is_ascii_uppercase() && b != b' ' {
            return 0;
        }
    }
    1
}

/// Check if every byte belongs to one of the inclusive byte ranges.
///
/// # Safety
/// `ptr` must be readable for `len` bytes. `ranges` must be readable for
/// `ranges_len` bytes and contain two bytes per range.
#[no_mangle]
pub unsafe extern "C" fn cobol_is_custom_class(
    ptr: *const u8,
    len: u32,
    ranges: *const u8,
    ranges_len: u32,
) -> i32 {
    if ptr.is_null() || ranges.is_null() || len == 0 || ranges_len < 2 {
        return 0;
    }
    let data = std::slice::from_raw_parts(ptr, len as usize);
    let range_data = std::slice::from_raw_parts(ranges, ranges_len as usize);
    for &b in data {
        let mut matched = false;
        for pair in range_data.chunks_exact(2) {
            let from = pair[0];
            let to = pair[1];
            let (low, high) = if from <= to { (from, to) } else { (to, from) };
            if b >= low && b <= high {
                matched = true;
                break;
            }
        }
        if !matched {
            return 0;
        }
    }
    1
}

// ---------------------------------------------------------------------------
// MOVE operations
// ---------------------------------------------------------------------------

/// MOVE string with COBOL semantics.
///
/// Alphanumeric MOVE: the source is left-justified in the destination,
/// padded with spaces on the right (or truncated on the right).
///
/// # Safety
/// `src_ptr` must be readable for `src_len` bytes.
/// `dst_ptr` must be writable for `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_string(
    src_ptr: *const u8,
    src_len: u32,
    dst_ptr: *mut u8,
    dst_len: u32,
) {
    if src_ptr.is_null() || dst_ptr.is_null() {
        return;
    }
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);

    let copy_len = src.len().min(dst.len());
    dst[..copy_len].copy_from_slice(&src[..copy_len]);

    // Pad remainder with spaces.
    for b in dst[copy_len..].iter_mut() {
        *b = b' ';
    }
}

/// Move an alphanumeric value with JUSTIFIED RIGHT: right-justify and
/// pad with spaces on the left.
///
/// # Safety
/// Both pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_string_right(
    src_ptr: *const u8,
    src_len: u32,
    dst_ptr: *mut u8,
    dst_len: u32,
) {
    if src_ptr.is_null() || dst_ptr.is_null() {
        return;
    }
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);

    let copy_len = src.len().min(dst.len());
    // Right-justify: pad left with spaces, copy data to the right end.
    let pad_len = dst.len() - copy_len;
    for b in dst[..pad_len].iter_mut() {
        *b = b' ';
    }
    // Copy source to the right portion of destination.
    // If src is longer than dst, take the RIGHTMOST characters of src.
    let src_start = src.len().saturating_sub(dst.len());
    dst[pad_len..].copy_from_slice(&src[src_start..src_start + copy_len]);
}

/// MOVE to an alphanumeric-edited PICTURE, applying insertion symbols.
///
/// # Safety
/// Pointers must be readable/writable for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_alphanumeric_edited(
    src_ptr: *const u8,
    src_len: u32,
    dst_ptr: *mut u8,
    dst_len: u32,
    pic_ptr: *const u8,
    pic_len: u32,
) {
    if src_ptr.is_null() || dst_ptr.is_null() || pic_ptr.is_null() {
        return;
    }

    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);
    let pic = std::slice::from_raw_parts(pic_ptr, pic_len as usize);

    dst.fill(b' ');

    let mut src_idx = 0usize;
    let mut dst_idx = 0usize;
    let mut pic_idx = 0usize;
    while pic_idx < pic.len() && dst_idx < dst.len() {
        let symbol = pic[pic_idx].to_ascii_uppercase();
        let repeat = picture_repeat_count(pic, &mut pic_idx);
        for _ in 0..repeat {
            if dst_idx >= dst.len() {
                break;
            }
            dst[dst_idx] = match symbol {
                b'A' | b'X' | b'9' => {
                    let value = src.get(src_idx).copied().unwrap_or(b' ');
                    src_idx += 1;
                    value
                }
                b'B' => b' ',
                b'0' => b'0',
                b'/' | b',' | b'.' => symbol,
                _ => symbol,
            };
            dst_idx += 1;
        }
        pic_idx += 1;
    }
}

fn picture_repeat_count(pic: &[u8], idx: &mut usize) -> u32 {
    if *idx + 1 >= pic.len() || pic[*idx + 1] != b'(' {
        return 1;
    }
    let mut end = *idx + 2;
    while end < pic.len() && pic[end] != b')' {
        end += 1;
    }
    if end >= pic.len() {
        return 1;
    }
    let count = std::str::from_utf8(&pic[*idx + 2..end])
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(1);
    *idx = end;
    count
}

/// MOVE numeric to alphanumeric display.
///
/// The numeric value is right-justified in the destination buffer with
/// leading spaces. A negative sign is placed immediately before the
/// first digit.
///
/// # Safety
/// `dst_ptr` must be writable for `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_move_numeric_to_display(
    value: i64,
    scale: i32,
    dst_ptr: *mut u8,
    dst_len: u32,
) {
    if dst_ptr.is_null() || dst_len == 0 {
        return;
    }
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);

    // Format the numeric value with the decimal point.
    let formatted = if scale > 0 {
        let abs = value.unsigned_abs();
        let factor = 10u64.pow(scale as u32);
        let int_part = abs / factor;
        let frac_part = abs % factor;
        let num_str = format!(
            "{}.{:0>width$}",
            int_part,
            frac_part,
            width = scale as usize
        );
        if value < 0 {
            format!("-{}", num_str)
        } else {
            num_str
        }
    } else {
        format!("{}", value)
    };

    let bytes = formatted.as_bytes();
    // Right-justify in the destination.
    if bytes.len() >= dst.len() {
        // Truncate from the left (show rightmost digits).
        let offset = bytes.len() - dst.len();
        dst.copy_from_slice(&bytes[offset..]);
    } else {
        let pad = dst.len() - bytes.len();
        for b in dst[..pad].iter_mut() {
            *b = b' ';
        }
        dst[pad..].copy_from_slice(bytes);
    }
}

/// Store an integer value as zoned decimal (zero-padded ASCII digits) in a
/// display numeric buffer within a group struct. PIC 99 with value 7 → "07".
/// Negative values use a trailing overpunch sign in the final digit.
///
/// # Safety
/// `dst_ptr` must point to a valid buffer of at least `dst_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_store_numeric_display(value: i64, dst_ptr: *mut u8, dst_len: u32) {
    if dst_ptr.is_null() || dst_len == 0 {
        return;
    }
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);
    dst.fill(b'0');
    let mut n = value.unsigned_abs();
    for slot in dst.iter_mut().rev() {
        *slot = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if value < 0 {
        if let Some(last) = dst.last_mut() {
            *last = match *last {
                b'0' => b'}',
                b'1' => b'J',
                b'2' => b'K',
                b'3' => b'L',
                b'4' => b'M',
                b'5' => b'N',
                b'6' => b'O',
                b'7' => b'P',
                b'8' => b'Q',
                b'9' => b'R',
                other => other,
            };
        }
    }
}

/// Store DISPLAY numeric with SIGN IS SEPARATE CHARACTER.
///
/// `position` is 0 for leading sign, 1 for trailing sign.
#[no_mangle]
pub unsafe extern "C" fn cobol_store_numeric_display_separate_sign(
    value: i64,
    dst_ptr: *mut u8,
    dst_len: u32,
    position: u32,
) {
    if dst_ptr.is_null() || dst_len == 0 {
        return;
    }
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);
    dst.fill(b'0');
    let sign = if value < 0 { b'-' } else { b'+' };
    let digit_len = dst.len().saturating_sub(1);
    let digits = if position == 0 {
        dst[0] = sign;
        &mut dst[1..]
    } else {
        let last = dst.len() - 1;
        dst[last] = sign;
        &mut dst[..last]
    };
    let mut n = value.unsigned_abs();
    for slot in digits.iter_mut().take(digit_len).rev() {
        *slot = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
}

/// Store DISPLAY numeric with SIGN IS LEADING, using embedded overpunch.
#[no_mangle]
pub unsafe extern "C" fn cobol_store_numeric_display_leading_sign(
    value: i64,
    dst_ptr: *mut u8,
    dst_len: u32,
) {
    if dst_ptr.is_null() || dst_len == 0 {
        return;
    }
    cobol_store_numeric_display(value.unsigned_abs() as i64, dst_ptr, dst_len);
    if value < 0 {
        let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);
        if let Some(first) = dst.first_mut() {
            *first = match *first {
                b'0' => b'}',
                b'1' => b'J',
                b'2' => b'K',
                b'3' => b'L',
                b'4' => b'M',
                b'5' => b'N',
                b'6' => b'O',
                b'7' => b'P',
                b'8' => b'Q',
                b'9' => b'R',
                other => other,
            };
        }
    }
}

/// Read a zoned decimal (ASCII digit) buffer and return its int64_t value.
/// Handles leading/trailing spaces and sign characters.
///
/// # Safety
/// `src_ptr` must point to a valid buffer of at least `src_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_display_to_int64(src_ptr: *const u8, src_len: u32) -> i64 {
    if src_ptr.is_null() || src_len == 0 {
        return 0;
    }
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let mut start = 0usize;
    let mut end = src.len();
    while start < end && src[start] == b' ' {
        start += 1;
    }
    while start < end && src[end - 1] == b' ' {
        end -= 1;
    }
    if start == end {
        return 0;
    }
    let mut negative = false;
    if src[start] == b'+' || src[start] == b'-' {
        negative = src[start] == b'-';
        start += 1;
    }
    if start < end && (src[end - 1] == b'+' || src[end - 1] == b'-') {
        negative = src[end - 1] == b'-';
        end -= 1;
    }
    let mut value = 0i64;
    let mut saw_digit = false;
    for (idx, &b) in src[start..end].iter().enumerate() {
        let is_first = idx == 0;
        let is_last = idx + 1 == end - start;
        let digit = if b.is_ascii_digit() {
            b - b'0'
        } else if is_first || is_last {
            match b {
                b'{' => {
                    negative = false;
                    0
                }
                b'A'..=b'I' => {
                    negative = false;
                    b - b'A' + 1
                }
                b'}' => {
                    negative = true;
                    0
                }
                b'J'..=b'R' => {
                    negative = true;
                    b - b'J' + 1
                }
                _ => return 0,
            }
        } else {
            return 0;
        };
        if b.is_ascii_digit() || is_first || is_last {
            saw_digit = true;
            value = value.saturating_mul(10).saturating_add(digit as i64);
        }
    }
    if !saw_digit {
        0
    } else if negative {
        -value
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// STRING statement
// ---------------------------------------------------------------------------

/// STRING statement -- concatenate multiple source strings into a
/// destination buffer, honoring delimiters and a POINTER variable.
///
/// `pointer` is a 1-based index into the destination. It is updated to
/// reflect the next available position after the operation.
///
/// Returns 0 on success, 1 if an overflow occurred (destination full).
///
/// # Safety
/// All pointers must be valid. `sources` must point to an array of
/// `source_count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_string_concat(
    sources: *const CobolStringSource,
    source_count: u32,
    dst_ptr: *mut u8,
    dst_len: u32,
    pointer: *mut u32,
) -> i32 {
    if sources.is_null() || dst_ptr.is_null() || pointer.is_null() {
        return 1;
    }
    let srcs = std::slice::from_raw_parts(sources, source_count as usize);
    let dst = std::slice::from_raw_parts_mut(dst_ptr, dst_len as usize);
    let ptr_val = &mut *pointer;

    // COBOL POINTER is 1-based.
    let mut pos = (*ptr_val as usize).saturating_sub(1);
    let mut overflow = false;

    for src in srcs {
        let src_data = std::slice::from_raw_parts(src.ptr, src.len as usize);

        // Determine the effective data to copy (up to delimiter).
        let effective = if !src.delim_ptr.is_null() && src.delim_len > 0 {
            let delim = std::slice::from_raw_parts(src.delim_ptr, src.delim_len as usize);
            find_delimiter(src_data, delim).unwrap_or(src_data.len())
        } else {
            src_data.len()
        };

        for &byte in src_data.iter().take(effective) {
            if pos >= dst.len() {
                overflow = true;
                break;
            }
            dst[pos] = byte;
            pos += 1;
        }

        if overflow {
            break;
        }
    }

    *ptr_val = (pos + 1) as u32; // back to 1-based
    if overflow {
        1
    } else {
        0
    }
}

/// Find the starting position of `delim` within `data`.
fn find_delimiter(data: &[u8], delim: &[u8]) -> Option<usize> {
    if delim.is_empty() || delim.len() > data.len() {
        return None;
    }
    data.windows(delim.len()).position(|w| w == delim)
}

fn find_unstring_delimiter<'a>(
    data: &[u8],
    single_delim: Option<&'a [u8]>,
    delimiter_sources: &'a [CobolStringSource],
) -> Option<(usize, &'a [u8])> {
    let mut best: Option<(usize, &'a [u8])> = None;
    if let Some(delim) = single_delim {
        if let Some(idx) = find_delimiter(data, delim) {
            best = Some((idx, delim));
        }
    }
    for source in delimiter_sources {
        if source.ptr.is_null() || source.len == 0 {
            continue;
        }
        let delim = unsafe { std::slice::from_raw_parts(source.ptr, source.len as usize) };
        let Some(idx) = find_delimiter(data, delim) else {
            continue;
        };
        if best.is_none_or(|(best_idx, _)| idx < best_idx) {
            best = Some((idx, delim));
        }
    }
    best
}

// ---------------------------------------------------------------------------
// UNSTRING statement
// ---------------------------------------------------------------------------

/// UNSTRING statement -- split a source string into multiple targets
/// using a delimiter.
///
/// `pointer` is a 1-based index into the source. It is updated to
/// reflect the next position after the last character examined.
///
/// `tallying` is incremented by the number of targets that received data.
///
/// Returns 0 on success, 1 if the pointer was out of range.
///
/// # Safety
/// All pointers must be valid. `targets` must point to an array of
/// `target_count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_unstring(
    src_ptr: *const u8,
    src_len: u32,
    delim_ptr: *const u8,
    delim_len: u32,
    targets: *mut CobolUnstringTarget,
    target_count: u32,
    pointer: *mut u32,
    tallying: *mut u32,
    collapse_all: u32,
    delimiter_sources: *const CobolStringSource,
    delimiter_count: u32,
) -> i32 {
    if src_ptr.is_null() || targets.is_null() {
        return 1;
    }
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);
    let single_delim = if !delim_ptr.is_null() && delim_len > 0 {
        Some(std::slice::from_raw_parts(delim_ptr, delim_len as usize))
    } else {
        None
    };
    let delimiter_sources = if !delimiter_sources.is_null() && delimiter_count > 0 {
        std::slice::from_raw_parts(delimiter_sources, delimiter_count as usize)
    } else {
        &[]
    };
    let tgts = std::slice::from_raw_parts_mut(targets, target_count as usize);

    // POINTER is 1-based.
    let start = if !pointer.is_null() {
        ((*pointer) as usize).saturating_sub(1)
    } else {
        0
    };

    if start >= src.len() {
        return 1;
    }

    let mut pos = start;
    let mut tally_count = 0u32;
    let mut overflow = false;
    let has_delimiter = single_delim.is_some() || !delimiter_sources.is_empty();

    for tgt in tgts.iter_mut() {
        if pos >= src.len() {
            break;
        }

        // Find the next delimiter.
        let remaining = &src[pos..];
        let matched_delim = find_unstring_delimiter(remaining, single_delim, delimiter_sources);
        let field_end = if let Some((idx, _)) = matched_delim {
            idx
        } else if has_delimiter {
            remaining.len()
        } else {
            (tgt.len as usize).min(remaining.len())
        };

        let field = &remaining[..field_end];

        // Move field data into the target according to the receiving category.
        let tgt_buf = std::slice::from_raw_parts_mut(tgt.ptr, tgt.len as usize);
        let copy_len = field.len().min(tgt_buf.len());
        match tgt.kind {
            1 => {
                for b in tgt_buf.iter_mut() {
                    *b = b' ';
                }
                let src_start = field.len().saturating_sub(copy_len);
                let dst_start = tgt_buf.len().saturating_sub(copy_len);
                tgt_buf[dst_start..].copy_from_slice(&field[src_start..]);
            }
            2 => {
                let src_start = field.len().saturating_sub(copy_len);
                let value = cobol_display_to_int64(field[src_start..].as_ptr(), copy_len as u32);
                *(tgt.ptr as *mut i64) = value;
            }
            _ => {
                tgt_buf[..copy_len].copy_from_slice(&field[..copy_len]);
                for b in tgt_buf[copy_len..].iter_mut() {
                    *b = b' ';
                }
            }
        }

        // Set the delimiter if requested.
        if !tgt.delimiter_ptr.is_null() && tgt.delimiter_len > 0 {
            let delim_buf =
                std::slice::from_raw_parts_mut(tgt.delimiter_ptr, tgt.delimiter_len as usize);
            if let Some((_, matched)) = matched_delim {
                let d_copy = matched.len().min(delim_buf.len());
                delim_buf[..d_copy].copy_from_slice(&matched[..d_copy]);
                for b in delim_buf[d_copy..].iter_mut() {
                    *b = b' ';
                }
            } else {
                for b in delim_buf.iter_mut() {
                    *b = b' ';
                }
            }
        }

        // Set the count if requested.
        if !tgt.count_ptr.is_null() {
            *tgt.count_ptr = field.len() as u32;
        }

        if field.len() > tgt_buf.len() {
            overflow = true;
        }

        tally_count += 1;

        // Advance past the field and the delimiter.
        pos += field_end;
        if let Some((_, matched)) = matched_delim {
            pos += matched.len();
            if collapse_all != 0 && !matched.is_empty() {
                while pos + matched.len() <= src.len() && &src[pos..pos + matched.len()] == matched
                {
                    pos += matched.len();
                }
            }
        }
    }

    if pos < src.len() {
        overflow = true;
    }

    if !pointer.is_null() {
        *pointer = (pos + 1) as u32; // 1-based
    }
    if !tallying.is_null() {
        *tallying += tally_count;
    }

    if overflow {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// INSPECT statement
// ---------------------------------------------------------------------------

/// INSPECT TALLYING -- count occurrences.
///
/// Modes: 0 = CHARACTERS (count all bytes), 1 = ALL, 2 = LEADING.
///
/// # Safety
/// Pointers must be valid for their respective lengths.
#[no_mangle]
pub unsafe extern "C" fn cobol_inspect_tallying(
    src_ptr: *const u8,
    src_len: u32,
    search_ptr: *const u8,
    search_len: u32,
    mode: u32,
) -> u32 {
    if src_ptr.is_null() {
        return 0;
    }
    let src = std::slice::from_raw_parts(src_ptr, src_len as usize);

    if mode == 0 {
        // CHARACTERS -- return the length of the source.
        return src_len;
    }

    if search_ptr.is_null() || search_len == 0 {
        return 0;
    }
    let search = std::slice::from_raw_parts(search_ptr, search_len as usize);
    if search.is_empty() {
        return 0;
    }

    let mut count = 0u32;
    let mut i = 0usize;

    while i + search.len() <= src.len() {
        if &src[i..i + search.len()] == search {
            count += 1;
            i += search.len();
            if mode == 2 {
                // LEADING -- stop at first non-match position.
                continue;
            }
        } else {
            if mode == 2 {
                // LEADING -- first non-match ends the tally.
                break;
            }
            i += 1;
        }
    }

    count
}

/// INSPECT REPLACING -- replace occurrences in-place.
///
/// Modes: 0 = CHARACTERS, 1 = ALL, 2 = LEADING, 3 = FIRST.
///
/// # Safety
/// `src_ptr` must be writable for `src_len` bytes. Search and replace
/// must have the same length (or replace_len >= search_len for CHARACTERS).
#[no_mangle]
pub unsafe extern "C" fn cobol_inspect_replacing(
    src_ptr: *mut u8,
    src_len: u32,
    search_ptr: *const u8,
    search_len: u32,
    replace_ptr: *const u8,
    replace_len: u32,
    mode: u32,
) {
    if src_ptr.is_null() || src_len == 0 {
        return;
    }
    let src = std::slice::from_raw_parts_mut(src_ptr, src_len as usize);

    if mode == 0 {
        // CHARACTERS -- replace every character with the first byte of
        // the replacement string.
        if replace_ptr.is_null() || replace_len == 0 {
            return;
        }
        let rep_byte = *replace_ptr;
        for b in src.iter_mut() {
            *b = rep_byte;
        }
        return;
    }

    if search_ptr.is_null() || replace_ptr.is_null() {
        return;
    }
    let search = std::slice::from_raw_parts(search_ptr, search_len as usize);
    let replace = std::slice::from_raw_parts(replace_ptr, replace_len as usize);
    if search.is_empty() {
        return;
    }

    let rep_len = search.len().min(replace.len());
    let mut i = 0usize;

    while i + search.len() <= src.len() {
        if &src[i..i + search.len()] == search {
            src[i..i + rep_len].copy_from_slice(&replace[..rep_len]);
            i += search.len();
            match mode {
                3 => return,   // FIRST -- only replace the first occurrence.
                2 => continue, // LEADING -- continue while matching.
                _ => {}        // ALL -- keep going.
            }
        } else {
            if mode == 2 {
                // LEADING -- stop at first non-match.
                return;
            }
            i += 1;
        }
    }
}

/// INSPECT CONVERTING -- translate characters.
///
/// Each character in the FROM string is replaced with the corresponding
/// character in the TO string (positional 1:1 mapping, like `tr`).
///
/// # Safety
/// `src_ptr` must be writable for `src_len` bytes.
/// `from_ptr` and `to_ptr` must be readable for their respective lengths.
/// `from_len` and `to_len` should be equal.
#[no_mangle]
pub unsafe extern "C" fn cobol_inspect_converting(
    src_ptr: *mut u8,
    src_len: u32,
    from_ptr: *const u8,
    from_len: u32,
    to_ptr: *const u8,
    to_len: u32,
) {
    if src_ptr.is_null() || from_ptr.is_null() || to_ptr.is_null() {
        return;
    }
    let src = std::slice::from_raw_parts_mut(src_ptr, src_len as usize);
    let from = std::slice::from_raw_parts(from_ptr, from_len as usize);
    let to = std::slice::from_raw_parts(to_ptr, to_len as usize);

    // Build a translation table for single-byte mapping.
    let mut table = [0u8; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        *entry = i as u8;
    }
    let map_len = from.len().min(to.len());
    for (&f, &t) in from.iter().zip(to.iter()).take(map_len) {
        table[f as usize] = t;
    }

    for b in src.iter_mut() {
        *b = table[*b as usize];
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_alphanumeric_equal() {
        let a = b"HELLO";
        let b = b"HELLO";
        let r = unsafe { cobol_compare_alphanumeric(a.as_ptr(), 5, b.as_ptr(), 5) };
        assert_eq!(r, 0);
    }

    #[test]
    fn test_compare_alphanumeric_less() {
        let a = b"APPLE";
        let b = b"BANANA";
        let r = unsafe { cobol_compare_alphanumeric(a.as_ptr(), 5, b.as_ptr(), 6) };
        assert!(r < 0);
    }

    #[test]
    fn test_compare_alphanumeric_space_padding() {
        // "HELLO" (5 bytes) should equal "HELLO     " (10 bytes with trailing spaces)
        let a = b"HELLO";
        let b = b"HELLO     ";
        let r = unsafe { cobol_compare_alphanumeric(a.as_ptr(), 5, b.as_ptr(), 10) };
        assert_eq!(r, 0);
    }

    #[test]
    fn test_compare_alphanumeric_shorter_with_padding() {
        // "AB" padded with spaces should be less than "ABC"
        let a = b"AB";
        let b = b"ABC";
        let r = unsafe { cobol_compare_alphanumeric(a.as_ptr(), 2, b.as_ptr(), 3) };
        assert!(r < 0, "AB (space-padded) should be less than ABC");
    }

    #[test]
    fn test_store_numeric_display_zero_pads_without_allocating() {
        let mut buf = [b' '; 4];
        unsafe {
            cobol_store_numeric_display(7, buf.as_mut_ptr(), buf.len() as u32);
        }
        assert_eq!(&buf, b"0007");
    }

    #[test]
    fn test_store_numeric_display_negative_overpunch() {
        let mut buf = [b' '; 4];
        unsafe {
            cobol_store_numeric_display(-123, buf.as_mut_ptr(), buf.len() as u32);
            assert_eq!(&buf, b"012L");
            assert_eq!(cobol_display_to_int64(buf.as_ptr(), buf.len() as u32), -123);
        }
    }

    #[test]
    fn test_store_numeric_display_leading_sign_negative_overpunch() {
        let mut buf = [b' '; 4];
        unsafe {
            cobol_store_numeric_display_leading_sign(-9127, buf.as_mut_ptr(), buf.len() as u32);
            assert_eq!(&buf, b"R127");
            assert_eq!(
                cobol_display_to_int64(buf.as_ptr(), buf.len() as u32),
                -9127
            );
        }
    }

    #[test]
    fn test_store_numeric_display_separate_sign_positions() {
        let mut leading = [b' '; 5];
        let mut trailing = [b' '; 5];
        unsafe {
            cobol_store_numeric_display_separate_sign(1234, leading.as_mut_ptr(), 5, 0);
            cobol_store_numeric_display_separate_sign(-1234, trailing.as_mut_ptr(), 5, 1);
        }
        assert_eq!(&leading, b"+1234");
        assert_eq!(&trailing, b"1234-");
    }

    #[test]
    fn test_display_to_int64_handles_spaces_and_signs() {
        unsafe {
            assert_eq!(cobol_display_to_int64(b"  +123".as_ptr(), 6), 123);
            assert_eq!(cobol_display_to_int64(b"456- ".as_ptr(), 5), -456);
            assert_eq!(cobol_display_to_int64(b"12L".as_ptr(), 3), -123);
            assert_eq!(cobol_display_to_int64(b"R127".as_ptr(), 4), -9127);
            assert_eq!(cobol_display_to_int64(b"   ".as_ptr(), 3), 0);
        }
    }

    #[test]
    fn test_is_numeric_digits() {
        let data = b"12345";
        assert_eq!(unsafe { cobol_is_numeric(data.as_ptr(), 5) }, 1);
    }

    #[test]
    fn test_is_numeric_with_sign() {
        let data = b"+123";
        assert_eq!(unsafe { cobol_is_numeric(data.as_ptr(), 4) }, 1);
    }

    #[test]
    fn test_is_numeric_strict_rejects_sign_and_spaces() {
        let signed = b"+123";
        let spaced = b"123  ";
        let digits = b"12345";
        assert_eq!(unsafe { cobol_is_numeric_strict(signed.as_ptr(), 4) }, 0);
        assert_eq!(unsafe { cobol_is_numeric_strict(spaced.as_ptr(), 5) }, 0);
        assert_eq!(unsafe { cobol_is_numeric_strict(digits.as_ptr(), 5) }, 1);
    }

    #[test]
    fn test_is_numeric_alpha() {
        let data = b"HELLO";
        assert_eq!(unsafe { cobol_is_numeric(data.as_ptr(), 5) }, 0);
    }

    #[test]
    fn test_is_alphabetic() {
        let data = b"HELLO WORLD";
        assert_eq!(unsafe { cobol_is_alphabetic(data.as_ptr(), 11) }, 1);
    }

    #[test]
    fn test_is_alphabetic_with_digit() {
        let data = b"HELLO1";
        assert_eq!(unsafe { cobol_is_alphabetic(data.as_ptr(), 6) }, 0);
    }

    #[test]
    fn test_is_alphabetic_lower() {
        let data = b"hello";
        assert_eq!(unsafe { cobol_is_alphabetic_lower(data.as_ptr(), 5) }, 1);
        let upper = b"HELLO";
        assert_eq!(unsafe { cobol_is_alphabetic_lower(upper.as_ptr(), 5) }, 0);
    }

    #[test]
    fn test_is_alphabetic_upper() {
        let data = b"HELLO";
        assert_eq!(unsafe { cobol_is_alphabetic_upper(data.as_ptr(), 5) }, 1);
        let lower = b"hello";
        assert_eq!(unsafe { cobol_is_alphabetic_upper(lower.as_ptr(), 5) }, 0);
    }

    #[test]
    fn test_move_string_padded() {
        let src = b"HELLO";
        let mut dst = [0u8; 10];
        unsafe { cobol_move_string(src.as_ptr(), 5, dst.as_mut_ptr(), 10) };
        assert_eq!(&dst, b"HELLO     ");
    }

    #[test]
    fn test_move_string_truncated() {
        let src = b"HELLO WORLD";
        let mut dst = [0u8; 5];
        unsafe { cobol_move_string(src.as_ptr(), 11, dst.as_mut_ptr(), 5) };
        assert_eq!(&dst, b"HELLO");
    }

    #[test]
    fn test_move_numeric_to_display() {
        let mut dst = [0u8; 10];
        unsafe { cobol_move_numeric_to_display(12345, 2, dst.as_mut_ptr(), 10) };
        let s = std::str::from_utf8(&dst).unwrap();
        assert_eq!(s, "    123.45");
    }

    #[test]
    fn test_move_numeric_to_display_negative() {
        let mut dst = [0u8; 10];
        unsafe { cobol_move_numeric_to_display(-4200, 2, dst.as_mut_ptr(), 10) };
        let s = std::str::from_utf8(&dst).unwrap();
        assert_eq!(s, "    -42.00");
    }

    #[test]
    fn test_string_concat() {
        let s1 = b"HELLO";
        let s2 = b" WORLD";
        let sources = [
            CobolStringSource {
                ptr: s1.as_ptr(),
                len: 5,
                delim_ptr: std::ptr::null(),
                delim_len: 0,
            },
            CobolStringSource {
                ptr: s2.as_ptr(),
                len: 6,
                delim_ptr: std::ptr::null(),
                delim_len: 0,
            },
        ];
        let mut dst = [b' '; 20];
        let mut pointer = 1u32; // 1-based
        let rc =
            unsafe { cobol_string_concat(sources.as_ptr(), 2, dst.as_mut_ptr(), 20, &mut pointer) };
        assert_eq!(rc, 0);
        assert_eq!(pointer, 12); // 1-based, next position
        assert_eq!(&dst[..11], b"HELLO WORLD");
    }

    #[test]
    fn test_string_concat_with_delimiter() {
        let s1 = b"HELLO, WORLD";
        let delim = b",";
        let sources = [CobolStringSource {
            ptr: s1.as_ptr(),
            len: 12,
            delim_ptr: delim.as_ptr(),
            delim_len: 1,
        }];
        let mut dst = [b' '; 20];
        let mut pointer = 1u32;
        let rc =
            unsafe { cobol_string_concat(sources.as_ptr(), 1, dst.as_mut_ptr(), 20, &mut pointer) };
        assert_eq!(rc, 0);
        assert_eq!(&dst[..5], b"HELLO");
    }

    #[test]
    fn test_string_concat_overflow() {
        let s1 = b"HELLO WORLD THIS IS LONG";
        let sources = [CobolStringSource {
            ptr: s1.as_ptr(),
            len: 24,
            delim_ptr: std::ptr::null(),
            delim_len: 0,
        }];
        let mut dst = [b' '; 5];
        let mut pointer = 1u32;
        let rc =
            unsafe { cobol_string_concat(sources.as_ptr(), 1, dst.as_mut_ptr(), 5, &mut pointer) };
        assert_eq!(rc, 1); // overflow
        assert_eq!(&dst, b"HELLO");
    }

    #[test]
    fn test_unstring() {
        let src = b"HELLO,WORLD,FOO";
        let delim = b",";
        let mut t1 = [0u8; 10];
        let mut t2 = [0u8; 10];
        let mut t3 = [0u8; 10];
        let mut c1 = 0u32;
        let mut c2 = 0u32;
        let mut c3 = 0u32;
        let mut targets = [
            CobolUnstringTarget {
                ptr: t1.as_mut_ptr(),
                len: 10,
                delimiter_ptr: std::ptr::null_mut(),
                delimiter_len: 0,
                count_ptr: &mut c1,
                kind: 0,
            },
            CobolUnstringTarget {
                ptr: t2.as_mut_ptr(),
                len: 10,
                delimiter_ptr: std::ptr::null_mut(),
                delimiter_len: 0,
                count_ptr: &mut c2,
                kind: 0,
            },
            CobolUnstringTarget {
                ptr: t3.as_mut_ptr(),
                len: 10,
                delimiter_ptr: std::ptr::null_mut(),
                delimiter_len: 0,
                count_ptr: &mut c3,
                kind: 0,
            },
        ];
        let mut pointer = 1u32;
        let mut tallying = 0u32;
        let rc = unsafe {
            cobol_unstring(
                src.as_ptr(),
                15,
                delim.as_ptr(),
                1,
                targets.as_mut_ptr(),
                3,
                &mut pointer,
                &mut tallying,
                0,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(rc, 0);
        assert_eq!(tallying, 3);
        assert_eq!(&t1[..5], b"HELLO");
        assert_eq!(&t2[..5], b"WORLD");
        assert_eq!(&t3[..3], b"FOO");
        assert_eq!(c1, 5);
        assert_eq!(c2, 5);
        assert_eq!(c3, 3);
    }

    #[test]
    fn test_inspect_tallying_all() {
        let src = b"ABCABC";
        let search = b"A";
        let count = unsafe { cobol_inspect_tallying(src.as_ptr(), 6, search.as_ptr(), 1, 1) };
        assert_eq!(count, 2);
    }

    #[test]
    fn test_inspect_tallying_leading() {
        let src = b"AAABBC";
        let search = b"A";
        let count = unsafe { cobol_inspect_tallying(src.as_ptr(), 6, search.as_ptr(), 1, 2) };
        assert_eq!(count, 3);
    }

    #[test]
    fn test_inspect_tallying_characters() {
        let src = b"HELLO";
        let count = unsafe { cobol_inspect_tallying(src.as_ptr(), 5, std::ptr::null(), 0, 0) };
        assert_eq!(count, 5);
    }

    #[test]
    fn test_inspect_replacing_all() {
        let mut src = *b"ABCABC";
        let search = b"A";
        let replace = b"X";
        unsafe {
            cobol_inspect_replacing(
                src.as_mut_ptr(),
                6,
                search.as_ptr(),
                1,
                replace.as_ptr(),
                1,
                1,
            )
        };
        assert_eq!(&src, b"XBCXBC");
    }

    #[test]
    fn test_inspect_replacing_first() {
        let mut src = *b"ABCABC";
        let search = b"A";
        let replace = b"X";
        unsafe {
            cobol_inspect_replacing(
                src.as_mut_ptr(),
                6,
                search.as_ptr(),
                1,
                replace.as_ptr(),
                1,
                3,
            )
        };
        assert_eq!(&src, b"XBCABC");
    }

    #[test]
    fn test_inspect_replacing_leading() {
        let mut src = *b"AABABC";
        let search = b"A";
        let replace = b"X";
        unsafe {
            cobol_inspect_replacing(
                src.as_mut_ptr(),
                6,
                search.as_ptr(),
                1,
                replace.as_ptr(),
                1,
                2,
            )
        };
        // Only leading A's are replaced: first two A's, then 'B' breaks the chain.
        assert_eq!(&src, b"XXBABC");
    }

    #[test]
    fn test_inspect_converting() {
        let mut src = *b"HELLO";
        let from = b"HELO";
        let to = b"helo";
        unsafe { cobol_inspect_converting(src.as_mut_ptr(), 5, from.as_ptr(), 4, to.as_ptr(), 4) };
        assert_eq!(&src, b"hello");
    }
}
