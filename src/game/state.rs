//! `GameState`: the whole simulated world, plus the pure `update` entry point.

use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

use super::ai;
use super::combat;
use super::entities::{AiState, DialogueState, Enemy, EnemyId, Facing, Hero, ObjectRef, Pos};
use super::puzzles::{self, PlateState};
use super::rng::Rng;
use super::tuning::{
    BAT_HP, BAT_REST_TICKS, GUARDIAN_HP, GUARDIAN_PATROL_TICKS, SLIME_HP, SLIME_IDLE_TICKS,
};
use super::world::{EnemyKind, LockKind, ObjectKind, Reward, Room, RoomIdx, Tile, World};

pub type Tick = u64;

/// What of the world the hero has done so far. `visited` is the only field phase 3 had; this
/// phase adds `opened_chests`, `lit_torches`, `solved_puzzles`, `flags`. `BTreeSet` keeps
/// iteration order deterministic for hashing and saving.
///
/// Deliberately not `Serialize`: `RoomIdx` is a dense index assigned by file order
/// (`loader::intern_room_ids`), so serializing it directly would make a save format built on it
/// invalidated by content reordering — exactly what DECISIONS.md's ids-not-indices rationale for
/// the save format (phase 6) rules out. Phase 6 defines the on-disk shape (ids, not indices)
/// rather than this phase accidentally fixing the wrong one (round-1 review, minor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub visited: BTreeSet<RoomIdx>,
    pub opened_chests: BTreeSet<ObjectRef>,
    pub lit_torches: BTreeSet<ObjectRef>,
    pub solved_puzzles: BTreeSet<ObjectRef>,
    /// Story flags, by authored id — the save vocabulary, few and short.
    pub flags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub tick: Tick,
    pub rng: Rng,
    pub hero: Hero,
    pub enemies: Vec<Enemy>,
    /// Immutable, shared, never part of a save or a state hash — see DECISIONS.md ("plan/02").
    pub world: Rc<World>,
    pub room: RoomIdx,
    pub progress: Progress,
    /// `Some` while a dialogue window is open; suspends the whole per-tick pipeline (see
    /// `update`).
    pub dialogue: Option<DialogueState>,
    /// Which of the current room's plates are currently pressed. Room-local and transient: reset
    /// on every room entry, so an unsolved puzzle resets on re-entry (spec §6).
    pub plates: PlateState,
}

impl GameState {
    /// Builds a fresh state at `world.start`. The world is assumed already validated (content
    /// preflight runs before any `GameState` is constructed), so the start room/spawn resolve.
    pub fn new(seed: u64, world: Rc<World>) -> Self {
        let start_room = world
            .start_room()
            .expect("validated world resolves start.room");
        let spawn = world
            .spawn_pos(start_room, &world.start.spawn)
            .expect("validated world resolves start.spawn");

        let mut visited = BTreeSet::new();
        visited.insert(start_room);

        let mut state = GameState {
            tick: 0,
            rng: Rng::new(seed),
            hero: Hero::at_spawn(spawn),
            enemies: Vec::new(),
            world,
            room: start_room,
            progress: Progress {
                visited,
                ..Progress::default()
            },
            dialogue: None,
            plates: PlateState::default(),
        };
        state.spawn_enemies();
        state
    }

    pub fn room(&self) -> &Room {
        self.world.room(self.room)
    }

    /// Rebuilds `enemies` from the current room's authored spawns, in authored order, so
    /// `EnemyId(i)` is always `enemies[i]`. Called on construction and on every room transition —
    /// enemies are room-local and respawn on re-entry (spec §10: transient combat state is
    /// deliberately discarded).
    pub fn spawn_enemies(&mut self) {
        self.plates = PlateState::default();
        let tick = self.tick;
        let enemies: Vec<Enemy> = self
            .room()
            .enemies
            .iter()
            .enumerate()
            .map(|(i, spawn)| Enemy {
                id: EnemyId(i as u16),
                kind: spawn.kind,
                pos: spawn.at,
                facing: Facing::South,
                hp: initial_hp(spawn.kind),
                ai: initial_ai_state(spawn.kind, tick),
                patrol: spawn.patrol.clone().unwrap_or_default(),
                move_ready_at: tick,
                alive: true,
            })
            .collect();
        self.enemies = enemies;
    }

