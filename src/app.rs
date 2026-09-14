//! Screen/mode state machine, pause semantics, and the clock-free `Pacer` (spec §9).

use std::rc::Rc;

use crate::config::Config;
use crate::game::{update, Action, GameEvent, GameState, Reward, Tick, World};
use crate::input::{coalesce, drop_pending};
use crate::save::{LoadOutcome, RestoreError, SaveFile, SaveIo, StoreOutcome};

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
    Dialogue,
    Map,
    Inventory,
    Victory,
    /// "New Game over an existing run" confirmation (spec §12).
    ConfirmNewGame,
    /// The slot is `Corrupt` or `FutureVersion`: offers a backup restore or a fresh run, and never
    /// writes anything (spec §10).
    SaveProblem,
}

/// What `App` knows about the on-disk slot, probed once at `App::new` and refreshed after every
/// successful `store`/backup restore.
#[derive(Debug, Clone)]
pub enum SlotState {
    Empty,
    Usable(Box<SaveFile>),
    Corrupt { detail: String },
    FutureVersion { found: u32, supported: u32 },
}

impl From<LoadOutcome> for SlotState {
    fn from(outcome: LoadOutcome) -> Self {
        match outcome {
            LoadOutcome::Ok(save) => SlotState::Usable(save),
            LoadOutcome::Missing => SlotState::Empty,
            LoadOutcome::Corrupt { detail, .. } => SlotState::Corrupt { detail },
            LoadOutcome::FutureVersion { found, supported } => {
                SlotState::FutureVersion { found, supported }
            }
        }
    }
}

/// The `Mode::SaveProblem` item list depends on why the slot is unusable (Design §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveProblemAction {
    RestoreBackup,
    NewGame,
    Back,
}

/// One of the four §10 autosave edges. Carried only for the diagnostic message on refusal; the
/// trigger itself is a local decision inside `App::tick` (each event fires at most once by
/// construction, so nothing needs to persist across ticks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveReason {
    RoomTransition,
    ImportantItem,
    PuzzleSolved,
    BossVictory,
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
    pub menu: MenuCursor,
    pub message: String,
    pub size: (u16, u16),
    dirty: bool,
    pub quit: Option<ExitReason>,
    tick_counter: Tick,
    seed: u64,
    world: Rc<World>,
    io: Box<dyn SaveIo>,
    /// What's on disk, probed once at startup and refreshed after every successful `store` or
    /// backup restore — `Continue`/retry/`Mode::SaveProblem` all read this rather than the disk.
    pub slot: SlotState,
    /// Cursor into `save_problem_items()`, reset to 0 whenever `Mode::SaveProblem` opens.
    pub save_problem_cursor: usize,
    /// Save failures buffered for `main` to flush to stderr after the terminal guard drops
    /// (CLAUDE.md: no log line may reach the screen). Drained by `take_diagnostics`.
    diagnostics: Vec<String>,
}

