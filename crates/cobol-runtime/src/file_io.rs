// COBOL Runtime - File I/O operations
//
// Implements COBOL file handling for Sequential, Line Sequential,
// Relative, and Indexed file organisations. Each file is identified
// by a numeric file_id assigned at compile time.
//
// File status codes follow the COBOL-85 standard:
//   00 = successful
//   02 = duplicate key (indexed)
//   10 = at end / no next record
//   22 = duplicate key on write
//   23 = record not found
//   30 = permanent I/O error
//   35 = file not found (OPEN INPUT)
//   37 = permission denied
//   41 = file already open
//   42 = file not open
//   43 = READ not permitted (output/extend mode)
//   44 = record length mismatch (rewrite)
//   46 = read error / no valid next record
//   47 = READ on file not opened INPUT or I-O
//   48 = WRITE on file not opened OUTPUT, I-O, or EXTEND
//   49 = REWRITE/DELETE on file not opened I-O
//
// All public functions use the C ABI.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// File organisation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOrganization {
    Sequential = 0,
    LineSequential = 1,
    Relative = 2,
    Indexed = 3,
}

/// Access mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAccessMode {
    Sequential = 0,
    Random = 1,
    Dynamic = 2,
}

/// OPEN mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOpenMode {
    Input = 0,
    Output = 1,
    IoMode = 2,
    Extend = 3,
}

// ---------------------------------------------------------------------------
// Internal file state
// ---------------------------------------------------------------------------

/// Internal representation of an open file.
enum CobolFileInner {
    /// Sequential or Line Sequential reader.
    Reader(BufReader<File>),
    /// Sequential or Line Sequential writer.
    Writer(BufWriter<File>),
    /// I-O mode — need both read and write.
    ReadWrite(File),
}

struct CobolFile {
    inner: CobolFileInner,
    org: FileOrganization,
    #[allow(dead_code)] // Used for access-mode validation in future expansions.
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
    /// For relative files: current relative record number (0-based).
    current_record: u64,
    /// For indexed files: sorted index of (key, file_offset) pairs.
    index: Vec<(Vec<u8>, u64)>,
}

// Global file table -- lazily initialised.
static FILE_TABLE: Mutex<Option<HashMap<u32, CobolFile>>> = Mutex::new(None);

fn with_file_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<u32, CobolFile>) -> R,
{
    let mut guard = FILE_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

// ---------------------------------------------------------------------------
// File status constants
// ---------------------------------------------------------------------------

const FS_OK: u32 = 0; // "00"
const FS_AT_END: u32 = 10;
const FS_REC_NOT_FOUND: u32 = 23;
const FS_IO_ERROR: u32 = 30;
const FS_NOT_FOUND: u32 = 35;
const FS_ALREADY_OPEN: u32 = 41;
const FS_NOT_OPEN: u32 = 42;
const FS_WRITE_NOT_PERMITTED: u32 = 48;
const FS_IO_MODE_REQUIRED: u32 = 49;

// ---------------------------------------------------------------------------
// C ABI functions
// ---------------------------------------------------------------------------

/// Open a file.
///
/// # Safety
/// `path_ptr` must point to a valid UTF-8 string of `path_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_open(
    file_id: u32,
    path_ptr: *const u8,
    path_len: u32,
    org: FileOrganization,
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
) -> u32 {
    if path_ptr.is_null() || path_len == 0 {
        return FS_IO_ERROR;
    }
    let path_slice = std::slice::from_raw_parts(path_ptr, path_len as usize);
    let path = match std::str::from_utf8(path_slice) {
        Ok(s) => s.trim(),
        Err(_) => return FS_IO_ERROR,
    };

    with_file_table(|table| {
        if table.contains_key(&file_id) {
            return FS_ALREADY_OPEN;
        }

        let result = match mode {
            FileOpenMode::Input => OpenOptions::new()
                .read(true)
                .open(path)
                .map(|f| CobolFileInner::Reader(BufReader::new(f))),
            FileOpenMode::Output => OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map(|f| CobolFileInner::Writer(BufWriter::new(f))),
            FileOpenMode::Extend => OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map(|f| CobolFileInner::Writer(BufWriter::new(f))),
            FileOpenMode::IoMode => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .map(CobolFileInner::ReadWrite),
        };

        match result {
            Ok(inner) => {
                table.insert(
                    file_id,
                    CobolFile {
                        inner,
                        org,
                        access,
                        mode,
                        record_len,
                        current_record: 0,
                        index: Vec::new(),
                    },
                );
                FS_OK
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => FS_NOT_FOUND,
                std::io::ErrorKind::PermissionDenied => 37, // permission
                _ => FS_IO_ERROR,
            },
        }
    })
}

