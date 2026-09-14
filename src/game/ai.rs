//! Room-local navigation plus the three per-kind enemy state machines (spec §6).
//!
//! `path_step`/`local_step` never consult the RNG, so navigation is deterministic independent of
//! seed (RISKS #14); the RNG is used only for slime wander direction, bat dart direction out of
//! range, and is threaded through the guardian for symmetry even though it makes no random choice
//! today.

use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use super::entities::{AiState, BossPattern, Enemy, Facing, Pos};
use super::rng::Rng;
use super::state::{step_target, GameEvent, GameState, Tick};
use super::tuning::{
    BAT_AGGRO_RADIUS, BAT_DART_STEPS, BAT_REST_TICKS, BAT_STEP_TICKS, BOSS_P1_STALK_TICKS,
    BOSS_P1_STEP_TICKS, BOSS_P1_VULNERABLE_TICKS, BOSS_P1_WINDUP_TICKS, BOSS_P2_STALK_TICKS,
    BOSS_P2_STEP_TICKS, BOSS_P2_VULNERABLE_TICKS, BOSS_P2_WINDUP_TICKS, BOSS_PHASE_TWO_HP,
    GUARDIAN_DASH_STEP_TICKS, GUARDIAN_DASH_TILES, GUARDIAN_PATROL_STEP_TICKS,
    GUARDIAN_PATROL_TICKS, GUARDIAN_RECOVER_TICKS, GUARDIAN_SIGHT, GUARDIAN_TELEGRAPH_TICKS,
    ROOM_H, ROOM_W, SLIME_AGGRO_RADIUS, SLIME_CHASE_TICKS, SLIME_IDLE_TICKS, SLIME_STEP_TICKS,
};
use super::world::{EnemyKind, Room, Tile};

const DIRS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// The room-local walkability check every enemy pathfinder uses: `GameState::walkable`'s rule
/// (ordinarily walkable, or a `Hidden` tile revealed this room, and free of a solid object),
/// expressed against an owned `room`/`revealed` pair instead of a live `&GameState` borrow — see
/// `GameState::revealed_set`.
fn tile_walkable(room: &Room, revealed: &HashSet<Pos>, pos: Pos) -> bool {
    let Some(tile) = room.tile_at(pos) else {
        return false;
    };
    let tile_ok = tile.is_walkable() || (tile == Tile::Hidden && revealed.contains(&pos));
    tile_ok && room.object_at(pos).is_none()
}

/// The tile-occupancy every enemy sees this tick: the hero's tile, every other live enemy's
/// position (never `exclude`'s own, reflecting moves already applied earlier this same tick), and
/// every current-room block (round-1 review, major: a block is solid to the hero via
/// `GameState::walkable`, and must be equally solid to enemy pathfinding/stepping, or an enemy can
/// stand on a block's tile).
pub fn occupied_positions(state: &GameState, exclude: super::entities::EnemyId) -> HashSet<Pos> {
    let mut set = HashSet::new();
    set.insert(state.hero.pos);
    for enemy in &state.enemies {
        if enemy.alive && enemy.id != exclude {
            set.insert(enemy.pos);
        }
    }
    set.extend(state.puzzle.blocks.iter().copied());
    set
}