impl App {
    /// `world` is the already-parsed-and-validated world (`main`'s content preflight, or a test's
    /// own `content::load()`) — `App` never re-parses or re-validates content itself, so there is
    /// no unwrap-family call on the content path here (CLAUDE.md) and no risk of gameplay running
    /// against a different world than the one the preflight checked. `io` is a required argument
    /// so `main` cannot forget to inject a real `SaveIo` and silently ship a build that never saves
    /// (Design §4); it is also probed exactly once, here, to fill `slot`.
    pub fn new(cfg: &Config, world: Rc<World>, mut io: Box<dyn SaveIo>) -> Self {
        let state = GameState::new(cfg.seed, Rc::clone(&world));
        let slot = SlotState::from(io.load());
        App {
            mode: Mode::MainMenu,
            prev_mode: Mode::MainMenu,
            state,
            menu: MenuCursor::NewGame,
            message: String::new(),
            size: (MIN_COLS, MIN_ROWS),
            dirty: true,
            quit: None,
            tick_counter: 0,
            seed: cfg.seed,
            world,
            io,
            slot,
            save_problem_cursor: 0,
            diagnostics: Vec::new(),
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

    /// Save failures buffered since the last call, for `main` to flush to stderr after the
    /// terminal guard drops.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    /// The main menu's `Continue` label, slot-dependent (Design §5).
    pub fn continue_label(&self) -> &'static str {
        match &self.slot {
            SlotState::Empty => "Continue (no save yet)",
            SlotState::Usable(_) => "Continue",
            SlotState::Corrupt { .. } => "Continue (save damaged)",
            SlotState::FutureVersion { .. } => "Continue (save is from a newer version)",
        }
    }

    /// The `Mode::SaveProblem` item list: three items over a corrupt slot (a backup may exist),
    /// two over a future-version slot (there is no backup path for "this build is too old").
    pub fn save_problem_items(&self) -> Vec<SaveProblemAction> {
        match &self.slot {
            SlotState::Corrupt { .. } => vec![
                SaveProblemAction::RestoreBackup,
                SaveProblemAction::NewGame,
                SaveProblemAction::Back,
            ],
            SlotState::FutureVersion { .. } => {
                vec![SaveProblemAction::NewGame, SaveProblemAction::Back]
            }
            SlotState::Empty | SlotState::Usable(_) => vec![SaveProblemAction::Back],
        }
    }

    /// Display labels for `save_problem_items()`, in the same order — `NewGame` is annotated over
    /// a future-version slot so the player knows that save is left untouched (Design §5's table).
    pub fn save_problem_labels(&self) -> Vec<&'static str> {
        let future_version = matches!(self.slot, SlotState::FutureVersion { .. });
        self.save_problem_items()
            .into_iter()
            .map(|item| match item {
                SaveProblemAction::RestoreBackup => "Restore backup",
                SaveProblemAction::NewGame if future_version => {
                    "New game (this save will not be overwritten)"
                }
                SaveProblemAction::NewGame => "New game",
                SaveProblemAction::Back => "Back",
            })
            .collect()
    }

    /// Resets `state`/`tick_counter` to a fresh run at the world's start spawn. Shared by every
    /// path that starts a new game (main menu, the `ConfirmNewGame`/`SaveProblem` confirmations,
    /// and a `GameOver` retry with no usable slot) — `tick_counter` is reset here too, since a
    /// fresh `GameState` starts at `tick = 0` and letting `tick_counter` keep counting from a
    /// previous run would immediately desync it from every zeroed hero timer.
    ///
    /// A `Usable` slot is also cleared to `Empty` here: it is the one case where `GameOver`'s
    /// retry would otherwise restore the *abandoned* run instead of starting fresh before the new
    /// run's first autosave (review round 1, minor #2). `Corrupt`/`FutureVersion` are left as they
    /// are — `GameOver`'s retry already treats them the same as `Empty` (a fresh start, never a
    /// restore), so there is no resurrection risk to guard against, and clearing them would only
    /// make `continue_label`/the message row misreport a real on-disk problem as "no save yet".
    fn start_new_game(&mut self) {
        if matches!(self.slot, SlotState::Usable(_)) {
            self.slot = SlotState::Empty;
        }
        self.state = GameState::new(self.seed, Rc::clone(&self.world));
        self.tick_counter = self.state.tick;
        // The start room's hint never fires a `RoomEntered` event (it is the initial room, not one
        // the hero transitions into — see `GameState::new`), so a fresh run shows it directly.
        if let Some(hint) = &self.state.room().hint {
            self.message = hint.clone();
        }
    }

    /// Applies `save.restore(...)` onto `self.state`/`tick_counter`. On success, reports how many
    /// unknown ids were dropped (if any) in the message row and diagnostics, and returns `true`. A
    /// `RestoreError` — the saved room itself no longer resolving, or that room having no authored
    /// spawn — is treated exactly like a corrupt slot (ADR 0006's "never silently unloadable" cuts
    /// both ways: a save this build cannot make sense of must not silently vanish either).
    fn try_restore(&mut self, save: &SaveFile) -> bool {
        match save.restore(Rc::clone(&self.world), self.seed) {
            Ok(restored) => {
                if !restored.dropped_ids.is_empty() {
                    self.message = format!(
                        "Save loaded ({} unknown id(s) dropped).",
                        restored.dropped_ids.len()
                    );
                    self.diagnostics.push(format!(
                        "save: dropped unknown ids on load: {:?}",
                        restored.dropped_ids
                    ));
                }
                self.state = restored.state;
                self.tick_counter = self.state.tick;
                true
            }
            Err(RestoreError::UnknownRoom(id)) => {
                self.slot = SlotState::Corrupt {
                    detail: format!("saved room {id:?} no longer exists"),
                };
                self.save_problem_cursor = 0;
                self.set_mode(Mode::SaveProblem);
                false
            }
            Err(RestoreError::NoSafeSpawn(id)) => {
                self.slot = SlotState::Corrupt {
                    detail: format!("room {id:?} has no authored spawn"),
                };
                self.save_problem_cursor = 0;
                self.set_mode(Mode::SaveProblem);
                false
            }
        }
    }