    /// Whether a `Tile::Hidden` position in `room` has been revealed: some lit torch or solved
    /// puzzle of that room lists it in `reveals`. Any other tile is not asked about here — see
    /// `walkable`.
    pub fn is_revealed(&self, room: RoomIdx, at: Pos) -> bool {
        let r = self.world.room(room);
        let by_torch = self.progress.lit_torches.iter().any(|obj| {
            obj.room == room
                && r.torches
                    .get(obj.index as usize)
                    .is_some_and(|t| t.reveals.contains(&at))
        });
        let by_puzzle = self.progress.solved_puzzles.iter().any(|obj| {
            obj.room == room
                && r.puzzles
                    .get(obj.index as usize)
                    .is_some_and(|p| p.reveals.contains(&at))
        });
        by_torch || by_puzzle
    }

    /// The solid authored object (chest, NPC or torch) at `room`/`at`, if any. Placement never
    /// depends on `progress` — an opened chest is still solid.
    pub fn object_at(&self, room: RoomIdx, at: Pos) -> Option<ObjectKind> {
        self.world.room(room).object_at(at)
    }

    /// The one walkability predicate the simulation uses: in bounds, either an ordinarily
    /// walkable tile or a revealed `Hidden` one, and free of a solid object. Every call site that
    /// used to test `Tile::is_walkable()` directly moves to this (hero movement, enemy AI,
    /// knockback) — `Tile::is_walkable()` itself stays the content-level predicate the validator's
    /// geometry checks use.
    pub fn walkable(&self, room: RoomIdx, at: Pos) -> bool {
        let Some(tile) = self.world.room(room).tile_at(at) else {
            return false;
        };
        let tile_ok = tile.is_walkable() || (tile == Tile::Hidden && self.is_revealed(room, at));
        tile_ok && self.object_at(room, at).is_none()
    }

    /// Every `Hidden` position in `room` revealed by a lit torch or a solved puzzle, as a set.
    /// `is_revealed` answers one position at a time (cheap for the single hero-movement check
    /// each tick); this is for the room-local pathfinding in `ai.rs`/`combat.rs`, which cannot
    /// hold a live `&GameState` borrow while it also holds a `&mut Enemy` split from
    /// `state.enemies` — so those callers compute this set once, up front, from an owned `Rc`
    /// clone of the room instead.
    pub(super) fn revealed_set(&self, room: RoomIdx) -> HashSet<Pos> {
        let r = self.world.room(room);
        let mut set = HashSet::new();
        for obj in &self.progress.lit_torches {
            if obj.room == room {
                if let Some(t) = r.torches.get(obj.index as usize) {
                    set.extend(t.reveals.iter().copied());
                }
            }
        }
        for obj in &self.progress.solved_puzzles {
            if obj.room == room {
                if let Some(p) = r.puzzles.get(obj.index as usize) {
                    set.extend(p.reveals.iter().copied());
                }
            }
        }
        set
    }
}

fn initial_hp(kind: EnemyKind) -> u8 {
    match kind {
        EnemyKind::Slime => SLIME_HP,
        EnemyKind::Bat => BAT_HP,
        EnemyKind::Guardian => GUARDIAN_HP,
    }
}

fn initial_ai_state(kind: EnemyKind, tick: Tick) -> AiState {
    match kind {
        EnemyKind::Slime => AiState::SlimeIdle {
            until: tick + SLIME_IDLE_TICKS,
        },
        EnemyKind::Bat => AiState::BatRest {
            until: tick + BAT_REST_TICKS,
        },
        EnemyKind::Guardian => AiState::GuardianPatrol {
            waypoint: 0,
            until: tick + GUARDIAN_PATROL_TICKS,
        },
    }
}

impl PartialEq for GameState {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
            && self.rng == other.rng
            && self.hero == other.hero
            && self.enemies == other.enemies
            && self.room == other.room
            && self.progress == other.progress
            && self.dialogue == other.dialogue
            && self.plates == other.plates
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveNorth,
    MoveEast,
    MoveSouth,
    MoveWest,
    Attack,
    UseLantern,
    Interact,
    Confirm,
    Cancel,
    ToggleMap,
    ToggleInventory,
    Help,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    HeroMoved {
        from: Pos,
        to: Pos,
    },
    MoveBlocked {
        at: Pos,
        facing: Facing,
    },
    Message(String),
    RoomEntered {
        room: RoomIdx,
        first_visit: bool,
    },
    HeroDamaged {
        remaining_halves: u8,
    },
    HeroDied,
    EnemyDamaged {
        id: EnemyId,
        remaining_hp: u8,
    },
    EnemyKilled {
        id: EnemyId,
        kind: EnemyKind,
        at: Pos,
    },
    AttackSwung {
        at: Option<Pos>,
    },
    AttackEnded,
    AttackDeflected {
        id: EnemyId,
    },
    EnemyMoved {
        id: EnemyId,
        from: Pos,
        to: Pos,
    },
    EnemyAiChanged {
        id: EnemyId,
    },
    ChestOpened {
        chest: ObjectRef,
    },
    ItemPicked {
        reward: Reward,
    },
    DialogueStarted {
        npc: ObjectRef,
    },
    DialogueAdvanced {
        npc: ObjectRef,
        node: u16,
    },
    DialogueEnded {
        npc: ObjectRef,
    },
    FlagSet {
        flag: String,
    },
    TorchLit {
        torch: ObjectRef,
    },
    PassageRevealed {
        room: RoomIdx,
        at: Pos,
    },
    DoorBlocked {
        lock: LockKind,
    },
    PlatePressed {
        index: u16,
    },
    PuzzleSolved {
        puzzle: ObjectRef,
    },
}

