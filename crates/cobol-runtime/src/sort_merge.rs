// COBOL Runtime - SORT/MERGE support
//
// Implements in-memory record sorting and file-based merge for the
// COBOL SORT and MERGE statements. Records are compared using one or
// more key fields specified by offset, length, and direction.
//
// All public functions use the C ABI for linking with generated code.

use crate::file_io;

/// Key type for sort comparison.
/// 0 = alphanumeric (byte comparison)
/// 1 = signed binary (COMP) - little-endian int
/// 2 = unsigned binary (COMP) - little-endian uint
/// 3 = display numeric (may have sign)
pub const SORT_KEY_ALPHA: u8 = 0;
pub const SORT_KEY_SIGNED_BINARY: u8 = 1;
pub const SORT_KEY_UNSIGNED_BINARY: u8 = 2;
pub const SORT_KEY_DISPLAY_NUMERIC: u8 = 3;

/// Descriptor for a sort/merge key within a record.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SortKey {
    /// Byte offset of the key within the record.
    pub offset: u32,
    /// Length of the key in bytes.
    pub length: u32,
    /// Sort direction: true = ascending, false = descending.
    pub ascending: bool,
    /// Key data type (0=alpha, 1=signed binary, 2=unsigned binary, 3=display numeric)
    pub key_type: u8,
}

/// Compare two records using the given sort keys.
///
/// Returns `Ordering` suitable for use in sort comparators.
fn compare_records(a: &[u8], b: &[u8], keys: &[SortKey]) -> std::cmp::Ordering {
    for key in keys {
        let start = (key.offset as usize).min(a.len());
        let end = (start + key.length as usize).min(a.len());
        let start_b = (key.offset as usize).min(b.len());
        let end_b = (start_b + key.length as usize).min(b.len());

        let ka = &a[start..end];
        let kb = &b[start_b..end_b];

        let ord = match key.key_type {
            SORT_KEY_SIGNED_BINARY => {
                // Compare as signed integer (little-endian)
                let va = read_signed_le(ka);
                let vb = read_signed_le(kb);
                va.cmp(&vb)
            }
            SORT_KEY_UNSIGNED_BINARY => {
                let va = read_unsigned_le(ka);
                let vb = read_unsigned_le(kb);
                va.cmp(&vb)
            }
            _ => {
                // Alphanumeric / display numeric: byte comparison
                ka.cmp(kb)
            }
        };
        if ord != std::cmp::Ordering::Equal {
            return if key.ascending { ord } else { ord.reverse() };
        }
    }
    std::cmp::Ordering::Equal
}

fn read_signed_le(bytes: &[u8]) -> i64 {
    match bytes.len() {
        1 => bytes[0] as i8 as i64,
        2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i64,
        4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
        8 => i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]),
        _ => {
            // Generic: sign-extend from last byte
            let mut val: i64 = 0;
            for (i, &b) in bytes.iter().enumerate() {
                val |= (b as i64) << (i * 8);
            }
            // Sign extend
            let bits = bytes.len() * 8;
            if bits < 64 && (val & (1i64 << (bits - 1))) != 0 {
                val |= !0i64 << bits;
            }
            val
        }
    }
}

fn read_unsigned_le(bytes: &[u8]) -> u64 {
    match bytes.len() {
        1 => bytes[0] as u64,
        2 => u16::from_le_bytes([bytes[0], bytes[1]]) as u64,
        4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64,
        8 => u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]),
        _ => {
            let mut val: u64 = 0;
            for (i, &b) in bytes.iter().enumerate() {
                val |= (b as u64) << (i * 8);
            }
            val
        }
    }
}