    /// `Mode::SaveProblem`'s `Restore backup` item: loads `save.json.bak` and, if it restores
    /// cleanly, replaces the slot and resumes play from it. Never writes anything — a failed
    /// restore only updates the message row.
    fn restore_backup(&mut self) {
        match self.io.load_backup() {
            LoadOutcome::Ok(save) => {
                if self.try_restore(&save) {
                    self.slot = SlotState::Usable(save);
                    self.set_mode(Mode::Playing);
                }
            }
            LoadOutcome::Missing => {
                self.message = "No backup is available.".to_string();
                self.dirty = true;
            }
            LoadOutcome::Corrupt { .. } => {
                self.message = "The backup save is also damaged.".to_string();
                self.dirty = true;
            }
            LoadOutcome::FutureVersion { .. } => {
                self.message = "The backup save is from a newer version.".to_string();
                self.dirty = true;
            }
        }
    }

    /// Captures the current state and writes it through `io`, updating `slot` on success and
    /// reporting a refusal or failure in the message row (and, for a failure, in diagnostics) —
    /// spec §10's "a save failure never ends the session": play always continues either way.
    fn save_now(&mut self, ok_message: &str) {
        let save = SaveFile::capture(&self.state);
        match self.io.store(&save) {
            StoreOutcome::Ok => {
                self.slot = SlotState::Usable(Box::new(save));
                self.message = ok_message.to_string();
            }
            StoreOutcome::RefusedFutureVersion { .. } => {
                self.message =
                    "Save not written: the existing save is from a newer version.".to_string();
            }
            StoreOutcome::RefusedDeathState => {
                // Unreachable via the four autosave triggers (a `HeroDied` batch clears the
                // pending save before this is called) and via manual save (refused earlier by
                // `in_combat`, which is true whenever `health_halves == 0` would even arise this
                // tick) — kept as a real match arm rather than a `panic!`, since `SaveIo` is a
                // trait boundary this code cannot prove every implementation respects.
                self.message = "Cannot save right now.".to_string();
            }
            StoreOutcome::Failed { detail } => {
                self.message = "Save failed; will try again later.".to_string();
                self.diagnostics.push(format!("save failed: {detail}"));
            }
        }
        self.dirty = true;
    }

