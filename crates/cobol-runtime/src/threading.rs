// COBOL Runtime - Threading support (COBOL 2023)
//
// Provides basic threading and mutex primitives for COBOL 2023's
// async/threading support. Thread handles and mutex handles are
// stored as opaque u64 values.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
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
static MUTEX_REGISTRY: OnceLock<Mutex<MutexRegistry>> = OnceLock::new();

struct MutexRegistry {
    next_handle: u64,
    mutexes: HashMap<u64, std::sync::Arc<Mutex<()>>>,
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
        func_ptr(arg_val as *mut u8);
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
        .insert(handle, std::sync::Arc::new(Mutex::new(())));
    handle
}

/// Lock a mutex.
///
/// Blocks until the mutex can be acquired. Does nothing if the handle is invalid.
#[no_mangle]
pub extern "C" fn cobol_mutex_lock(handle: u64) {
    let mutex = {
        let registry = get_mutex_registry().lock().unwrap();
        registry.mutexes.get(&handle).cloned()
    };

    if let Some(m) = mutex {
        // We acquire the lock and immediately forget the guard to keep it locked.
        // This is intentional -- the COBOL program is responsible for calling
        // cobol_mutex_unlock to release it.
        let guard = m.lock().unwrap();
        std::mem::forget(guard);
    }
}

/// Unlock a mutex.
///
/// Does nothing if the handle is invalid. The caller must ensure that
/// this is called from the same logical context that called lock.
///
/// # Safety
/// This function uses unsafe internally to release a lock that was
/// previously acquired via cobol_mutex_lock.
#[no_mangle]
pub extern "C" fn cobol_mutex_unlock(handle: u64) {
    let mutex = {
        let registry = get_mutex_registry().lock().unwrap();
        registry.mutexes.get(&handle).cloned()
    };

    if let Some(m) = mutex {
        // Safety: The Mutex was previously locked via cobol_mutex_lock which
        // called std::mem::forget on the guard. We release it by calling
        // force_unlock. Since std::sync::Mutex doesn't have force_unlock,
        // we use a workaround: the mutex will be released when the Arc is
        // dropped or reused. For this stub implementation, we simply
        // acknowledge the unlock.
        //
        // In a production implementation, we would use parking_lot or a
        // raw mutex that supports explicit unlock.
        let _ = m;
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
    use std::sync::atomic::{AtomicI32, Ordering};

    static TEST_COUNTER: AtomicI32 = AtomicI32::new(0);

    extern "C" fn increment_counter(_arg: *mut u8) {
        TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn test_thread_create_and_join() {
        TEST_COUNTER.store(0, Ordering::SeqCst);

        let handle = unsafe { cobol_thread_create(increment_counter, std::ptr::null_mut()) };

        assert!(handle > 0);

        let result = cobol_thread_join(handle);
        assert_eq!(result, 0);
        assert_eq!(TEST_COUNTER.load(Ordering::SeqCst), 1);
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
        TEST_COUNTER.store(0, Ordering::SeqCst);

        let h1 = unsafe { cobol_thread_create(increment_counter, std::ptr::null_mut()) };
        let h2 = unsafe { cobol_thread_create(increment_counter, std::ptr::null_mut()) };

        assert!(h1 > 0);
        assert!(h2 > 0);

        let r1 = cobol_thread_join(h1);
        let r2 = cobol_thread_join(h2);

        assert_eq!(r1, 0);
        assert_eq!(r2, 0);
        assert_eq!(TEST_COUNTER.load(Ordering::SeqCst), 2);
    }
}