fn facing_for(action: Action) -> Option<Facing> {
    match action {
        Action::MoveNorth => Some(Facing::North),
        Action::MoveEast => Some(Facing::East),
        Action::MoveSouth => Some(Facing::South),
        Action::MoveWest => Some(Facing::West),
        _ => None,
    }
}

/// One tile in `facing`'s direction from `pos`, or `None` off-grid (`Pos` is unsigned so only the
/// low edges need a checked op; the high edges are caught by `Room::tile_at`'s bounds-checked
/// `get`). Shared by hero movement, sword hitbox, knockback and AI stepping.
pub(super) fn step_target(pos: Pos, facing: Facing) -> Option<Pos> {
    match facing {
        Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
        Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
        Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
        Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
    }
}

/// The only simulation entry point. Total: never panics, never touches a clock, returns events.
///
/// While `state.dialogue` is `Some`, only `Confirm`/`Cancel` are processed and the six-step
/// per-tick pipeline below does not run at all: nothing moves, no timer fires, no damage lands
/// while a dialogue is open (see `advance_dialogue`).
///
/// Otherwise runs a fixed six-step pipeline per tick (Design, `state.rs`): hero actions, sword
/// resolution, enemy AI, contact damage, death, swing expiry. The order is part of the contract —
/// it is what makes a contested tile deterministic.
///
/// An attempted move always sets facing; a step applies only once the cooldown has elapsed; it is
/// refused off-grid, onto a non-walkable tile (an authored solid object counts as non-walkable,
/// see `GameState::walkable`), or onto a tile a live enemy occupies (round-2 review, minor: an
/// unblocked overlap let the hero's glyph hide the enemy's), in which case facing still updates
/// and a `MoveBlocked` event is emitted. A locked door refuses the step the same way, plus a
/// `DoorBlocked` event and a message. Stepping onto an unlocked door tile relocates the hero to
/// the target room's spawn, grows `progress.visited`, emits `RoomEntered` after `HeroMoved`,
/// clears any live swing (its target tile and hit-list refer to the room just left), rebuilds the
/// enemy vector for the new room and ends the tick immediately: steps 2-5 never run against a
/// room the hero has already left. The door's target is validator-guaranteed to resolve, so an
/// unresolved target leaves the hero standing on the door tile instead of panicking (the
/// simulation is total). A step onto an ordinary tile also runs `puzzles::on_hero_moved`.
///
/// `Attack` starts a sword swing if the hero has the sword and the cooldown has elapsed;
/// without the sword it emits a message and starts nothing (no cooldown paid). `Interact` and
/// `UseLantern` resolve the faced tile (see `handle_interact`/`handle_use_lantern`). The
/// remaining non-movement actions (`ToggleMap`, `ToggleInventory`, `Confirm`, `Cancel`, `Quit`,
/// `Help`) are accepted and ignored — they are `App`-level mode switches.
pub fn update(state: &mut GameState, actions: &[Action], tick: Tick) -> Vec<GameEvent> {
    state.tick = tick;
    let mut events = Vec::new();

    if state.dialogue.is_some() {
        for &action in actions {
            let Some(dialogue) = state.dialogue else {
                break;
            };
            match action {
                Action::Confirm => events.extend(advance_dialogue(state, dialogue)),
                Action::Cancel => {
                    state.dialogue = None;
                    events.push(GameEvent::DialogueEnded { npc: dialogue.npc });
                }
                _ => {}
            }
        }
        return events;
    }

    for &action in actions {
        if let Some(facing) = facing_for(action) {
            state.hero.facing = facing;
            if !state.hero.can_step(tick) {
                continue;
            }
            let from = state.hero.pos;
            let target = step_target(from, facing);
            let walkable = target.is_some_and(|p| state.walkable(state.room, p))
                && target.is_some_and(|p| !state.enemies.iter().any(|e| e.alive && e.pos == p));
            if walkable {
                let to = target.expect("walkable implies a valid target");
                match resolve_door(state, state.room, to) {
                    DoorCheck::Blocked(lock) => {
                        events.push(GameEvent::MoveBlocked { at: from, facing });
                        events.push(GameEvent::Message(door_blocked_message(&lock)));
                        events.push(GameEvent::DoorBlocked { lock });
                    }
                    DoorCheck::Open(transition) => {
                        state.hero.pos = to;
                        state.hero.mark_stepped(tick);
                        events.push(GameEvent::HeroMoved { from, to });
                        state.room = transition.room;
                        state.hero.pos = transition.spawn;
                        let first_visit = state.progress.visited.insert(transition.room);
                        events.push(GameEvent::RoomEntered {
                            room: transition.room,
                            first_visit,
                        });
                        // A live swing carries the pre-transition tile and enemy ids; both are
                        // meaningless (and dangerous) against the new room's rebuilt enemy vector,
                        // so it must not survive the door (round-1 review, major).
                        // `attack_ready_at` is left untouched: the cooldown was already paid and
                        // still applies.
                        state.hero.attack = None;
                        state.spawn_enemies();
                        return events;
                    }
                    DoorCheck::None => {
                        state.hero.pos = to;
                        state.hero.mark_stepped(tick);
                        events.push(GameEvent::HeroMoved { from, to });
                        events.extend(puzzles::on_hero_moved(state, tick));
                    }
                }
            } else {
                events.push(GameEvent::MoveBlocked { at: from, facing });
            }
        } else if action == Action::Attack {
            if !state.hero.has_sword {
                events.push(GameEvent::Message("You have no weapon.".to_string()));
            } else if let Some(event) = combat::start_swing(&mut state.hero, tick) {
                events.push(event);
            }
        } else if action == Action::Interact {
            events.extend(handle_interact(state));
        } else if action == Action::UseLantern {
            events.extend(handle_use_lantern(state));
        }
        // ToggleMap, ToggleInventory, Confirm, Cancel, Quit, Help: no-op here, App-level.
    }

    events.extend(combat::resolve_swing(state, tick));
    events.extend(ai::step(state, tick));
    events.extend(combat::apply_contact_damage(state, tick));
    if combat::expire_swing(&mut state.hero, tick) {
        events.push(GameEvent::AttackEnded);
    }

    if !state.hero.died && state.hero.health_halves == 0 {
        state.hero.died = true;
        events.push(GameEvent::HeroDied);
    }

    events
}