/// Build the in-memory index for an indexed file by scanning all records.
///
/// Reads the file from the beginning, extracting the key from each record
/// at the given offset and length, and stores (key, file_offset) pairs
/// sorted by key.
fn build_index(file: &mut CobolFile, key_offset: u32, key_len: u32) {
    if file.org != FileOrganization::Indexed {
        return;
    }
    let rec_len = file.record_len as usize;
    if rec_len == 0 {
        return;
    }
    let key_off = key_offset as usize;
    let key_length = key_len as usize;
    if key_off + key_length > rec_len {
        return;
    }

    let f = match &mut file.inner {
        CobolFileInner::Reader(r) => r.get_mut(),
        CobolFileInner::ReadWrite(f) => f,
        _ => return,
    };

    // Seek to the beginning.
    if f.seek(SeekFrom::Start(0)).is_err() {
        return;
    }

    let mut buf = vec![0u8; rec_len];
    let mut offset = 0u64;
    file.index.clear();

    while f.read_exact(&mut buf).is_ok() {
        let key = buf[key_off..key_off + key_length].to_vec();
        file.index.push((key, offset));
        offset += rec_len as u64;
    }

    // Sort index by key.
    file.index.sort_by(|a, b| a.0.cmp(&b.0));

    // Reset file position to beginning.
    let _ = f.seek(SeekFrom::Start(0));
    file.current_record = 0;
}

/// Open an indexed file with key information.
///
/// This extends `cobol_file_open` by also specifying the primary key's
/// offset and length within each record. After opening, the index is
/// built by scanning all existing records (for INPUT and I-O modes).
///
/// # Safety
/// `path_ptr` must point to a valid UTF-8 string of `path_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_open_indexed(
    file_id: u32,
    path_ptr: *const u8,
    path_len: u32,
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
    key_offset: u32,
    key_len: u32,
) -> u32 {
    // Delegate the actual open to the standard function.
    let rc = cobol_file_open(
        file_id,
        path_ptr,
        path_len,
        FileOrganization::Indexed,
        access,
        mode,
        record_len,
    );
    if rc != FS_OK {
        return rc;
    }

    // Build index for INPUT or I-O mode.
    if mode == FileOpenMode::Input || mode == FileOpenMode::IoMode {
        with_file_table(|table| {
            if let Some(file) = table.get_mut(&file_id) {
                build_index(file, key_offset, key_len);
            }
        });
    }

    FS_OK
}

/// Close a file.
#[no_mangle]
pub extern "C" fn cobol_file_close(file_id: u32) -> u32 {
    with_file_table(|table| {
        if let Some(mut f) = table.remove(&file_id) {
            // Flush if writer.
            if let CobolFileInner::Writer(ref mut w) = f.inner {
                let _ = w.flush();
            }
            FS_OK
        } else {
            FS_NOT_OPEN
        }
    })
}

