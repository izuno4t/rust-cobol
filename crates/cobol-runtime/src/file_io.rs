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
//   38 = file locked by CLOSE WITH LOCK
//   41 = file already open
//   42 = file not open
//   43 = sequential REWRITE without a valid preceding READ
//   44 = record length mismatch (rewrite)
//   46 = read error / no valid next record
//   47 = READ on file not opened INPUT or I-O
//   48 = WRITE on file not opened OUTPUT, I-O, or EXTEND
//   49 = REWRITE/DELETE on file not opened I-O
//
// All public functions use the C ABI.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Seek, SeekFrom, Write};
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
    path: String,
    org: FileOrganization,
    #[allow(dead_code)] // Used for access-mode validation in future expansions.
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
    variable_records: bool,
    current_record_len: u32,
    /// For relative files: current relative record number (0-based).
    current_record: u64,
    /// True only when the last positioning operation selected a concrete record.
    last_read_valid: bool,
    /// True after an AT END condition until another successful positioning operation.
    at_end_seen: bool,
    /// For indexed files: active index used by sequential READ NEXT.
    current_index: usize,
    /// Last concrete record offset read/positioned for indexed rewrite/delete.
    current_offset: Option<u64>,
    /// For indexed files: sorted indices for primary and alternate keys.
    indices: Vec<FileIndex>,
}

#[derive(Clone)]
struct FileIndex {
    key_offset: u32,
    key_len: u32,
    duplicates: bool,
    entries: Vec<(Vec<u8>, u64)>,
}

// Global file table -- lazily initialised.
static FILE_TABLE: Mutex<Option<HashMap<u32, CobolFile>>> = Mutex::new(None);
static LOCKED_PATHS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn file_debug_enabled() -> bool {
    std::env::var_os("COBOL_FILE_DEBUG").is_some()
}

fn file_debug_log(message: &str) {
    if file_debug_enabled() {
        eprintln!("[FILE] {message}");
    }
}

fn file_debug_preview(data: &[u8]) -> String {
    let preview_len = data.len().min(40);
    data[..preview_len]
        .iter()
        .map(|&b| match b {
            b' '..=b'~' => b as char,
            _ => '.',
        })
        .collect()
}

fn mark_at_end(file: &mut CobolFile) -> u32 {
    file.last_read_valid = false;
    if file.at_end_seen {
        FS_NO_VALID_NEXT_RECORD
    } else {
        file.at_end_seen = true;
        FS_AT_END
    }
}

fn with_file_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<u32, CobolFile>) -> R,
{
    let mut guard = FILE_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

fn with_locked_paths<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashSet<String>) -> R,
{
    let mut guard = LOCKED_PATHS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = guard.get_or_insert_with(HashSet::new);
    f(paths)
}

// ---------------------------------------------------------------------------
// File status constants
// ---------------------------------------------------------------------------

const FS_OK: u32 = 0; // "00"
const FS_DUPLICATE_ALT_SUCCESS: u32 = 2; // "02"
const FS_OPTIONAL_CREATED: u32 = 5;
const FS_AT_END: u32 = 10;
const FS_SEQUENCE_ERROR: u32 = 21;
const FS_DUPLICATE_KEY: u32 = 22;
const FS_REC_NOT_FOUND: u32 = 23;
const FS_IO_ERROR: u32 = 30;
const FS_NOT_FOUND: u32 = 35;
const FS_LOCKED: u32 = 38;
const FS_ALREADY_OPEN: u32 = 41;
const FS_NOT_OPEN: u32 = 42;
const FS_REWRITE_WITHOUT_READ: u32 = 43;
const FS_RECORD_LENGTH_MISMATCH: u32 = 44;
const FS_NO_VALID_NEXT_RECORD: u32 = 46;
const FS_READ_NOT_PERMITTED: u32 = 47;
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
    cobol_file_open_internal(FileOpenRequest {
        file_id,
        path_ptr,
        path_len,
        org,
        access,
        mode,
        record_len,
        optional: false,
    })
}

struct FileOpenRequest {
    file_id: u32,
    path_ptr: *const u8,
    path_len: u32,
    org: FileOrganization,
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
    optional: bool,
}

unsafe fn cobol_file_open_internal(req: FileOpenRequest) -> u32 {
    let FileOpenRequest {
        file_id,
        path_ptr,
        path_len,
        org,
        access,
        mode,
        record_len,
        optional,
    } = req;
    if path_ptr.is_null() || path_len == 0 {
        return FS_IO_ERROR;
    }
    let path_slice = std::slice::from_raw_parts(path_ptr, path_len as usize);
    let path = match std::str::from_utf8(path_slice) {
        Ok(s) => s.trim(),
        Err(_) => return FS_IO_ERROR,
    };
    if with_locked_paths(|paths| paths.contains(path)) {
        file_debug_log(&format!("open id={file_id} path={path} rc={FS_LOCKED}"));
        return FS_LOCKED;
    }

    with_file_table(|table| {
        if table.contains_key(&file_id) {
            file_debug_log(&format!(
                "open id={file_id} path={path} rc={FS_ALREADY_OPEN}"
            ));
            return FS_ALREADY_OPEN;
        }

        let result = match mode {
            FileOpenMode::Input => OpenOptions::new()
                .read(true)
                .open(path)
                .map(|f| CobolFileInner::Reader(BufReader::new(f))),
            FileOpenMode::Output if org == FileOrganization::Relative => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map(CobolFileInner::ReadWrite),
            FileOpenMode::Output => OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map(|f| CobolFileInner::Writer(BufWriter::new(f))),
            FileOpenMode::Extend => OpenOptions::new()
                .append(true)
                .open(path)
                .map(|f| CobolFileInner::Writer(BufWriter::new(f))),
            FileOpenMode::IoMode => OpenOptions::new()
                .read(true)
                .write(true)
                .create(false)
                .truncate(false)
                .open(path)
                .map(CobolFileInner::ReadWrite),
        };

        let result = match result {
            Ok(inner) => Ok((inner, FS_OK)),
            Err(e)
                if optional
                    && e.kind() == std::io::ErrorKind::NotFound
                    && matches!(
                        mode,
                        FileOpenMode::Input | FileOpenMode::IoMode | FileOpenMode::Extend
                    ) =>
            {
                let created = match mode {
                    FileOpenMode::Input => OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(path)
                        .map(CobolFileInner::ReadWrite),
                    FileOpenMode::IoMode => OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(false)
                        .open(path)
                        .map(CobolFileInner::ReadWrite),
                    FileOpenMode::Extend => OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(path)
                        .map(|f| CobolFileInner::Writer(BufWriter::new(f))),
                    _ => unreachable!(),
                };
                created.map(|inner| (inner, FS_OPTIONAL_CREATED))
            }
            Err(e) => Err(e),
        };

        match result {
            Ok((inner, rc)) => {
                table.insert(
                    file_id,
                    CobolFile {
                        inner,
                        path: path.to_string(),
                        org,
                        access,
                        mode,
                        record_len,
                        variable_records: false,
                        current_record_len: 0,
                        current_record: 0,
                        last_read_valid: false,
                        at_end_seen: false,
                        current_index: 0,
                        current_offset: None,
                        indices: Vec::new(),
                    },
                );
                file_debug_log(&format!(
                    "open id={file_id} path={path} mode={mode:?} org={org:?} optional={optional} rc={rc}"
                ));
                rc
            }
            Err(e) => {
                let rc = match e.kind() {
                    std::io::ErrorKind::NotFound => FS_NOT_FOUND,
                    std::io::ErrorKind::PermissionDenied => 37, // permission
                    _ => FS_IO_ERROR,
                };
                file_debug_log(&format!(
                    "open id={file_id} path={path} mode={mode:?} org={org:?} rc={rc} err={e}"
                ));
                rc
            }
        }
    })
}

/// Mark an open file as using runtime-framed variable-length records.
#[no_mangle]
pub extern "C" fn cobol_file_set_variable(file_id: u32) -> u32 {
    with_file_table(|table| {
        let Some(file) = table.get_mut(&file_id) else {
            return FS_NOT_OPEN;
        };
        file.variable_records = true;
        if file.org == FileOrganization::Indexed {
            rebuild_all_indices(file);
        }
        FS_OK
    })
}

