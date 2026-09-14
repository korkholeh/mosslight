//! Shared headless driver for the overworld/dialogue tests (and phase 5's `tests/playthrough.rs`,
//! which reuses this module unchanged): a fixed-30Hz `App::apply` + `App::tick` step, plus
//! `walk_to`/`face` helpers built on `GameState::walkable` so a test can move the hero to any
//! floor tile without hand-deriving a path.
//!
//! Not every test file that imports this module uses every helper — `#[allow(dead_code)]` on
//! individual items would be noise; the module itself is allowed to have unused items instead.
#![allow(dead_code)]

use std::path::PathBuf;
use std::rc::Rc;

use mosslight::app::App;
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::tuning::{ROOM_H, ROOM_W};
use mosslight::game::{update, Action, Facing, GameEvent, GameState, Pos, Tick, World};

pub fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

pub fn cfg(seed: u64) -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps: Fps::F30,
        save_dir: PathBuf::from("/tmp"),
        seed,
        debug_panic: false,
        debug_content: None,
    }
}

/// A fresh `App` in `Mode::Playing` at the world's start spawn.
pub fn new_game(seed: u64) -> App {
    let mut app = App::new(&cfg(seed), world());
    app.apply(&[Action::Confirm]);
    app
}

/// One loop iteration at the fixed simulation rate: `actions` go through `App::apply` (mode
/// transitions, menu navigation), and whatever lands on the simulation ticks immediately — no
/// pacer, no coalescing, since these tests drive the simulation directly rather than through
/// `main.rs`'s real-time loop (see `app::advance_iteration` for that path, covered by
/// `tests/loop_timing.rs`).
pub fn step(app: &mut App, actions: &[Action]) {
    let sim_actions = app.apply(actions);
    app.tick(&sim_actions);
}

/// A tick with no input.
pub fn idle(app: &mut App) {
    step(app, &[]);
}

fn action_for(facing: Facing) -> Action {
    match facing {
        Facing::North => Action::MoveNorth,
        Facing::South => Action::MoveSouth,
        Facing::East => Action::MoveEast,
        Facing::West => Action::MoveWest,
    }
}

/// Turns the hero to face `facing` without necessarily moving: a movement action always sets
/// facing immediately, even when the step itself is refused (spec §6) — facing a solid object
/// (to then `Interact`/`UseLantern` it) is exactly the blocked case.
pub fn face(app: &mut App, facing: Facing) {
    step(app, &[action_for(facing)]);
}

fn offset(pos: Pos, facing: Facing) -> Option<Pos> {
    match facing {
        Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
        Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
        Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
        Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
    }
}

const DIRS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// Walks the hero to `target` one tile at a time, ticking the simulation at the hero's real step
/// cooldown (spec §6) — a held-key approximation, not a teleport. Panics if `target` is
/// unreachable or the walk does not converge within a generous tick budget (a real bug, not a
/// slow-but-working path, at that point).
pub fn walk_to(app: &mut App, target: Pos) {
    let mut budget = 10_000;
    while app.state.hero.pos != target {
        budget -= 1;
        assert!(
            budget > 0,
            "walk_to({target:?}): did not arrive within the tick budget from {:?}",
            app.state.hero.pos
        );
        let dir = next_step_in(&app.state, target).unwrap_or_else(|| {
            panic!("walk_to({target:?}): no path from {:?}", app.state.hero.pos)
        });
        step(app, &[action_for(dir)]);
    }
}

/// A room-local BFS over `GameState::walkable` from the hero's current tile to `target` (which
/// must itself be a tile the hero can stand on — an object's own tile is solid, so approach it by
/// walking to a neighbouring floor tile and `face`-ing it instead). Returns the first step's
/// facing, or `None` if `target` is already the hero's position or unreachable. Shared by
/// `walk_to` (via `&app.state`) and `Runner::walk_to` (via `&self.state`) — one BFS, not two
/// (round-2 review, minor).
fn next_step_in(state: &GameState, target: Pos) -> Option<Facing> {
    let start = state.hero.pos;
    if start == target {
        return None;
    }
    let mut visited = vec![vec![false; ROOM_W]; ROOM_H];
    let mut first: Vec<Vec<Option<Facing>>> = vec![vec![None; ROOM_W]; ROOM_H];
    visited[start.y as usize][start.x as usize] = true;

    let mut queue = std::collections::VecDeque::new();
    queue.push_back(start);

    while let Some(pos) = queue.pop_front() {
        for &dir in &DIRS {
            let Some(next) = offset(pos, dir) else {
                continue;
            };
            if (next.x as usize) >= ROOM_W || (next.y as usize) >= ROOM_H {
                continue;
            }
            if visited[next.y as usize][next.x as usize] {
                continue;
            }
            if !state.walkable(state.room, next) {
                continue;
            }
            let step_dir = if pos == start {
                dir
            } else {
                first[pos.y as usize][pos.x as usize].expect("visited cells record a first step")
            };
            visited[next.y as usize][next.x as usize] = true;
            first[next.y as usize][next.x as usize] = Some(step_dir);
            if next == target {
                return Some(step_dir);
            }
            queue.push_back(next);
        }
    }
    None
}

/// A headless driver at the `GameState`/`game::update` level, one step below `App` — for tests
/// that need the raw `GameEvent`s a step produced (which `App` folds into mode/message/checkpoint
/// side effects and does not expose). Owns its own tick counter, since `update` takes `Tick`
/// explicitly rather than reading a clock.
pub struct Runner {
    pub state: GameState,
    tick: Tick,
}

impl Runner {
    pub fn new(seed: u64) -> Self {
        Runner {
            state: GameState::new(seed, world()),
            tick: 0,
        }
    }

    pub fn step(&mut self, actions: &[Action]) -> Vec<GameEvent> {
        self.tick += 1;
        update(&mut self.state, actions, self.tick)
    }

    pub fn idle(&mut self) -> Vec<GameEvent> {
        self.step(&[])
    }

    pub fn face(&mut self, facing: Facing) -> Vec<GameEvent> {
        self.step(&[action_for(facing)])
    }

    /// Repeats `action` until `self.state.room` changes — crossing a door takes exactly one real
    /// step, but the hero's step cooldown (spec §6) may still be running down from whatever move
    /// landed them next to it, so a single `step` call can land on a cooldown tick and only
    /// update facing without actually moving. Panics if the room never changes within a generous
    /// tick budget (the door is locked, or `action` does not face it).
    pub fn cross_door(&mut self, action: Action) -> Vec<GameEvent> {
        let start_room = self.state.room;
        let mut events = Vec::new();
        let mut budget = 10_000;
        loop {
            budget -= 1;
            assert!(
                budget > 0,
                "Runner::cross_door({action:?}): room never changed from {start_room:?}"
            );
            events.extend(self.step(&[action]));
            if self.state.room != start_room {
                return events;
            }
        }
    }

    /// Walks to `target` one tile at a time (see `walk_to`), returning every event from every
    /// tick along the way, in order.
    pub fn walk_to(&mut self, target: Pos) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let mut budget = 10_000;
        while self.state.hero.pos != target {
            budget -= 1;
            assert!(
                budget > 0,
                "Runner::walk_to({target:?}): did not arrive within the tick budget from {:?}",
                self.state.hero.pos
            );
            let dir = next_step_in(&self.state, target).unwrap_or_else(|| {
                panic!(
                    "Runner::walk_to({target:?}): no path from {:?}",
                    self.state.hero.pos
                )
            });
            events.extend(self.step(&[action_for(dir)]));
        }
        events
    }
}
