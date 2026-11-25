//! SIGINT/SIGTERM → the process interruption flag.

use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// The process-wide interruption flag the handlers set.
pub fn flag() -> &'static AtomicBool {
    &INTERRUPTED
}

/// Install SIGINT and SIGTERM handlers that only set [`flag`].
pub fn install() -> Result<(), String> {
    extern "C" fn handler(_signal: libc::c_int) {
        // Async-signal-safe: one atomic store, nothing else.
        INTERRUPTED.store(true, Ordering::SeqCst);
    }
    for (name, signal) in [("SIGINT", libc::SIGINT), ("SIGTERM", libc::SIGTERM)] {
        // SAFETY: a zeroed sigaction with an empty mask and a handler that only
        // stores an atomic; the pointer arguments are valid for the call.
        let status = unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handler as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = 0;
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if status != 0 {
            return Err(format!("cannot install the {name} handler"));
        }
    }
    Ok(())
}