/// Build the in-memory index for an indexed file by scanning all records.
///
/// Reads the file from the beginning, extracting the key from each record
/// at the given offset and length, and stores (key, file_offset) pairs
/// sorted by key.
fn build_index_entries(file: &mut CobolFile, key_offset: u32, key_len: u32) -> Vec<(Vec<u8>, u64)> {
    if file.org != FileOrganization::Indexed {
        return Vec::new();
    }
    let rec_len = file.record_len as usize;
    if rec_len == 0 {
        return Vec::new();
    }
    let key_off = key_offset as usize;
    let key_length = key_len as usize;
    if key_off + key_length > rec_len {
        return Vec::new();
    }

    let f = match &mut file.inner {
        CobolFileInner::Reader(r) => r.get_mut(),
        CobolFileInner::ReadWrite(f) => f,
        _ => return Vec::new(),
    };

    // Seek to the beginning.
    if f.seek(SeekFrom::Start(0)).is_err() {
        return Vec::new();
    }

    let mut offset = 0u64;
    let mut entries = Vec::new();

    if file.variable_records {
        let mut buf = vec![0u8; rec_len];
        loop {
            offset = match f.stream_position() {
                Ok(pos) => pos,
                Err(_) => break,
            };
            match read_variable_record(f, &mut buf) {
                Ok(Some(actual_len)) => {
                    if key_off + key_length <= actual_len as usize && !is_deleted_record(&buf) {
                        let key = buf[key_off..key_off + key_length].to_vec();
                        entries.push((key, offset));
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    } else {
        let mut buf = vec![0u8; rec_len];
        while f.read_exact(&mut buf).is_ok() {
            if !is_deleted_record(&buf) {
                let key = buf[key_off..key_off + key_length].to_vec();
                entries.push((key, offset));
            }
            offset += rec_len as u64;
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let _ = f.seek(SeekFrom::Start(0));
    entries
}

fn rebuild_all_indices(file: &mut CobolFile) {
    if file.org != FileOrganization::Indexed {
        return;
    }
    let defs: Vec<(u32, u32, bool)> = file
        .indices
        .iter()
        .map(|idx| (idx.key_offset, idx.key_len, idx.duplicates))
        .collect();
    let mut rebuilt = Vec::with_capacity(defs.len());
    for (key_offset, key_len, duplicates) in defs {
        rebuilt.push(FileIndex {
            key_offset,
            key_len,
            duplicates,
            entries: build_index_entries(file, key_offset, key_len),
        });
    }
    file.indices = rebuilt;
}

fn write_indexed_record(file: &mut CobolFile, data: &[u8], key_offset: u32, key_len: u32) -> u32 {
    let key_off = key_offset as usize;
    let key_length = key_len as usize;
    if key_off + key_length > data.len() {
        return FS_IO_ERROR;
    }
    let key = data[key_off..key_off + key_length].to_vec();

    if file.access == FileAccessMode::Sequential {
        if let Some((last_key, _)) = file.indices.first().and_then(|index| index.entries.last()) {
            if last_key.as_slice() >= key.as_slice() {
                return FS_SEQUENCE_ERROR;
            }
        }
    }

    for index in &file.indices {
        if index.duplicates {
            continue;
        }
        let key_off = index.key_offset as usize;
        let key_len = index.key_len as usize;
        if key_off + key_len > data.len() {
            return FS_IO_ERROR;
        }
        let candidate = data[key_off..key_off + key_len].to_vec();
        let pos = index
            .entries
            .partition_point(|(existing, _)| existing.as_slice() < candidate.as_slice());
        if pos < index.entries.len() && index.entries[pos].0 == candidate {
            return FS_DUPLICATE_KEY;
        }
    }

    let offset = match &mut file.inner {
        CobolFileInner::Writer(w) => {
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

    let write_result = match &mut file.inner {
        CobolFileInner::Writer(w) => {
            if file.variable_records {
                write_variable_record(w, data)
            } else {
                w.write_all(data)
            }
        }
        CobolFileInner::ReadWrite(f) => {
            if file.variable_records {
                write_variable_record(f, data)
            } else {
                f.write_all(data)
            }
        }
        _ => return FS_WRITE_NOT_PERMITTED,
    };

    match write_result {
        Ok(()) => {
            if file.indices.is_empty() {
                file.indices.push(FileIndex {
                    key_offset,
                    key_len,
                    duplicates: false,
                    entries: Vec::new(),
                });
            }
            for (idx_pos, index) in file.indices.iter_mut().enumerate() {
                let index_key = if idx_pos == 0 {
                    key.clone()
                } else {
                    let key_off = index.key_offset as usize;
                    let key_len = index.key_len as usize;
                    if key_off + key_len > data.len() {
                        return FS_IO_ERROR;
                    }
                    data[key_off..key_off + key_len].to_vec()
                };
                let insert_pos = if index.duplicates {
                    index.entries.partition_point(|(existing, _)| {
                        existing.as_slice() <= index_key.as_slice()
                    })
                } else {
                    index
                        .entries
                        .partition_point(|(existing, _)| existing.as_slice() < index_key.as_slice())
                };
                index.entries.insert(insert_pos, (index_key, offset));
            }
            file.current_record_len = data.len() as u32;
            file.current_record += 1;
            FS_OK
        }
        Err(_) => FS_IO_ERROR,
    }
}

fn compare_index_key_prefix(index_key: &[u8], probe_key: &[u8]) -> std::cmp::Ordering {
    let shared_len = index_key.len().min(probe_key.len());
    index_key[..shared_len].cmp(&probe_key[..shared_len])
}

fn select_index_position_for_key(file: &CobolFile, key_offset: u32, key_len: u32) -> Option<usize> {
    if key_offset != u32::MAX {
        file.indices
            .iter()
            .enumerate()
            .filter(|(_, index)| index.key_offset == key_offset && key_len <= index.key_len)
            .min_by_key(|(_, index)| index.key_len)
            .map(|(pos, _)| pos)
    } else {
        file.indices
            .iter()
            .enumerate()
            .filter(|(_, index)| key_len <= index.key_len)
            .min_by_key(|(_, index)| index.key_len)
            .map(|(pos, _)| pos)
    }
}

fn find_index_window(index: &FileIndex, key: &[u8]) -> (usize, usize) {
    let lower = index
        .entries
        .partition_point(|(candidate, _)| compare_index_key_prefix(candidate, key).is_lt());
    let upper = index
        .entries
        .partition_point(|(candidate, _)| !compare_index_key_prefix(candidate, key).is_gt());
    (lower, upper)
}

fn indexed_success_status(index: &FileIndex, pos: usize) -> u32 {
    if !index.duplicates {
        return FS_OK;
    }
    let Some((key, _)) = index.entries.get(pos) else {
        return FS_OK;
    };
    let has_previous_duplicate = pos > 0
        && index
            .entries
            .get(pos - 1)
            .is_some_and(|(previous, _)| previous == key);
    let has_next_duplicate = index
        .entries
        .get(pos + 1)
        .is_some_and(|(next, _)| next == key);
    if has_previous_duplicate || has_next_duplicate {
        FS_DUPLICATE_ALT_SUCCESS
    } else {
        FS_OK
    }
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
    cobol_file_open_indexed_optional(
        file_id, path_ptr, path_len, access, mode, record_len, key_offset, key_len, 0,
    )
}

/// Open an indexed file, optionally treating a missing INPUT/I-O file as empty.
///
/// # Safety
/// `path_ptr` must point to a valid UTF-8 string of `path_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_open_indexed_optional(
    file_id: u32,
    path_ptr: *const u8,
    path_len: u32,
    access: FileAccessMode,
    mode: FileOpenMode,
    record_len: u32,
    key_offset: u32,
    key_len: u32,
    optional: u32,
) -> u32 {
    let rc = cobol_file_open_internal(FileOpenRequest {
        file_id,
        path_ptr,
        path_len,
        org: FileOrganization::Indexed,
        access,
        mode,
        record_len,
        optional: optional != 0,
    });
    if rc != FS_OK && rc != FS_OPTIONAL_CREATED {
        return rc;
    }

    with_file_table(|table| {
        if let Some(file) = table.get_mut(&file_id) {
            file.indices = vec![FileIndex {
                key_offset,
                key_len,
                duplicates: false,
                entries: build_index_entries(file, key_offset, key_len),
            }];
        }
    });

    rc
}

#[no_mangle]
pub extern "C" fn cobol_file_add_alternate_index(
    file_id: u32,
    key_offset: u32,
    key_len: u32,
    duplicates: u32,
) -> u32 {
    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };
        if file.org != FileOrganization::Indexed {
            return FS_IO_ERROR;
        }
        let entries = build_index_entries(file, key_offset, key_len);
        file.indices.push(FileIndex {
            key_offset,
            key_len,
            duplicates: duplicates != 0,
            entries,
        });
        FS_OK
    })
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
            file_debug_log(&format!("close id={file_id} rc={FS_OK}"));
            FS_OK
        } else {
            file_debug_log(&format!("close id={file_id} rc={FS_NOT_OPEN}"));
            FS_NOT_OPEN
        }
    })
}

/// Close a file and prevent later OPEN attempts for the same path in this process.
#[no_mangle]
pub extern "C" fn cobol_file_close_with_lock(file_id: u32) -> u32 {
    let closed_path = with_file_table(|table| {
        if let Some(mut f) = table.remove(&file_id) {
            if let CobolFileInner::Writer(ref mut w) = f.inner {
                let _ = w.flush();
            }
            Some(f.path)
        } else {
            None
        }
    });

    if let Some(path) = closed_path {
        with_locked_paths(|paths| {
            paths.insert(path.clone());
        });
        file_debug_log(&format!(
            "close-with-lock id={file_id} path={path} rc={FS_OK}"
        ));
        FS_OK
    } else {
        file_debug_log(&format!("close-with-lock id={file_id} rc={FS_NOT_OPEN}"));
        FS_NOT_OPEN
    }
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
            None => {
                file_debug_log(&format!(
                    "read-next id={file_id} len={record_len} rc={FS_READ_NOT_PERMITTED}"
                ));
                return FS_READ_NOT_PERMITTED;
            }
        };

        match file.mode {
            FileOpenMode::Input | FileOpenMode::IoMode => {}
            _ => return FS_READ_NOT_PERMITTED,
        }

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
                            return mark_at_end(file);
                        }
                        let bytes = line.as_bytes();
                        file.current_record_len = bytes.len() as u32;
                        let copy_len = bytes.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                        for b in buf[copy_len..].iter_mut() {
                            *b = b' ';
                        }
                        file.current_record += 1;
                        file.last_read_valid = true;
                        file.at_end_seen = false;
                        file_debug_log(&format!(
                            "read-next id={file_id} org=line-sequential rec={} rc={FS_OK} preview={:?}",
                            file.current_record,
                            file_debug_preview(buf)
                        ));
                        return FS_OK;
                    }
                    _ => return FS_READ_NOT_PERMITTED,
                };

                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => mark_at_end(file),
                    Ok(_) => {
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        let bytes = trimmed.as_bytes();
                        file.current_record_len = bytes.len() as u32;
                        let copy_len = bytes.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                        for b in buf[copy_len..].iter_mut() {
                            *b = b' ';
                        }
                        file.current_record += 1;
                        file.last_read_valid = true;
                        file.at_end_seen = false;
                        file_debug_log(&format!(
                            "read-next id={file_id} org=line-sequential rec={} rc={FS_OK} preview={:?}",
                            file.current_record,
                            file_debug_preview(buf)
                        ));
                        FS_OK
                    }
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Sequential => {
                let variable_records = file.variable_records;
                let reader = match &mut file.inner {
                    CobolFileInner::Reader(r) => r as &mut dyn Read,
                    CobolFileInner::ReadWrite(f) => f as &mut dyn Read,
                    _ => return FS_READ_NOT_PERMITTED,
                };

                let read_result = if variable_records {
                    read_variable_record(reader, buf)
                } else {
                    reader.read_exact(buf).map(|()| Some(record_len))
                };

                match read_result {
                    Ok(None) => mark_at_end(file),
                    Ok(Some(actual_len)) => {
                        file.current_record_len = actual_len;
                        file.current_record += 1;
                        file.last_read_valid = true;
                        file.at_end_seen = false;
                        file_debug_log(&format!(
                            "read-next id={file_id} org=sequential rec={} rc={FS_OK} preview={:?}",
                            file.current_record,
                            file_debug_preview(buf)
                        ));
                        FS_OK
                    }
                    Err(e) if e.kind() == ErrorKind::UnexpectedEof => mark_at_end(file),
                    Err(_) => FS_IO_ERROR,
                }
            }
            FileOrganization::Relative => {
                // Sequential read of relative file skips deleted slots.
                let variable_records = file.variable_records;
                let reader = match &mut file.inner {
                    CobolFileInner::Reader(r) => r as &mut dyn Read,
                    CobolFileInner::ReadWrite(f) => f as &mut dyn Read,
                    _ => return FS_READ_NOT_PERMITTED,
                };

                loop {
                    let read_result = if variable_records {
                        read_variable_record(reader, buf)
                    } else {
                        reader.read_exact(buf).map(|()| Some(record_len))
                    };
                    match read_result {
                        Ok(None) => return mark_at_end(file),
                        Ok(Some(actual_len)) => {
                            file.current_record_len = actual_len;
                            file.current_record += 1;
                            file.last_read_valid = true;
                            file.at_end_seen = false;
                            if is_deleted_record(buf) {
                                file_debug_log(&format!(
                                    "read-next id={file_id} org=relative rec={} rc=deleted-skip",
                                    file.current_record
                                ));
                                continue;
                            }
                            file_debug_log(&format!(
                                "read-next id={file_id} org=relative rec={} rc={FS_OK} preview={:?}",
                                file.current_record,
                                file_debug_preview(buf)
                            ));
                            return FS_OK;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            return mark_at_end(file)
                        }
                        Err(_) => return FS_IO_ERROR,
                    }
                }
            }
            FileOrganization::Indexed => {
                // Sequential read through index order.
                let idx = file.current_record as usize;
                let active = match file.indices.get(file.current_index) {
                    Some(index) => index,
                    None => return FS_REC_NOT_FOUND,
                };
                if idx >= active.entries.len() {
                    return mark_at_end(file);
                }
                let offset = active.entries[idx].1;
                let success_status = indexed_success_status(active, idx);

                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_READ_NOT_PERMITTED,
                };

                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_IO_ERROR;
                }
                let read_result = if file.variable_records {
                    read_variable_record(f, buf)
                } else {
                    f.read_exact(buf).map(|()| Some(record_len))
                };
                match read_result {
                    Ok(Some(actual_len)) => {
                        file.current_record_len = actual_len;
                        file.current_record += 1;
                        file.last_read_valid = true;
                        file.at_end_seen = false;
                        file.current_offset = Some(offset);
                        file_debug_log(&format!(
                            "read-next id={file_id} org=indexed active_index={} idx={} rc={} preview={:?}",
                            file.current_index,
                            file.current_record,
                            success_status,
                            file_debug_preview(buf)
                        ));
                        success_status
                    }
                    Ok(None) => mark_at_end(file),
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
    key_offset: u32,
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
            None => return FS_READ_NOT_PERMITTED,
        };

        match file.mode {
            FileOpenMode::Input | FileOpenMode::IoMode => {}
            _ => return FS_READ_NOT_PERMITTED,
        }

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
                        file.current_record_len = record_len;
                        if is_deleted_record(buf) {
                            file.last_read_valid = false;
                            return FS_REC_NOT_FOUND;
                        }
                        file.current_record = rec_num;
                        file.last_read_valid = true;
                        file.at_end_seen = false;
                        FS_OK
                    }
                    Err(_) => {
                        file.last_read_valid = false;
                        FS_REC_NOT_FOUND
                    }
                }
            }
            FileOrganization::Indexed => {
                let Some(index_pos) = select_index_position_for_key(file, key_offset, key_len)
                else {
                    file_debug_log(&format!(
                        "read-key id={file_id} key_offset={key_offset} key_len={key_len} selected_index=none rc={FS_REC_NOT_FOUND}"
                    ));
                    return FS_REC_NOT_FOUND;
                };
                let index = &file.indices[index_pos];
                let (lower, upper) = find_index_window(index, key);
                file_debug_log(&format!(
                    "read-key id={file_id} key_offset={key_offset} key_len={key_len} selected_index={} selected_offset={} selected_len={} lower={} upper={} probe={:?}",
                    index_pos,
                    index.key_offset,
                    index.key_len,
                    lower,
                    upper,
                    file_debug_preview(key)
                ));
                if lower >= upper {
                    file_debug_log(&format!(
                        "read-key id={file_id} key_offset={key_offset} key_len={key_len} rc={FS_REC_NOT_FOUND}"
                    ));
                    file.last_read_valid = false;
                    return FS_REC_NOT_FOUND;
                }
                let matched_offset = index.entries[lower].1;
                let success_status = indexed_success_status(index, lower);
                file.current_index = index_pos;
                // READ by key establishes the current record and advances the
                // sequential cursor to the following entry in the same key order.
                file.current_record = (lower + 1) as u64;
                file.last_read_valid = true;
                file.at_end_seen = false;
                file.current_offset = Some(matched_offset);
                let offset = matched_offset;
                let f = match &mut file.inner {
                    CobolFileInner::Reader(r) => r.get_mut(),
                    CobolFileInner::ReadWrite(f) => f,
                    _ => return FS_IO_ERROR,
                };
                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_IO_ERROR;
                }
                let read_result = if file.variable_records {
                    read_variable_record(f, buf)
                } else {
                    f.read_exact(buf).map(|()| Some(record_len))
                };
                match read_result {
                    Ok(Some(actual_len)) => {
                        file.current_record_len = actual_len;
                        file_debug_log(&format!(
                            "read-key id={file_id} matched_offset={} current_record={} rc={} preview={:?}",
                            offset,
                            file.current_record,
                            success_status,
                            file_debug_preview(buf)
                        ));
                        success_status
                    }
                    Ok(None) => FS_REC_NOT_FOUND,
                    Err(_) => FS_IO_ERROR,
                }
            }
            _ => FS_IO_ERROR, // Random read not supported for sequential files.
        }
    })
}

