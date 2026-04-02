// COBOL Runtime - COMMUNICATION SECTION support
//
// This runtime models COBOL communication queues in-process so generated
// programs can execute ENABLE/DISABLE/SEND/RECEIVE/PURGE statements.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::Mutex;

#[derive(Default)]
struct CommunicationConfig {
    queue_name: Option<String>,
    sub_queue_1: Option<String>,
    sub_queue_2: Option<String>,
    sub_queue_3: Option<String>,
    source_names: Vec<String>,
    password: Option<String>,
    destinations: Vec<String>,
}

#[derive(Default)]
struct CommunicationMessage {
    segments: Vec<Vec<u8>>,
}

#[derive(Default)]
struct ActiveSegmentReceive {
    segments: Vec<Vec<u8>>,
    segment_index: usize,
    offset: usize,
}

#[derive(Default)]
struct PendingMessage {
    segments: Vec<Vec<u8>>,
}

#[derive(Default)]
struct CommunicationState {
    enabled: bool,
    messages: VecDeque<CommunicationMessage>,
    pending_send: PendingMessage,
    active_segment_receive: Option<ActiveSegmentReceive>,
    last_end_key: bool,
}

#[derive(Default)]
struct CommunicationRuntime {
    queues: HashMap<String, CommunicationState>,
    configs: HashMap<String, CommunicationConfig>,
    routes: HashMap<String, Vec<String>>,
    loaded_script: Option<String>,
}

static COMM_RUNTIME: Mutex<Option<CommunicationRuntime>> = Mutex::new(None);

fn normalize_comm_name(name: &str) -> String {
    name.trim().replace('-', "_")
}

fn comm_debug_enabled() -> bool {
    std::env::var("COBOL_COMM_DEBUG").as_deref() == Ok("1")
}

fn normalize_comm_value(value: &str) -> String {
    let normalized = value
        .replace('\0', "")
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase();
    match normalized.as_str() {
        "BLANK" | "SPACE" | "SPACES" => String::new(),
        _ => normalized,
    }
}

fn parse_raw_value(ptr: *const u8, len: u32) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    if len as usize == std::mem::size_of::<i64>()
        && bytes.iter().any(|b| !b.is_ascii_graphic() && *b != b' ')
    {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        let value = i64::from_ne_bytes(raw);
        return Some(value.to_string());
    }
    Some(normalize_comm_value(&String::from_utf8_lossy(bytes)))
}

fn load_comm_script(runtime: &mut CommunicationRuntime, script_path: &str) {
    let Ok(contents) = fs::read_to_string(script_path) else {
        return;
    };

    runtime.queues.clear();
    runtime.configs.clear();
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
                    .push_back(CommunicationMessage {
                        segments: vec![payload],
                    });
            }
            "link" => {
                let mut link_parts = rest.split_whitespace();
                let src = normalize_comm_name(link_parts.next().unwrap_or_default());
                let dst = normalize_comm_name(link_parts.next().unwrap_or_default());
                if !src.is_empty() && !dst.is_empty() {
                    runtime.routes.entry(src).or_default().push(dst);
                }
            }
            "config" => {
                let mut cfg_parts = rest.split_whitespace();
                let name = normalize_comm_name(cfg_parts.next().unwrap_or_default());
                let key = cfg_parts.next().unwrap_or_default().to_ascii_lowercase();
                let value = cfg_parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() || key.is_empty() || value.is_empty() {
                    continue;
                }
                let config = runtime.configs.entry(name).or_default();
                match key.as_str() {
                    "queue" => config.queue_name = Some(normalize_comm_value(&value)),
                    "sub1" => config.sub_queue_1 = Some(normalize_comm_value(&value)),
                    "sub2" => config.sub_queue_2 = Some(normalize_comm_value(&value)),
                    "sub3" => config.sub_queue_3 = Some(normalize_comm_value(&value)),
                    "source" => config.source_names.push(normalize_comm_value(&value)),
                    "key" => config.password = Some(normalize_comm_value(&value)),
                    "dest" => config.destinations.push(normalize_comm_value(&value)),
                    _ => {}
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
        runtime.configs.clear();
        runtime.routes.clear();
        if let Some(path) = script_path {
            load_comm_script(runtime, &path);
        }
    }
    f(runtime)
}