/// Read the next record (sequential access).
///
/// For sequential organisation: reads `record_len` bytes.
/// For line sequential: reads one line (up to record_len bytes, padded).
///
/// # Safety
/// `record_ptr` must be writable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_read_next(
    file_id: u32,
    record_ptr: *mut u8,
    record_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let buf = std::slice::from_raw_parts_mut(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        match file.org {
            FileOrganization::LineSequential => {
                let reader = match &mut file.inner {
                    CobolFileInner::Reader(r) => r,
                    CobolFileInner::ReadWrite(f) => {
                        // For I-O mode, read byte-by-byte to avoid BufReader
                        // position desync with the underlying file.
                        let mut line = String::new();
                        let mut byte_buf = [0u8; 1];
                        let mut bytes_read_total = 0usize;
                        loop {
                            match f.read(&mut byte_buf) {
                                Ok(0) => break, // EOF
                                Ok(_) => {
                                    bytes_read_total += 1;
                                    if byte_buf[0] == b'\n' {
                                        break;
                                    }
                                    if byte_buf[0] != b'\r' {
                                        line.push(byte_buf[0] as char);
                                    }
                                }
                                Err(_) => return FS_IO_ERROR,
                            }
                        }
                        if bytes_read_total == 0 {
                            return FS_AT_END;
                        }
                        let bytes = line.as_bytes();
                        let copy_len = bytes.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                        for b in buf[copy_len..].iter_mut() {
                            *b = b' ';
                        }
                        file.current_record += 1;
                        return FS_OK;
                    }
                    _ => return FS_WRITE_NOT_PERMITTED,
                };

                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => FS_AT_END,
                    Ok(_) => {
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        let bytes = trimmed.as_bytes();
                        let copy_len = bytes.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                        for b in buf[copy_len..].iter_mut() {
                            *b = b' ';
                        }
                        file.current_record += 1;
                        FS_OK
                    }
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Sequential => {
                let reader = match &mut file.inner {
                    CobolFileInner::Reader(r) => r as &mut dyn Read,
                    CobolFileInner::ReadWrite(f) => f as &mut dyn Read,
                    _ => return FS_WRITE_NOT_PERMITTED,
                };

                match reader.read_exact(buf) {
                    Ok(()) => {
                        file.current_record += 1;
                        FS_OK
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => FS_AT_END,
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Relative => {
                // Sequential read of relative file: read the next record-sized block.
                let reader = match &mut file.inner {
                    CobolFileInner::Reader(r) => r as &mut dyn Read,
                    CobolFileInner::ReadWrite(f) => f as &mut dyn Read,
                    _ => return FS_WRITE_NOT_PERMITTED,
                };

                match reader.read_exact(buf) {
                    Ok(()) => {
                        file.current_record += 1;
                        FS_OK
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => FS_AT_END,
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Indexed => {
                // Sequential read through index order.
                let idx = file.current_record as usize;
                if idx >= file.index.len() {
                    return FS_AT_END;
                }
                let offset = file.index[idx].1;

                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_WRITE_NOT_PERMITTED,
                };

                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_IO_ERROR;
                }
                match f.read_exact(buf) {
                    Ok(()) => {
                        file.current_record += 1;
                        FS_OK
                    }
                    Err(_) => FS_IO_ERROR,
                }
            }
        }
    })
}

/// Read a record by key (random access).
///
/// For relative files: `key_ptr` is interpreted as a big-endian record number.
/// For indexed files: `key_ptr` is the record key to look up.
///
/// # Safety
/// `record_ptr` must be writable for `record_len` bytes.
/// `key_ptr` must be readable for `key_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_read_key(
    file_id: u32,
    key_ptr: *const u8,
    key_len: u32,
    record_ptr: *mut u8,
    record_len: u32,
) -> u32 {
    if key_ptr.is_null() || record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let key = std::slice::from_raw_parts(key_ptr, key_len as usize);
    let buf = std::slice::from_raw_parts_mut(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        match file.org {
            FileOrganization::Relative => {
                // Key is a big-endian record number (1-based).
                let mut rec_num = 0u64;
                for &b in key.iter() {
                    rec_num = rec_num * 256 + b as u64;
                }
                if rec_num == 0 {
                    return FS_REC_NOT_FOUND;
                }
                let offset = (rec_num - 1) * (file.record_len as u64);
                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_IO_ERROR,
                };
                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_REC_NOT_FOUND;
                }
                match f.read_exact(buf) {
                    Ok(()) => {
                        file.current_record = rec_num;
                        FS_OK
                    }
                    Err(_) => FS_REC_NOT_FOUND,
                }
            }
            FileOrganization::Indexed => {
                // Binary search the sorted index.
                let pos = file.index.partition_point(|(k, _)| k.as_slice() < key);
                if pos >= file.index.len() || file.index[pos].0 != key {
                    return FS_REC_NOT_FOUND;
                }
                let offset = file.index[pos].1;
                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_IO_ERROR,
                };
                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_IO_ERROR;
                }
                match f.read_exact(buf) {
                    Ok(()) => {
                        file.current_record = (pos + 1) as u64;
                        FS_OK
                    }
                    Err(_) => FS_IO_ERROR,
                }
            }
            _ => FS_IO_ERROR, // Random read not supported for sequential files.
        }
    })
}