/// Breadth-first search over the current room only, bounded to `ROOM_W * ROOM_H` cells with a
/// fixed N/E/S/W expansion order. Returns the first step of the shortest path from `from` to
/// `to`, or `None` if `from == to` or no path exists. `to` itself is exempt from the occupancy
/// check (it is typically the hero's own tile) so a path can always be found up to the target;
/// callers are responsible for not actually stepping onto an occupied tile (see `try_move`).
pub fn path_step(
    room: &Room,
    revealed: &HashSet<Pos>,
    occupied: &HashSet<Pos>,
    from: Pos,
    to: Pos,
) -> Option<Facing> {
    if from == to {
        return None;
    }
    // Total by construction, not by caller discipline (round-1 review, minor): every position
    // handed to a room-local BFS must fit the fixed-size grids below, or indexing them panics.
    if (from.x as usize) >= ROOM_W
        || (from.y as usize) >= ROOM_H
        || (to.x as usize) >= ROOM_W
        || (to.y as usize) >= ROOM_H
    {
        return None;
    }

    let mut visited = [[false; ROOM_W]; ROOM_H];
    let mut first_step: [[Option<Facing>; ROOM_W]; ROOM_H] = [[None; ROOM_W]; ROOM_H];
    visited[from.y as usize][from.x as usize] = true;

    let mut queue = VecDeque::new();
    queue.push_back(from);

    while let Some(pos) = queue.pop_front() {
        for &dir in &DIRS {
            let Some(next) = step_target(pos, dir) else {
                continue;
            };
            if (next.y as usize) >= ROOM_H || (next.x as usize) >= ROOM_W {
                continue;
            }
            if visited[next.y as usize][next.x as usize] {
                continue;
            }
            let is_goal = next == to;
            let walkable = tile_walkable(room, revealed, next);
            if !is_goal && (!walkable || occupied.contains(&next)) {
                continue;
            }

            let step_dir = if pos == from {
                dir
            } else {
                // Every visited non-start cell records a first step when it is marked visited
                // below, so this is unreachable in practice; skip rather than panic if it ever
                // isn't (total by construction, round-1 review, minor).
                let Some(step_dir) = first_step[pos.y as usize][pos.x as usize] else {
                    continue;
                };
                step_dir
            };
            visited[next.y as usize][next.x as usize] = true;
            first_step[next.y as usize][next.x as usize] = Some(step_dir);

            if is_goal {
                return Some(step_dir);
            }
            queue.push_back(next);
        }
    }
    None
}

/// Total fallback when `path_step` finds nothing: try the axis with the larger delta first, then
/// the other, then no move. Never enters `to`'s tile.
pub fn local_step(
    room: &Room,
    revealed: &HashSet<Pos>,
    occupied: &HashSet<Pos>,
    from: Pos,
    to: Pos,
) -> Option<Facing> {
    let dx = to.x as i32 - from.x as i32;
    let dy = to.y as i32 - from.y as i32;
    if dx == 0 && dy == 0 {
        return None;
    }

    let x_dir = if dx >= 0 { Facing::East } else { Facing::West };
    let y_dir = if dy >= 0 {
        Facing::South
    } else {
        Facing::North
    };
    let order = if dx.abs() >= dy.abs() {
        [x_dir, y_dir]
    } else {
        [y_dir, x_dir]
    };

    for dir in order {
        let Some(next) = step_target(from, dir) else {
            continue;
        };
        if (next.y as usize) >= ROOM_H || (next.x as usize) >= ROOM_W {
            continue;
        }
        let walkable = tile_walkable(room, revealed, next);
        if walkable && (next == to || !occupied.contains(&next)) {
            return Some(dir);
        }
    }
    None
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.x as i32 - b.x as i32).abs() + (a.y as i32 - b.y as i32).abs()
}

fn on_facing_axis(from: Pos, to: Pos, facing: Facing) -> bool {
    match facing {
        Facing::North => to.x == from.x && to.y <= from.y,
        Facing::South => to.x == from.x && to.y >= from.y,
        Facing::East => to.y == from.y && to.x >= from.x,
        Facing::West => to.y == from.y && to.x <= from.x,
    }
}

fn opposite(facing: Facing) -> Facing {
    match facing {
        Facing::North => Facing::South,
        Facing::South => Facing::North,
        Facing::East => Facing::West,
        Facing::West => Facing::East,
    }
}

fn random_facing(rng: &mut Rng) -> Facing {
    match rng.below(4) {
        0 => Facing::North,
        1 => Facing::East,
        2 => Facing::South,
        _ => Facing::West,
    }
}