    /// One of the four §10 autosave edges. Unlike a manual save, a successful autosave stays
    /// silent on the message row — it must not steal a more interesting line the same tick already
    /// set (the room's hint, a pickup's own flavour text) — but a refusal or a failure still needs
    /// the player's attention, so those cases do report.
    fn autosave(&mut self, _reason: SaveReason) {
        let save = SaveFile::capture(&self.state);
        match self.io.store(&save) {
            StoreOutcome::Ok => {
                self.slot = SlotState::Usable(Box::new(save));
            }
            StoreOutcome::RefusedFutureVersion { .. } => {
                self.message =
                    "Save not written: the existing save is from a newer version.".to_string();
                self.dirty = true;
            }
            StoreOutcome::RefusedDeathState => {
                // The `died` guard in `tick` clears `pending_save` before this is ever reached.
                self.diagnostics
                    .push("autosave: unexpected death-state refusal".to_string());
            }
            StoreOutcome::Failed { detail } => {
                self.message = "Save failed; will try again later.".to_string();
                self.diagnostics.push(format!("autosave failed: {detail}"));
                self.dirty = true;
            }
        }
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
                Mode::Dialogue => self.apply_dialogue(action),
                Mode::Map => self.apply_overlay(action, Mode::Map, Action::ToggleMap),
                Mode::Inventory => {
                    self.apply_overlay(action, Mode::Inventory, Action::ToggleInventory)
                }
                Mode::Victory => self.apply_victory(action),
                Mode::ConfirmNewGame => self.apply_confirm_new_game(action),
                Mode::SaveProblem => self.apply_save_problem(action),
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
            Action::MoveNorth => {
                self.menu = self.menu.prev();
                self.dirty = true;
            }
            Action::MoveSouth => {
                self.menu = self.menu.next();
                self.dirty = true;
            }
            Action::Confirm => match self.menu {
                MenuCursor::Continue => self.continue_from_menu(),
                MenuCursor::NewGame => self.new_game_from_menu(),
                MenuCursor::Help => self.set_mode(Mode::Help),
                MenuCursor::Quit => self.set_mode(Mode::ConfirmQuit),
            },
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn continue_from_menu(&mut self) {
        match self.slot.clone() {
            SlotState::Empty => {
                self.message = "No save yet".to_string();
                self.dirty = true;
            }
            SlotState::Usable(save) => {
                if self.try_restore(&save) {
                    self.set_mode(Mode::Playing);
                }
            }
            SlotState::Corrupt { .. } | SlotState::FutureVersion { .. } => {
                self.save_problem_cursor = 0;
                self.set_mode(Mode::SaveProblem);
            }
        }
    }

    /// New Game over a `Usable` or `FutureVersion` slot requires confirmation (spec §12: "New
    /// Game over an existing playthrough requires confirmation"); an `Empty` or `Corrupt` slot has
    /// no existing progress to lose, so it starts directly, as before this phase.
    fn new_game_from_menu(&mut self) {
        match &self.slot {
            SlotState::Usable(_) | SlotState::FutureVersion { .. } => {
                self.set_mode(Mode::ConfirmNewGame);
            }
            SlotState::Empty | SlotState::Corrupt { .. } => {
                self.start_new_game();
                self.set_mode(Mode::Playing);
            }
        }
    }

    fn apply_confirm_new_game(&mut self, action: Action) {
        match action {
            Action::Confirm => {
                self.start_new_game();
                self.set_mode(Mode::Playing);
            }
            Action::Cancel => self.set_mode(Mode::MainMenu),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn apply_save_problem(&mut self, action: Action) {
        let items = self.save_problem_items();
        match action {
            Action::MoveNorth => {
                self.save_problem_cursor = if self.save_problem_cursor == 0 {
                    items.len() - 1
                } else {
                    self.save_problem_cursor - 1
                };
                self.dirty = true;
            }
            Action::MoveSouth => {
                self.save_problem_cursor = (self.save_problem_cursor + 1) % items.len();
                self.dirty = true;
            }
            Action::Confirm => match items[self.save_problem_cursor] {
                SaveProblemAction::RestoreBackup => self.restore_backup(),
                SaveProblemAction::NewGame => {
                    self.start_new_game();
                    self.set_mode(Mode::Playing);
                }
                SaveProblemAction::Back => self.set_mode(Mode::MainMenu),
            },
            Action::Cancel => self.set_mode(Mode::MainMenu),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn apply_playing(&mut self, action: Action, sim_actions: &mut Vec<Action>) {
        match action {
            Action::Cancel => self.set_mode(Mode::Paused),
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            Action::ToggleMap => self.set_mode(Mode::Map),
            Action::ToggleInventory => self.set_mode(Mode::Inventory),
            other => sim_actions.push(other),
        }
    }

    /// `Map` and `Inventory` share one shape: `Cancel` or the key that opened them closes back to
    /// `Playing`, `Quit` still reaches `ConfirmQuit`, everything else is ignored — both are
    /// read-only screens over `self.state`.
    fn apply_overlay(&mut self, action: Action, mode: Mode, toggle: Action) {
        debug_assert_eq!(self.mode, mode);
        if action == Action::Cancel || action == toggle {
            self.set_mode(Mode::Playing);
        } else if action == Action::Quit {
            self.set_mode(Mode::ConfirmQuit);
        }
    }

    /// Routes `Confirm`/`Cancel` straight into `update` at the *current* tick (not an incremented
    /// one), so a dialogue responds to the keypress within the same iteration and `tick_counter`
    /// provably does not advance while it is open (see `game::state::update`'s dialogue branch).
    /// `DialogueEnded` closes back to `Playing`; the existing `mode_before != Playing && mode ==
    /// Playing` rule in `apply` then drops the rest of the batch, so a movement key queued behind
    /// the closing `Confirm` cannot leak into gameplay the same iteration.
    fn apply_dialogue(&mut self, action: Action) {
        match action {
            Action::Confirm | Action::Cancel => {
                let events = update(&mut self.state, &[action], self.tick_counter);
                self.dirty |= !events.is_empty();
                if events
                    .iter()
                    .any(|e| matches!(e, GameEvent::DialogueEnded { .. }))
                {
                    self.set_mode(Mode::Playing);
                }
            }
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    /// `Confirm` (E/Enter, already mapped) is a manual save, refused during combat (spec §10) —
    /// `Mode::Paused` freezes the simulation, so `self.state.in_combat()` reflects the tick play
    /// actually stopped on, not a stale earlier one.
    fn apply_paused(&mut self, action: Action) {
        match action {
            Action::Cancel => self.set_mode(Mode::Playing),
            Action::Help => self.set_mode(Mode::Help),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            Action::Confirm => {
                if self.state.in_combat() {
                    self.message = "Cannot save during combat.".to_string();
                    self.dirty = true;
                } else {
                    self.save_now("Game saved.");
                }
            }
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

    /// `Confirm`/`Cancel` both return to the main menu — the run that just ended is over, exactly
    /// like `apply_game_over`'s `Cancel`. `Quit` still routes through `ConfirmQuit`.
    fn apply_victory(&mut self, action: Action) {
        match action {
            Action::Confirm | Action::Cancel => self.set_mode(Mode::MainMenu),
            Action::Quit => self.set_mode(Mode::ConfirmQuit),
            _ => {}
        }
    }

    fn apply_help(&mut self, action: Action) {
        match action {
            Action::Cancel | Action::Help => self.set_mode(self.prev_mode),
            _ => {}
        }
    }

    /// `Confirm` retries from the last autosave (phase 3's in-memory checkpoint, repointed at the
    /// save file — see DECISIONS.md) rather than a fresh hero; with no usable slot it starts a
    /// fresh run instead, with a message saying so. Never writes anything: a death state is never
    /// saved (§10), so retry can only ever read. `Cancel` returns to the main menu rather than to
    /// `Playing`, since the run that just ended is over.
    fn apply_game_over(&mut self, action: Action) {
        match action {
            Action::Confirm => match self.slot.clone() {
                SlotState::Usable(save) => {
                    if self.try_restore(&save) {
                        self.set_mode(Mode::Playing);
                    }
                }
                SlotState::Empty | SlotState::Corrupt { .. } | SlotState::FutureVersion { .. } => {
                    self.start_new_game();
                    self.message = "No save to retry from; starting a new run.".to_string();
                    self.set_mode(Mode::Playing);
                }
            },
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

        // Folded out of this tick's events rather than kept as a field across ticks: each of the
        // four autosave-triggering events fires at most once by construction (they are edges —
        // entering a room, picking up an item, solving a puzzle, defeating the boss — not level
        // state), so nothing needs to survive past the point this tick resolves it (Design §5).
        let mut pending_save: Option<SaveReason> = None;
        let mut died = false;
        for event in &events {
            match event {
                GameEvent::RoomEntered { room, .. } => {
                    pending_save = Some(SaveReason::RoomTransition);
                    // The §7 teaching prompt: a room's authored hint replaces the message row on
                    // entry, e.g. the start room's movement/interaction prompt.
                    if let Some(hint) = &self.state.world.room(*room).hint {
                        self.message = hint.clone();
                    }
                }
                GameEvent::ItemPicked { reward } => {
                    // `Reward::Message` is lore, not an item (Design §5's trigger table).
                    if !matches!(reward, Reward::Message(_)) {
                        pending_save = Some(SaveReason::ImportantItem);
                    }
                }
                GameEvent::PuzzleSolved { .. } => pending_save = Some(SaveReason::PuzzleSolved),
                GameEvent::BossDefeated { .. } => pending_save = Some(SaveReason::BossVictory),
                GameEvent::HeroDied => {
                    died = true;
                    self.set_mode(Mode::GameOver);
                }
                GameEvent::GameWon => self.set_mode(Mode::Victory),
                GameEvent::DialogueStarted { .. } => self.set_mode(Mode::Dialogue),
                GameEvent::Message(text) => self.message = text.clone(),
                _ => {}
            }
        }

        // The §10 death rule, enforced twice (here and in `save::store`): a death state must never
        // overwrite the last usable save, even if the same tick's batch also crossed a room
        // boundary or picked up an item.
        if died {
            pending_save = None;
        }
        if let Some(reason) = pending_save {
            self.autosave(reason);
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

/// The draw gate: a frame is drawn only when the pacer says a frame is due **and** something
/// changed since the last draw. Consumes `app`'s dirty bit via `take_dirty`, so it must be called
/// exactly once per loop iteration — calling it twice would silently swallow the second draw.
pub fn draw_due(app: &mut App, due: Due) -> bool {
    due.draw && app.take_dirty()
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
