//! Swing lifecycle, hit resolution, damage, invulnerability and knockback (spec §6).
//!
//! Total: no `unwrap`, no clock. Called from the six-step pipeline in `state::update`.

use std::collections::HashSet;
use std::rc::Rc;

use super::entities::{AiState, Facing, Hero, Pos, Swing};
use super::state::{step_target, GameEvent, GameState, Tick};
use super::tuning::{
    CONTACT_DAMAGE_HALVES, INVULN_TICKS, KNOCKBACK_TILES, SWORD_ACTIVE_TICKS, SWORD_COOLDOWN_TICKS,
    SWORD_DAMAGE_HP,
};
use super::world::{EnemyKind, Room, Tile};

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
/// once per tick while the active window lasts. A guardian outside `GuardianRecover` deflects the
/// hit instead of taking damage (still costs the cooldown, still marks the target as hit this
/// swing).
pub fn resolve_swing(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let GameState { hero, enemies, .. } = state;

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

        if matches!(enemy.kind, EnemyKind::Guardian)
            && !matches!(enemy.ai, AiState::GuardianRecover { .. })
        {
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
        } else {
            events.push(GameEvent::EnemyDamaged {
                id: enemy.id,
                remaining_hp: remaining,
            });
        }
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

/// The first live enemy (in `Vec` order) orthogonally adjacent to or on the hero's tile deals
/// contact damage and knocks the hero back one tile away from it, unless the hero is still
/// invulnerable from a previous hit.
pub fn apply_contact_damage(state: &mut GameState, tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let hero_pos = state.hero.pos;

    let Some(source_pos) = state
        .enemies
        .iter()
        .find(|e| e.alive && orthogonally_adjacent_or_same(e.pos, hero_pos))
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