/// Attempts to step `enemy` one tile in `facing`; always updates `enemy.facing`. Returns whether
/// the move actually happened (blocked moves still cost the caller's move-interval cooldown).
fn try_move(
    enemy: &mut Enemy,
    room: &Room,
    revealed: &HashSet<Pos>,
    occupied: &HashSet<Pos>,
    facing: Facing,
    events: &mut Vec<GameEvent>,
) -> bool {
    enemy.facing = facing;
    let Some(target) = step_target(enemy.pos, facing) else {
        return false;
    };
    let walkable = tile_walkable(room, revealed, target);
    if !walkable || occupied.contains(&target) {
        return false;
    }
    let from = enemy.pos;
    enemy.pos = target;
    events.push(GameEvent::EnemyMoved {
        id: enemy.id,
        from,
        to: target,
    });
    true
}

#[allow(clippy::too_many_arguments)]
fn step_slime(
    enemy: &mut Enemy,
    room: &Room,
    revealed: &HashSet<Pos>,
    hero_pos: Pos,
    occupied: &HashSet<Pos>,
    rng: &mut Rng,
    tick: Tick,
    events: &mut Vec<GameEvent>,
) {
    match enemy.ai {
        AiState::SlimeIdle { until } => {
            if tick >= enemy.move_ready_at {
                let dir = random_facing(rng);
                try_move(enemy, room, revealed, occupied, dir, events);
                enemy.move_ready_at = tick + SLIME_STEP_TICKS;
            }
            if tick >= until {
                enemy.ai = if manhattan(enemy.pos, hero_pos) <= SLIME_AGGRO_RADIUS {
                    AiState::SlimeChase {
                        until: tick + SLIME_CHASE_TICKS,
                    }
                } else {
                    AiState::SlimeWander {
                        until: tick + SLIME_IDLE_TICKS,
                        facing: random_facing(rng),
                    }
                };
            }
        }
        AiState::SlimeWander { until, facing } => {
            if tick >= enemy.move_ready_at {
                if !try_move(enemy, room, revealed, occupied, facing, events) {
                    // Redraw only when blocked, so the committed direction is what makes this
                    // phase behaviourally distinct from SlimeIdle's per-step redraw.
                    let redrawn = random_facing(rng);
                    enemy.ai = AiState::SlimeWander {
                        until,
                        facing: redrawn,
                    };
                }
                enemy.move_ready_at = tick + SLIME_STEP_TICKS;
            }
            if tick >= until {
                enemy.ai = if manhattan(enemy.pos, hero_pos) <= SLIME_AGGRO_RADIUS {
                    AiState::SlimeChase {
                        until: tick + SLIME_CHASE_TICKS,
                    }
                } else {
                    AiState::SlimeIdle {
                        until: tick + SLIME_IDLE_TICKS,
                    }
                };
            }
        }
        AiState::SlimeChase { until } => {
            if tick >= enemy.move_ready_at {
                if let Some(dir) = path_step(room, revealed, occupied, enemy.pos, hero_pos)
                    .or_else(|| local_step(room, revealed, occupied, enemy.pos, hero_pos))
                {
                    try_move(enemy, room, revealed, occupied, dir, events);
                }
                enemy.move_ready_at = tick + SLIME_STEP_TICKS;
            }
            if tick >= until {
                enemy.ai = AiState::SlimeIdle {
                    until: tick + SLIME_IDLE_TICKS,
                };
            }
        }
        _ => unreachable!("a Slime enemy never carries a non-slime AiState"),
    }
}

