// COBOL Runtime - COMMUNICATION SECTION support
//
// This runtime models COBOL communication queues in-process so generated
// programs can execute ENABLE/DISABLE/SEND/RECEIVE/PURGE statements.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Mutex;

#[derive(Default)]
struct CommunicationState {
    enabled: bool,
    messages: VecDeque<Vec<u8>>,
}

#[derive(Default)]
struct CommunicationRuntime {
    queues: HashMap<String, CommunicationState>,
    routes: HashMap<String, Vec<String>>,
    loaded_script: Option<String>,
}

static COMM_RUNTIME: Mutex<Option<CommunicationRuntime>> = Mutex::new(None);

fn normalize_comm_name(name: &str) -> String {
    name.trim().replace('-', "_")
}

fn load_comm_script(runtime: &mut CommunicationRuntime, script_path: &str) {
    let Ok(contents) = fs::read_to_string(script_path) else {
        return;
    };

    runtime.queues.clear();
    runtime.routes.clear();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.splitn(2, char::is_whitespace);
        let command = parts.next().unwrap_or_default().to_ascii_lowercase();
        let rest = parts.next().unwrap_or("").trim();

        match command.as_str() {
            "enable" => {
                let name = normalize_comm_name(rest);
                runtime.queues.entry(name).or_default().enabled = true;
            }
            "disable" => {
                let name = normalize_comm_name(rest);
                runtime.queues.entry(name).or_default().enabled = false;
            }
            "message" => {
                let mut msg_parts = rest.splitn(2, char::is_whitespace);
                let name = normalize_comm_name(msg_parts.next().unwrap_or_default());
                let payload = msg_parts.next().unwrap_or("").as_bytes().to_vec();
                runtime
                    .queues
                    .entry(name)
                    .or_default()
                    .messages
                    .push_back(payload);
            }
            "link" => {
                let mut link_parts = rest.split_whitespace();
                let src = normalize_comm_name(link_parts.next().unwrap_or_default());
                let dst = normalize_comm_name(link_parts.next().unwrap_or_default());
                if !src.is_empty() && !dst.is_empty() {
                    runtime.routes.entry(src).or_default().push(dst);
                }
            }
            _ => {}
        }
    }
}

fn with_comm_runtime<F, R>(f: F) -> R
where
    F: FnOnce(&mut CommunicationRuntime) -> R,
{
    let mut guard = COMM_RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
    let runtime = guard.get_or_insert_with(CommunicationRuntime::default);
    let script_path = std::env::var("COBOL_COMM_SCRIPT").ok();
    if runtime.loaded_script != script_path {
        runtime.loaded_script = script_path.clone();
        runtime.queues.clear();
        runtime.routes.clear();
        if let Some(path) = script_path {
            load_comm_script(runtime, &path);
        }
    }
    f(runtime)
}

unsafe fn key_from_raw(ptr: *const u8, len: u32) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    Some(normalize_comm_name(&String::from_utf8_lossy(bytes)))
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
    with_comm_runtime(|runtime| {
        runtime.queues.entry(name).or_default().enabled = true;
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
    with_comm_runtime(|runtime| {
        runtime.queues.entry(name).or_default().enabled = false;
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
    with_comm_runtime(|runtime| {
        let state = runtime.queues.entry(name.clone()).or_default();
        if !state.enabled {
            return 99;
        }
        if let Some(routes) = runtime.routes.get(&name).cloned() {
            for target in routes {
                runtime
                    .queues
                    .entry(target)
                    .or_default()
                    .messages
                    .push_back(payload.clone());
            }
        } else {
            state.messages.push_back(payload);
        }
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
    with_comm_runtime(|runtime| {
        let state = runtime.queues.entry(name).or_default();
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
    with_comm_runtime(|runtime| {
        runtime.queues.entry(name).or_default().messages.clear();
    });
    0
}

#[no_mangle]
pub unsafe extern "C" fn cobol_comm_message_count(name_ptr: *const u8, name_len: u32) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 0;
    };
    with_comm_runtime(|runtime| {
        runtime
            .queues
            .get(&name)
            .map(|state| state.messages.len() as u32)
            .unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn reset_runtime() {
        let mut guard = COMM_RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(CommunicationRuntime::default());
        unsafe {
            std::env::remove_var("COBOL_COMM_SCRIPT");
        }
    }

    #[test]
    fn test_comm_script_preloads_messages() {
        reset_runtime();
        let mut script = tempfile::NamedTempFile::new().unwrap();
        writeln!(script, "enable CM-INQUE-1").unwrap();
        writeln!(script, "message CM-INQUE-1 KILL").unwrap();
        unsafe {
            std::env::set_var("COBOL_COMM_SCRIPT", script.path());
        }

        let mut buf = [b' '; 8];
        let mut text_len = 0u32;
        let rc = unsafe {
            cobol_comm_receive(
                b"CM_INQUE_1".as_ptr(),
                10,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut text_len,
            )
        };
        assert_eq!(rc, 0);
        assert_eq!(text_len, 4);
        assert_eq!(&buf[..4], b"KILL");
    }

    #[test]
    fn test_comm_script_routes_output_to_input() {
        reset_runtime();
        let mut script = tempfile::NamedTempFile::new().unwrap();
        writeln!(script, "enable CM-OUTQUE-1").unwrap();
        writeln!(script, "enable CM-INQUE-1").unwrap();
        writeln!(script, "link CM-OUTQUE-1 CM-INQUE-1").unwrap();
        unsafe {
            std::env::set_var("COBOL_COMM_SCRIPT", script.path());
        }

        let send_rc = unsafe {
            cobol_comm_send(
                b"CM_OUTQUE_1".as_ptr(),
                11,
                b"PING".as_ptr(),
                4,
                0,
                0,
                0,
            )
        };
        assert_eq!(send_rc, 0);
        assert_eq!(
            unsafe { cobol_comm_message_count(b"CM_INQUE_1".as_ptr(), 10) },
            1
        );
    }
}