/// Read a relative record by its 1-based relative record number.
///
/// # Safety
/// `record_ptr` must be writable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_read_relative(
    file_id: u32,
    rec_num: u64,
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
            None => return FS_READ_NOT_PERMITTED,
        };
        read_relative_record(file, rec_num, buf, record_len)
    })
}

fn read_relative_record(
    file: &mut CobolFile,
    rec_num: u64,
    buf: &mut [u8],
    record_len: u32,
) -> u32 {
    match file.mode {
        FileOpenMode::Input | FileOpenMode::IoMode => {}
        _ => return FS_READ_NOT_PERMITTED,
    }
    if file.org != FileOrganization::Relative {
        return FS_IO_ERROR;
    }
    if rec_num == 0 {
        return FS_REC_NOT_FOUND;
    }
    let offset = (rec_num - 1) * (file.record_len as u64);
    let read_result = match &mut file.inner {
        CobolFileInner::Reader(r) => r
            .seek(SeekFrom::Start(offset))
            .and_then(|_| r.read_exact(buf)),
        CobolFileInner::ReadWrite(f) => f
            .seek(SeekFrom::Start(offset))
            .and_then(|_| f.read_exact(buf)),
        _ => return FS_IO_ERROR,
    };
    match read_result {
        Ok(()) => {
            file.current_record_len = record_len;
            if is_deleted_record(buf) {
                file.last_read_valid = false;
                return FS_REC_NOT_FOUND;
            }
            file.current_record = rec_num;
            file.last_read_valid = true;
            file.at_end_seen = false;
            FS_OK
        }
        Err(_) => {
            file.last_read_valid = false;
            FS_REC_NOT_FOUND
        }
    }
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
    let raw_data = std::slice::from_raw_parts(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_WRITE_NOT_PERMITTED,
        };

        match file.mode {
            FileOpenMode::Output | FileOpenMode::Extend => {}
            FileOpenMode::IoMode
                if !matches!(
                    file.org,
                    FileOrganization::Sequential | FileOrganization::LineSequential
                ) => {}
            _ => {
                file_debug_log(&format!(
                    "write id={file_id} len={record_len} rc={FS_WRITE_NOT_PERMITTED}"
                ));
                return FS_WRITE_NOT_PERMITTED;
            }
        }

        let data = raw_data;

        if file.org == FileOrganization::Indexed {
            let Some((key_offset, key_len)) = file
                .indices
                .first()
                .map(|index| (index.key_offset, index.key_len))
            else {
                return FS_IO_ERROR;
            };
            return write_indexed_record(file, data, key_offset, key_len);
        }

        let write_result = match &mut file.inner {
            CobolFileInner::Writer(w) => {
                if file.org == FileOrganization::LineSequential {
                    // Trim trailing spaces and write as a line.
                    let trimmed = trim_trailing_spaces(data);
                    w.write_all(trimmed)
                        .and_then(|()| w.write_all(b"\n"))
                        .and_then(|()| w.flush())
                } else if matches!(
                    file.org,
                    FileOrganization::Sequential
                        | FileOrganization::Relative
                        | FileOrganization::Indexed
                ) && file.variable_records
                {
                    write_variable_record(w, data)
                } else {
                    w.write_all(data)
                }
            }
            CobolFileInner::ReadWrite(f) => {
                if file.org == FileOrganization::LineSequential {
                    let trimmed = trim_trailing_spaces(data);
                    f.write_all(trimmed).and_then(|()| f.write_all(b"\n"))
                } else if matches!(
                    file.org,
                    FileOrganization::Sequential
                        | FileOrganization::Relative
                        | FileOrganization::Indexed
                ) && file.variable_records
                {
                    write_variable_record(f, data)
                } else {
                    f.write_all(data)
                }
            }
            _ => return FS_WRITE_NOT_PERMITTED,
        };

        match write_result {
            Ok(()) => {
                file.current_record_len = record_len;
                file.current_record += 1;
                file_debug_log(&format!(
                    "write id={file_id} len={record_len} rc={FS_OK} preview={:?}",
                    file_debug_preview(data)
                ));
                FS_OK
            }
            Err(err) => {
                file_debug_log(&format!(
                    "write id={file_id} len={record_len} rc={FS_IO_ERROR} err={err}"
                ));
                FS_IO_ERROR
            }
        }
    })
}