#[allow(clippy::too_many_arguments)]
fn step_bat(
    enemy: &mut Enemy,
    room: &Room,
    revealed: &HashSet<Pos>,
    hero_pos: Pos,
    occupied: &HashSet<Pos>,
    rng: &mut Rng,
    tick: Tick,
    events: &mut Vec<GameEvent>,
) {
    match enemy.ai {
        AiState::BatDart { steps_left, facing } => {
            if tick >= enemy.move_ready_at {
                let moved = try_move(enemy, room, revealed, occupied, facing, events);
                enemy.move_ready_at = tick + BAT_STEP_TICKS;
                let remaining = steps_left.saturating_sub(1);
                enemy.ai = if !moved || remaining == 0 {
                    AiState::BatRest {
                        until: tick + BAT_REST_TICKS,
                    }
                } else {
                    AiState::BatDart {
                        steps_left: remaining,
                        facing,
                    }
                };
            }
        }
        AiState::BatRest { until } => {
            if tick >= until {
                let in_range = manhattan(enemy.pos, hero_pos) <= BAT_AGGRO_RADIUS;
                let facing = if in_range {
                    path_step(room, revealed, occupied, enemy.pos, hero_pos)
                        .or_else(|| local_step(room, revealed, occupied, enemy.pos, hero_pos))
                        .unwrap_or_else(|| random_facing(rng))
                } else {
                    random_facing(rng)
                };
                enemy.ai = AiState::BatDart {
                    steps_left: BAT_DART_STEPS,
                    facing,
                };
            }
        }
        _ => unreachable!("a Bat enemy never carries a non-bat AiState"),
    }
}

#[allow(clippy::too_many_arguments)]
fn step_guardian(
    enemy: &mut Enemy,
    room: &Room,
    revealed: &HashSet<Pos>,
    hero_pos: Pos,
    occupied: &HashSet<Pos>,
    _rng: &mut Rng,
    tick: Tick,
    events: &mut Vec<GameEvent>,
) {
    match enemy.ai {
        AiState::GuardianPatrol { waypoint, until } => {
            if tick >= enemy.move_ready_at {
                if enemy.patrol.is_empty() {
                    let dir = enemy.facing;
                    if !try_move(enemy, room, revealed, occupied, dir, events) {
                        enemy.facing = opposite(dir);
                    }
                } else {
                    let target = enemy.patrol[waypoint as usize % enemy.patrol.len()];
                    if enemy.pos == target {
                        let next_wp = (waypoint + 1) % enemy.patrol.len() as u8;
                        enemy.ai = AiState::GuardianPatrol {
                            waypoint: next_wp,
                            until,
                        };
                    } else if let Some(dir) = path_step(room, revealed, occupied, enemy.pos, target)
                        .or_else(|| local_step(room, revealed, occupied, enemy.pos, target))
                    {
                        try_move(enemy, room, revealed, occupied, dir, events);
                    }
                }
                enemy.move_ready_at = tick + GUARDIAN_PATROL_STEP_TICKS;
            }

            let sees_hero = on_facing_axis(enemy.pos, hero_pos, enemy.facing)
                && manhattan(enemy.pos, hero_pos) <= GUARDIAN_SIGHT;
            if sees_hero || tick >= until {
                enemy.ai = AiState::GuardianTelegraph {
                    until: tick + GUARDIAN_TELEGRAPH_TICKS,
                    facing: enemy.facing,
                };
            }
        }
        AiState::GuardianTelegraph { until, facing } => {
            if tick >= until {
                enemy.facing = facing;
                enemy.ai = AiState::GuardianDash {
                    steps_left: GUARDIAN_DASH_TILES,
                    facing,
                };
            }
        }
        AiState::GuardianDash { steps_left, facing } => {
            if tick >= enemy.move_ready_at {
                let moved = try_move(enemy, room, revealed, occupied, facing, events);
                enemy.move_ready_at = tick + GUARDIAN_DASH_STEP_TICKS;
                let remaining = steps_left.saturating_sub(1);
                enemy.ai = if !moved || remaining == 0 {
                    AiState::GuardianRecover {
                        until: tick + GUARDIAN_RECOVER_TICKS,
                    }
                } else {
                    AiState::GuardianDash {
                        steps_left: remaining,
                        facing,
                    }
                };
            }
        }
        AiState::GuardianRecover { until } => {
            if tick >= until {
                enemy.ai = AiState::GuardianPatrol {
                    waypoint: 0,
                    until: tick + GUARDIAN_PATROL_TICKS,
                };
            }
        }
        _ => unreachable!("a Guardian enemy never carries a non-guardian AiState"),
    }
}

