//! Terminal lifecycle: raw mode, alternate screen, the RAII guard, the panic hook, signal flags,
//! and the size probe (spec §11, ADR 0007).
//!
//! `TerminalOps` is the seam that makes restoration testable without a PTY: `CrosstermOps` is the
//! real implementation, `RecordingOps` (in tests) records the call sequence instead.

use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ratatui::crossterm::terminal as ct_terminal;
use ratatui::crossterm::{cursor, execute};

/// The terminal side effects a guard owns. A trait so tests can substitute a recording fake.
pub trait TerminalOps {
    fn enable_raw(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn enter_alt(&mut self) -> io::Result<()>;
    fn leave_alt(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn reset_styles(&mut self) -> io::Result<()>;
}

/// The real implementation, over `io::stdout()`, using `ratatui::crossterm` exclusively.
pub struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn enable_raw(&mut self) -> io::Result<()> {
        ct_terminal::enable_raw_mode()
    }

    fn disable_raw(&mut self) -> io::Result<()> {
        ct_terminal::disable_raw_mode()
    }

    fn enter_alt(&mut self) -> io::Result<()> {
        execute!(io::stdout(), ct_terminal::EnterAlternateScreen)
    }

    fn leave_alt(&mut self) -> io::Result<()> {
        execute!(io::stdout(), ct_terminal::LeaveAlternateScreen)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), cursor::Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(io::stdout(), cursor::Show)
    }

    fn reset_styles(&mut self) -> io::Result<()> {
        use ratatui::crossterm::style::{Attribute, SetAttribute};
        execute!(io::stdout(), SetAttribute(Attribute::Reset))
    }
}

/// RAII guard for terminal lifecycle. `restore` is idempotent; `Drop` calls it.
pub struct TerminalGuard<O: TerminalOps> {
    ops: O,
    restored: Arc<AtomicBool>,
}

impl<O: TerminalOps> TerminalGuard<O> {
    /// Enables raw mode, enters the alternate screen and hides the cursor. Does **not** install a
    /// panic hook by itself — call [`install_panic_hook`] with this guard's [`restored_flag`] and
    /// a matching restore closure right after, as `main.rs` does; keeping the two separate is what
    /// lets tests build a guard over `RecordingOps` without mutating the process-global panic hook.
    ///
    /// [`restored_flag`]: TerminalGuard::restored_flag
    pub fn enter(mut ops: O) -> io::Result<Self> {
        ops.enable_raw()?;
        let mut guard = TerminalGuard {
            ops,
            restored: Arc::new(AtomicBool::new(false)),
        };
        // The guard exists before `enter_alt`/`hide_cursor` run, so a failure partway through
        // still unwinds raw mode via `restore` instead of stranding the caller's shell in it
        // (round-2 review: a partial failure here previously had no guard to undo `enable_raw`).
        if let Err(e) = guard.ops.enter_alt().and_then(|()| guard.ops.hide_cursor()) {
            guard.restore();
            return Err(e);
        }
        Ok(guard)
    }

    /// Restores the terminal: show cursor, reset styles, leave alt screen, disable raw mode.
    /// Safe to call more than once, and safe to race with a panic hook sharing the same
    /// [`restored_flag`] — only the first caller to flip the flag has any effect.
    pub fn restore(&mut self) {
        if self.restored.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.ops.show_cursor();
        let _ = self.ops.reset_styles();
        let _ = self.ops.leave_alt();
        let _ = self.ops.disable_raw();
    }

    /// A flag shared with [`install_panic_hook`] so whichever of "the guard drops" or "a panic
    /// unwinds" runs its restore sequence first, the other becomes a no-op instead of emitting the
    /// escape sequences a second time after the panic message has already printed.
    pub fn restored_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.restored)
    }
}

impl<O: TerminalOps> Drop for TerminalGuard<O> {
    fn drop(&mut self) {
        self.restore();
    }
}

/// Installs a panic hook that calls `restore` first, then the previously installed hook (so the
/// panic message itself prints after the terminal is usable again). `restored` should be the same
/// flag as the live `TerminalGuard`'s [`TerminalGuard::restored_flag`], so unwinding through the
/// guard's `Drop` afterwards does not run the restore sequence a second time. Not idempotent
/// across several different `restore` closures — `main.rs` calls it exactly once, right after
/// [`TerminalGuard::enter`].
pub fn install_panic_hook(restored: Arc<AtomicBool>, restore: impl Fn() + Send + Sync + 'static) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !restored.swap(true, Ordering::SeqCst) {
            restore();
        }
        previous(info);
    }));
}

/// What `preflight` inspects before raw mode is ever enabled.
#[derive(Debug, Clone)]
pub struct Probe {
    pub stdin_tty: bool,
    pub stdout_tty: bool,
    pub term: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupError {
    pub message: String,
    pub code: i32,
}

/// Refuses to start without a usable terminal (spec §3): both stdin and stdout must be a TTY,
/// and `TERM` must be set to something other than `dumb`. Runs before any raw-mode call.
///
/// `TERM` is checked first so the two refusal messages stay distinguishable in a spawned child
/// process that never has a TTY at all (a TTY cannot be handed to a spawned test process, so the
/// `TERM=dumb` half of the end-to-end coverage can only reach this branch if it runs first; see
/// `tests/config.rs::term_dumb_exits_2_with_one_stderr_line_and_no_stdout` and DECISIONS.md).
pub fn preflight(p: &Probe) -> Result<(), StartupError> {
    match p.term.as_deref() {
        None | Some("") => {
            return Err(StartupError {
                message: "mosslight requires TERM to be set".to_string(),
                code: 2,
            })
        }
        Some("dumb") => {
            return Err(StartupError {
                message: "mosslight does not support TERM=dumb".to_string(),
                code: 2,
            })
        }
        _ => {}
    }
    if !p.stdin_tty || !p.stdout_tty {
        return Err(StartupError {
            message: "mosslight requires an interactive terminal (stdin/stdout must be a TTY)"
                .to_string(),
            code: 2,
        });
    }
    Ok(())
}

/// Current terminal size in columns, rows.
pub fn size() -> io::Result<(u16, u16)> {
    ct_terminal::size()
}

/// Registers SIGTERM/SIGHUP as atomic flags polled by the main loop (spec §11): no file I/O runs
/// inside the signal handler itself.
pub struct SignalFlags {
    term: Arc<AtomicBool>,
    hup: Arc<AtomicBool>,
}

impl SignalFlags {
    pub fn register() -> io::Result<Self> {
        let term = Arc::new(AtomicBool::new(false));
        let hup = Arc::new(AtomicBool::new(false));
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&term))?;
        signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&hup))?;
        Ok(SignalFlags { term, hup })
    }

    pub fn pending(&self) -> bool {
        self.term.load(Ordering::Relaxed) || self.hup.load(Ordering::Relaxed)
    }
}

/// Diagnostics are buffered in memory and flushed to stderr only after the guard has dropped
/// (spec §11): no log line may ever reach the game screen.
#[derive(Debug, Default)]
pub struct Diagnostics {
    lines: Vec<String>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics::default()
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn flush_to_stderr(&self) {
        for line in &self.lines {
            eprintln!("{line}");
        }
    }
}