/// Write a record.
///
/// For line sequential: appends a newline after the record.
///
/// # Safety
/// `record_ptr` must be readable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_write(
    file_id: u32,
    record_ptr: *const u8,
    record_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let data = std::slice::from_raw_parts(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        match file.mode {
            FileOpenMode::Output | FileOpenMode::Extend | FileOpenMode::IoMode => {}
            _ => return FS_WRITE_NOT_PERMITTED,
        }

        let write_result = match &mut file.inner {
            CobolFileInner::Writer(w) => {
                if file.org == FileOrganization::LineSequential {
                    // Trim trailing spaces and write as a line.
                    let trimmed = trim_trailing_spaces(data);
                    w.write_all(trimmed).and_then(|()| w.write_all(b"\n"))
                } else {
                    w.write_all(data)
                }
            }
            CobolFileInner::ReadWrite(f) => {
                if file.org == FileOrganization::LineSequential {
                    let trimmed = trim_trailing_spaces(data);
                    f.write_all(trimmed).and_then(|()| f.write_all(b"\n"))
                } else {
                    f.write_all(data)
                }
            }
            _ => return FS_WRITE_NOT_PERMITTED,
        };

        match write_result {
            Ok(()) => {
                file.current_record += 1;
                FS_OK
            }
            Err(_) => FS_IO_ERROR,
        }
    })
}

/// Write a record to an indexed file, updating the in-memory index.
///
/// The key is extracted from the record at the given offset and length.
///
/// # Safety
/// `record_ptr` must be readable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_write_indexed(
    file_id: u32,
    record_ptr: *const u8,
    record_len: u32,
    key_offset: u32,
    key_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let data = std::slice::from_raw_parts(record_ptr, record_len as usize);
    let key_off = key_offset as usize;
    let key_length = key_len as usize;
    if key_off + key_length > record_len as usize {
        return FS_IO_ERROR;
    }

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        if file.org != FileOrganization::Indexed {
            return FS_IO_ERROR;
        }

        match file.mode {
            FileOpenMode::Output | FileOpenMode::Extend | FileOpenMode::IoMode => {}
            _ => return FS_WRITE_NOT_PERMITTED,
        }

        // Check for duplicate key.
        let key = data[key_off..key_off + key_length].to_vec();
        let insert_pos = file
            .index
            .partition_point(|(k, _)| k.as_slice() < key.as_slice());
        if insert_pos < file.index.len() && file.index[insert_pos].0 == key {
            return 22; // FS_DUPLICATE_KEY
        }

        // Get current write position.
        let offset = match &mut file.inner {
            CobolFileInner::Writer(w) => {
                // Flush first to get the correct stream position.
                let _ = w.flush();
                match w.get_ref().stream_position() {
                    Ok(pos) => pos,
                    Err(_) => return FS_IO_ERROR,
                }
            }
            CobolFileInner::ReadWrite(f) => match f.seek(SeekFrom::End(0)) {
                Ok(pos) => pos,
                Err(_) => return FS_IO_ERROR,
            },
            _ => return FS_WRITE_NOT_PERMITTED,
        };

        // Write the record.
        let write_result = match &mut file.inner {
            CobolFileInner::Writer(w) => w.write_all(data),
            CobolFileInner::ReadWrite(f) => f.write_all(data),
            _ => return FS_WRITE_NOT_PERMITTED,
        };

        match write_result {
            Ok(()) => {
                // Insert into sorted index.
                file.index.insert(insert_pos, (key, offset));
                file.current_record += 1;
                FS_OK
            }
            Err(_) => FS_IO_ERROR,
        }
    })
}