fn boss_pattern_for_phase(phase: u8) -> BossPattern {
    if phase == 1 {
        BossPattern::Slam
    } else {
        BossPattern::Sweep
    }
}

/// A boss enemy should never carry a non-boss `AiState` (only `initial_ai_state` writes a Boss's
/// initial state today), but `game::update` is total by contract — never panics — and phase 6
/// deserializes `AiState` from a save file, where a corrupt or hand-edited slot could reach this.
/// Recovers to phase 1 rather than panicking (round-1 review, minor); `step_boss`'s own match
/// makes the matching recovery to the state itself.
fn boss_ai_phase(ai: AiState) -> u8 {
    match ai {
        AiState::BossStalk { phase, .. }
        | AiState::BossWindup { phase, .. }
        | AiState::BossStrike { phase, .. }
        | AiState::BossVulnerable { phase, .. } => phase,
        _ => 1,
    }
}

/// The tiles a boss strike of `pattern` centred on `pos` would hit (spec §6: a distinct pattern
/// per phase, asserted on the tile sets rather than the variant name). `Slam` is `pos` and its
/// four orthogonal neighbours; `Sweep` is the full row and column through `pos`, each direction
/// stopped at the first `Tile::Wall` (a door, floor or hazard tile does not block the line).
/// Shared by `step_boss` (telegraph tiles) and `combat::apply_boss_strike` (hit tiles) so a
/// telegraph and its strike can never disagree about what they cover.
pub fn strike_tiles(pattern: BossPattern, pos: Pos, room: &Room) -> Vec<Pos> {
    match pattern {
        BossPattern::Slam => {
            let mut tiles = vec![pos];
            for dir in DIRS {
                if let Some(next) = step_target(pos, dir) {
                    if room.tile_at(next).is_some() {
                        tiles.push(next);
                    }
                }
            }
            tiles
        }
        BossPattern::Sweep => {
            let mut tiles = vec![pos];
            for dir in DIRS {
                let mut cur = pos;
                while let Some(next) = step_target(cur, dir) {
                    match room.tile_at(next) {
                        Some(Tile::Wall) | None => break,
                        Some(_) => {
                            tiles.push(next);
                            cur = next;
                        }
                    }
                }
            }
            tiles
        }
    }
}

