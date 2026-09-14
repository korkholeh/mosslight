//! Swing lifecycle, hit resolution, damage, invulnerability and knockback (spec §6).
//!
//! Total: no `unwrap`, no clock. Called from the six-step pipeline in `state::update`.

use std::collections::HashSet;
use std::rc::Rc;

use super::ai::strike_tiles;
use super::entities::{AiState, Facing, Hero, Pos, Swing};
use super::state::{apply_reward, step_target, GameEvent, GameState, Tick};
use super::tuning::{
    BOSS_STRIKE_DAMAGE_HALVES, CONTACT_DAMAGE_HALVES, INVULN_TICKS, KNOCKBACK_TILES,
    SWORD_ACTIVE_TICKS, SWORD_COOLDOWN_TICKS, SWORD_DAMAGE_HP,
};
use super::world::{EnemyKind, Reward, Room, Tile};

/// Starts a sword swing if the hero's cooldown has elapsed. The hitbox is exactly the tile the
/// hero faces; an off-grid facing (only possible at the room edge) still costs the cooldown and
/// simply hits nothing (`Swing::at` is `None`).
pub fn start_swing(hero: &mut Hero, tick: Tick) -> Option<GameEvent> {
    if !hero.can_attack(tick) {
        return None;
    }
    let at = step_target(hero.pos, hero.facing);
    hero.attack = Some(Swing {
        started_at: tick,
        facing: hero.facing,
        at,
        hit: Vec::new(),
    });
    hero.attack_ready_at = tick + SWORD_COOLDOWN_TICKS;
    Some(GameEvent::AttackSwung { at })
}

/// Damages every live enemy standing on the swing's target tile that this swing has not yet hit,
/// once per tick while the active window lasts. A guardian outside `GuardianRecover`, or a boss
/// outside `BossVulnerable`, deflects the hit instead of taking damage (still costs the cooldown,
/// still marks the target as hit this swing). A defeated boss sets its authored `defeat_flag` and
/// grants its authored `drops` through the ordinary `apply_reward` path (round-1 review would
/// otherwise ask why the ember pickup looks different from every other reward — it doesn't).
pub fn resolve_swing(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let room_idx = state.room;
    let world = Rc::clone(&state.world);
    let GameState {
        hero,
        enemies,
        progress,
        ..
    } = state;

    let mut boss_reward: Option<Reward> = None;

    {
        let Some(swing) = hero.attack.as_mut() else {
            return events;
        };
        if tick >= swing.started_at + SWORD_ACTIVE_TICKS {
            return events;
        }
        let Some(at) = swing.at else {
            return events;
        };

        for enemy in enemies.iter_mut() {
            if !enemy.alive || enemy.pos != at || swing.hit.contains(&enemy.id) {
                continue;
            }
            swing.hit.push(enemy.id);

            let deflects = match enemy.kind {
                EnemyKind::Guardian => !matches!(enemy.ai, AiState::GuardianRecover { .. }),
                EnemyKind::Boss => !matches!(enemy.ai, AiState::BossVulnerable { .. }),
                EnemyKind::Slime | EnemyKind::Bat => false,
            };
            if deflects {
                events.push(GameEvent::AttackDeflected { id: enemy.id });
                continue;
            }

            let remaining = enemy.hp.saturating_sub(SWORD_DAMAGE_HP);
            enemy.hp = remaining;
            if remaining == 0 {
                enemy.alive = false;
                events.push(GameEvent::EnemyKilled {
                    id: enemy.id,
                    kind: enemy.kind,
                    at: enemy.pos,
                });
                if enemy.kind == EnemyKind::Boss {
                    let spawn = &world.room(room_idx).enemies[enemy.id.0 as usize];
                    if let Some(flag) = &spawn.defeat_flag {
                        if progress.flags.insert(flag.clone()) {
                            events.push(GameEvent::FlagSet { flag: flag.clone() });
                        }
                    }
                    events.push(GameEvent::BossDefeated { id: enemy.id });
                    boss_reward = spawn.drops.clone();
                }
            } else {
                events.push(GameEvent::EnemyDamaged {
                    id: enemy.id,
                    remaining_hp: remaining,
                });
            }
        }
    }

    if let Some(reward) = boss_reward {
        events.extend(apply_reward(hero, &reward));
    }

    events
}

/// Clears a swing once its active window has passed. Returns whether a swing was actually
/// cleared this tick — the caller must turn that into a `GameEvent`, or the tick that ends the
/// sword animation reports no change and the sword glyph never gets a redraw to clear it (round-1
/// review, major).
pub fn expire_swing(hero: &mut Hero, tick: Tick) -> bool {
    if let Some(swing) = &hero.attack {
        if tick >= swing.started_at + SWORD_ACTIVE_TICKS {
            hero.attack = None;
            return true;
        }
    }
    false
}

fn orthogonally_adjacent_or_same(a: Pos, b: Pos) -> bool {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    (dx == 0 && dy == 0) || (dx == 1 && dy == 0) || (dx == 0 && dy == 1)
}

/// The cardinal direction from `from` toward `toward`; `fallback` covers the degenerate case
/// where the two positions coincide. To knock an entity *away* from a source, call this with the
/// source as `from` and the entity as `toward`.
fn direction_toward(from: Pos, toward: Pos, fallback: Facing) -> Facing {
    let dx = toward.x as i32 - from.x as i32;
    let dy = toward.y as i32 - from.y as i32;
    if dx > 0 {
        Facing::East
    } else if dx < 0 {
        Facing::West
    } else if dy > 0 {
        Facing::South
    } else if dy < 0 {
        Facing::North
    } else {
        fallback
    }
}

