//! argv -> Config -> TerminalGuard -> loop -> exit code. The only file that owns a terminal.

use std::io::{self};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::tty::IsTty;
use ratatui::Terminal;

use mosslight::app::{advance_iteration, App, Pacer};
use mosslight::config::{Config, Env, OutputStream};
use mosslight::content;
use mosslight::game::tuning::INPUT_EVENT_HARD_CAP;
use mosslight::game::{Action, World};
use mosslight::input::{apply_overflow_policy, coalesce, drain_ready, map_key, EventSource};
use mosslight::render::{self, Theme};
use mosslight::terminal::{
    self, install_panic_hook, CrosstermOps, Diagnostics, Probe, SignalFlags, TerminalGuard,
    TerminalOps,
};

fn main() -> ExitCode {
    let env = Env::from_process();
    let config = match Config::from_args(std::env::args_os(), &env) {
        Ok(c) => c,
        Err(e) => {
            match e.stream {
                OutputStream::Stdout => println!("{}", e.message),
                OutputStream::Stderr => eprintln!("{}", e.message),
            }
            return exit_code(e.code);
        }
    };

    let world = match content_preflight(&config) {
        Ok(w) => w,
        Err(code) => return exit_code(code),
    };

    let probe = Probe {
        stdin_tty: io::stdin().is_tty(),
        stdout_tty: io::stdout().is_tty(),
        term: std::env::var("TERM").ok(),
    };
    if let Err(e) = terminal::preflight(&probe) {
        eprintln!("{}", e.message);
        return exit_code(e.code);
    }

    let mut diagnostics = Diagnostics::new();
    let code = run(&config, &mut diagnostics, world);

    diagnostics.flush_to_stderr();
    exit_code(code)
}

fn exit_code(code: i32) -> ExitCode {
    u8::try_from(code)
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE)
}

/// Validates the world before anything touches the terminal, so a bad world is reported even in
/// a non-TTY environment (spec §7/§15). `--debug-content PATH` reads that file instead of the
/// embedded string; reading the file is `main`'s job, `content::parse` stays pure. Returns the
/// parsed world so `run`/`App::new` play the exact world this preflight checked, rather than
/// re-parsing (and, with `--debug-content`, potentially playing a *different* world than the one
/// validated — round-1 review). `Err` means errors are already printed to stderr.
fn content_preflight(config: &Config) -> Result<World, i32> {
    let src = match &config.debug_content {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "failed to read --debug-content file {}: {e}",
                    path.display()
                );
                return Err(2);
            }
        },
        None => content::EMBEDDED.to_string(),
    };
    content::parse(&src).map_err(|errors| {
        eprintln!("{}", content::report(&errors));
        2
    })
}

/// Restores the terminal from a bare `CrosstermOps`, independent of any live `TerminalGuard`.
/// Used by the panic hook, which cannot reach the guard living on `run`'s stack.
fn restore_raw_terminal() {
    let mut ops = CrosstermOps;
    let _ = ops.show_cursor();
    let _ = ops.reset_styles();
    let _ = ops.leave_alt();
    let _ = ops.disable_raw();
}

fn run(config: &Config, diagnostics: &mut Diagnostics, world: World) -> i32 {
    let mut guard = match TerminalGuard::enter(CrosstermOps) {
        Ok(g) => g,
        Err(e) => {
            diagnostics.push(format!("failed to enter terminal: {e}"));
            return 1;
        }
    };
    install_panic_hook(guard.restored_flag(), restore_raw_terminal);

    if config.debug_panic {
        panic!("--debug-panic: deliberate panic after the terminal guard is up");
    }

    let signals = match SignalFlags::register() {
        Ok(s) => s,
        Err(e) => {
            diagnostics.push(format!("failed to register signal handlers: {e}"));
            guard.restore();
            return 1;
        }
    };

    let backend = CrosstermBackend::new(io::stdout());
    let mut term = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            diagnostics.push(format!("failed to initialise terminal backend: {e}"));
            guard.restore();
            return 1;
        }
    };

    let mut app = App::new(config, Rc::new(world));
    let theme = Theme::new(config.theme);
    let mut pacer = Pacer::new(config.fps.as_u32());
    let mut last_instant = Instant::now();
    // Simulation actions carry over between iterations until a step actually consumes them; see
    // `advance_iteration`'s doc comment (round-2 review blocker).
    let mut pending: Vec<Action> = Vec::new();

    // Crossterm emits no `Resize` event at startup, so without this probe `Mode::TooSmall` is
    // unreachable until the terminal is resized once; a terminal already too small at launch would
    // render straight into a layout that panics on an out-of-bounds buffer index.
    if let Ok((w, h)) = terminal::size() {
        app.on_resize(w, h);
    }

    let exit_code = 'outer: loop {
        let deadline = Duration::from_nanos(pacer.next_deadline_ns());
        let (events, resized) = match poll_events(deadline, diagnostics) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(format!("input error: {e}"));
                break 'outer 1;
            }
        };

        let raw_actions: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Key(key) => map_key(app.mode, *key),
                _ => None,
            })
            .collect();
        let mut actions = apply_overflow_policy(&raw_actions);
        coalesce(&mut actions);

        if let Some((w, h)) = resized {
            app.on_resize(w, h);
        }

        let now = Instant::now();
        let elapsed_ns = now
            .duration_since(last_instant)
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        last_instant = now;
        let due = advance_iteration(&mut app, &mut pacer, &mut pending, &actions, elapsed_ns);

        if due.draw && app.take_dirty() {
            let draw_result = term.draw(|frame| render::draw(frame, &app, theme));
            if let Err(e) = draw_result {
                diagnostics.push(format!("write error: {e}"));
                break 'outer 1;
            }
        }

        if app.quit.is_some() {
            break 'outer 0;
        }
        if signals.pending() {
            break 'outer 0;
        }
    };

    guard.restore();
    exit_code
}

type PolledEvents = (Vec<Event>, Option<(u16, u16)>);

/// The real `EventSource`: a zero-timeout `crossterm::event::poll`/`read` pair. The blocking wait
/// for the *first* event of an iteration happens separately in `poll_events`, via `event::poll`
/// with the pacer's deadline, so the loop can sleep between iterations instead of busy-polling.
struct CrosstermEvents;

impl EventSource for CrosstermEvents {
    fn next_ready(&mut self) -> io::Result<Option<Event>> {
        if event::poll(Duration::from_millis(0))? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }
}

/// Reads every event available this iteration. Once the first event arrives (or the pacer's
/// deadline elapses), the rest of the currently-buffered queue is drained to exhaustion — leaving
/// any of it queued would let a burst replay as further steps over the following iterations,
/// which is exactly what spec §5 forbids. `drain_ready`'s `hard_cap` is a livelock guard only; the
/// real overflow policy (`apply_overflow_policy`) runs on the mapped actions afterwards. A resize
/// event's last-seen size always survives.
fn poll_events(deadline: Duration, diagnostics: &mut Diagnostics) -> io::Result<PolledEvents> {
    let mut events = Vec::new();
    let mut resized = None;

    if event::poll(deadline)? {
        events.push(event::read()?);
    }
    let drained = drain_ready(&mut CrosstermEvents, INPUT_EVENT_HARD_CAP)?;
    events.extend(drained.events);
    if drained.discarded > 0 {
        diagnostics.push(format!(
            "input overflow: discarded {} events past the per-iteration guard",
            drained.discarded
        ));
    }

    for e in &events {
        if let Event::Resize(w, h) = e {
            resized = Some((*w, *h));
        }
    }

    Ok((events, resized))
}