/// Write a relative record by its 1-based relative record number.
///
/// # Safety
/// `record_ptr` must be readable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_write_relative(
    file_id: u32,
    rec_num: u64,
    record_ptr: *const u8,
    record_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        file_debug_log(&format!(
            "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_IO_ERROR} invalid-args"
        ));
        return FS_IO_ERROR;
    }
    let data = std::slice::from_raw_parts(record_ptr, record_len as usize);
    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => {
                file_debug_log(&format!(
                    "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_WRITE_NOT_PERMITTED} not-open"
                ));
                return FS_WRITE_NOT_PERMITTED;
            }
        };
        if file.org != FileOrganization::Relative {
            file_debug_log(&format!(
                "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_IO_ERROR} org={:?}",
                file.org
            ));
            return FS_IO_ERROR;
        }
        match file.mode {
            FileOpenMode::Output | FileOpenMode::IoMode => {}
            _ => {
                file_debug_log(&format!(
                    "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_WRITE_NOT_PERMITTED} mode={:?}",
                    file.mode
                ));
                return FS_WRITE_NOT_PERMITTED;
            }
        }
        if rec_num == 0 {
            file_debug_log(&format!(
                "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_REC_NOT_FOUND} zero-key"
            ));
            return FS_REC_NOT_FOUND;
        }
        let offset = (rec_num - 1) * (file.record_len as u64);
        let f = match &mut file.inner {
            CobolFileInner::Writer(w) => w.get_mut(),
            CobolFileInner::ReadWrite(f) => f,
            _ => {
                file_debug_log(&format!(
                    "write-relative id={file_id} rec={rec_num} len={record_len} rc={FS_WRITE_NOT_PERMITTED} inner"
                ));
                return FS_WRITE_NOT_PERMITTED;
            }
        };
        if let Ok(end_pos) = f.seek(SeekFrom::End(0)) {
            if offset < end_pos {
                if f.seek(SeekFrom::Start(offset)).is_err() {
                    return FS_IO_ERROR;
                }
                let mut existing = vec![0u8; file.record_len as usize];
                if f.read_exact(&mut existing).is_ok() && !is_deleted_record(&existing) {
                    file_debug_log(&format!(
                        "write-relative id={file_id} rec={rec_num} len={record_len} offset={offset} rc={FS_DUPLICATE_KEY} duplicate"
                    ));
                    return FS_DUPLICATE_KEY;
                }
            }
        }
        if let Err(err) = f.seek(SeekFrom::Start(offset)) {
            file_debug_log(&format!(
                "write-relative id={file_id} rec={rec_num} len={record_len} offset={offset} rc={FS_IO_ERROR} seek-err={err}"
            ));
            return FS_IO_ERROR;
        }
        match f.write_all(data) {
            Ok(()) => {
                file.current_record = rec_num;
                file.current_record_len = record_len;
                file.last_read_valid = false;
                file_debug_log(&format!(
                    "write-relative id={file_id} rec={rec_num} len={record_len} offset={offset} rc={FS_OK}"
                ));
                FS_OK
            }
            Err(err) => {
                file_debug_log(&format!(
                    "write-relative id={file_id} rec={rec_num} len={record_len} offset={offset} rc={FS_IO_ERROR} write-err={err}"
                ));
                FS_IO_ERROR
            }
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

        write_indexed_record(file, data, key_offset, key_len)
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
            None => return FS_IO_MODE_REQUIRED,
        };

        if file.mode != FileOpenMode::IoMode {
            return FS_IO_MODE_REQUIRED;
        }
        if !file.last_read_valid {
            return FS_REWRITE_WITHOUT_READ;
        }
        if record_len != file.record_len {
            return FS_RECORD_LENGTH_MISMATCH;
        }

        let rec_offset = match file.org {
            FileOrganization::Indexed => {
                let Some(primary) = file.indices.first() else {
                    return FS_REC_NOT_FOUND;
                };
                let key_off = primary.key_offset as usize;
                let key_len = primary.key_len as usize;
                if key_off + key_len > data.len() {
                    return FS_IO_ERROR;
                }
                let key = &data[key_off..key_off + key_len];
                let (lower, upper) = find_index_window(primary, key);
                if lower >= upper {
                    return FS_REC_NOT_FOUND;
                }
                primary.entries[lower].1
            }
            _ => (file.current_record.saturating_sub(1)) * (file.record_len as u64),
        };

        let f = match &mut file.inner {
            CobolFileInner::ReadWrite(f) => f,
            _ => return FS_IO_MODE_REQUIRED,
        };

        if file.org == FileOrganization::Indexed {
            let current_offset = rec_offset;
            for index in &file.indices {
                if index.duplicates {
                    continue;
                }
                let key_off = index.key_offset as usize;
                let key_len = index.key_len as usize;
                let key = data[key_off..key_off + key_len].to_vec();
                let pos = index
                    .entries
                    .partition_point(|(k, _)| k.as_slice() < key.as_slice());
                if pos < index.entries.len()
                    && index.entries[pos].0 == key
                    && index.entries[pos].1 != current_offset
                {
                    return FS_DUPLICATE_KEY;
                }
            }
        }

        if f.seek(SeekFrom::Start(rec_offset)).is_err() {
            return FS_IO_ERROR;
        }

        match f.write_all(data) {
            Ok(()) => {
                if file.org == FileOrganization::Indexed {
                    let active_index = file.current_index;
                    let rewritten_offset = rec_offset;
                    let next_offset = file
                        .indices
                        .get(active_index)
                        .and_then(|index| index.entries.get(file.current_record as usize))
                        .map(|(_, offset)| *offset);
                    rebuild_all_indices(file);
                    for index in &mut file.indices {
                        if !index.duplicates {
                            continue;
                        }
                        let key_off = index.key_offset as usize;
                        let key_len = index.key_len as usize;
                        if key_off + key_len > data.len() {
                            continue;
                        }
                        let key = data[key_off..key_off + key_len].to_vec();
                        if let Some(pos) = index
                            .entries
                            .iter()
                            .position(|(_, offset)| *offset == rewritten_offset)
                        {
                            index.entries.remove(pos);
                            let insert_pos = index.entries.partition_point(|(existing, _)| {
                                existing.as_slice() <= key.as_slice()
                            });
                            index.entries.insert(insert_pos, (key, rewritten_offset));
                        }
                    }
                    if let Some(index) = file.indices.get(active_index) {
                        file.current_record = next_offset
                            .and_then(|offset| {
                                index
                                    .entries
                                    .iter()
                                    .position(|(_, entry_offset)| *entry_offset == offset)
                            })
                            .map(|pos| pos as u64)
                            .unwrap_or_else(|| file.current_record.min(index.entries.len() as u64));
                    }
                }
                FS_OK
            }
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
            None => return FS_IO_MODE_REQUIRED,
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
                let offset = match file.current_offset {
                    Some(offset) => offset,
                    None => return FS_REC_NOT_FOUND,
                };
                delete_indexed_at_offset(file, offset)
            }
            _ => FS_IO_ERROR, // DELETE not meaningful for sequential.
        }
    })
}