/// Walks up to `tiles` steps one tile at a time from `from` in `facing`, stopping *before* the
/// first tile that is out of bounds, non-walkable (a revealed `Hidden` tile counts as walkable),
/// a `Tile::Door` (a knockback landing on a door would teleport the entity into the next room
/// mid-hit), a solid authored object, or occupied by an enemy. No hazard tile is ever walkable, so
/// excluding hazards needs no separate term — `is_walkable()`/`is_hazard()` are disjoint by
/// construction.
pub fn knockback(
    room: &Room,
    revealed: &HashSet<Pos>,
    occupied: &HashSet<Pos>,
    from: Pos,
    facing: Facing,
    tiles: u8,
) -> Pos {
    let mut pos = from;
    for _ in 0..tiles {
        let Some(next) = step_target(pos, facing) else {
            break;
        };
        let tile_ok = room.tile_at(next).is_some_and(|t| {
            (t.is_walkable() && t != Tile::Door) || (t == Tile::Hidden && revealed.contains(&next))
        });
        let clear = tile_ok && room.object_at(next).is_none() && !occupied.contains(&next);
        if !clear {
            break;
        }
        pos = next;
    }
    pos
}

/// Resolves a telegraphed boss strike: any live boss whose `ai` is `BossStrike` this tick hits
/// every tile `ai::strike_tiles` computes for its pattern, always emitting `BossStruck` (so the
/// telegraph-lead-time test measures the tiles a `BossTelegraph` promised against the tiles a
/// `BossStruck` actually hit). If the hero stands on one of those tiles and is not invulnerable,
/// applies `BOSS_STRIKE_DAMAGE_HALVES`, knockback and `INVULN_TICKS` — the same three effects
/// `apply_contact_damage` applies, since the boss deals no contact damage of its own (it never
/// appears in that function's search) and this is its only damage path. Runs between `ai::step`
/// and `apply_contact_damage` in the pipeline, so it always sees the tick `ai::step` set
/// `BossStrike` on.
pub fn apply_boss_strike(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let world = Rc::clone(&state.world);
    let room = world.room(state.room);

    let boss_strike = state.enemies.iter().find_map(|e| {
        if !e.alive {
            return None;
        }
        match e.ai {
            AiState::BossStrike { pattern, .. } => Some((e.pos, pattern)),
            _ => None,
        }
    });
    let Some((boss_pos, pattern)) = boss_strike else {
        return events;
    };

    let tiles = strike_tiles(pattern, boss_pos, room);
    events.push(GameEvent::BossStruck {
        tiles: tiles.clone(),
    });

    if !tiles.contains(&state.hero.pos) || state.hero.is_invulnerable(tick) {
        return events;
    }

    state.hero.health_halves = state
        .hero
        .health_halves
        .saturating_sub(BOSS_STRIKE_DAMAGE_HALVES);
    state.hero.invuln_until = tick + INVULN_TICKS;

    let revealed = state.revealed_set(state.room);
    let mut occupied: HashSet<Pos> = state
        .enemies
        .iter()
        .filter(|e| e.alive)
        .map(|e| e.pos)
        .collect();
    occupied.remove(&state.hero.pos);
    let facing = direction_toward(boss_pos, state.hero.pos, state.hero.facing);
    state.hero.pos = knockback(
        room,
        &revealed,
        &occupied,
        state.hero.pos,
        facing,
        KNOCKBACK_TILES,
    );

    events.push(GameEvent::HeroDamaged {
        remaining_halves: state.hero.health_halves,
    });
    events
}

/// The first live, non-boss enemy (in `Vec` order) orthogonally adjacent to or on the hero's tile
/// deals contact damage and knocks the hero back one tile away from it, unless the hero is still
/// invulnerable from a previous hit. The boss is excluded by construction (see
/// `apply_boss_strike`'s doc comment): the sword hits the tile the hero faces, so hitting the boss
/// *requires* standing adjacent to it, and a contact-damaging boss would make the scripted
/// no-damage fight impossible and turn it into the damage race spec §6 forbids.
pub fn apply_contact_damage(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let hero_pos = state.hero.pos;

    let Some(source_pos) = state
        .enemies
        .iter()
        .find(|e| {
            e.alive && e.kind != EnemyKind::Boss && orthogonally_adjacent_or_same(e.pos, hero_pos)
        })
        .map(|e| e.pos)
    else {
        return events;
    };

    if state.hero.is_invulnerable(tick) {
        return events;
    }

    state.hero.health_halves = state
        .hero
        .health_halves
        .saturating_sub(CONTACT_DAMAGE_HALVES);
    state.hero.invuln_until = tick + INVULN_TICKS;

    let world = Rc::clone(&state.world);
    let room = world.room(state.room);
    let revealed = state.revealed_set(state.room);
    let mut occupied: HashSet<Pos> = state
        .enemies
        .iter()
        .filter(|e| e.alive)
        .map(|e| e.pos)
        .collect();
    occupied.remove(&hero_pos);
    let facing = direction_toward(source_pos, hero_pos, state.hero.facing);
    state.hero.pos = knockback(
        room,
        &revealed,
        &occupied,
        hero_pos,
        facing,
        KNOCKBACK_TILES,
    );

    events.push(GameEvent::HeroDamaged {
        remaining_halves: state.hero.health_halves,
    });
    events
}