/// Advances an open dialogue by one node on `Confirm`; past the last node, closes it instead.
/// Applies the newly-shown node's `sets_flag`, if any.
fn advance_dialogue(state: &mut GameState, dialogue: DialogueState) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let nodes = state.world.room(dialogue.npc.room).npcs[dialogue.npc.index as usize]
        .dialogue
        .clone();
    let next = dialogue.node + 1;
    if (next as usize) >= nodes.len() {
        state.dialogue = None;
        events.push(GameEvent::DialogueEnded { npc: dialogue.npc });
        return events;
    }
    state.dialogue = Some(DialogueState {
        npc: dialogue.npc,
        node: next,
    });
    events.push(GameEvent::DialogueAdvanced {
        npc: dialogue.npc,
        node: next,
    });
    if let Some(flag) = &nodes[next as usize].sets_flag {
        if state.progress.flags.insert(flag.clone()) {
            events.push(GameEvent::FlagSet { flag: flag.clone() });
        }
    }
    events
}

/// `Interact` on the tile the hero faces: opens a chest, starts a dialogue, or does nothing.
fn handle_interact(state: &mut GameState) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let Some(target) = step_target(state.hero.pos, state.hero.facing) else {
        return events;
    };
    let room_idx = state.room;
    match state.object_at(room_idx, target) {
        Some(ObjectKind::Chest(index)) => {
            let obj = ObjectRef {
                room: room_idx,
                index,
            };
            if state.progress.opened_chests.contains(&obj) {
                events.push(GameEvent::Message("The chest is empty.".to_string()));
            } else {
                let reward = state.world.room(room_idx).chests[index as usize]
                    .contains
                    .clone();
                state.progress.opened_chests.insert(obj);
                events.push(GameEvent::ChestOpened { chest: obj });
                events.extend(apply_reward(&mut state.hero, &reward));
            }
        }
        Some(ObjectKind::Npc(index)) => {
            let npc = &state.world.room(room_idx).npcs[index as usize];
            let satisfied = match &npc.condition {
                None => true,
                Some(flag) => state.progress.flags.contains(flag),
            };
            if !satisfied {
                events.push(GameEvent::Message(
                    "They have nothing to say yet.".to_string(),
                ));
            } else if !npc.dialogue.is_empty() {
                let obj = ObjectRef {
                    room: room_idx,
                    index,
                };
                state.dialogue = Some(DialogueState { npc: obj, node: 0 });
                events.push(GameEvent::DialogueStarted { npc: obj });
                if let Some(flag) = &npc.dialogue[0].sets_flag {
                    if state.progress.flags.insert(flag.clone()) {
                        events.push(GameEvent::FlagSet { flag: flag.clone() });
                    }
                }
            }
        }
        _ => {}
    }
    events
}