/// Rewrite the current record (requires I-O mode, sequential or relative).
///
/// # Safety
/// `record_ptr` must be readable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_rewrite(
    file_id: u32,
    record_ptr: *const u8,
    record_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let data = std::slice::from_raw_parts(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        if file.mode != FileOpenMode::IoMode {
            return FS_IO_MODE_REQUIRED;
        }

        let f = match &mut file.inner {
            CobolFileInner::ReadWrite(f) => f,
            _ => return FS_IO_MODE_REQUIRED,
        };

        // Seek back to the start of the current record.
        let rec_offset = (file.current_record.saturating_sub(1)) * (file.record_len as u64);
        if f.seek(SeekFrom::Start(rec_offset)).is_err() {
            return FS_IO_ERROR;
        }

        match f.write_all(data) {
            Ok(()) => FS_OK,
            Err(_) => FS_IO_ERROR,
        }
    })
}

/// Delete the current record (for relative/indexed files, I-O mode).
#[no_mangle]
pub extern "C" fn cobol_file_delete(file_id: u32) -> u32 {
    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        if file.mode != FileOpenMode::IoMode {
            return FS_IO_MODE_REQUIRED;
        }

        match file.org {
            FileOrganization::Relative => {
                // Zero-fill the current record slot to mark it as deleted.
                let f = match &mut file.inner {
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_IO_MODE_REQUIRED,
                };

                let rec_offset = (file.current_record.saturating_sub(1)) * (file.record_len as u64);
                if f.seek(SeekFrom::Start(rec_offset)).is_err() {
                    return FS_IO_ERROR;
                }
                let zeros = vec![0u8; file.record_len as usize];
                match f.write_all(&zeros) {
                    Ok(()) => FS_OK,
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Indexed => {
                // Remove from the index.
                let idx = file.current_record.saturating_sub(1) as usize;
                if idx < file.index.len() {
                    file.index.remove(idx);
                    FS_OK
                } else {
                    FS_REC_NOT_FOUND
                }
            }
            _ => FS_IO_ERROR, // DELETE not meaningful for sequential.
        }
    })
}

