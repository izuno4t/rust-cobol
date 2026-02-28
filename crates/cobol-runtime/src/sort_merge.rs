// COBOL Runtime - SORT/MERGE support
//
// Implements in-memory record sorting and file-based merge for the
// COBOL SORT and MERGE statements. Records are compared using one or
// more key fields specified by offset, length, and direction.
//
// All public functions use the C ABI for linking with generated code.

use crate::file_io;

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
}

/// Compare two records using the given sort keys.
///
/// Returns `Ordering` suitable for use in sort comparators.
fn compare_records(a: &[u8], b: &[u8], keys: &[SortKey]) -> std::cmp::Ordering {
    for key in keys {
        let start = key.offset as usize;
        let end = start + key.length as usize;

        let ka = &a[start..end.min(a.len())];
        let kb = &b[start..end.min(b.len())];

        let ord = ka.cmp(kb);
        if ord != std::cmp::Ordering::Equal {
            return if key.ascending { ord } else { ord.reverse() };
        }
    }
    std::cmp::Ordering::Equal
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
    let total = record_count as usize * record_len as usize;
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
            },
            SortKey {
                offset: 1,
                length: 1,
                ascending: true,
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
                    500,
                    p1.as_ptr(),
                    p1.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Output,
                    5,
                ),
                0
            );
            assert_eq!(file_io::cobol_file_write(500, b"AAAAA".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_write(500, b"CCCCC".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_close(500), 0);

            // Write sorted input file 2: BBBBB, DDDDD
            assert_eq!(
                file_io::cobol_file_open(
                    501,
                    p2.as_ptr(),
                    p2.len() as u32,
                    file_io::FileOrganization::Sequential,
                    file_io::FileAccessMode::Sequential,
                    file_io::FileOpenMode::Output,
                    5,
                ),
                0
            );
            assert_eq!(file_io::cobol_file_write(501, b"BBBBB".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_write(501, b"DDDDD".as_ptr(), 5), 0);
            assert_eq!(file_io::cobol_file_close(501), 0);

            // Re-open inputs for reading.
            assert_eq!(
                file_io::cobol_file_open(
                    510,
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
                    511,
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
                    520,
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
            let inputs = [510u32, 511];
            let keys = [SortKey {
                offset: 0,
                length: 5,
                ascending: true,
            }];
            let status = cobol_merge(inputs.as_ptr(), 2, 520, keys.as_ptr(), 1, 5);
            assert_eq!(status, 0);

            assert_eq!(file_io::cobol_file_close(510), 0);
            assert_eq!(file_io::cobol_file_close(511), 0);
            assert_eq!(file_io::cobol_file_close(520), 0);
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