/// `UseLantern` on the tile the hero faces: lights an unlit torch, or reports why nothing
/// happened.
fn handle_use_lantern(state: &mut GameState) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let room_idx = state.room;
    let object = step_target(state.hero.pos, state.hero.facing)
        .and_then(|target| state.object_at(room_idx, target));

    match object {
        Some(ObjectKind::Torch(index)) => {
            if !state.hero.has_lantern {
                events.push(GameEvent::Message("You have no lantern.".to_string()));
                return events;
            }
            let obj = ObjectRef {
                room: room_idx,
                index,
            };
            if state.progress.lit_torches.insert(obj) {
                events.push(GameEvent::TorchLit { torch: obj });
                let reveals = state.world.room(room_idx).torches[index as usize]
                    .reveals
                    .clone();
                for at in reveals {
                    events.push(GameEvent::PassageRevealed { room: room_idx, at });
                    events.push(GameEvent::Message("A passage opens.".to_string()));
                }
            }
        }
        _ => events.push(GameEvent::Message("Nothing to light here.".to_string())),
    }
    events
}

/// Applies a granted reward to the hero and reports the effect. `HeartContainer` raises
/// `max_health_halves` by 2, capped at 10 (5 hearts, spec §2), and heals by the same amount
/// without exceeding the new cap. Every variant but `Message` also emits a `Message` naming the
/// pickup (round-2 review, minor): the HUD message row is where the game has been teaching the
/// player to look (the start room's `hint` uses it), so a silent item grant left no feedback there.
/// `Message` grants nothing and only emits the message.
pub(super) fn apply_reward(hero: &mut Hero, reward: &Reward) -> Vec<GameEvent> {
    match reward {
        Reward::Sword => {
            hero.has_sword = true;
            vec![
                GameEvent::ItemPicked {
                    reward: reward.clone(),
                },
                GameEvent::Message("You take the forest sword.".to_string()),
            ]
        }
        Reward::Lantern => {
            hero.has_lantern = true;
            vec![
                GameEvent::ItemPicked {
                    reward: reward.clone(),
                },
                GameEvent::Message("You take the lantern.".to_string()),
            ]
        }
        Reward::SmallKey => {
            hero.keys += 1;
            vec![
                GameEvent::ItemPicked {
                    reward: reward.clone(),
                },
                GameEvent::Message("You take a small key.".to_string()),
            ]
        }
        Reward::HeartContainer => {
            hero.max_health_halves = hero.max_health_halves.saturating_add(2).min(10);
            hero.health_halves = hero
                .health_halves
                .saturating_add(2)
                .min(hero.max_health_halves);
            vec![
                GameEvent::ItemPicked {
                    reward: reward.clone(),
                },
                GameEvent::Message("Your heart grows stronger.".to_string()),
            ]
        }
        Reward::Ember => {
            hero.has_ember = true;
            vec![
                GameEvent::ItemPicked {
                    reward: reward.clone(),
                },
                GameEvent::Message("You take the ancient ember.".to_string()),
            ]
        }
        Reward::Message(text) => vec![GameEvent::Message(text.clone())],
    }
}

struct Transition {
    room: RoomIdx,
    spawn: Pos,
}

enum DoorCheck {
    None,
    Blocked(LockKind),
    Open(Transition),
}

fn door_blocked_message(lock: &LockKind) -> String {
    match lock {
        LockKind::Lantern => "The way is dark; you need a lantern.".to_string(),
        LockKind::Flag(_) => "Something still blocks the way.".to_string(),
        LockKind::SmallKey => "The door is locked.".to_string(),
    }
}

