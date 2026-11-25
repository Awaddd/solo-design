//! Raises real signals, so it lives in its own test binary (one test only).

use solo_design::signals::{flag, install};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn wait_for_flag() -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if flag().load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn sigint_and_sigterm_set_the_interruption_flag() {
    let installed = install();
    assert!(installed.is_ok(), "handlers install: {installed:?}");
    for (name, signal) in [("SIGINT", libc::SIGINT), ("SIGTERM", libc::SIGTERM)] {
        flag().store(false, Ordering::SeqCst);
        // SAFETY: raising a signal whose handler was just installed.
        assert_eq!(unsafe { libc::raise(signal) }, 0, "raise {name}");
        assert!(wait_for_flag(), "{name} sets the interruption flag");
    }
}
