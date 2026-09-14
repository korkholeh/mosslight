//! Screen/mode state machine, pause semantics, and the clock-free `Pacer` (spec §9).

use std::rc::Rc;

use crate::config::Config;
use crate::game::{update, Action, GameEvent, GameState, Tick, World};
use crate::input::{coalesce, drop_pending};

pub const MIN_COLS: u16 = 60;
pub const MIN_ROWS: u16 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    MainMenu,
    Playing,
    Paused,
    ConfirmQuit,
    Help,
    TooSmall,
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuCursor {
    Continue,
    NewGame,
    Help,
    Quit,
}

impl MenuCursor {
    fn next(self) -> Self {
        match self {
            MenuCursor::Continue => MenuCursor::NewGame,
            MenuCursor::NewGame => MenuCursor::Help,
            MenuCursor::Help => MenuCursor::Quit,
            MenuCursor::Quit => MenuCursor::Continue,
        }
    }

    fn prev(self) -> Self {
        match self {
            MenuCursor::Continue => MenuCursor::Quit,
            MenuCursor::NewGame => MenuCursor::Continue,
            MenuCursor::Help => MenuCursor::NewGame,
            MenuCursor::Quit => MenuCursor::Help,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    Confirmed,
}

pub struct App {
    pub mode: Mode,
    prev_mode: Mode,
    pub state: GameState,
    /// The state as of the last room entry (New Game counts as entering the start room). Retry
    /// from `Mode::GameOver` restores this rather than the save file — phase 6 repoints the
    /// restore at the save and keeps this rewind rule (see DECISIONS.md).
    checkpoint: GameState,
    pub menu: MenuCursor,
    pub message: String,
    pub size: (u16, u16),
    dirty: bool,
    pub quit: Option<ExitReason>,
    tick_counter: Tick,
    seed: u64,
    world: Rc<World>,
}

impl App {
    /// `world` is the already-parsed-and-validated world (`main`'s content preflight, or a test's
    /// own `content::load()`) — `App` never re-parses or re-validates content itself, so there is
    /// no unwrap-family call on the content path here (CLAUDE.md) and no risk of gameplay running
    /// against a different world than the one the preflight checked.
    pub fn new(cfg: &Config, world: Rc<World>) -> Self {
        let state = GameState::new(cfg.seed, Rc::clone(&world));
        let checkpoint = state.clone();
        App {
            mode: Mode::MainMenu,
            prev_mode: Mode::MainMenu,
            state,
            checkpoint,
            menu: MenuCursor::NewGame,
            message: String::new(),
            size: (MIN_COLS, MIN_ROWS),
            dirty: true,
            quit: None,
            tick_counter: 0,
            seed: cfg.seed,
            world,
        }
    }

    pub fn simulating(&self) -> bool {
        self.mode == Mode::Playing
    }

    pub fn take_dirty(&mut self) -> bool {
        let d = self.dirty;
        self.dirty = false;
        d
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.prev_mode = self.mode;
            self.mode = mode;
            self.dirty = true;
        }
    }

    /// Applies mode transitions and menu navigation, and collects the actions meant for the
    /// simulation this iteration (only ever non-empty while already `Playing`). If some action in
    /// the batch closes an overlay and resumes `Playing`, everything collected so far is dropped
    /// and the rest of the batch is ignored — a menu/dialogue close must not let a movement key
    /// queued behind it leak into gameplay the same iteration (spec §5).
    pub fn apply(&mut self, actions: &[Action]) -> Vec<Action> {
        let mut sim_actions = Vec::new();
        let mut suppress_rest = false;
        for &action in actions {
            if suppress_rest {
                continue;
            }
            let mode_before = self.mode;
            match self.mode {
                Mode::MainMenu => self.apply_main_menu(action),
                Mode::Playing => self.apply_playing(action, &mut sim_actions),
                Mode::Paused => self.apply_paused(action),
                Mode::ConfirmQuit => self.apply_confirm_quit(action),
                Mode::Help => self.apply_help(action),
                Mode::TooSmall => self.apply_too_small(action),
                Mode::GameOver => self.apply_game_over(action),
            }
            if mode_before != Mode::Playing && self.mode == Mode::Playing {
                drop_pending(&mut sim_actions);
                suppress_rest = true;
            }
        }
        sim_actions
    }

    fn apply_main_menu(&mut self, action: Action) {
        match action {
            Action::MoveNorth => self.menu = self.menu.prev(),
            Action::MoveSouth => self.menu = self.menu.next(),
            Action::Confirm => match self.menu {
                MenuCursor::Continue => {
                    self.message = "No save yet".to_string();
                    self.dirty = true;
                }
                MenuCursor::NewGame => {
                    self.state = GameState::new(self.seed, Rc::clone(&self.world));
                    self.checkpoint = self.state.clone();
                    self.set_mode(Mode::Playing);
                }
                MenuCursor::Help => self.set_mode(Mode::Help),
                MenuCursor::Quit => self.set_mode(Mode::ConfirmQuit),
            },
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn apply_playing(&mut self, action: Action, sim_actions: &mut Vec<Action>) {
        match action {
            Action::Cancel => self.set_mode(Mode::Paused),
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            other => sim_actions.push(other),
        }
    }

    fn apply_paused(&mut self, action: Action) {
        match action {
            Action::Cancel => self.set_mode(Mode::Playing),
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn apply_confirm_quit(&mut self, action: Action) {
        match action {
            Action::Confirm => self.quit = Some(ExitReason::Confirmed),
            Action::Cancel => self.set_mode(self.prev_mode),
            _ => {}
        }
    }

    fn apply_help(&mut self, action: Action) {
        match action {
            Action::Cancel | Action::Help => self.set_mode(self.prev_mode),
            _ => {}
        }
    }

    /// `Confirm` retries from the last room-entry checkpoint (health included) and rewinds
    /// `tick_counter` to match — every simulation timer is an absolute `Tick`, so resuming at the
    /// checkpoint's tick while the counter kept advancing would fire every cooldown,
    /// invulnerability window and AI timer at once (see DECISIONS.md). `Cancel` returns to the
    /// main menu rather than to `Playing`, since the run that just ended is over.
    fn apply_game_over(&mut self, action: Action) {
        match action {
            Action::Confirm => {
                self.state = self.checkpoint.clone();
                self.tick_counter = self.state.tick;
                self.set_mode(Mode::Playing);
            }
            Action::Cancel => self.set_mode(Mode::MainMenu),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    /// A confirm-quit dialog cannot be rendered at a too-small size, so `Quit` (including the
    /// Ctrl+C mapping) quits directly instead of routing through `ConfirmQuit` — spec §11 requires
    /// Ctrl+C to be a graceful-shutdown request in every mode, and a shrunk terminal must not strand
    /// the player with no keyboard way out.
    fn apply_too_small(&mut self, action: Action) {
        if action == Action::Quit {
            self.quit = Some(ExitReason::Confirmed);
        }
    }

    /// Runs `game::update` for one simulation tick, but only in `Mode::Playing`; the tick counter
    /// does not advance otherwise. `actions` are the actions to apply on this step (the caller
    /// passes `&[]` for catch-up steps beyond the first, so one keypress cannot multiply into
    /// several steps after a stall). Returns whether the step actually changed anything; `dirty`
    /// is set only in that case, so a `Playing` tick with nothing to report does not force a draw
    /// (CLAUDE.md: "do not draw when nothing changed" — round-2 review).
    pub fn tick(&mut self, actions: &[Action]) -> bool {
        if !self.simulating() {
            return false;
        }
        self.tick_counter += 1;
        let events = update(&mut self.state, actions, self.tick_counter);
        let changed = !events.is_empty();
        self.dirty |= changed;

        for event in &events {
            match event {
                GameEvent::RoomEntered { .. } => self.checkpoint = self.state.clone(),
                GameEvent::HeroDied => self.set_mode(Mode::GameOver),
                _ => {}
            }
        }

        changed
    }

    pub fn on_resize(&mut self, w: u16, h: u16) {
        self.size = (w, h);
        let too_small = w < MIN_COLS || h < MIN_ROWS;
        if too_small {
            self.set_mode(Mode::TooSmall);
        } else if self.mode == Mode::TooSmall {
            // Recovery never resumes straight into Playing (spec §9): the hero must not take an
            // unseen hit while the terminal was too small to render it.
            if self.prev_mode == Mode::Playing {
                self.set_mode(Mode::Paused);
            } else {
                let restored = self.prev_mode;
                self.set_mode(restored);
            }
        }
    }
}

/// What the pacer decided is due for one loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Due {
    pub sim_steps: u32,
    pub draw: bool,
}

/// Clock-free pacing arithmetic: elapsed time arrives as integer nanoseconds, so
/// fps-independence is provable without a terminal or a real clock.
pub struct Pacer {
    tick_interval_ns: u64,
    frame_interval_ns: u64,
    sim_acc_ns: u64,
    frame_acc_ns: u64,
}

const NANOS_PER_SEC: u64 = 1_000_000_000;

impl Pacer {
    pub fn new(fps: u32) -> Self {
        Pacer {
            tick_interval_ns: NANOS_PER_SEC / crate::game::tuning::TICK_HZ,
            frame_interval_ns: NANOS_PER_SEC / u64::from(fps.max(1)),
            sim_acc_ns: 0,
            frame_acc_ns: 0,
        }
    }

    /// Advances the accumulators by `elapsed_ns`, capping simulation catch-up at
    /// `MAX_CATCHUP_STEPS` and discarding any surplus accumulated time (spec §9).
    pub fn advance(&mut self, elapsed_ns: u64) -> Due {
        self.sim_acc_ns += elapsed_ns;
        self.frame_acc_ns += elapsed_ns;

        let mut sim_steps = 0u32;
        while self.sim_acc_ns >= self.tick_interval_ns
            && sim_steps < crate::game::tuning::MAX_CATCHUP_STEPS
        {
            self.sim_acc_ns -= self.tick_interval_ns;
            sim_steps += 1;
        }
        // Surplus beyond MAX_CATCHUP_STEPS is discarded, not carried forward.
        if sim_steps == crate::game::tuning::MAX_CATCHUP_STEPS {
            self.sim_acc_ns = 0;
        }

        let draw = if self.frame_acc_ns >= self.frame_interval_ns {
            self.frame_acc_ns %= self.frame_interval_ns;
            true
        } else {
            false
        };

        Due { sim_steps, draw }
    }

    /// The deadline `main.rs` should pass to `event::poll`: time left until the next tick.
    pub fn next_deadline_ns(&self) -> u64 {
        self.tick_interval_ns.saturating_sub(self.sim_acc_ns)
    }
}

/// One loop iteration's bookkeeping, shared by `main.rs` and its tests. Applies `actions` to
/// `app` (mode transitions such as menu navigation or Esc/Q happen immediately, every iteration);
/// whatever lands on the simulation is merged into `pending` and coalesced there, *not* consumed
/// yet. `pending` survives across calls, so a keypress that arrives in an iteration whose elapsed
/// time does not cross a tick boundary (`due.sim_steps == 0`, the normal case for a keypress) is
/// not silently dropped — it is only cleared once a simulation step actually runs.
///
/// Round-2 review blocker: the previous `main.rs` loop recomputed sim actions as a local on every
/// iteration and threw them away whenever `due.sim_steps == 0`, which made movement barely
/// functional over a real terminal. Extracting this into a free function also makes the loop's
/// timing-dependent wiring itself testable without a PTY (see `tests/loop_timing.rs`).
pub fn advance_iteration(
    app: &mut App,
    pacer: &mut Pacer,
    pending: &mut Vec<Action>,
    actions: &[Action],
    elapsed_ns: u64,
) -> Due {
    let sim_actions = app.apply(actions);
    pending.extend(sim_actions);
    coalesce(pending);

    let due = pacer.advance(elapsed_ns);
    for step in 0..due.sim_steps {
        if step == 0 {
            app.tick(pending);
            pending.clear();
        } else {
            app.tick(&[]);
        }
    }
    due
}