/// Sort an array of fixed-length records in-place.
///
/// Records are laid out contiguously in memory: each record occupies
/// exactly `record_len` bytes, and there are `record_count` records
/// starting at `records_ptr`.
///
/// The sort is stable.
///
/// # Safety
/// `records_ptr` must be writable for `record_count * record_len` bytes.
/// `keys` must point to an array of `key_count` elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort(
    records_ptr: *mut u8,
    record_count: u32,
    record_len: u32,
    keys: *const SortKey,
    key_count: u32,
) {
    if record_count <= 1 || record_len == 0 || key_count == 0 {
        return;
    }

    let key_slice = std::slice::from_raw_parts(keys, key_count as usize);
    let total = (record_count as usize)
        .checked_mul(record_len as usize)
        .expect("record buffer size overflow");
    let data = std::slice::from_raw_parts_mut(records_ptr, total);

    // Collect records into a Vec of owned slices for sorting.
    let mut records: Vec<Vec<u8>> = data
        .chunks_exact(record_len as usize)
        .map(|c| c.to_vec())
        .collect();

    records.sort_by(|a, b| compare_records(a, b, key_slice));

    // Write sorted records back.
    for (i, rec) in records.iter().enumerate() {
        let offset = i * record_len as usize;
        data[offset..offset + record_len as usize].copy_from_slice(rec);
    }
}

/// Merge multiple pre-sorted input files into a single output file.
///
/// Each input file must already be sorted by the same key fields.
/// The merge reads records sequentially from each input and writes
/// them to the output in sorted order (k-way merge).
///
/// Returns 0 on success, non-zero on error (file status code).
///
/// # Safety
/// `input_files` must point to an array of `input_count` file IDs.
/// `keys` must point to an array of `key_count` elements.
/// All files must be open with appropriate modes before calling.
#[no_mangle]
pub unsafe extern "C" fn cobol_merge(
    input_files: *const u32,
    input_count: u32,
    output_file: u32,
    keys: *const SortKey,
    key_count: u32,
    record_len: u32,
) -> u32 {
    let file_ids = std::slice::from_raw_parts(input_files, input_count as usize);
    let key_slice = std::slice::from_raw_parts(keys, key_count as usize);

    // Read the first record from each input file.
    let mut buffers: Vec<Option<Vec<u8>>> = Vec::with_capacity(input_count as usize);
    for &fid in file_ids {
        let mut buf = vec![0u8; record_len as usize];
        let status = file_io::cobol_file_read_next(fid, buf.as_mut_ptr(), record_len);
        if status == 0 {
            buffers.push(Some(buf));
        } else {
            buffers.push(None); // empty or error
        }
    }

    loop {
        // Find the smallest (by key) non-exhausted record.
        let mut best_idx: Option<usize> = None;
        for (i, buf) in buffers.iter().enumerate() {
            if let Some(ref rec) = buf {
                match best_idx {
                    None => best_idx = Some(i),
                    Some(bi) => {
                        if let Some(ref best_rec) = buffers[bi] {
                            if compare_records(rec, best_rec, key_slice) == std::cmp::Ordering::Less
                            {
                                best_idx = Some(i);
                            }
                        }
                    }
                }
            }
        }

        let bi = match best_idx {
            Some(i) => i,
            None => break, // all inputs exhausted
        };

        // Write the winning record to the output.
        if let Some(ref rec) = buffers[bi] {
            let status = file_io::cobol_file_write(output_file, rec.as_ptr(), record_len);
            if status != 0 {
                return status;
            }
        }

        // Read the next record from the winning input.
        let mut buf = vec![0u8; record_len as usize];
        let status = file_io::cobol_file_read_next(file_ids[bi], buf.as_mut_ptr(), record_len);
        if status == 0 {
            buffers[bi] = Some(buf);
        } else {
            buffers[bi] = None; // exhausted
        }
    }

    0
}

// ---------------------------------------------------------------------------
// Sort buffer management for INPUT/OUTPUT PROCEDURE
// ---------------------------------------------------------------------------

use std::sync::Mutex;

struct SortBuffer {
    data: Vec<u8>,
    record_len: usize,
    record_count: usize,
    read_index: usize,
}

static SORT_BUFFERS: Mutex<Vec<Option<SortBuffer>>> = Mutex::new(Vec::new());

/// Initialize a sort buffer for INPUT PROCEDURE. Returns a buffer ID.
/// # Safety
/// Caller must ensure valid buffer ID usage.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort_buffer_init(record_len: u32) -> u32 {
    let mut buffers = SORT_BUFFERS.lock().unwrap();
    let buf = SortBuffer {
        data: Vec::with_capacity(64 * record_len as usize),
        record_len: record_len as usize,
        record_count: 0,
        read_index: 0,
    };
    // Find an empty slot or push new
    for (i, slot) in buffers.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(buf);
            return i as u32;
        }
    }
    buffers.push(Some(buf));
    (buffers.len() - 1) as u32
}