unsafe fn key_from_raw(ptr: *const u8, len: u32) -> Option<String> {
    parse_raw_value(ptr, len).map(|value| normalize_comm_name(&value))
}

struct SelectorInputs {
    queue: (*const u8, u32),
    sub1: (*const u8, u32),
    sub2: (*const u8, u32),
    sub3: (*const u8, u32),
}

fn validate_selectors(config: Option<&CommunicationConfig>, selectors: SelectorInputs) -> u32 {
    let Some(config) = config else {
        return 0;
    };
    let validations = [
        (
            &config.queue_name,
            parse_raw_value(selectors.queue.0, selectors.queue.1),
        ),
        (
            &config.sub_queue_1,
            parse_raw_value(selectors.sub1.0, selectors.sub1.1),
        ),
        (
            &config.sub_queue_2,
            parse_raw_value(selectors.sub2.0, selectors.sub2.1),
        ),
        (
            &config.sub_queue_3,
            parse_raw_value(selectors.sub3.0, selectors.sub3.1),
        ),
    ];
    for (expected, actual) in validations {
        if let Some(expected) = expected {
            if comm_debug_enabled() {
                eprintln!(
                    "[COMM] selector expected='{expected}' actual='{}'",
                    actual.as_deref().unwrap_or_default()
                );
            }
            if actual.as_deref().unwrap_or_default() != expected {
                return 20;
            }
        }
    }
    0
}

fn validate_source(
    config: Option<&CommunicationConfig>,
    source_ptr: *const u8,
    source_len: u32,
) -> u32 {
    let Some(config) = config else {
        return 0;
    };
    if !config.source_names.is_empty() {
        let actual = parse_raw_value(source_ptr, source_len);
        if comm_debug_enabled() {
            eprintln!(
                "[COMM] source expected={:?} actual='{}'",
                config.source_names,
                actual.as_deref().unwrap_or_default()
            );
        }
        if !config
            .source_names
            .iter()
            .any(|expected| Some(expected.as_str()) == actual.as_deref())
        {
            return 21;
        }
    }
    0
}

fn validate_key(config: Option<&CommunicationConfig>, key_ptr: *const u8, key_len: u32) -> u32 {
    let Some(config) = config else {
        return 0;
    };
    if let Some(expected) = &config.password {
        let actual = parse_raw_value(key_ptr, key_len).unwrap_or_default();
        if comm_debug_enabled() {
            eprintln!("[COMM] key expected='{expected}' actual='{actual}'");
        }
        let matches = actual == *expected
            || match (actual.parse::<i64>(), expected.parse::<i64>()) {
                (Ok(actual_num), Ok(expected_num)) => actual_num == expected_num,
                _ => false,
            };
        if !matches {
            return 40;
        }
    }
    0
}

unsafe fn write_error_key_flags(
    error_key_ptr: *mut u8,
    error_key_len: u32,
    invalid_destination: Option<usize>,
) {
    if error_key_ptr.is_null() || error_key_len == 0 {
        return;
    }
    std::ptr::write_bytes(error_key_ptr, b'0', error_key_len as usize);
    if let Some(index) = invalid_destination {
        if index < error_key_len as usize {
            *error_key_ptr.add(index) = b'1';
        }
    }
}