/// The boss state machine: `BossStalk` (approach the hero, timer- or pattern-triggered windup) ->
/// `BossWindup` (telegraph hold) -> `BossStrike` (one tick, resolved by
/// `combat::apply_boss_strike`) -> `BossVulnerable` (the only window the boss takes damage) ->
/// back to `BossStalk`. The phase check runs first and unconditionally: crossing the HP threshold
/// interrupts a windup or a vulnerability window rather than waiting for it to finish, so the
/// fight can never strand a telegraph without a strike (the design's soft-lock guard).
#[allow(clippy::too_many_arguments)]
fn step_boss(
    enemy: &mut Enemy,
    room: &Room,
    revealed: &HashSet<Pos>,
    hero_pos: Pos,
    occupied: &HashSet<Pos>,
    tick: Tick,
    events: &mut Vec<GameEvent>,
) {
    let phase = boss_ai_phase(enemy.ai);
    if phase == 1 && enemy.hp <= BOSS_PHASE_TWO_HP {
        enemy.ai = AiState::BossStalk {
            phase: 2,
            until: tick + BOSS_P2_STALK_TICKS,
        };
        events.push(GameEvent::BossPhaseChanged {
            id: enemy.id,
            phase: 2,
        });
        return;
    }

    match enemy.ai {
        AiState::BossStalk { phase, until } => {
            let step_ticks = if phase == 1 {
                BOSS_P1_STEP_TICKS
            } else {
                BOSS_P2_STEP_TICKS
            };
            if tick >= enemy.move_ready_at {
                if let Some(dir) = path_step(room, revealed, occupied, enemy.pos, hero_pos)
                    .or_else(|| local_step(room, revealed, occupied, enemy.pos, hero_pos))
                {
                    try_move(enemy, room, revealed, occupied, dir, events);
                }
                enemy.move_ready_at = tick + step_ticks;
            }
            let pattern = boss_pattern_for_phase(phase);
            let on_pattern = strike_tiles(pattern, enemy.pos, room).contains(&hero_pos);
            if on_pattern || tick >= until {
                let windup_ticks = if phase == 1 {
                    BOSS_P1_WINDUP_TICKS
                } else {
                    BOSS_P2_WINDUP_TICKS
                };
                enemy.ai = AiState::BossWindup {
                    phase,
                    until: tick + windup_ticks,
                    pattern,
                };
                events.push(GameEvent::BossTelegraph {
                    id: enemy.id,
                    tiles: strike_tiles(pattern, enemy.pos, room),
                });
            }
        }
        AiState::BossWindup {
            phase,
            until,
            pattern,
        } => {
            if tick >= until {
                enemy.ai = AiState::BossStrike { phase, pattern };
            }
        }
        AiState::BossStrike { phase, .. } => {
            let vuln_ticks = if phase == 1 {
                BOSS_P1_VULNERABLE_TICKS
            } else {
                BOSS_P2_VULNERABLE_TICKS
            };
            enemy.ai = AiState::BossVulnerable {
                phase,
                until: tick + vuln_ticks,
            };
        }
        AiState::BossVulnerable { phase, until } => {
            if tick >= until {
                let stalk_ticks = if phase == 1 {
                    BOSS_P1_STALK_TICKS
                } else {
                    BOSS_P2_STALK_TICKS
                };
                enemy.ai = AiState::BossStalk {
                    phase,
                    until: tick + stalk_ticks,
                };
            }
        }
        // Same total-by-construction recovery as `boss_ai_phase` above: a corrupt non-boss
        // `AiState` on a Boss enemy resets to a fresh phase-1 stalk instead of panicking.
        _ => {
            enemy.ai = AiState::BossStalk {
                phase: 1,
                until: tick + BOSS_P1_STALK_TICKS,
            };
        }
    }
}

/// Advances every live enemy by one tick, iterating `enemies` by index in authored order so two
/// enemies contesting a tile resolve by that order (ADR 0004). Each enemy sees the occupancy as
/// updated by every enemy processed earlier this same tick.
pub fn step(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let hero_pos = state.hero.pos;
    let world = Rc::clone(&state.world);
    let room = world.room(state.room);
    let revealed = state.revealed_set(state.room);

    let count = state.enemies.len();
    for i in 0..count {
        if !state.enemies[i].alive {
            continue;
        }
        let id = state.enemies[i].id;
        let occupied = occupied_positions(state, id);

        let before = std::mem::discriminant(&state.enemies[i].ai);
        let GameState { enemies, rng, .. } = &mut *state;
        let enemy = &mut enemies[i];
        match enemy.kind {
            EnemyKind::Slime => step_slime(
                enemy,
                room,
                &revealed,
                hero_pos,
                &occupied,
                rng,
                tick,
                &mut events,
            ),
            EnemyKind::Bat => step_bat(
                enemy,
                room,
                &revealed,
                hero_pos,
                &occupied,
                rng,
                tick,
                &mut events,
            ),
            EnemyKind::Guardian => step_guardian(
                enemy,
                room,
                &revealed,
                hero_pos,
                &occupied,
                rng,
                tick,
                &mut events,
            ),
            EnemyKind::Boss => step_boss(
                enemy,
                room,
                &revealed,
                hero_pos,
                &occupied,
                tick,
                &mut events,
            ),
        }
        if std::mem::discriminant(&state.enemies[i].ai) != before {
            events.push(GameEvent::EnemyAiChanged { id });
        }
    }
    events
}