/// Delete a relative record by its 1-based relative record number.
#[no_mangle]
pub extern "C" fn cobol_file_delete_relative(file_id: u32, rec_num: u64) -> u32 {
    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_IO_MODE_REQUIRED,
        };
        if file.mode != FileOpenMode::IoMode {
            return FS_IO_MODE_REQUIRED;
        }
        if file.org != FileOrganization::Relative {
            return FS_IO_ERROR;
        }
        if rec_num == 0 {
            return FS_REC_NOT_FOUND;
        }
        let f = match &mut file.inner {
            CobolFileInner::ReadWrite(f) => f,
            _ => return FS_IO_MODE_REQUIRED,
        };
        let rec_offset = (rec_num - 1) * (file.record_len as u64);
        if f.seek(SeekFrom::Start(rec_offset)).is_err() {
            return FS_IO_ERROR;
        }
        let zeros = vec![0u8; file.record_len as usize];
        match f.write_all(&zeros) {
            Ok(()) => {
                file.current_record = rec_num;
                file.last_read_valid = false;
                FS_OK
            }
            Err(_) => FS_IO_ERROR,
        }
    })
}

/// Delete an indexed record identified by the primary key in the current record area.
///
/// # Safety
/// `record_ptr` must be readable for `record_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cobol_file_delete_record(
    file_id: u32,
    record_ptr: *const u8,
    record_len: u32,
) -> u32 {
    if record_ptr.is_null() || record_len == 0 {
        return FS_IO_ERROR;
    }
    let record = std::slice::from_raw_parts(record_ptr, record_len as usize);

    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_IO_MODE_REQUIRED,
        };

        if file.mode != FileOpenMode::IoMode {
            return FS_IO_MODE_REQUIRED;
        }
        if file.org != FileOrganization::Indexed {
            return FS_IO_ERROR;
        }

        let Some(primary) = file.indices.first() else {
            return FS_REC_NOT_FOUND;
        };
        let key_offset = primary.key_offset as usize;
        let key_len = primary.key_len as usize;
        if key_offset + key_len > record.len() {
            return FS_IO_ERROR;
        }
        let key = &record[key_offset..key_offset + key_len];
        let (lower, upper) = find_index_window(primary, key);
        if lower >= upper {
            file.last_read_valid = false;
            return FS_REC_NOT_FOUND;
        }
        let offset = primary.entries[lower].1;
        delete_indexed_at_offset(file, offset)
    })
}

fn delete_indexed_at_offset(file: &mut CobolFile, offset: u64) -> u32 {
    let active_index = file.current_index;
    let deleted_position = file
        .indices
        .get(active_index)
        .and_then(|index| {
            index
                .entries
                .iter()
                .position(|(_, entry_offset)| *entry_offset == offset)
        })
        .map(|pos| pos as u64);
    let f = match &mut file.inner {
        CobolFileInner::ReadWrite(f) => f,
        _ => return FS_IO_MODE_REQUIRED,
    };
    if f.seek(SeekFrom::Start(offset)).is_err() {
        return FS_IO_ERROR;
    }
    let zeros = vec![0u8; file.record_len as usize];
    match f.write_all(&zeros) {
        Ok(()) => {
            rebuild_all_indices(file);
            if let Some(index) = file.indices.get(active_index) {
                file.current_record = deleted_position
                    .map(|pos| pos.min(index.entries.len() as u64))
                    .unwrap_or_else(|| file.current_record.min(index.entries.len() as u64));
            }
            file.current_offset = None;
            file.last_read_valid = false;
            FS_OK
        }
        Err(_) => FS_IO_ERROR,
    }
}

#[no_mangle]
pub extern "C" fn cobol_file_current_record(file_id: u32) -> u64 {
    with_file_table(|table| {
        table
            .get(&file_id)
            .map(|file| file.current_record)
            .unwrap_or(0)
    })
}

