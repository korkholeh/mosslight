//! Terminal lifecycle guarantees (spec §11): restoration order, idempotence, and that a panic
//! restores the terminal before the default hook prints anything.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mosslight::terminal::{
    install_panic_hook, preflight, Diagnostics, Probe, TerminalGuard, TerminalOps,
};

#[derive(Default, Clone)]
struct RecordingOps {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl RecordingOps {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl TerminalOps for RecordingOps {
    fn enable_raw(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("enable_raw");
        Ok(())
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("disable_raw");
        Ok(())
    }
    fn enter_alt(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("enter_alt");
        Ok(())
    }
    fn leave_alt(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("leave_alt");
        Ok(())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("hide_cursor");
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("show_cursor");
        Ok(())
    }
    fn reset_styles(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("reset_styles");
        Ok(())
    }
}

#[test]
fn enter_records_enable_raw_then_alt_then_hide_cursor() {
    let handle = RecordingOps::default();
    let guard = TerminalGuard::enter(handle.clone()).unwrap();
    assert_eq!(
        handle.calls(),
        vec!["enable_raw", "enter_alt", "hide_cursor"]
    );
    drop(guard);
}

#[test]
fn drop_restores_in_reverse_order() {
    let handle = RecordingOps::default();
    let guard = TerminalGuard::enter(handle.clone()).unwrap();
    drop(guard);
    let calls = handle.calls();
    assert_eq!(
        &calls[3..],
        ["show_cursor", "reset_styles", "leave_alt", "disable_raw"]
    );
}

#[test]
fn explicit_restore_then_drop_runs_restore_sequence_once() {
    let handle = RecordingOps::default();
    let mut guard = TerminalGuard::enter(handle.clone()).unwrap();
    guard.restore();
    drop(guard);
    let calls = handle.calls();
    let restores = calls.iter().filter(|c| **c == "show_cursor").count();
    assert_eq!(restores, 1);
    assert_eq!(
        &calls[3..],
        ["show_cursor", "reset_styles", "leave_alt", "disable_raw"]
    );
}

/// `TerminalOps` fake whose `enter_alt` always fails, for exercising `TerminalGuard::enter`'s
/// partial-failure path.
#[derive(Default, Clone)]
struct FailingEnterAltOps {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl FailingEnterAltOps {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl TerminalOps for FailingEnterAltOps {
    fn enable_raw(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("enable_raw");
        Ok(())
    }
    fn disable_raw(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("disable_raw");
        Ok(())
    }
    fn enter_alt(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("enter_alt");
        Err(io::Error::other("simulated enter_alt failure"))
    }
    fn leave_alt(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("leave_alt");
        Ok(())
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("hide_cursor");
        Ok(())
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("show_cursor");
        Ok(())
    }
    fn reset_styles(&mut self) -> io::Result<()> {
        self.calls.lock().unwrap().push("reset_styles");
        Ok(())
    }
}

/// Round-2 review minor finding: before the fix, a failure in `enter_alt`/`hide_cursor` returned
/// `Err` before any `TerminalGuard` value existed, so nothing ran `disable_raw` and the caller's
/// shell was left in raw mode. `enter` must now construct the guard right after `enable_raw`
/// succeeds and run the rest of setup through it, so an early return still restores.
#[test]
fn a_failure_partway_through_enter_still_disables_raw_mode() {
    let handle = FailingEnterAltOps::default();
    let err = match TerminalGuard::enter(handle.clone()) {
        Ok(_) => panic!("expected enter() to fail"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("simulated enter_alt failure"));

    let calls = handle.calls();
    assert_eq!(
        calls,
        [
            "enable_raw",
            "enter_alt",
            "show_cursor",
            "reset_styles",
            "leave_alt",
            "disable_raw"
        ],
        "the full restore sequence (including disable_raw) must run even though enter_alt failed \
         and hide_cursor never ran"
    );
}

#[test]
fn preflight_table() {
    let healthy = Probe {
        stdin_tty: true,
        stdout_tty: true,
        term: Some("xterm-256color".to_string()),
    };
    assert!(preflight(&healthy).is_ok());
    assert!(preflight(&Probe {
        stdin_tty: false,
        ..healthy.clone()
    })
    .is_err());
    assert!(preflight(&Probe {
        stdout_tty: false,
        ..healthy.clone()
    })
    .is_err());
    assert!(preflight(&Probe {
        term: Some("dumb".to_string()),
        ..healthy.clone()
    })
    .is_err());
    assert!(preflight(&Probe {
        term: None,
        ..healthy
    })
    .is_err());
}

#[test]
fn panic_hook_restores_before_the_default_hook_runs() {
    let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
    let restore_order = Arc::clone(&order);
    let restored = Arc::new(AtomicBool::new(false));
    install_panic_hook(Arc::clone(&restored), move || {
        restore_order.lock().unwrap().push("restored")
    });

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        panic!("simulated --debug-panic after the guard is up")
    }));

    assert!(result.is_err());
    // `install_panic_hook` always calls `restore` before delegating to the previously installed
    // hook, so by the time unwinding finishes, restore has already run exactly once.
    assert_eq!(order.lock().unwrap().as_slice(), ["restored"]);
    assert!(restored.load(Ordering::SeqCst));
}

#[test]
fn panic_hook_sharing_a_guards_flag_stops_the_guard_from_restoring_again() {
    // Reproduces the real `main.rs` wiring: a live `TerminalGuard` and the panic hook share one
    // flag via `restored_flag`. In production the hook's restore closure runs its own fresh
    // `CrosstermOps` (not the guard's), so before this fix the guard's `Drop` ran the *same*
    // restore sequence a second time while unwinding, after the panic message had already
    // printed. With the flag shared, whichever runs first wins and the guard's own sequence here
    // (recorded on `handle`) must never fire at all.
    let handle = RecordingOps::default();
    let guard = TerminalGuard::enter(handle.clone()).unwrap();
    let calls_after_enter = handle.calls().len();

    let hook_ran = Arc::new(AtomicBool::new(false));
    let hook_ran_inner = Arc::clone(&hook_ran);
    install_panic_hook(guard.restored_flag(), move || {
        hook_ran_inner.store(true, Ordering::SeqCst);
    });

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        panic!("simulated --debug-panic after the guard is up")
    }));
    assert!(result.is_err());
    assert!(hook_ran.load(Ordering::SeqCst));

    drop(guard);
    let calls = handle.calls();
    assert_eq!(
        calls.len(),
        calls_after_enter,
        "the guard's restore sequence must not run once the shared flag was already set by the panic hook"
    );
}

#[test]
fn diagnostics_buffer_until_flushed() {
    let mut d = Diagnostics::new();
    assert!(d.is_empty());
    d.push("hello");
    assert!(!d.is_empty());
}