unsafe fn validate_output_destinations(
    config: Option<&CommunicationConfig>,
    dest_ptr: *const u8,
    dest_item_len: u32,
    dest_count: u32,
    dest_table_count: u32,
    error_key_ptr: *mut u8,
    error_key_len: u32,
) -> u32 {
    write_error_key_flags(error_key_ptr, error_key_len, None);
    let Some(config) = config else {
        return 0;
    };
    if config.destinations.is_empty() {
        return 0;
    }
    if comm_debug_enabled() {
        eprintln!(
            "[COMM] validate dest_count={dest_count} dest_table_count={dest_table_count} dest_item_len={dest_item_len} configured={:?}",
            config.destinations
        );
        if !dest_ptr.is_null() && dest_item_len != 0 {
            for idx in 0..dest_count as usize {
                let offset = idx * dest_item_len as usize;
                let raw = std::slice::from_raw_parts(dest_ptr.add(offset), dest_item_len as usize);
                let actual = normalize_comm_value(&String::from_utf8_lossy(raw));
                eprintln!("[COMM] validate destination[{idx}]='{actual}'");
            }
        }
    }
    if dest_count == 0 {
        return 30;
    }
    if dest_table_count != 0 && dest_count > dest_table_count {
        return 30;
    }
    if dest_ptr.is_null() || dest_item_len == 0 {
        write_error_key_flags(error_key_ptr, error_key_len, Some(0));
        return 20;
    }
    for idx in 0..dest_count as usize {
        let offset = idx * dest_item_len as usize;
        let raw = std::slice::from_raw_parts(dest_ptr.add(offset), dest_item_len as usize);
        let actual = normalize_comm_value(&String::from_utf8_lossy(raw));
        if !config.destinations.iter().any(|dest| dest == &actual) {
            write_error_key_flags(error_key_ptr, error_key_len, Some(idx));
            return 20;
        }
    }
    0
}

#[no_mangle]
/// # Safety
///
/// `name_ptr`, `key_ptr`, `queue_ptr`, `sub1_ptr`, `sub2_ptr`, `sub3_ptr`, and
/// `source_ptr` must each be either null with a zero length or valid for reads
/// of the corresponding byte length for the duration of this call.
pub unsafe extern "C" fn cobol_comm_enable(
    name_ptr: *const u8,
    name_len: u32,
    mode: i32,
    terminal: i32,
    key_ptr: *const u8,
    key_len: u32,
    queue_ptr: *const u8,
    queue_len: u32,
    sub1_ptr: *const u8,
    sub1_len: u32,
    sub2_ptr: *const u8,
    sub2_len: u32,
    sub3_ptr: *const u8,
    sub3_len: u32,
    source_ptr: *const u8,
    source_len: u32,
    dest_ptr: *const u8,
    dest_item_len: u32,
    dest_count: u32,
    dest_table_count: u32,
    error_key_ptr: *mut u8,
    error_key_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_runtime(|runtime| {
        let config = runtime.configs.get(&name);
        let selector_rc = validate_selectors(
            config,
            SelectorInputs {
                queue: (queue_ptr, queue_len),
                sub1: (sub1_ptr, sub1_len),
                sub2: (sub2_ptr, sub2_len),
                sub3: (sub3_ptr, sub3_len),
            },
        );
        if selector_rc != 0 {
            if comm_debug_enabled() {
                eprintln!("[COMM] enable {name} rc={selector_rc}");
            }
            return selector_rc;
        }
        let key_rc = validate_key(config, key_ptr, key_len);
        if key_rc != 0 {
            if comm_debug_enabled() {
                eprintln!("[COMM] enable {name} rc={key_rc}");
            }
            return key_rc;
        }
        if terminal != 0 {
            let source_rc = validate_source(config, source_ptr, source_len);
            if source_rc != 0 {
                if comm_debug_enabled() {
                    eprintln!("[COMM] enable {name} rc={source_rc}");
                }
                return source_rc;
            }
        }
        if mode != 0 {
            let dest_rc = validate_output_destinations(
                config,
                dest_ptr,
                dest_item_len,
                dest_count,
                dest_table_count,
                error_key_ptr,
                error_key_len,
            );
            if dest_rc != 0 {
                return dest_rc;
            }
        }
        runtime.queues.entry(name.clone()).or_default().enabled = true;
        if comm_debug_enabled() {
            eprintln!("[COMM] enable {name} rc=0");
        }
        0
    })
}