fn resolve_door(state: &GameState, room: RoomIdx, at: Pos) -> DoorCheck {
    let Some(door) = state.world.door_at(room, at) else {
        return DoorCheck::None;
    };
    let passable = match &door.lock {
        None => true,
        Some(LockKind::Lantern) => state.hero.has_lantern,
        Some(LockKind::Flag(flag)) => state.progress.flags.contains(flag),
        // Key consumption is phase 5; no SmallKey door is authored before then.
        Some(LockKind::SmallKey) => false,
    };
    if !passable {
        return DoorCheck::Blocked(door.lock.clone().expect("passable=false implies a lock"));
    }
    let Some(target_room) = state.world.room_idx(&door.to_room) else {
        return DoorCheck::None;
    };
    let Some(spawn) = state.world.spawn_pos(target_room, &door.to_spawn) else {
        return DoorCheck::None;
    };
    DoorCheck::Open(Transition {
        room: target_room,
        spawn,
    })
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Hand-rolled FNV-1a 64 (see DECISIONS.md: `std::collections::hash_map::DefaultHasher` is not
/// stable across Rust releases, and a hash crate would breach the fixed dependency set).
struct StateHasher(u64);

impl StateHasher {
    fn new() -> Self {
        StateHasher(FNV_OFFSET_BASIS)
    }

    fn write_u8(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u8(b);
        }
    }

    fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    fn write_bool(&mut self, v: bool) {
        self.write_u8(u8::from(v));
    }

    fn write_pos(&mut self, p: Pos) {
        self.write_u8(p.x);
        self.write_u8(p.y);
    }

    fn write_facing(&mut self, f: Facing) {
        self.write_u8(facing_code(f));
    }

    fn write_object_ref(&mut self, obj: ObjectRef) {
        self.write_u16(obj.room.0);
        self.write_u16(obj.index);
    }

    fn write_ai(&mut self, ai: AiState) {
        match ai {
            AiState::SlimeIdle { until } => {
                self.write_u8(0);
                self.write_u64(until);
            }
            AiState::SlimeChase { until } => {
                self.write_u8(1);
                self.write_u64(until);
            }
            AiState::BatDart { steps_left, facing } => {
                self.write_u8(2);
                self.write_u8(steps_left);
                self.write_facing(facing);
            }
            AiState::BatRest { until } => {
                self.write_u8(3);
                self.write_u64(until);
            }
            AiState::GuardianPatrol { waypoint, until } => {
                self.write_u8(4);
                self.write_u8(waypoint);
                self.write_u64(until);
            }
            AiState::GuardianTelegraph { until, facing } => {
                self.write_u8(5);
                self.write_u64(until);
                self.write_facing(facing);
            }
            AiState::GuardianDash { steps_left, facing } => {
                self.write_u8(6);
                self.write_u8(steps_left);
                self.write_facing(facing);
            }
            AiState::GuardianRecover { until } => {
                self.write_u8(7);
                self.write_u64(until);
            }
            AiState::SlimeWander { until, facing } => {
                self.write_u8(8);
                self.write_u64(until);
                self.write_facing(facing);
            }
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn facing_code(f: Facing) -> u8 {
    match f {
        Facing::North => 0,
        Facing::East => 1,
        Facing::South => 2,
        Facing::West => 3,
    }
}

fn enemy_kind_code(kind: EnemyKind) -> u8 {
    match kind {
        EnemyKind::Slime => 0,
        EnemyKind::Bat => 1,
        EnemyKind::Guardian => 2,
    }
}

/// A canonical hash of everything that determines the simulation's future: `tick`, `rng`, the
/// hero, every enemy in `Vec` order, the current room and `progress.visited`. `world` contributes
/// only its version, since it is immutable, `Rc`-shared input rather than simulated state (see
/// DECISIONS.md).
pub fn state_hash(state: &GameState) -> u64 {
    let mut h = StateHasher::new();

    h.write_u64(state.tick);
    h.write_u64(state.rng.raw_state());

    h.write_pos(state.hero.pos);
    h.write_facing(state.hero.facing);
    h.write_u8(state.hero.health_halves);
    h.write_u8(state.hero.max_health_halves);
    h.write_u8(state.hero.keys);
    h.write_u64(state.hero.step_ready_at);
    h.write_u64(state.hero.attack_ready_at);
    h.write_u64(state.hero.invuln_until);
    h.write_bool(state.hero.died);
    h.write_bool(state.hero.has_sword);
    h.write_bool(state.hero.has_lantern);
    h.write_bool(state.hero.has_ember);
    match &state.hero.attack {
        None => h.write_bool(false),
        Some(swing) => {
            h.write_bool(true);
            h.write_u64(swing.started_at);
            h.write_facing(swing.facing);
            match swing.at {
                None => h.write_bool(false),
                Some(p) => {
                    h.write_bool(true);
                    h.write_pos(p);
                }
            }
            let mut hit: Vec<u16> = swing.hit.iter().map(|id| id.0).collect();
            hit.sort_unstable();
            h.write_u16(hit.len() as u16);
            for id in hit {
                h.write_u16(id);
            }
        }
    }

    h.write_u16(state.room.0);

    h.write_u32(state.enemies.len() as u32);
    for enemy in &state.enemies {
        h.write_u16(enemy.id.0);
        h.write_u8(enemy_kind_code(enemy.kind));
        h.write_pos(enemy.pos);
        h.write_facing(enemy.facing);
        h.write_u8(enemy.hp);
        h.write_bool(enemy.alive);
        h.write_u64(enemy.move_ready_at);
        h.write_ai(enemy.ai);
    }

    h.write_u32(state.progress.visited.len() as u32);
    for room in &state.progress.visited {
        h.write_u16(room.0);
    }

    h.write_u32(state.progress.opened_chests.len() as u32);
    for obj in &state.progress.opened_chests {
        h.write_object_ref(*obj);
    }

    h.write_u32(state.progress.lit_torches.len() as u32);
    for obj in &state.progress.lit_torches {
        h.write_object_ref(*obj);
    }

    h.write_u32(state.progress.solved_puzzles.len() as u32);
    for obj in &state.progress.solved_puzzles {
        h.write_object_ref(*obj);
    }

    h.write_u32(state.progress.flags.len() as u32);
    for flag in &state.progress.flags {
        h.write_u32(flag.len() as u32);
        h.write_bytes(flag.as_bytes());
    }

    match &state.dialogue {
        None => h.write_bool(false),
        Some(dialogue) => {
            h.write_bool(true);
            h.write_object_ref(dialogue.npc);
            h.write_u16(dialogue.node);
        }
    }

    h.write_u32(state.plates.pressed.len() as u32);
    for &index in &state.plates.pressed {
        h.write_u16(index);
    }

    h.write_u32(state.world.version);

    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GameState {
        let world = Rc::new(crate::content::load().expect("embedded world validates"));
        GameState::new(1, world)
    }

    #[test]
    fn stepping_onto_a_door_tile_changes_room_and_lands_on_a_walkable_tile() {
        let mut state = fresh();
        let start_room = state.room;

        // room.lighthouse's north door sits at (12, 0); one tile south of it is walkable floor.
        state.hero.pos = Pos { x: 12, y: 1 };
        let door_at = state
            .world
            .door_at(start_room, Pos { x: 12, y: 0 })
            .expect("room.lighthouse has a north door in the real world");
        let target_room = state
            .world
            .room_idx(&door_at.to_room)
            .expect("validated door target resolves");

        let events = update(&mut state, &[Action::MoveNorth], 0);

        assert_eq!(state.room, target_room);
        assert_ne!(state.room, start_room);
        assert!(state
            .room()
            .tile_at(state.hero.pos)
            .is_some_and(Tile::is_walkable));
        assert_ne!(
            state.room().tile_at(state.hero.pos),
            Some(Tile::Door),
            "arrival tile must not itself be a door"
        );
        assert!(state.progress.visited.contains(&target_room));
        assert!(events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { room, .. } if *room == target_room)));
    }

    #[test]
    fn two_states_built_the_same_way_hash_equal() {
        assert_eq!(state_hash(&fresh()), state_hash(&fresh()));
    }

    type GameStateMutation = Box<dyn Fn(&mut GameState)>;

    #[test]
    fn mutating_any_hashed_field_changes_the_hash() {
        // Table-driven so a field added to `state_hash` later without a mutation here fails
        // loudly instead of the hash silently ignoring a real state change.
        let mutations: Vec<(&str, GameStateMutation)> = vec![
            ("tick", Box::new(|s: &mut GameState| s.tick = 99)),
            (
                "rng",
                Box::new(|s: &mut GameState| {
                    s.rng.next_u64();
                }),
            ),
            (
                "hero.pos",
                Box::new(|s: &mut GameState| s.hero.pos = Pos { x: 3, y: 3 }),
            ),
            (
                "hero.facing",
                Box::new(|s: &mut GameState| s.hero.facing = Facing::West),
            ),
            (
                "hero.health_halves",
                Box::new(|s: &mut GameState| s.hero.health_halves = 1),
            ),
            (
                "hero.max_health_halves",
                Box::new(|s: &mut GameState| s.hero.max_health_halves = 10),
            ),
            ("hero.keys", Box::new(|s: &mut GameState| s.hero.keys = 2)),
            (
                "hero.step_ready_at",
                Box::new(|s: &mut GameState| s.hero.step_ready_at = 7),
            ),
            (
                "hero.attack_ready_at",
                Box::new(|s: &mut GameState| s.hero.attack_ready_at = 7),
            ),
            (
                "hero.invuln_until",
                Box::new(|s: &mut GameState| s.hero.invuln_until = 7),
            ),
            (
                "hero.died",
                Box::new(|s: &mut GameState| s.hero.died = true),
            ),
            (
                "hero.attack",
                Box::new(|s: &mut GameState| {
                    s.hero.attack = Some(super::super::entities::Swing {
                        started_at: 0,
                        facing: Facing::North,
                        at: Some(Pos { x: 1, y: 1 }),
                        hit: vec![EnemyId(0)],
                    });
                }),
            ),
            (
                "room",
                Box::new(|s: &mut GameState| {
                    let other = RoomIdx(if s.room.0 == 0 { 1 } else { 0 });
                    s.room = other;
                }),
            ),
            (
                "progress.visited",
                Box::new(|s: &mut GameState| {
                    s.progress.visited.insert(RoomIdx(0));
                    s.progress.visited.insert(RoomIdx(1));
                }),
            ),
            (
                "hero.has_sword",
                Box::new(|s: &mut GameState| s.hero.has_sword = true),
            ),
            (
                "hero.has_lantern",
                Box::new(|s: &mut GameState| s.hero.has_lantern = true),
            ),
            (
                "hero.has_ember",
                Box::new(|s: &mut GameState| s.hero.has_ember = true),
            ),
            (
                "progress.opened_chests",
                Box::new(|s: &mut GameState| {
                    s.progress.opened_chests.insert(ObjectRef {
                        room: RoomIdx(0),
                        index: 0,
                    });
                }),
            ),
            (
                "progress.lit_torches",
                Box::new(|s: &mut GameState| {
                    s.progress.lit_torches.insert(ObjectRef {
                        room: RoomIdx(0),
                        index: 0,
                    });
                }),
            ),
            (
                "progress.solved_puzzles",
                Box::new(|s: &mut GameState| {
                    s.progress.solved_puzzles.insert(ObjectRef {
                        room: RoomIdx(0),
                        index: 0,
                    });
                }),
            ),
            (
                "progress.flags",
                Box::new(|s: &mut GameState| {
                    s.progress.flags.insert("flag.x".to_string());
                }),
            ),
            (
                "dialogue",
                Box::new(|s: &mut GameState| {
                    s.dialogue = Some(DialogueState {
                        npc: ObjectRef {
                            room: RoomIdx(0),
                            index: 0,
                        },
                        node: 0,
                    });
                }),
            ),
            (
                "plates.pressed",
                Box::new(|s: &mut GameState| {
                    s.plates.pressed.insert(0);
                }),
            ),
        ];

        for (name, mutate) in mutations {
            let base = fresh();
            let mut mutated = fresh();
            mutate(&mut mutated);
            assert_ne!(
                state_hash(&base),
                state_hash(&mutated),
                "mutating {name} did not change state_hash()"
            );
        }
    }

    #[test]
    fn mutating_an_enemy_field_changes_the_hash() {
        let mut state = fresh();
        state.enemies.push(Enemy {
            id: EnemyId(0),
            kind: EnemyKind::Guardian,
            pos: Pos { x: 5, y: 5 },
            facing: Facing::South,
            hp: 4,
            ai: AiState::GuardianPatrol {
                waypoint: 0,
                until: 100,
            },
            patrol: Vec::new(),
            move_ready_at: 0,
            alive: true,
        });

        type EnemyMutation = Box<dyn Fn(&mut Enemy)>;
        let mutations: Vec<EnemyMutation> = vec![
            Box::new(|e: &mut Enemy| e.pos = Pos { x: 6, y: 6 }),
            Box::new(|e: &mut Enemy| e.facing = Facing::West),
            Box::new(|e: &mut Enemy| e.hp = 0),
            Box::new(|e: &mut Enemy| e.alive = false),
            Box::new(|e: &mut Enemy| e.move_ready_at = 42),
            Box::new(|e: &mut Enemy| e.ai = AiState::GuardianRecover { until: 999 }),
        ];

        for mutate in mutations {
            let mut mutated = state.clone();
            mutate(&mut mutated.enemies[0]);
            assert_ne!(state_hash(&state), state_hash(&mutated));
        }
    }
}