/// Add a record to the sort buffer (RELEASE).
/// # Safety
/// `record_ptr` must point to valid memory of `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort_buffer_release(
    buf_id: u32,
    record_ptr: *const u8,
    record_len: u32,
) {
    let mut buffers = SORT_BUFFERS.lock().unwrap();
    if let Some(Some(ref mut buf)) = buffers.get_mut(buf_id as usize) {
        let rec = std::slice::from_raw_parts(record_ptr, record_len as usize);
        buf.data.extend_from_slice(rec);
        buf.record_count += 1;
    }
}

/// Sort the buffered records using the given keys.
/// # Safety
/// `keys` must point to valid `key_count` SortKey elements.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort_buffer_sort(buf_id: u32, keys: *const SortKey, key_count: u32) {
    let mut buffers = SORT_BUFFERS.lock().unwrap();
    if let Some(Some(ref mut buf)) = buffers.get_mut(buf_id as usize) {
        if buf.record_count <= 1 {
            buf.read_index = 0;
            return;
        }
        let key_slice = std::slice::from_raw_parts(keys, key_count as usize);
        let rlen = buf.record_len;
        let mut records: Vec<Vec<u8>> = buf.data.chunks_exact(rlen).map(|c| c.to_vec()).collect();
        records.sort_by(|a, b| compare_records(a, b, key_slice));
        buf.data.clear();
        for rec in &records {
            buf.data.extend_from_slice(rec);
        }
        buf.read_index = 0;
    }
}

/// Read the next sorted record (RETURN). Returns 0 on success, 10 on AT END.
/// # Safety
/// `record_ptr` must be writable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort_buffer_return(
    buf_id: u32,
    record_ptr: *mut u8,
    record_len: u32,
) -> u32 {
    let mut buffers = SORT_BUFFERS.lock().unwrap();
    if let Some(Some(ref mut buf)) = buffers.get_mut(buf_id as usize) {
        if buf.read_index >= buf.record_count {
            return 10; // AT END
        }
        let offset = buf.read_index * buf.record_len;
        let end = offset + record_len as usize;
        if end <= buf.data.len() {
            let dest = std::slice::from_raw_parts_mut(record_ptr, record_len as usize);
            dest.copy_from_slice(&buf.data[offset..end]);
        }
        buf.read_index += 1;
        0
    } else {
        10
    }
}