#[no_mangle]
/// # Safety
///
/// `name_ptr`, `key_ptr`, `queue_ptr`, `sub1_ptr`, `sub2_ptr`, `sub3_ptr`, and
/// `source_ptr` must each be either null with a zero length or valid for reads
/// of the corresponding byte length for the duration of this call.
pub unsafe extern "C" fn cobol_comm_disable(
    name_ptr: *const u8,
    name_len: u32,
    mode: i32,
    terminal: i32,
    key_ptr: *const u8,
    key_len: u32,
    queue_ptr: *const u8,
    queue_len: u32,
    sub1_ptr: *const u8,
    sub1_len: u32,
    sub2_ptr: *const u8,
    sub2_len: u32,
    sub3_ptr: *const u8,
    sub3_len: u32,
    source_ptr: *const u8,
    source_len: u32,
    dest_ptr: *const u8,
    dest_item_len: u32,
    dest_count: u32,
    dest_table_count: u32,
    error_key_ptr: *mut u8,
    error_key_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_runtime(|runtime| {
        let config = runtime.configs.get(&name);
        let selector_rc = validate_selectors(
            config,
            SelectorInputs {
                queue: (queue_ptr, queue_len),
                sub1: (sub1_ptr, sub1_len),
                sub2: (sub2_ptr, sub2_len),
                sub3: (sub3_ptr, sub3_len),
            },
        );
        if selector_rc != 0 {
            if comm_debug_enabled() {
                eprintln!("[COMM] disable {name} rc={selector_rc}");
            }
            return selector_rc;
        }
        let key_rc = validate_key(config, key_ptr, key_len);
        if key_rc != 0 {
            if comm_debug_enabled() {
                eprintln!("[COMM] disable {name} rc={key_rc}");
            }
            return key_rc;
        }
        if terminal != 0 {
            let source_rc = validate_source(config, source_ptr, source_len);
            if source_rc != 0 {
                if comm_debug_enabled() {
                    eprintln!("[COMM] disable {name} rc={source_rc}");
                }
                return source_rc;
            }
        }
        if mode != 0 {
            let dest_rc = validate_output_destinations(
                config,
                dest_ptr,
                dest_item_len,
                dest_count,
                dest_table_count,
                error_key_ptr,
                error_key_len,
            );
            if dest_rc != 0 {
                return dest_rc;
            }
        }
        runtime.queues.entry(name.clone()).or_default().enabled = false;
        if comm_debug_enabled() {
            eprintln!("[COMM] disable {name} rc=0");
        }
        0
    })
}

#[no_mangle]
/// # Safety
///
/// `name_ptr`, `from_ptr`, and `dest_ptr` must be either null with a zero
/// length or valid for reads of the supplied byte lengths. `error_key_ptr`
/// must be either null with a zero length or valid for writes of
/// `error_key_len` bytes for the duration of this call.
pub unsafe extern "C" fn cobol_comm_send(
    name_ptr: *const u8,
    name_len: u32,
    from_ptr: *const u8,
    from_len: u32,
    effective_len: u32,
    option_kind: i32,
    option_value: i64,
    _replacing_line: i32,
    dest_ptr: *const u8,
    dest_item_len: u32,
    dest_count: u32,
    dest_table_count: u32,
    error_key_ptr: *mut u8,
    error_key_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    let payload = if from_ptr.is_null() || effective_len == 0 || from_len == 0 {
        Vec::new()
    } else {
        let actual_len = usize::min(effective_len as usize, from_len as usize);
        std::slice::from_raw_parts(from_ptr, actual_len).to_vec()
    };
    let payload_len = payload.len();
    with_comm_runtime(|runtime| {
        let config = runtime.configs.get(&name);
        write_error_key_flags(error_key_ptr, error_key_len, None);
        let dest_rc = validate_output_destinations(
            config,
            dest_ptr,
            dest_item_len,
            dest_count,
            dest_table_count,
            error_key_ptr,
            error_key_len,
        );
        if dest_rc != 0 {
            if comm_debug_enabled() {
                eprintln!("[COMM] send {name} rc={dest_rc}");
            }
            return dest_rc;
        }
        if payload.is_empty() {
            if comm_debug_enabled() {
                eprintln!(
                    "[COMM] send {name} rc=60 from_len={from_len} effective_len={effective_len} payload_len={payload_len}"
                );
            }
            return 60;
        }
        if effective_len > from_len {
            if comm_debug_enabled() {
                eprintln!(
                    "[COMM] send {name} rc=50 from_len={from_len} effective_len={effective_len}"
                );
            }
            return 50;
        }
        let state = runtime.queues.entry(name.clone()).or_default();
        if !state.enabled {
            if comm_debug_enabled() {
                eprintln!("[COMM] send {name} rc=10");
            }
            return 10;
        }
        let is_continuation = option_kind == 3 || (option_kind == 4 && option_value == 0);
        state.pending_send.segments.push(payload);
        if !is_continuation {
            let message = CommunicationMessage {
                segments: std::mem::take(&mut state.pending_send.segments),
            };
            if let Some(routes) = runtime.routes.get(&name).cloned() {
                for target in routes {
                    runtime
                        .queues
                        .entry(target)
                        .or_default()
                        .messages
                        .push_back(CommunicationMessage {
                            segments: message.segments.clone(),
                        });
                }
            } else {
                state.messages.push_back(message);
            }
        }
        if comm_debug_enabled() {
            eprintln!(
                "[COMM] send {name} rc=0 from_len={from_len} effective_len={effective_len} payload_len={payload_len}"
            );
        }
        0
    })
}