#[no_mangle]
pub extern "C" fn cobol_file_current_record_length(file_id: u32) -> u32 {
    with_file_table(|table| {
        table
            .get(&file_id)
            .map(|file| file.current_record_len)
            .unwrap_or(0)
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
    key_offset: u32,
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
                if rec_num == 0 {
                    return FS_REC_NOT_FOUND;
                }
                position_relative_start(file, rec_num, mode)
            }
            FileOrganization::Indexed => {
                let mut target_offset = None;
                let index_pos = select_index_position_for_key(file, key_offset, key_len);
                if let Some(index_pos) = index_pos {
                    let index = &file.indices[index_pos];
                    let (lower, upper) = find_index_window(index, key);
                    file_debug_log(&format!(
                        "start id={file_id} mode={mode} key_offset={key_offset} key_len={key_len} selected_index={} selected_offset={} selected_len={} lower={} upper={} entries={}",
                        index_pos,
                        index.key_offset,
                        index.key_len,
                        lower,
                        upper,
                        index.entries.len()
                    ));
                    let target = match mode {
                        0 => (lower < upper).then_some(lower),
                        1 => (upper < index.entries.len()).then_some(upper),
                        2 => (lower < index.entries.len()).then_some(lower),
                        3 => (lower > 0).then_some(lower - 1),
                        4 => (upper > 0).then_some(upper - 1),
                        _ => None,
                    };
                    if let Some(idx) = target {
                        file.current_index = index_pos;
                        file.current_record = idx as u64;
                        file.current_offset = None;
                        file_debug_log(&format!(
                            "start id={file_id} mode={mode} target_idx={idx} target_key={:?} target_offset={}",
                            file_debug_preview(&index.entries[idx].0),
                            index.entries[idx].1
                        ));
                        target_offset = Some(index.entries[idx].1);
                    }
                } else {
                    file_debug_log(&format!(
                        "start id={file_id} mode={mode} key_offset={key_offset} key_len={key_len} selected_index=none"
                    ));
                }

                match target_offset {
                    Some(_) => FS_OK,
                    None => FS_REC_NOT_FOUND,
                }
            }
            _ => FS_IO_ERROR,
        }
    })
}

/// START for relative files using a numeric 1-based relative record number.
#[no_mangle]
pub extern "C" fn cobol_file_start_relative(file_id: u32, rec_num: u64, mode: u32) -> u32 {
    with_file_table(|table| {
        let file = match table.get_mut(&file_id) {
            Some(f) => f,
            None => return FS_NOT_OPEN,
        };
        if file.org != FileOrganization::Relative {
            return FS_IO_ERROR;
        }
        if rec_num == 0 {
            return FS_REC_NOT_FOUND;
        }
        position_relative_start(file, rec_num, mode)
    })
}

fn position_relative_start(file: &mut CobolFile, rec_num: u64, mode: u32) -> u32 {
    let max_record = match relative_max_record(file) {
        Some(max_record) => max_record,
        None => return FS_IO_ERROR,
    };
    if max_record == 0 {
        return FS_REC_NOT_FOUND;
    }

    let found = match mode {
        0 => relative_record_is_present(file, rec_num).then_some(rec_num),
        1 => find_present_relative_forward(file, rec_num.saturating_add(1), max_record),
        2 => find_present_relative_forward(file, rec_num, max_record),
        3 => find_present_relative_backward(file, rec_num.saturating_sub(1), max_record),
        4 => find_present_relative_backward(file, rec_num, max_record),
        _ => relative_record_is_present(file, rec_num).then_some(rec_num),
    };

    let Some(target) = found else {
        return FS_REC_NOT_FOUND;
    };
    let offset = (target - 1) * (file.record_len as u64);
    let seek_result = match &mut file.inner {
        CobolFileInner::Reader(r) => r.seek(SeekFrom::Start(offset)),
        CobolFileInner::ReadWrite(f) => f.seek(SeekFrom::Start(offset)),
        _ => return FS_IO_ERROR,
    };
    if seek_result.is_err() {
        return FS_REC_NOT_FOUND;
    }
    file.current_record = target - 1;
    file.last_read_valid = false;
    file.at_end_seen = false;
    FS_OK
}

fn relative_max_record(file: &mut CobolFile) -> Option<u64> {
    if file.record_len == 0 {
        return None;
    }
    let len = match &mut file.inner {
        CobolFileInner::Reader(r) => r.get_ref().metadata().ok()?.len(),
        CobolFileInner::ReadWrite(f) => f.metadata().ok()?.len(),
        CobolFileInner::Writer(_) => return None,
    };
    Some(len / file.record_len as u64)
}

fn find_present_relative_forward(file: &mut CobolFile, start: u64, max_record: u64) -> Option<u64> {
    if start == 0 || start > max_record {
        return None;
    }
    (start..=max_record).find(|&rec| relative_record_is_present(file, rec))
}

fn find_present_relative_backward(
    file: &mut CobolFile,
    start: u64,
    max_record: u64,
) -> Option<u64> {
    let mut rec = start.min(max_record);
    while rec > 0 {
        if relative_record_is_present(file, rec) {
            return Some(rec);
        }
        rec -= 1;
    }
    None
}