/// Free a sort buffer.
/// # Safety
/// Buffer ID must have been returned by `cobol_sort_buffer_init`.
#[no_mangle]
pub unsafe extern "C" fn cobol_sort_buffer_free(buf_id: u32) {
    let mut buffers = SORT_BUFFERS.lock().unwrap();
    if let Some(slot) = buffers.get_mut(buf_id as usize) {
        *slot = None;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_ascending() {
        // 3 records of 5 bytes each.
        let mut data = *b"CCCCCAAAAABBBBB";
        let keys = [SortKey {
            offset: 0,
            length: 5,
            ascending: true,
            key_type: 0,
        }];
        unsafe { cobol_sort(data.as_mut_ptr(), 3, 5, keys.as_ptr(), 1) };
        assert_eq!(&data, b"AAAAABBBBBCCCCC");
    }

    #[test]
    fn test_sort_descending() {
        let mut data = *b"AAAAABBBBBCCCCC";
        let keys = [SortKey {
            offset: 0,
            length: 5,
            ascending: false,
            key_type: 0,
        }];
        unsafe { cobol_sort(data.as_mut_ptr(), 3, 5, keys.as_ptr(), 1) };
        assert_eq!(&data, b"CCCCCBBBBBAAAAA");
    }

    #[test]
    fn test_sort_multiple_keys() {
        // Records: "B1", "A2", "A1", "B2"
        let mut data = *b"B1A2A1B2";
        let keys = [
            SortKey {
                offset: 0,
                length: 1,
                ascending: true,
                key_type: 0,
            },
            SortKey {
                offset: 1,
                length: 1,
                ascending: true,
                key_type: 0,
            },
        ];
        unsafe { cobol_sort(data.as_mut_ptr(), 4, 2, keys.as_ptr(), 2) };
        assert_eq!(&data, b"A1A2B1B2");
    }

    #[test]
    fn test_sort_single_record() {
        let mut data = *b"HELLO";
        let keys = [SortKey {
            offset: 0,
            length: 5,
            ascending: true,
            key_type: 0,
        }];
        unsafe { cobol_sort(data.as_mut_ptr(), 1, 5, keys.as_ptr(), 1) };
        assert_eq!(&data, b"HELLO"); // unchanged
    }

    #[test]
    fn test_sort_empty() {
        let keys = [SortKey {
            offset: 0,
            length: 5,
            ascending: true,
            key_type: 0,
        }];
        // Should not panic.
        unsafe { cobol_sort(std::ptr::null_mut(), 0, 5, keys.as_ptr(), 1) };
    }

    #[test]
    fn test_compare_records() {
        let keys = [SortKey {
            offset: 0,
            length: 3,
            ascending: true,
            key_type: 0,
        }];
        assert_eq!(
            compare_records(b"ABC", b"DEF", &keys),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_records(b"DEF", b"ABC", &keys),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_records(b"ABC", b"ABC", &keys),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn test_merge_two_files() {
        let dir = tempfile::tempdir().unwrap();

        let f1_path = dir.path().join("merge_in1.dat");
        let f2_path = dir.path().join("merge_in2.dat");
        let fo_path = dir.path().join("merge_out.dat");

        // Write sorted input file 1: AAAAA, CCCCC
        let p1 = f1_path.to_str().unwrap().as_bytes();
        let p2 = f2_path.to_str().unwrap().as_bytes();
        let po = fo_path.to_str().unwrap().as_bytes();

        unsafe {
            assert_eq!(
                file_io::cobol_file_open(
                    900,
                    p1.as_ptr(),
                    p1.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Output,
                    5,
                ),
                0
            );
            assert_eq!(file_io::cobol_file_write(900, b"AAAAA".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_write(900, b"CCCCC".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_close(900), 0);

            // Write sorted input file 2: BBBBB, DDDDD
            assert_eq!(
                file_io::cobol_file_open(
                    901,
                    p2.as_ptr(),
                    p2.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Output,
                    5,
                ),
                0
            );
            assert_eq!(file_io::cobol_file_write(901, b"BBBBB".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_write(901, b"DDDDD".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_close(901), 0);

            // Re-open inputs for reading.
            assert_eq!(
                file_io::cobol_file_open(
                    910,
                    p1.as_ptr(),
                    p1.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Input,
                    5,
                ),
                0
            );
            assert_eq!(
                file_io::cobol_file_open(
                    911,
                    p2.as_ptr(),
                    p2.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Input,
                    5,
                ),
                0
            );

            // Open output.
            assert_eq!(
                file_io::cobol_file_open(
                    920,
                    po.as_ptr(),
                    po.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Output,
                    5,
                ),
                0
            );

            // Merge.
            let inputs = [910u32, 911];
            let keys = [SortKey {
                offset: 0,
                length: 5,
                ascending: true,
                key_type: 0,
            }];
            let status = cobol_merge(inputs.as_ptr(), 2, 920, keys.as_ptr(), 1, 5);
            assert_eq!(status, 0);

            assert_eq!(file_io::cobol_file_close(910), 0);
            assert_eq!(file_io::cobol_file_close(911), 0);
            assert_eq!(file_io::cobol_file_close(920), 0);
        }

        // Verify merged output.
        let merged = std::fs::read(&fo_path).unwrap();
        assert_eq!(merged.len(), 20); // 4 records of 5 bytes
        assert_eq!(&merged[0..5], b"AAAAA");
        assert_eq!(&merged[5..10], b"BBBBB");
        assert_eq!(&merged[10..15], b"CCCCC");
        assert_eq!(&merged[15..20], b"DDDDD");
    }
}
