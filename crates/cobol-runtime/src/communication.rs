// COBOL Runtime - COMMUNICATION SECTION support
//
// This runtime models COBOL communication queues in-process so generated
// programs can execute ENABLE/DISABLE/SEND/RECEIVE/PURGE statements.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

#[derive(Default)]
struct CommunicationState {
    enabled: bool,
    messages: VecDeque<Vec<u8>>,
}

static COMM_TABLE: Mutex<Option<HashMap<String, CommunicationState>>> = Mutex::new(None);

fn with_comm_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, CommunicationState>) -> R,
{
    let mut guard = COMM_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    f(table)
}

unsafe fn key_from_raw(ptr: *const u8, len: u32) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    Some(String::from_utf8_lossy(bytes).trim_end().to_string())
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_enable(
    name_ptr: *const u8,
    name_len: u32,
    _mode: i32,
    _terminal: i32,
    _key_ptr: *const u8,
    _key_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_table(|table| {
        table.entry(name).or_default().enabled = true;
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_disable(
    name_ptr: *const u8,
    name_len: u32,
    _mode: i32,
    _terminal: i32,
    _key_ptr: *const u8,
    _key_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_table(|table| {
        table.entry(name).or_default().enabled = false;
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_send(
    name_ptr: *const u8,
    name_len: u32,
    from_ptr: *const u8,
    from_len: u32,
    _option_kind: i32,
    _option_value: i64,
    _replacing_line: i32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    let payload = if from_ptr.is_null() || from_len == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(from_ptr, from_len as usize).to_vec()
    };
    with_comm_table(|table| {
        let state = table.entry(name).or_default();
        if !state.enabled {
            return 99;
        }
        state.messages.push_back(payload);
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_receive(
    name_ptr: *const u8,
    name_len: u32,
    into_ptr: *mut u8,
    into_len: u32,
    text_length: *mut u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_table(|table| {
        let state = table.entry(name).or_default();
        let Some(message) = state.messages.pop_front() else {
            if !text_length.is_null() {
                unsafe {
                    *text_length = 0;
                }
            }
            return 10;
        };

        let copy_len = usize::min(message.len(), into_len as usize);
        if !into_ptr.is_null() && copy_len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(message.as_ptr(), into_ptr, copy_len);
            }
        }
        if !into_ptr.is_null() && copy_len < into_len as usize {
            unsafe {
                std::ptr::write_bytes(into_ptr.add(copy_len), b' ', into_len as usize - copy_len);
            }
        }
        if !text_length.is_null() {
            unsafe {
                *text_length = copy_len as u32;
            }
        }
        0
    })
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_purge(name_ptr: *const u8, name_len: u32) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_table(|table| {
        table.entry(name).or_default().messages.clear();
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_message_count(name_ptr: *const u8, name_len: u32) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 0;
    };
    with_comm_table(|table| {
        table
            .get(&name)
            .map(|state| state.messages.len() as u32)
            .unwrap_or(0)
    })
}