fn relative_record_is_present(file: &mut CobolFile, rec_num: u64) -> bool {
    if rec_num == 0 || file.record_len == 0 {
        return false;
    }
    let offset = (rec_num - 1) * (file.record_len as u64);
    let mut probe = vec![0u8; file.record_len as usize];
    let read_result = match &mut file.inner {
        CobolFileInner::Reader(r) => r
            .seek(SeekFrom::Start(offset))
            .and_then(|_| r.read_exact(&mut probe))
            .and_then(|_| r.seek(SeekFrom::Start(offset)).map(|_| ())),
        CobolFileInner::ReadWrite(f) => f
            .seek(SeekFrom::Start(offset))
            .and_then(|_| f.read_exact(&mut probe))
            .and_then(|_| f.seek(SeekFrom::Start(offset)).map(|_| ())),
        _ => return false,
    };
    read_result.is_ok() && !is_deleted_record(&probe)
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

fn write_variable_record(writer: &mut dyn Write, data: &[u8]) -> std::io::Result<()> {
    let len = data.len() as u32;
    writer
        .write_all(&len.to_le_bytes())
        .and_then(|()| writer.write_all(data))
}

fn read_variable_record(reader: &mut dyn Read, buf: &mut [u8]) -> std::io::Result<Option<u32>> {
    let mut len_bytes = [0u8; 4];
    let mut read_len = 0usize;
    while read_len < len_bytes.len() {
        match reader.read(&mut len_bytes[read_len..]) {
            Ok(0) if read_len == 0 => return Ok(None),
            Ok(0) => return Err(std::io::Error::from(ErrorKind::UnexpectedEof)),
            Ok(n) => read_len += n,
            Err(e) => return Err(e),
        }
    }

    let actual_len = u32::from_le_bytes(len_bytes) as usize;
    let mut record = vec![0u8; actual_len];
    reader.read_exact(&mut record)?;

    let copy_len = actual_len.min(buf.len());
    buf[..copy_len].copy_from_slice(&record[..copy_len]);
    for b in buf[copy_len..].iter_mut() {
        *b = b' ';
    }
    Ok(Some(actual_len as u32))
}

fn is_deleted_record(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 0)
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
    fn test_close_with_lock_blocks_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("close-with-lock.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open(
                101,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                80,
            )
        };
        assert_eq!(status, FS_OK);

        assert_eq!(cobol_file_close_with_lock(101), FS_OK);

        let reopen_status = unsafe {
            cobol_file_open(
                102,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                80,
            )
        };
        assert_eq!(reopen_status, FS_LOCKED);
    }

    #[test]
    fn test_extend_missing_file_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-extend.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open(
                103,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Extend,
                80,
            )
        };
        assert_eq!(status, FS_NOT_FOUND);
        assert!(!path.exists());
    }

    #[test]
    fn test_optional_indexed_io_missing_file_creates_with_status_05() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("optional-indexed.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open_indexed_optional(
                104,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                16,
                0,
                4,
                1,
            )
        };
        assert_eq!(status, FS_OPTIONAL_CREATED);
        assert!(path.exists());

        let record = *b"KEY1indexed-data";
        let write_status =
            unsafe { cobol_file_write_indexed(104, record.as_ptr(), record.len() as u32, 0, 4) };
        assert_eq!(write_status, FS_OK);
        assert_eq!(cobol_file_close(104), FS_OK);
    }

    #[test]
    fn test_optional_indexed_input_missing_file_behaves_as_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("optional-indexed-input.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open_indexed_optional(
                106,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                16,
                0,
                4,
                1,
            )
        };
        assert_eq!(status, FS_OPTIONAL_CREATED);

        let key = b"KEY1";
        assert_eq!(
            unsafe { cobol_file_start(106, key.as_ptr(), 4, 0, 0) },
            FS_REC_NOT_FOUND
        );
        let mut buf = [0u8; 16];
        assert_eq!(
            unsafe { cobol_file_read_key(106, key.as_ptr(), 4, 0, buf.as_mut_ptr(), 16) },
            FS_REC_NOT_FOUND
        );
        assert_eq!(
            unsafe { cobol_file_read_next(106, buf.as_mut_ptr(), 16) },
            FS_AT_END
        );
        assert_eq!(cobol_file_close(106), FS_OK);
    }

    #[test]
    fn test_variable_indexed_records_preserve_record_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("variable-indexed.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open_indexed(
                105,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                8,
                0,
                2,
            )
        };
        assert_eq!(status, FS_OK);
        assert_eq!(cobol_file_set_variable(105), FS_OK);

        let long = *b"01ABCDEF";
        let short = *b"02XYZ";
        assert_eq!(
            unsafe { cobol_file_write(105, long.as_ptr(), long.len() as u32) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_write(105, short.as_ptr(), short.len() as u32) },
            FS_OK
        );
        assert_eq!(cobol_file_close(105), FS_OK);

        let status = unsafe {
            cobol_file_open_indexed(
                106,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                8,
                0,
                2,
            )
        };
        assert_eq!(status, FS_OK);
        assert_eq!(cobol_file_set_variable(106), FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(106, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(cobol_file_current_record_length(106), 8);
        assert_eq!(&buf, b"01ABCDEF");

        assert_eq!(
            unsafe { cobol_file_read_next(106, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(cobol_file_current_record_length(106), 5);
        assert_eq!(&buf[..5], b"02XYZ");
        assert_eq!(cobol_file_close(106), FS_OK);
    }

    #[test]
    fn test_sequential_indexed_write_rejects_descending_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed-sequence.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open_indexed(
                107,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                8,
                0,
                2,
            )
        };
        assert_eq!(status, FS_OK);

        let first = *b"02BBBBBB";
        let second = *b"01AAAAAA";
        assert_eq!(
            unsafe { cobol_file_write(107, first.as_ptr(), first.len() as u32) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_write(107, second.as_ptr(), second.len() as u32) },
            FS_SEQUENCE_ERROR
        );
        assert_eq!(cobol_file_close(107), FS_OK);
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
    fn test_variable_sequential_records_preserve_record_boundaries_and_lengths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seq_variable.dat");
        let path_str = path.to_str().unwrap();
        let path_bytes = path_str.as_bytes();

        let status = unsafe {
            cobol_file_open(
                202,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                8,
            )
        };
        assert_eq!(status, FS_OK);
        assert_eq!(cobol_file_set_variable(202), FS_OK);

        unsafe {
            assert_eq!(cobol_file_write(202, b"ABC".as_ptr(), 3), FS_OK);
            assert_eq!(cobol_file_write(202, b"DEFGH".as_ptr(), 5), FS_OK);
        }
        assert_eq!(cobol_file_close(202), FS_OK);

        let status = unsafe {
            cobol_file_open(
                203,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                8,
            )
        };
        assert_eq!(status, FS_OK);
        assert_eq!(cobol_file_set_variable(203), FS_OK);

        let mut buf = [b'X'; 8];
        unsafe {
            assert_eq!(cobol_file_read_next(203, buf.as_mut_ptr(), 8), FS_OK);
        }
        assert_eq!(cobol_file_current_record_length(203), 3);
        assert_eq!(&buf, b"ABC     ");

        unsafe {
            assert_eq!(cobol_file_read_next(203, buf.as_mut_ptr(), 8), FS_OK);
        }
        assert_eq!(cobol_file_current_record_length(203), 5);
        assert_eq!(&buf, b"DEFGH   ");

        unsafe {
            assert_eq!(cobol_file_read_next(203, buf.as_mut_ptr(), 8), FS_AT_END);
        }
        assert_eq!(cobol_file_close(203), FS_OK);
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
                0,
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
                0,
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
                0,
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
                0,
                buf.as_mut_ptr(),
                rec_len,
            )
        };
        assert_eq!(rc, FS_REC_NOT_FOUND);

        let _ = cobol_file_close(fid);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_io_mode_open_missing_file_returns_35() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let rc = unsafe {
            cobol_file_open(
                700,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::IoMode,
                8,
            )
        };
        assert_eq!(rc, FS_NOT_FOUND);
    }

    #[test]
    fn test_read_on_output_file_returns_47() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output_only.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 701;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Output,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), buf.len() as u32) };
        assert_eq!(rc, FS_READ_NOT_PERMITTED);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_write_on_closed_file_returns_48() {
        let buf = *b"12345678";
        let rc = unsafe { cobol_file_write(702, buf.as_ptr(), buf.len() as u32) };
        assert_eq!(rc, FS_WRITE_NOT_PERMITTED);
    }

    #[test]
    fn test_rewrite_on_closed_file_returns_49() {
        let buf = *b"12345678";
        let rc = unsafe { cobol_file_rewrite(703, buf.as_ptr(), buf.len() as u32) };
        assert_eq!(rc, FS_IO_MODE_REQUIRED);
    }

    #[test]
    fn test_sequential_rewrite_after_at_end_returns_43() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rewrite_after_at_end.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"12345678").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 804u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::IoMode,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_AT_END
        );
        let rc = unsafe { cobol_file_rewrite(fid, buf.as_ptr(), 8) };
        assert_eq!(rc, FS_REWRITE_WITHOUT_READ);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_second_sequential_read_after_at_end_returns_46() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("second_read_after_at_end.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"12345678").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 807u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_AT_END
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_NO_VALID_NEXT_RECORD
        );

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_sequential_rewrite_length_mismatch_returns_44() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rewrite_length_mismatch.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"12345678").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 805u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::IoMode,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        let short = *b"1234";
        let rc = unsafe { cobol_file_rewrite(fid, short.as_ptr(), short.len() as u32) };
        assert_eq!(rc, FS_RECORD_LENGTH_MISMATCH);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_sequential_write_on_io_mode_returns_48() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("write_io_mode.dat");
        std::fs::File::create(&path).unwrap();
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 806u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Sequential,
                FileAccessMode::Sequential,
                FileOpenMode::IoMode,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let buf = *b"12345678";
        let rc = unsafe { cobol_file_write(fid, buf.as_ptr(), buf.len() as u32) };
        assert_eq!(rc, FS_WRITE_NOT_PERMITTED);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_relative_sequential_read_skips_deleted_slots() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relative_deleted.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"REC1xxxx").unwrap();
            f.write_all(&[0u8; 8]).unwrap();
            f.write_all(b"REC3xxxx").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 704u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Relative,
                FileAccessMode::Sequential,
                FileOpenMode::Input,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"REC1xxxx");

        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"REC3xxxx");

        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) };
        assert_eq!(rc, FS_AT_END);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_relative_random_write_duplicate_key_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relative_duplicate.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 706u32;
        let rc = unsafe {
            cobol_file_open(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileOrganization::Relative,
                FileAccessMode::Random,
                FileOpenMode::Output,
                8,
            )
        };
        assert_eq!(rc, FS_OK);

        let first = *b"REC1xxxx";
        let second = *b"REC2xxxx";
        let rc = unsafe { cobol_file_write_relative(fid, 2, first.as_ptr(), 8) };
        assert_eq!(rc, FS_OK);
        let rc = unsafe { cobol_file_write_relative(fid, 2, second.as_ptr(), 8) };
        assert_eq!(rc, FS_DUPLICATE_KEY);

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_delete_is_persisted_across_reopen() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_delete.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AAAfirst ").unwrap();
            f.write_all(b"BBBsecond").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 705u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                9,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);

        let key = b"AAA";
        let mut buf = [0u8; 9];
        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 3, 0, buf.as_mut_ptr(), 9) };
        assert_eq!(rc, FS_OK);
        let rc = cobol_file_delete(fid);
        assert_eq!(rc, FS_OK);
        let _ = cobol_file_close(fid);

        let fid = 706u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Random,
                FileOpenMode::Input,
                9,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 3, 0, buf.as_mut_ptr(), 9) };
        assert_eq!(rc, FS_REC_NOT_FOUND);
        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_delete_record_uses_primary_key_from_record_area() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_delete_record.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AAAfirst ").unwrap();
            f.write_all(b"BBBsecond").unwrap();
            f.write_all(b"CCCthird ").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 717u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                9,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut current = [0u8; 9];
        let rc = unsafe { cobol_file_read_next(fid, current.as_mut_ptr(), current.len() as u32) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&current[..3], b"AAA");

        let target = *b"BBBdelete";
        let rc = unsafe { cobol_file_delete_record(fid, target.as_ptr(), target.len() as u32) };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 9];
        let rc = unsafe { cobol_file_read_key(fid, b"AAA".as_ptr(), 3, 0, buf.as_mut_ptr(), 9) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"AAAfirst ");
        let rc = unsafe { cobol_file_read_key(fid, b"BBB".as_ptr(), 3, 0, buf.as_mut_ptr(), 9) };
        assert_eq!(rc, FS_REC_NOT_FOUND);
        let rc = unsafe { cobol_file_read_key(fid, b"CCC".as_ptr(), 3, 0, buf.as_mut_ptr(), 9) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"CCCthird ");
        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_delete_record_positions_next_read_after_deleted_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_delete_cursor.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 718u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Output,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        for rec in [b"017rec17", b"018rec18", b"019rec19", b"020rec20"] {
            assert_eq!(unsafe { cobol_file_write(fid, rec.as_ptr(), 8) }, FS_OK);
        }
        let _ = cobol_file_close(fid);

        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"018".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_delete_record(fid, b"018rec18".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(&buf, b"019rec19");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_read_key_uses_matching_alternate_offset() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_alt_same_len.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AAA111BBB222").unwrap();
            f.write_all(b"CCC222DDD111").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 707u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Random,
                FileOpenMode::Input,
                12,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 3, 0), FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 6, 3, 0), FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 9, 3, 0), FS_OK);

        let key = b"111";
        let mut buf = [0u8; 12];
        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 3, 9, buf.as_mut_ptr(), 12) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"CCC222DDD111");

        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 3, 3, buf.as_mut_ptr(), 12) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"AAA111BBB222");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_start_uses_matching_alternate_offset() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_alt_start_same_len.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AAA111BBB222").unwrap();
            f.write_all(b"CCC222DDD111").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 708u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                12,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 3, 0), FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 6, 3, 0), FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 9, 3, 0), FS_OK);

        let key = b"111";
        let rc = unsafe { cobol_file_start(fid, key.as_ptr(), 3, 9, 0) };
        assert_eq!(rc, FS_OK);

        let mut buf = [0u8; 12];
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 12) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"CCC222DDD111");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_duplicate_alternate_index_preserves_write_order_and_returns_status_02() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_alt_duplicates.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 713u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Output,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 1), FS_OK);
        assert_eq!(
            unsafe { cobol_file_write(fid, b"001AAone".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_write(fid, b"002AAtwo".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_write(fid, b"003BBtre".as_ptr(), 8) },
            FS_OK
        );
        let _ = cobol_file_close(fid);

        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 1), FS_OK);
        let key = b"AA";
        assert_eq!(
            unsafe { cobol_file_start(fid, key.as_ptr(), 2, 3, 0) },
            FS_OK
        );

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_DUPLICATE_ALT_SUCCESS
        );
        assert_eq!(&buf, b"001AAone");
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_DUPLICATE_ALT_SUCCESS
        );
        assert_eq!(&buf, b"002AAtwo");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_rewrite_preserves_next_position_in_active_alternate_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_rewrite_cursor.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 714u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Output,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 5, 0), FS_OK);
        for rec in [b"001A001A", b"002A002A", b"003A003A", b"004A004A"] {
            assert_eq!(unsafe { cobol_file_write(fid, rec.as_ptr(), 8) }, FS_OK);
        }
        let _ = cobol_file_close(fid);

        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 5, 0), FS_OK);

        let key = b"A002A";
        assert_eq!(
            unsafe { cobol_file_start(fid, key.as_ptr(), 5, 3, 0) },
            FS_OK
        );
        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(&buf, b"002A002A");

        let rewritten = b"002A003M";
        assert_eq!(
            unsafe { cobol_file_rewrite(fid, rewritten.as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            &buf, b"003A003A",
            "REWRITE must not advance the active alternate-key cursor to the moved record"
        );

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_rewrite_uses_primary_key_from_record_area_after_other_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_rewrite_by_key.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 715u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Output,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 0), FS_OK);
        for rec in [b"001AAone", b"002BBtwo", b"003CCtre"] {
            assert_eq!(unsafe { cobol_file_write(fid, rec.as_ptr(), 8) }, FS_OK);
        }
        let _ = cobol_file_close(fid);

        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 0), FS_OK);

        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"003".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"001".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_rewrite(fid, b"003DDtre".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"003".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(&buf, b"003DDtre");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_rewrite_moves_duplicate_alternate_key_to_end_of_equal_key_group() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_rewrite_duplicate_order.dat");
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 716u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Output,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 1), FS_OK);
        for rec in [b"004AA004", b"176BB176"] {
            assert_eq!(unsafe { cobol_file_write(fid, rec.as_ptr(), 8) }, FS_OK);
        }
        let _ = cobol_file_close(fid);

        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::IoMode,
                8,
                0,
                3,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 3, 2, 1), FS_OK);
        let mut buf = [0u8; 8];
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"176".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_rewrite(fid, b"176DC176".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_key(fid, b"004".as_ptr(), 3, 0, buf.as_mut_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_rewrite(fid, b"004DC004".as_ptr(), 8) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_start(fid, b"DC".as_ptr(), 2, 3, 0) },
            FS_OK
        );
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_DUPLICATE_ALT_SUCCESS
        );
        assert_eq!(&buf, b"176DC176");
        assert_eq!(
            unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 8) },
            FS_DUPLICATE_ALT_SUCCESS
        );
        assert_eq!(&buf, b"004DC004");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_start_matches_subordinate_prefix_and_positions_ge_group() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_start_prefix_ge.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AAAAA00000001").unwrap();
            f.write_all(b"DDDDDDDDDD040").unwrap();
            f.write_all(b"DDDDDDZZZZ050").unwrap();
            f.write_all(b"TTTTTAAAAA189").unwrap();
            f.write_all(b"TTTTTTTTTT380").unwrap();
            f.write_all(b"TTTTUUUUUU392").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 709u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                13,
                0,
                13,
            )
        };
        assert_eq!(rc, FS_OK);

        let key = b"DDDDDDD";
        let rc = unsafe { cobol_file_start(fid, key.as_ptr(), 7, 0, 2) };
        assert_eq!(rc, FS_OK);
        let mut buf = [0u8; 13];
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 13) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"DDDDDDDDDD040");

        let key = b"TTTTT";
        let rc = unsafe { cobol_file_start(fid, key.as_ptr(), 5, 0, 1) };
        assert_eq!(rc, FS_OK);
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 13) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"TTTTUUUUUU392");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_read_key_matches_subordinate_prefix() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_read_key_prefix.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"CCCCCCCCCC018").unwrap();
            f.write_all(b"CCCCCCDDDD019").unwrap();
            f.write_all(b"DDDDDDDDDD040").unwrap();
        }
        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 710u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                13,
                0,
                13,
            )
        };
        assert_eq!(rc, FS_OK);

        let key = b"CCCCCCD";
        let mut buf = [0u8; 13];
        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 7, 0, buf.as_mut_ptr(), 13) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf, b"CCCCCCDDDD019");

        let _ = cobol_file_close(fid);
    }

    #[test]
    fn test_indexed_nist_layout_random_and_alternate_start() {
        use std::io::Write;

        fn make_record(recno: u32, total: u32) -> [u8; 240] {
            let mut rec = [b' '; 240];
            let head = format!("FILE=IX-FD1,RECORD=R1-F-G/0,RECNO={recno:06}");
            rec[..head.len()].copy_from_slice(head.as_bytes());
            let key = format!("{recno:010}");
            let alt = format!("{:010}", total - recno + 1);
            rec[147..157].copy_from_slice(key.as_bytes());
            rec[185..195].copy_from_slice(alt.as_bytes());
            rec
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("indexed_nist_layout.dat");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            for recno in 1..=200u32 {
                f.write_all(&make_record(recno, 200)).unwrap();
            }
        }

        let path_bytes = path.to_str().unwrap().as_bytes();
        let fid = 711u32;
        let rc = unsafe {
            cobol_file_open_indexed(
                fid,
                path_bytes.as_ptr(),
                path_bytes.len() as u32,
                FileAccessMode::Dynamic,
                FileOpenMode::Input,
                240,
                147,
                10,
            )
        };
        assert_eq!(rc, FS_OK);
        assert_eq!(cobol_file_add_alternate_index(fid, 185, 10, 0), FS_OK);

        let mut buf = [0u8; 240];
        let key = b"0000000010";
        let rc = unsafe { cobol_file_read_key(fid, key.as_ptr(), 10, 147, buf.as_mut_ptr(), 240) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf[147..157], key);

        let alt = b"0000000001";
        let rc = unsafe { cobol_file_start(fid, alt.as_ptr(), 10, 185, 0) };
        assert_eq!(rc, FS_OK);
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 240) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf[147..157], b"0000000200");
        let rc = unsafe { cobol_file_read_next(fid, buf.as_mut_ptr(), 240) };
        assert_eq!(rc, FS_OK);
        assert_eq!(&buf[147..157], b"0000000199");

        let _ = cobol_file_close(fid);
    }
}