unsafe fn write_receive_bytes(
    into_ptr: *mut u8,
    into_len: u32,
    text_length: *mut u32,
    bytes: &[u8],
) {
    let copy_len = usize::min(bytes.len(), into_len as usize);
    if !into_ptr.is_null() && copy_len > 0 {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), into_ptr, copy_len);
    }
    if !into_ptr.is_null() && copy_len < into_len as usize {
        std::ptr::write_bytes(into_ptr.add(copy_len), b' ', into_len as usize - copy_len);
    }
    if !text_length.is_null() {
        *text_length = copy_len as u32;
    }
}

unsafe fn receive_message(
    state: &mut CommunicationState,
    into_ptr: *mut u8,
    into_len: u32,
    text_length: *mut u32,
) -> Option<u32> {
    let message = state.messages.pop_front()?;
    let bytes: Vec<u8> = message.segments.into_iter().flatten().collect();
    write_receive_bytes(into_ptr, into_len, text_length, &bytes);
    // RECEIVE MESSAGE consumes exactly one message. If the target area is
    // shorter than the message, excess bytes are discarded rather than
    // delivered via subsequent RECEIVE MESSAGE calls.
    state.last_end_key = false;
    Some(0)
}

unsafe fn receive_segment(
    state: &mut CommunicationState,
    into_ptr: *mut u8,
    into_len: u32,
    text_length: *mut u32,
) -> Option<u32> {
    if state.active_segment_receive.is_none() {
        let message = state.messages.pop_front()?;
        state.active_segment_receive = Some(ActiveSegmentReceive {
            segments: message.segments,
            segment_index: 0,
            offset: 0,
        });
    }
    let active = state.active_segment_receive.as_mut().unwrap();
    let segment = &active.segments[active.segment_index];
    let remaining = &segment[active.offset..];
    write_receive_bytes(into_ptr, into_len, text_length, remaining);
    let copied = usize::min(remaining.len(), into_len as usize);
    active.offset += copied;
    if active.offset < segment.len() {
        state.last_end_key = false;
        return Some(0);
    }
    let is_last_segment = active.segment_index + 1 >= active.segments.len();
    state.last_end_key = is_last_segment;
    if is_last_segment {
        state.active_segment_receive = None;
    } else {
        active.segment_index += 1;
        active.offset = 0;
    }
    Some(0)
}