/// START -- position the file for subsequent sequential reads.
///
/// For relative/indexed files. Mode: 0=EQ, 1=GT, 2=GE, 3=LT, 4=LE.
///
/// # Safety
/// `key_ptr` must be readable for `key_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_start(
    file_id: u32,
    key_ptr: *const u8,
    key_len: u32,
    mode: u32,
) -> u32 {
    let key = std::slice::from_raw_parts(key_ptr, key_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };

        match file.org {
            FileOrganization::Relative => {
                // Key is interpreted as a relative record number (big-endian).
                let mut rec_num = 0u64;
                for &b in key.iter() {
                    rec_num = rec_num * 256 + b as u64;
                }
                file.current_record = rec_num;

                // Seek the underlying file.
                let offset = rec_num * (file.record_len as u64);
                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_IO_ERROR,
                };
                match f.seek(SeekFrom::Start(offset)) {
                    Ok(_) => FS_OK,
                    Err(_) => FS_REC_NOT_FOUND,
                }
            }
            FileOrganization::Indexed => {
                // Binary search the index for the key.
                let pos = file.index.partition_point(|(k, _)| k.as_slice() < key);

                let target = match mode {
                    0 => {
                        // EQ
                        if pos < file.index.len() && file.index[pos].0 == key {
                            Some(pos)
                        } else {
                            None
                        }
                    }
                    1 => {
                        // GT
                        let mut p = pos;
                        while p < file.index.len() && file.index[p].0 == key {
                            p += 1;
                        }
                        if p < file.index.len() {
                            Some(p)
                        } else {
                            None
                        }
                    }
                    2 => {
                        // GE
                        if pos < file.index.len() {
                            Some(pos)
                        } else {
                            None
                        }
                    }
                    3 => {
                        // LT
                        if pos > 0 {
                            Some(pos - 1)
                        } else {
                            None
                        }
                    }
                    4 => {
                        // LE
                        if pos < file.index.len() && file.index[pos].0 == key {
                            Some(pos)
                        } else if pos > 0 {
                            Some(pos - 1)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                match target {
                    Some(idx) => {
                        file.current_record = idx as u64;
                        FS_OK
                    }
                    None => FS_REC_NOT_FOUND,
                }
            }
            _ => FS_IO_ERROR,
        }
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Close all open files, flushing writers. Called during program shutdown.
pub fn close_all_files() {
    let mut guard = FILE_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(table) = guard.as_mut() {
        for (_id, mut file) in table.drain() {
            if let CobolFileInner::Writer(ref mut w) = file.inner {
                let _ = w.flush();
            }
        }
    }
}

/// Trim trailing ASCII spaces from a byte slice.
fn trim_trailing_spaces(data: &[u8]) -> &[u8] {
    let mut end = data.len();
    while end > 0 && data[end - 1] == b' ' {
        end -= 1;
    }
    &data[..end]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_open_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        // Open for output (creates the file).
        let status = unsafe {
            cobol_file_open(
                100,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                80,
            )
        };
        assert_eq!(status, FS_OK);

        // Double open should fail.
        let status2 = unsafe {
            cobol_file_open(
                100,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                80,
            )
        };
        assert_eq!(status2, FS_ALREADY_OPEN);

        // Close.
        let status3 = cobol_file_close(100);
        assert_eq!(status3, FS_OK);

        // Close again should fail.
        let status4 = cobol_file_close(100);
        assert_eq!(status4, FS_NOT_OPEN);
    }

    #[test]
    fn test_file_write_read_sequential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seq.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        // Write two 10-byte records.
        let status = unsafe {
            cobol_file_open(
                200,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                10,
            )
        };
        assert_eq!(status, FS_OK);

        let rec1 = b"RECORD ONE";
        let rec2 = b"RECORD TWO";
        unsafe {
            assert_eq!(cobol_file_write(200, rec1.as_ptr(), 10), FS_OK);
            assert_eq!(cobol_file_write(200, rec2.as_ptr(), 10), FS_OK);
        }
        assert_eq!(cobol_file_close(200), FS_OK);

        // Read them back.
        let status = unsafe {
            cobol_file_open(
                201,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                10,
            )
        };
        assert_eq!(status, FS_OK);

        let mut buf = [0u8; 10];
        unsafe {
            assert_eq!(cobol_file_read_next(201, buf.as_mut_ptr(), 10), FS_OK);
            assert_eq!(&buf, b"RECORD ONE");

            assert_eq!(cobol_file_read_next(201, buf.as_mut_ptr(), 10), FS_OK);
            assert_eq!(&buf, b"RECORD TWO");

            // End of file.
            assert_eq!(cobol_file_read_next(201, buf.as_mut_ptr(), 10), FS_AT_END);
        }

        assert_eq!(cobol_file_close(201), FS_OK);
    }

    #[test]
    fn test_file_line_sequential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        // Write lines.
        let status = unsafe {
            cobol_file_open(
                300,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::LineSequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                20,
            )
        };
        assert_eq!(status, FS_OK);

        // "HELLO" padded to 20 bytes -- trailing spaces will be trimmed on write.
        let mut rec = [b' '; 20];
        rec[..5].copy_from_slice(b"HELLO");
        unsafe {
            assert_eq!(cobol_file_write(300, rec.as_ptr(), 20), FS_OK);
        }

        rec = [b' '; 20];
        rec[..5].copy_from_slice(b"WORLD");
        unsafe {
            assert_eq!(cobol_file_write(300, rec.as_ptr(), 20), FS_OK);
        }

        assert_eq!(cobol_file_close(300), FS_OK);

        // Verify file content.
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "HELLO\nWORLD\n");

        // Read lines back.
        let status = unsafe {
            cobol_file_open(
                301,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::LineSequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                20,
            )
        };
        assert_eq!(status, FS_OK);

        let mut buf = [0u8; 20];
        unsafe {
            assert_eq!(cobol_file_read_next(301, buf.as_mut_ptr(), 20), FS_OK);
        }
        // "HELLO" left-justified, space-padded.
        assert_eq!(&buf[..5], b"HELLO");
        assert_eq!(&buf[5..], &[b' '; 15]);

        unsafe {
            assert_eq!(cobol_file_read_next(301, buf.as_mut_ptr(), 20), FS_OK);
        }
        assert_eq!(&buf[..5], b"WORLD");

        unsafe {
            assert_eq!(cobol_file_read_next(301, buf.as_mut_ptr(), 20), FS_AT_END);
        }

        assert_eq!(cobol_file_close(301), FS_OK);
    }

    #[test]
    fn test_file_not_found() {
        let path = b"/tmp/cobol_test_nonexistent_file_12345.dat";
        let status = unsafe {
            cobol_file_open(
                400,
                path.as_ptr(),
                path.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                80,
            )
        };
        assert_eq!(status, FS_NOT_FOUND);
    }

    #[test]
    fn test_trim_trailing_spaces() {
        assert_eq!(trim_trailing_spaces(b"HELLO     "), b"HELLO");
        assert_eq!(trim_trailing_spaces(b"HELLO"), b"HELLO");
        assert_eq!(trim_trailing_spaces(b"     "), b"");
        assert_eq!(trim_trailing_spaces(b""), b"");
    }

    #[test]
    fn test_relative_file_random_read() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("cobol_test_relative.dat");
        // Write 3 fixed-length records.
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"REC1xxxx").unwrap(); // record 1 (8 bytes)
            f.write_all(b"REC2xxxx").unwrap(); // record 2
            f.write_all(b"REC3xxxx").unwrap(); // record 3
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 600u32;
        let rec_len = 8u32;

        // Open as relative, sequential access, input.
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Relative,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                rec_len,
            )
        };
        assert_eq!(rc, FS_OK);

        // Read record 2 by key (big-endian 2).
        let key = 2u32.to_be_bytes();
        let mut buf = [0u8; 8];
        let rc = unsafe {
            cobol_file_read_key(
                fid,
                key.as_ptr(),
                key.len() as u32,
                buf.as_mut_ptr(),
                rec_len,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"REC2xxxx");

        let _ = cobol_file_close(fid);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_indexed_file_open_and_read_key() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("cobol_test_indexed.dat");
        // Write 3 records: each 10 bytes, key is first 3 bytes.
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"BBBrecord2").unwrap();
            f.write_all(b"AAArecord1").unwrap();
            f.write_all(b"CCCrecord3").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 601u32;
        let rec_len = 10u32;

        // Open as indexed with key at offset 0, length 3.
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Random,
                FileOpenMode::Input,
                rec_len,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);

        // Read by key "AAA".
        let key = b"AAA";
        let mut buf = [0u8; 10];
        let rc = unsafe {
            cobol_file_read_key(
                fid,
                key.as_ptr(),
                key.len() as u32,
                buf.as_mut_ptr(),
                rec_len,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"AAArecord1");

        // Read by key "CCC".
        let key = b"CCC";
        let rc = unsafe {
            cobol_file_read_key(
                fid,
                key.as_ptr(),
                key.len() as u32,
                buf.as_mut_ptr(),
                rec_len,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"CCCrecord3");

        // Read by nonexistent key.
        let key = b"ZZZ";
        let rc = unsafe {
            cobol_file_read_key(
                fid,
                key.as_ptr(),
                key.len() as u32,
                buf.as_mut_ptr(),
                rec_len,
            )
        };
        assert_eq!(rc, FS_REC_NOT_FOUND);

        let _ = cobol_file_close(fid);
        let _ = std::fs::remove_file(&path);
    }
}
