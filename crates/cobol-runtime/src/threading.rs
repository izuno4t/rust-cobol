// COBOL Runtime - Threading support (COBOL 2023)
//
// Provides basic threading and mutex primitives for COBOL 2023's
// async/threading support. Thread handles and mutex handles are
// stored as opaque u64 values.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

/// Global registry of active threads, keyed by handle.
static THREAD_REGISTRY: OnceLock<Mutex<ThreadRegistry>> = OnceLock::new();

struct ThreadRegistry {
    next_handle: u64,
    threads: HashMap<u64, thread::JoinHandle<()>>,
}

impl ThreadRegistry {
    fn new() -> Self {
        Self {
            next_handle: 1,
            threads: HashMap::new(),
        }
    }
}

fn get_thread_registry() -> &'static Mutex<ThreadRegistry> {
    THREAD_REGISTRY.get_or_init(|| Mutex::new(ThreadRegistry::new()))
}

/// Global registry of mutexes, keyed by handle.
/// Each mutex is an AtomicBool used as a spinlock (false = unlocked, true = locked).
static MUTEX_REGISTRY: OnceLock<Mutex<MutexRegistry>> = OnceLock::new();

struct MutexRegistry {
    next_handle: u64,
    mutexes: HashMap<u64, Arc<AtomicBool>>,
}

impl MutexRegistry {
    fn new() -> Self {
        Self {
            next_handle: 1,
            mutexes: HashMap::new(),
        }
    }
}

fn get_mutex_registry() -> &'static Mutex<MutexRegistry> {
    MUTEX_REGISTRY.get_or_init(|| Mutex::new(MutexRegistry::new()))
}

/// Create a new thread that runs the given function with the given argument.
///
/// Returns a thread handle (non-zero on success, 0 on failure).
///
/// # Safety
/// - `func_ptr` must be a valid function pointer that accepts a `*mut u8` argument.
/// - `arg` must be a valid pointer (or null) that the function can safely use.
/// - The function is responsible for its own memory safety.
#[no_mangle]
pub unsafe extern "C" fn cobol_thread_create(
    func_ptr: extern "C" fn(*mut u8),
    arg: *mut u8,
) -> u64 {
    // Wrap the raw pointer in a Send-able wrapper
    let arg_val = arg as usize;

    let join_handle = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            func_ptr(arg_val as *mut u8);
        }));
        if result.is_err() {
            eprintln!("COBOL thread panicked");
        }
    });

    let mut registry = get_thread_registry().lock().unwrap();
    let handle = registry.next_handle;
    registry.next_handle += 1;
    registry.threads.insert(handle, join_handle);

    handle
}

/// Wait for a thread to complete.
///
/// Returns 0 on success, 1 on failure (e.g., invalid handle, thread panicked).
#[no_mangle]
pub extern "C" fn cobol_thread_join(handle: u64) -> u32 {
    let join_handle = {
        let mut registry = get_thread_registry().lock().unwrap();
        registry.threads.remove(&handle)
    };

    match join_handle {
        Some(jh) => match jh.join() {
            Ok(()) => 0,
            Err(_) => 1,
        },
        None => 1,
    }
}

/// Create a new mutex.
///
/// Returns a mutex handle (non-zero on success).
#[no_mangle]
pub extern "C" fn cobol_mutex_create() -> u64 {
    let mut registry = get_mutex_registry().lock().unwrap();
    let handle = registry.next_handle;
    registry.next_handle += 1;
    registry
        .mutexes
        .insert(handle, Arc::new(AtomicBool::new(false)));
    handle
}

/// Lock a mutex.
///
/// Blocks (spins with yield) until the mutex can be acquired.
/// Does nothing if the handle is invalid.
#[no_mangle]
pub extern "C" fn cobol_mutex_lock(handle: u64) {
    let mutex = {
        let registry = get_mutex_registry().lock().unwrap();
        registry.mutexes.get(&handle).cloned()
    };

    if let Some(m) = mutex {
        // Spin until we successfully set the lock from false (unlocked) to true (locked).
        while m
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            thread::yield_now();
        }
    }
}

/// Unlock a mutex.
///
/// Does nothing if the handle is invalid. The caller must ensure that
/// this is called from the same logical context that called lock.
#[no_mangle]
pub extern "C" fn cobol_mutex_unlock(handle: u64) {
    let mutex = {
        let registry = get_mutex_registry().lock().unwrap();
        registry.mutexes.get(&handle).cloned()
    };

    if let Some(m) = mutex {
        m.store(false, Ordering::Release);
    }
}

/// Destroy a mutex and release its resources.
///
/// Does nothing if the handle is invalid.
#[no_mangle]
pub extern "C" fn cobol_mutex_destroy(handle: u64) {
    let mut registry = get_mutex_registry().lock().unwrap();
    registry.mutexes.remove(&handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    static SINGLE_COUNTER: AtomicI32 = AtomicI32::new(0);
    static MULTI_COUNTER: AtomicI32 = AtomicI32::new(0);

    extern "C" fn increment_single(_arg: *mut u8) {
        SINGLE_COUNTER.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn increment_multi(_arg: *mut u8) {
        MULTI_COUNTER.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn test_thread_create_and_join() {
        let before = SINGLE_COUNTER.load(Ordering::SeqCst);

        let handle = unsafe { cobol_thread_create(increment_single, std::ptr::null_mut()) };

        assert!(handle > 0);

        let result = cobol_thread_join(handle);
        assert_eq!(result, 0);
        assert_eq!(SINGLE_COUNTER.load(Ordering::SeqCst), before + 1);
    }

    #[test]
    fn test_thread_join_invalid_handle() {
        let result = cobol_thread_join(999999);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_mutex_create_and_destroy() {
        let handle = cobol_mutex_create();
        assert!(handle > 0);
        cobol_mutex_destroy(handle);
    }

    #[test]
    fn test_mutex_lock_unlock() {
        let handle = cobol_mutex_create();
        assert!(handle > 0);

        cobol_mutex_lock(handle);
        cobol_mutex_unlock(handle);
        cobol_mutex_destroy(handle);
    }

    #[test]
    fn test_mutex_destroy_invalid() {
        // Should not panic
        cobol_mutex_destroy(999999);
    }

    #[test]
    fn test_mutex_lock_invalid() {
        // Should not panic
        cobol_mutex_lock(999999);
    }

    #[test]
    fn test_multiple_threads() {
        let before = MULTI_COUNTER.load(Ordering::SeqCst);

        let h1 = unsafe { cobol_thread_create(increment_multi, std::ptr::null_mut()) };
        let h2 = unsafe { cobol_thread_create(increment_multi, std::ptr::null_mut()) };

        assert!(h1 > 0);
        assert!(h2 > 0);

        let r1 = cobol_thread_join(h1);
        let r2 = cobol_thread_join(h2);

        assert_eq!(r1, 0);
        assert_eq!(r2, 0);
        assert_eq!(MULTI_COUNTER.load(Ordering::SeqCst), before + 2);
    }
}