#[no_mangle]
/// # Safety
///
/// `name_ptr`, `queue_ptr`, `sub1_ptr`, `sub2_ptr`, and `sub3_ptr` must be
/// either null with a zero length or valid for reads of the supplied byte
/// lengths. `into_ptr` must be valid for writes of `into_len` bytes when
/// non-null, and `text_length` must be valid for writes of one `u32` when
/// non-null.
pub unsafe extern "C" fn cobol_comm_receive(
    name_ptr: *const u8,
    name_len: u32,
    mode: i32,
    into_ptr: *mut u8,
    into_len: u32,
    text_length: *mut u32,
    queue_ptr: *const u8,
    queue_len: u32,
    sub1_ptr: *const u8,
    sub1_len: u32,
    sub2_ptr: *const u8,
    sub2_len: u32,
    sub3_ptr: *const u8,
    sub3_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_runtime(|runtime| {
        let config = runtime.configs.get(&name);
        let selector_rc = validate_selectors(
            config,
            SelectorInputs {
                queue: (queue_ptr, queue_len),
                sub1: (sub1_ptr, sub1_len),
                sub2: (sub2_ptr, sub2_len),
                sub3: (sub3_ptr, sub3_len),
            },
        );
        if selector_rc != 0 {
            if !text_length.is_null() {
                unsafe {
                    *text_length = 0;
                }
            }
            runtime.queues.entry(name.clone()).or_default().last_end_key = true;
            if comm_debug_enabled() {
                eprintln!("[COMM] receive {name} rc={selector_rc}");
            }
            return selector_rc;
        }
        let state = runtime.queues.entry(name.clone()).or_default();
        let receive_rc = match mode {
            2 => receive_segment(state, into_ptr, into_len, text_length),
            _ => receive_message(state, into_ptr, into_len, text_length),
        };
        if let Some(rc) = receive_rc {
            if comm_debug_enabled() {
                eprintln!("[COMM] receive {name} rc={rc}");
            }
            return rc;
        }
        let Some(message) = state.messages.pop_front() else {
            if !text_length.is_null() {
                unsafe {
                    *text_length = 0;
                }
            }
            state.last_end_key = true;
            if comm_debug_enabled() {
                eprintln!("[COMM] receive {name} rc=10");
            }
            return 10;
        };
        state.messages.push_front(message);
        let rc = match mode {
            2 => receive_segment(state, into_ptr, into_len, text_length),
            _ => receive_message(state, into_ptr, into_len, text_length),
        }
        .unwrap_or(10);
        if comm_debug_enabled() {
            eprintln!("[COMM] receive {name} rc={rc}");
        }
        rc
    })
}

#[no_mangle]
/// # Safety
///
/// `name_ptr` must be either null with a zero length or valid for reads of
/// `name_len` bytes for the duration of this call.
pub unsafe extern "C" fn cobol_comm_purge(name_ptr: *const u8, name_len: u32) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_runtime(|runtime| {
        let state = runtime.queues.entry(name).or_default();
        state.messages.clear();
        state.pending_send.segments.clear();
        state.active_segment_receive = None;
        state.last_end_key = true;
    });
    0
}

#[no_mangle]
/// # Safety
///
/// `name_ptr` must be either null with a zero length or valid for reads of
/// `name_len` bytes for the duration of this call.
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

#[no_mangle]
/// # Safety
///
/// `name_ptr` must be either null with a zero length or valid for reads of
/// `name_len` bytes for the duration of this call.
pub unsafe extern "C" fn cobol_comm_last_end_key(name_ptr: *const u8, name_len: u32) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 1;
    };
    with_comm_runtime(|runtime| {
        runtime
            .queues
            .get(&name)
            .map(|state| u32::from(state.last_end_key))
            .unwrap_or(1)
    })
}

