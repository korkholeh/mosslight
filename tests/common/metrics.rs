//! Drives the real `main.rs` loop wiring (`advance_iteration` + `app::draw_due` +
//! `Terminal::draw`) over `ratatui`'s actual `CrosstermBackend`, but writing to a byte-counting
//! `io::Write` instead of stdout — so `tests/metrics.rs` measures the exact write path the shipped
//! binary uses (spec §13's terminal output volume), not a hand-rolled approximation of it.

use std::cell::Cell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

use mosslight::app::{advance_iteration, draw_due, App, Mode, Pacer};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::{Action, Tick, World};
use mosslight::render::{self, Theme};
use mosslight::save::MemorySaveIo;

/// An `io::Write` that only counts bytes and discards them — cheaply `Clone`-able (shares one
/// `Rc<Cell<usize>>`) so the count stays readable after the counter itself is moved into a
/// `CrosstermBackend`.
#[derive(Default, Clone)]
pub struct ByteCounter(Rc<Cell<usize>>);

impl ByteCounter {
    pub fn get(&self) -> usize {
        self.0.get()
    }
}

impl io::Write for ByteCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.set(self.0.get() + buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn fps_variant(fps: u32) -> Fps {
    match fps {
        10 => Fps::F10,
        20 => Fps::F20,
        30 => Fps::F30,
        other => panic!("unsupported fps {other}: only 10, 20 and 30 are valid (spec §12)"),
    }
}

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

/// A headless copy of `main.rs`'s loop, minus terminal setup/input polling: `iterate` runs exactly
/// one `advance_iteration` + `draw_due` + `Terminal::draw` cycle, the same three calls `run` makes
/// per iteration.
pub struct Harness {
    terminal: Terminal<CrosstermBackend<ByteCounter>>,
    app: App,
    pacer: Pacer,
    pending: Vec<Action>,
    theme: Theme,
    counter: ByteCounter,
}

impl Harness {
    /// `size` is the fixed viewport (spec §4's 60x24 minimum and 80x24 both make sense here).
    /// Construction never probes a real terminal size — `Viewport::Fixed` is exactly what lets
    /// this run in CI, which has no TTY.
    pub fn new(size: (u16, u16), fps: u32, seed: u64) -> Self {
        let config = Config {
            glyphs: GlyphSet::Ascii,
            color: ColorMode::Always,
            theme: ThemeName::Gameboy,
            fps: fps_variant(fps),
            save_dir: PathBuf::from("/mosslight-metrics-harness-unused"),
            seed,
            debug_panic: false,
            debug_content: None,
        };
        let app = App::new(&config, world(), Box::new(MemorySaveIo::new()));
        let theme = Theme::new(config.theme, config.color);
        let counter = ByteCounter::default();
        let backend = CrosstermBackend::new(counter.clone());
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, size.0, size.1)),
            },
        )
        .expect("fixed-viewport terminal construction never touches a real TTY");

        Harness {
            terminal,
            app,
            pacer: Pacer::new(fps),
            pending: Vec::new(),
            theme,
            counter,
        }
    }

    /// One main-loop iteration with `elapsed_ns` of wall time and these actions. Mirrors
    /// `main.rs::run`'s body: `advance_iteration`, then draw iff `app::draw_due(&mut app, due)`.
    pub fn iterate(&mut self, elapsed_ns: u64, actions: &[Action]) {
        let due = advance_iteration(
            &mut self.app,
            &mut self.pacer,
            &mut self.pending,
            actions,
            elapsed_ns,
        );
        if draw_due(&mut self.app, due) {
            let app = &self.app;
            let theme = self.theme;
            self.terminal
                .draw(|frame| render::draw(frame, app, theme))
                .expect("draw to a byte-counting backend never fails");
        }
    }

    pub fn bytes(&self) -> usize {
        self.counter.get()
    }

    pub fn mode(&self) -> Mode {
        self.app.mode
    }

    /// Simulated ticks run so far — used to prove output scales with `--fps` while the
    /// simulation itself does not (RISKS #8).
    pub fn ticks(&self) -> Tick {
        self.app.state.tick
    }

    /// Test-only setup, not part of `main.rs`'s own wiring: jumps the hero straight into
    /// `room_id` (as `App::new`'s spawn room has none of the world's enemies — see the phase-3
    /// pacing table) so a byte-budget measurement can pick a room with live enemies without
    /// walking there through `iterate` first.
    pub fn enter_room(&mut self, room_id: &str) {
        let idx = self
            .app
            .state
            .world
            .room_idx(room_id)
            .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
        self.app.state.room = idx;
        self.app.state.enter_room();
    }
}