#[no_mangle]
/// # Safety
///
/// `name_ptr`, `queue_ptr`, `sub1_ptr`, `sub2_ptr`, and `sub3_ptr` must be
/// either null with a zero length or valid for reads of the supplied byte
/// lengths. `count_out` must be valid for writes of one `u32` when non-null.
pub unsafe extern "C" fn cobol_comm_accept_count(
    name_ptr: *const u8,
    name_len: u32,
    count_out: *mut u32,
    queue_ptr: *const u8,
    queue_len: u32,
    sub1_ptr: *const u8,
    sub1_len: u32,
    sub2_ptr: *const u8,
    sub2_len: u32,
    sub3_ptr: *const u8,
    sub3_len: u32,
) -> u32 {
    let Some(name) = key_from_raw(name_ptr, name_len) else {
        return 99;
    };
    with_comm_runtime(|runtime| {
        let config = runtime.configs.get(&name);
        let selector_rc = validate_selectors(
            config,
            SelectorInputs {
                queue: (queue_ptr, queue_len),
                sub1: (sub1_ptr, sub1_len),
                sub2: (sub2_ptr, sub2_len),
                sub3: (sub3_ptr, sub3_len),
            },
        );
        let count = runtime
            .queues
            .get(&name)
            .map(|state| state.messages.len() as u32)
            .unwrap_or(0);
        if !count_out.is_null() {
            unsafe {
                *count_out = count;
            }
        }
        if comm_debug_enabled() {
            eprintln!("[COMM] accept_count {name} rc={selector_rc} count={count}");
        }
        selector_rc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    static COMM_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn reset_runtime() {
        let mut guard = COMM_RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(CommunicationRuntime::default());
        unsafe {
            std::env::remove_var("COBOL_COMM_SCRIPT");
        }
    }

    fn with_comm_test<T>(f: impl FnOnce() -> T) -> T {
        let _guard = COMM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_runtime();
        let result = f();
        reset_runtime();
        result
    }

    #[test]
    fn test_comm_script_preloads_messages() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-INQUE-1").unwrap();
            writeln!(script, "config CM-INQUE-1 queue INQUEUE").unwrap();
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
                    1,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut text_len,
                    b"INQUEUE".as_ptr(),
                    7,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                )
            };
            assert_eq!(rc, 0);
            assert_eq!(text_len, 4);
            assert_eq!(&buf[..4], b"KILL");
        });
    }

    #[test]
    fn test_comm_script_routes_output_to_input() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-OUTQUE-1").unwrap();
            writeln!(script, "enable CM-INQUE-1").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE").unwrap();
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
                    4,
                    0,
                    0,
                    0,
                    b"OUTQUEUE".as_ptr(),
                    8,
                    1,
                    1,
                    std::ptr::null_mut(),
                    0,
                )
            };
            assert_eq!(send_rc, 0);
            assert_eq!(
                unsafe { cobol_comm_message_count(b"CM_INQUE_1".as_ptr(), 10) },
                1
            );
        });
    }

    #[test]
    fn test_comm_send_marks_invalid_destination() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-OUTQUE-1").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE-2").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
            }

            let mut error_key = [b'0'; 2];
            let rc = unsafe {
                cobol_comm_send(
                    b"CM_OUTQUE_1".as_ptr(),
                    11,
                    b"PING".as_ptr(),
                    4,
                    4,
                    0,
                    0,
                    0,
                    b"OUTQUEUE     GARBAGE     ".as_ptr(),
                    12,
                    2,
                    2,
                    error_key.as_mut_ptr(),
                    2,
                )
            };
            assert_eq!(rc, 20);
            assert_eq!(&error_key, b"01");
        });
    }

    #[test]
    fn test_comm_send_rejects_zero_destination_count() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-OUTQUE-1").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
            }

            let rc = unsafe {
                cobol_comm_send(
                    b"CM_OUTQUE_1".as_ptr(),
                    11,
                    b"PING".as_ptr(),
                    4,
                    4,
                    0,
                    0,
                    0,
                    b"OUTQUEUE".as_ptr(),
                    8,
                    0,
                    1,
                    std::ptr::null_mut(),
                    0,
                )
            };
            assert_eq!(rc, 30);
        });
    }

    #[test]
    fn test_comm_receive_message_flattens_segmented_message() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-OUTQUE-1").unwrap();
            writeln!(script, "enable CM-INQUE-1").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE").unwrap();
            writeln!(script, "link CM-OUTQUE-1 CM-INQUE-1").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
                assert_eq!(
                    cobol_comm_send(
                        b"CM_OUTQUE_1".as_ptr(),
                        11,
                        b"HELLO".as_ptr(),
                        5,
                        5,
                        3,
                        0,
                        0,
                        b"OUTQUEUE".as_ptr(),
                        8,
                        1,
                        1,
                        std::ptr::null_mut(),
                        0,
                    ),
                    0
                );
                assert_eq!(
                    cobol_comm_send(
                        b"CM_OUTQUE_1".as_ptr(),
                        11,
                        b"WORLD".as_ptr(),
                        5,
                        5,
                        1,
                        0,
                        0,
                        b"OUTQUEUE".as_ptr(),
                        8,
                        1,
                        1,
                        std::ptr::null_mut(),
                        0,
                    ),
                    0
                );
            }

            let mut buf = [b' '; 16];
            let mut text_len = 0u32;
            let rc = unsafe {
                cobol_comm_receive(
                    b"CM_INQUE_1".as_ptr(),
                    10,
                    1,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut text_len,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                )
            };
            assert_eq!(rc, 0);
            assert_eq!(text_len, 10);
            assert_eq!(&buf[..10], b"HELLOWORLD");
            assert_eq!(
                unsafe { cobol_comm_last_end_key(b"CM_INQUE_1".as_ptr(), 10) },
                0
            );
        });
    }

    #[test]
    fn test_comm_enable_rejects_invalid_destination() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "config CM-OUTQUE-1 key 0001").unwrap();
            writeln!(script, "config CM-OUTQUE-1 dest OUTQUEUE").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
            }

            let key = 1i64.to_ne_bytes();
            let mut error_key = [b'0'; 1];
            let rc = unsafe {
                cobol_comm_enable(
                    b"CM_OUTQUE_1".as_ptr(),
                    11,
                    1,
                    0,
                    key.as_ptr(),
                    key.len() as u32,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    b"GARBAGE".as_ptr(),
                    7,
                    1,
                    1,
                    error_key.as_mut_ptr(),
                    1,
                )
            };
            assert_eq!(rc, 20);
            assert_eq!(&error_key, b"1");
        });
    }

    #[test]
    fn test_comm_enable_accepts_cm101_initial_values() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "config CM-INQUE-1 queue INQUEUE").unwrap();
            writeln!(script, "config CM-INQUE-1 sub1 BLANK").unwrap();
            writeln!(script, "config CM-INQUE-1 sub2 BLANK").unwrap();
            writeln!(script, "config CM-INQUE-1 sub3 BLANK").unwrap();
            writeln!(script, "config CM-INQUE-1 key 0001").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
            }

            let queue = *b"INQUEUE     \0";
            let blank = *b"            \0";
            let key = 1i64.to_ne_bytes();
            let rc = unsafe {
                cobol_comm_enable(
                    b"CM_INQUE_1".as_ptr(),
                    10,
                    0,
                    0,
                    key.as_ptr(),
                    key.len() as u32,
                    queue.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                    0,
                )
            };
            assert_eq!(rc, 0);
        });
    }

    #[test]
    fn test_comm_accept_count_accepts_cm101_initial_values() {
        with_comm_test(|| {
            let mut script = tempfile::NamedTempFile::new().unwrap();
            writeln!(script, "enable CM-INQUE-1").unwrap();
            writeln!(script, "config CM-INQUE-1 queue INQUEUE").unwrap();
            writeln!(script, "config CM-INQUE-1 sub1 BLANK").unwrap();
            writeln!(script, "config CM-INQUE-1 sub2 BLANK").unwrap();
            writeln!(script, "config CM-INQUE-1 sub3 BLANK").unwrap();
            writeln!(script, "message CM-INQUE-1 KILL").unwrap();
            unsafe {
                std::env::set_var("COBOL_COMM_SCRIPT", script.path());
            }

            let queue = *b"INQUEUE     \0";
            let blank = *b"            \0";
            let mut count = 0u32;
            let rc = unsafe {
                cobol_comm_accept_count(
                    b"CM_INQUE_1".as_ptr(),
                    10,
                    &mut count,
                    queue.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                    blank.as_ptr(),
                    12,
                )
            };
            assert_eq!(rc, 0);
            assert_eq!(count, 1);
        });
    }
}
