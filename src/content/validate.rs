//! The full §7 validity list: structural checks (part A) plus the `(unlocked, items)`
//! reachability search (part B, see ADR 0005 / DECISIONS.md).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::game::entities::Pos;
use crate::game::tuning::{ROOM_H, ROOM_W};

use super::error::{ContentError, IdKind};
use super::schema::{EnemyKind, LockKind, PuzzleKind, Reward, Room, RoomIdx, Tile, World};

/// Maximum number of `SmallKey`-locked doors the `(unlocked, items)` search's `u64` bitmask can
/// track — see [`ContentError::TooManySmallKeyDoors`].
const MAX_SMALLKEY_DOORS: usize = 64;

/// Runs every §7 check and collects every error rather than stopping at the first — the module's
/// public surface (`content::validate`). `loader::parse` uses [`collect_errors`] directly so it
/// can skip the reachability search when the decode stage already failed.
pub fn validate(world: &World) -> Result<(), Vec<ContentError>> {
    let errors = collect_errors(world, true);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub(crate) fn collect_errors(world: &World, check_reachability: bool) -> Vec<ContentError> {
    let mut errors = Vec::new();
    errors.extend(check_unique_ids(world));
    errors.extend(check_transitions(world));
    errors.extend(check_spawns(world));
    errors.extend(check_door_geometry(world));
    errors.extend(check_reciprocity(world));
    errors.extend(check_enemy_spawns(world));
    errors.extend(check_object_placement(world));
    errors.extend(check_npc_conditions(world));
    errors.extend(check_puzzles(world));
    errors.extend(check_torches(world));
    errors.extend(check_boss(world));
    errors.extend(check_beacon(world));
    errors.extend(check_secrets(world));
    if check_reachability {
        errors.extend(check_reachability_and_route(world));
    }
    errors
}

fn in_bounds(pos: Pos) -> bool {
    (pos.x as usize) < ROOM_W && (pos.y as usize) < ROOM_H
}

fn check_unique_ids(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    let mut seen_rooms = HashSet::new();
    let mut seen_doors = HashSet::new();
    let mut seen_spawns = HashSet::new();
    let mut seen_chests = HashSet::new();
    let mut seen_npcs = HashSet::new();
    let mut seen_puzzles = HashSet::new();
    let mut seen_torches = HashSet::new();
    let mut seen_plates = HashSet::new();
    let mut seen_blocks = HashSet::new();
    let mut seen_beacons = HashSet::new();
    let mut seen_map_index: HashSet<(u8, u8)> = HashSet::new();

    for room in &world.rooms {
        if !seen_rooms.insert(room.id.clone()) {
            errors.push(ContentError::DuplicateId {
                kind: IdKind::Room,
                id: room.id.clone(),
            });
        }
        if let Some(mi) = room.map_index {
            if !seen_map_index.insert(mi) {
                errors.push(ContentError::DuplicateMapIndex { at: mi });
            }
        }
        for d in &room.doors {
            if !seen_doors.insert(d.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Door,
                    id: d.id.clone(),
                });
            }
        }
        for s in &room.spawns {
            if !seen_spawns.insert(s.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Spawn,
                    id: s.id.clone(),
                });
            }
        }
        for c in &room.chests {
            if !seen_chests.insert(c.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Chest,
                    id: c.id.clone(),
                });
            }
        }
        for n in &room.npcs {
            if !seen_npcs.insert(n.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Npc,
                    id: n.id.clone(),
                });
            }
        }
        for p in &room.puzzles {
            if !seen_puzzles.insert(p.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Puzzle,
                    id: p.id.clone(),
                });
            }
        }
        for t in &room.torches {
            if !seen_torches.insert(t.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Torch,
                    id: t.id.clone(),
                });
            }
        }
        for p in &room.plates {
            if !seen_plates.insert(p.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Plate,
                    id: p.id.clone(),
                });
            }
        }
        for b in &room.blocks {
            if !seen_blocks.insert(b.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Block,
                    id: b.id.clone(),
                });
            }
        }
        for b in &room.beacons {
            if !seen_beacons.insert(b.id.clone()) {
                errors.push(ContentError::DuplicateId {
                    kind: IdKind::Beacon,
                    id: b.id.clone(),
                });
            }
        }
    }
    errors
}

fn check_transitions(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();

    match world.room_idx(&world.start.room) {
        None => errors.push(ContentError::UnknownRoom {
            referenced_by: "start".to_string(),
            room: world.start.room.clone(),
        }),
        Some(idx) => {
            if world.room(idx).spawn_at(&world.start.spawn).is_none() {
                errors.push(ContentError::UnknownSpawn {
                    referenced_by: "start".to_string(),
                    room: world.start.room.clone(),
                    spawn: world.start.spawn.clone(),
                });
            }
        }
    }

    if world.room_idx(&world.route.home).is_none() {
        errors.push(ContentError::UnknownRoom {
            referenced_by: "route.home".to_string(),
            room: world.route.home.clone(),
        });
    }

    if world.room_idx(&world.route.goal).is_none() {
        errors.push(ContentError::UnknownRoom {
            referenced_by: "route.goal".to_string(),
            room: world.route.goal.clone(),
        });
    }

    for room in &world.rooms {
        for door in &room.doors {
            match world.room_idx(&door.to_room) {
                None => errors.push(ContentError::UnknownRoom {
                    referenced_by: door.id.clone(),
                    room: door.to_room.clone(),
                }),
                Some(idx) => {
                    if world.room(idx).spawn_at(&door.to_spawn).is_none() {
                        errors.push(ContentError::UnknownSpawn {
                            referenced_by: door.id.clone(),
                            room: door.to_room.clone(),
                            spawn: door.to_spawn.clone(),
                        });
                    }
                }
            }
        }
    }
    errors
}

fn check_spawns(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        for spawn in &room.spawns {
            if !in_bounds(spawn.at) {
                errors.push(ContentError::PositionOutOfBounds {
                    room: room.id.clone(),
                    what: format!("spawn '{}'", spawn.id),
                    at: spawn.at,
                });
                continue;
            }
            let tile = room.tile_at(spawn.at).expect("checked in bounds");
            if tile == Tile::Door {
                errors.push(ContentError::SpawnOnDoorTile {
                    room: room.id.clone(),
                    spawn: spawn.id.clone(),
                    at: spawn.at,
                });
            } else if !tile.is_walkable() {
                errors.push(ContentError::SpawnNotWalkable {
                    room: room.id.clone(),
                    spawn: spawn.id.clone(),
                    at: spawn.at,
                    tile,
                });
            }
        }
    }
    errors
}

/// Every enemy spawn position, and every patrol waypoint, must be in bounds, walkable, not a
/// hazard and not a door tile — the same walkability bar a hero spawn has to clear.
fn check_enemy_spawns(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        for enemy in &room.enemies {
            if !spawn_walkable(room, enemy.at) {
                errors.push(ContentError::EnemySpawnNotWalkable {
                    room: room.id.clone(),
                    at: enemy.at,
                });
            }
            for &waypoint in enemy.patrol.iter().flatten() {
                if !spawn_walkable(room, waypoint) {
                    errors.push(ContentError::EnemyPatrolInvalid {
                        room: room.id.clone(),
                        at: waypoint,
                    });
                }
            }
        }
    }
    errors
}

fn spawn_walkable(room: &Room, at: Pos) -> bool {
    in_bounds(at)
        && room
            .tile_at(at)
            .is_some_and(|t| t.is_walkable() && !t.is_hazard() && t != Tile::Door)
}

/// Every chest/npc/torch/plate `at` must be in bounds, sit on plain `Tile::Floor`, and not
/// coincide with a spawn, an enemy spawn or an enemy patrol waypoint (all of those are checked
/// against Floor separately, so an object can share a *tile kind* with them but never the exact
/// position) — and at most one object may occupy the same tile.
fn check_object_placement(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        let spawn_positions: HashSet<Pos> = room.spawns.iter().map(|s| s.at).collect();
        let mut enemy_positions: HashSet<Pos> = HashSet::new();
        for enemy in &room.enemies {
            enemy_positions.insert(enemy.at);
            for &wp in enemy.patrol.iter().flatten() {
                enemy_positions.insert(wp);
            }
        }

        let mut objects: Vec<(String, Pos)> = Vec::new();
        for c in &room.chests {
            objects.push((format!("chest '{}'", c.id), c.at));
        }
        for n in &room.npcs {
            objects.push((format!("npc '{}'", n.id), n.at));
        }
        for t in &room.torches {
            objects.push((format!("torch '{}'", t.id), t.at));
        }
        for p in &room.plates {
            objects.push((format!("plate '{}'", p.id), p.at));
        }
        for b in &room.blocks {
            objects.push((format!("block '{}'", b.id), b.at));
        }
        for b in &room.beacons {
            objects.push((format!("beacon '{}'", b.id), b.at));
        }

        for (what, at) in &objects {
            if !in_bounds(*at) {
                errors.push(ContentError::PositionOutOfBounds {
                    room: room.id.clone(),
                    what: what.clone(),
                    at: *at,
                });
                continue;
            }
            let tile = room.tile_at(*at).expect("checked in bounds");
            let usable = tile == Tile::Floor
                && !spawn_positions.contains(at)
                && !enemy_positions.contains(at);
            if !usable {
                errors.push(ContentError::ObjectNotOnFloor {
                    room: room.id.clone(),
                    what: what.clone(),
                    at: *at,
                });
            }
        }

        let mut tile_counts: HashMap<Pos, usize> = HashMap::new();
        for (_, at) in &objects {
            *tile_counts.entry(*at).or_insert(0) += 1;
        }
        for (at, count) in tile_counts {
            if count > 1 {
                errors.push(ContentError::ObjectTileConflict {
                    room: room.id.clone(),
                    at,
                });
            }
        }
    }
    errors
}

/// Every `Npc.condition` flag and every `LockKind::Flag` flag must be set by some
/// `DialogueNode.sets_flag` somewhere in the world, or it can never be satisfied.
fn check_npc_conditions(world: &World) -> Vec<ContentError> {
    let mut set_flags: HashSet<&str> = HashSet::new();
    for room in &world.rooms {
        for npc in &room.npcs {
            for node in &npc.dialogue {
                if let Some(flag) = &node.sets_flag {
                    set_flags.insert(flag.as_str());
                }
            }
        }
        for enemy in &room.enemies {
            if let Some(flag) = &enemy.defeat_flag {
                set_flags.insert(flag.as_str());
            }
        }
    }

    let mut needed: HashSet<&str> = HashSet::new();
    for room in &world.rooms {
        for npc in &room.npcs {
            if let Some(flag) = &npc.condition {
                needed.insert(flag.as_str());
            }
        }
        for door in &room.doors {
            if let Some(LockKind::Flag(flag)) = &door.lock {
                needed.insert(flag.as_str());
            }
        }
    }

    let mut missing: Vec<&str> = needed
        .into_iter()
        .filter(|f| !set_flags.contains(f))
        .collect();
    missing.sort_unstable();
    missing
        .into_iter()
        .map(|flag| ContentError::FlagNeverSet {
            flag: flag.to_string(),
        })
        .collect()
}

/// Every `Puzzle.plates` id must name a plate that exists in the same room, and every
/// `Puzzle.reveals`/`Torch.reveals` position must be a `Tile::Hidden` tile in that room. Per-kind
/// shape rules: `BlockOnPlates` names exactly one block (resolving in the same room) and at least
/// one plate; `TorchSequence` names at least two distinct, resolving torches, none of which
/// carries its own `reveals` (the puzzle owns the reveal). A torch or block may be claimed by at
/// most one puzzle in its room.
fn check_puzzles(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        let mut claimed_torches: HashMap<&str, usize> = HashMap::new();
        let mut claimed_blocks: HashMap<&str, usize> = HashMap::new();

        for puzzle in &room.puzzles {
            for plate_id in &puzzle.plates {
                if room.plate_index(plate_id).is_none() {
                    errors.push(ContentError::UnknownPlate {
                        room: room.id.clone(),
                        puzzle: puzzle.id.clone(),
                        plate: plate_id.clone(),
                    });
                }
            }
            for &at in &puzzle.reveals {
                if room.tile_at(at) != Some(Tile::Hidden) {
                    errors.push(ContentError::RevealNotHidden {
                        room: room.id.clone(),
                        what: format!("puzzle '{}'", puzzle.id),
                        at,
                    });
                }
            }

            match puzzle.kind {
                PuzzleKind::StepPlates => {}
                PuzzleKind::BlockOnPlates => {
                    let block_ok =
                        puzzle.blocks.len() == 1 && room.block_index(&puzzle.blocks[0]).is_some();
                    if !block_ok || puzzle.plates.is_empty() {
                        errors.push(ContentError::BlockPuzzleShape {
                            room: room.id.clone(),
                            puzzle: puzzle.id.clone(),
                        });
                    }
                    for block_id in &puzzle.blocks {
                        *claimed_blocks.entry(block_id.as_str()).or_insert(0) += 1;
                    }
                }
                PuzzleKind::TorchSequence => {
                    let mut unique = HashSet::new();
                    let all_unique = puzzle.torches.iter().all(|id| unique.insert(id.as_str()));
                    let all_resolve_and_bare = puzzle.torches.iter().all(|id| {
                        room.torches
                            .iter()
                            .find(|t| &t.id == id)
                            .is_some_and(|t| t.reveals.is_empty())
                    });
                    if puzzle.torches.len() < 2 || !all_unique || !all_resolve_and_bare {
                        errors.push(ContentError::TorchSequenceShape {
                            room: room.id.clone(),
                            puzzle: puzzle.id.clone(),
                        });
                    }
                    for torch_id in &puzzle.torches {
                        *claimed_torches.entry(torch_id.as_str()).or_insert(0) += 1;
                    }
                }
            }
        }

        for (torch_id, count) in claimed_torches {
            if count > 1 {
                errors.push(ContentError::PuzzleObjectClaimedTwice {
                    room: room.id.clone(),
                    what: format!("torch '{torch_id}'"),
                });
            }
        }
        for (block_id, count) in claimed_blocks {
            if count > 1 {
                errors.push(ContentError::PuzzleObjectClaimedTwice {
                    room: room.id.clone(),
                    what: format!("block '{block_id}'"),
                });
            }
        }
    }
    errors
}

/// At most one `EnemyKind::Boss` spawn world-wide; `drops`/`defeat_flag` only on a boss spawn; a
/// boss that drops a reward must carry a `defeat_flag` (or the reward could never be recorded as
/// collected).
fn check_boss(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    let mut boss_count = 0usize;
    for room in &world.rooms {
        for enemy in &room.enemies {
            if enemy.kind == EnemyKind::Boss {
                boss_count += 1;
                if enemy.drops.is_some() && enemy.defeat_flag.is_none() {
                    errors.push(ContentError::BossDropMissingFlag {
                        room: room.id.clone(),
                    });
                }
            } else if enemy.drops.is_some() || enemy.defeat_flag.is_some() {
                errors.push(ContentError::BossFieldOnRegularEnemy {
                    room: room.id.clone(),
                });
            }
        }
    }
    if boss_count > 1 {
        errors.push(ContentError::MultipleBosses { count: boss_count });
    }
    errors
}

/// Exactly one `Beacon` world-wide, and it must live in `route.home`.
fn check_beacon(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    let beacons: Vec<&Room> = world
        .rooms
        .iter()
        .filter(|r| !r.beacons.is_empty())
        .collect();
    let total: usize = beacons.iter().map(|r| r.beacons.len()).sum();
    if total == 0 {
        errors.push(ContentError::BeaconMissing);
    } else if total > 1 {
        errors.push(ContentError::MultipleBeacons { count: total });
    } else if let Some(room) = beacons.first() {
        if room.id != world.route.home {
            errors.push(ContentError::BeaconNotAtHome {
                room: room.id.clone(),
            });
        }
    }
    errors
}

fn check_torches(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        for torch in &room.torches {
            for &at in &torch.reveals {
                if room.tile_at(at) != Some(Tile::Hidden) {
                    errors.push(ContentError::RevealNotHidden {
                        room: room.id.clone(),
                        what: format!("torch '{}'", torch.id),
                        at,
                    });
                }
            }
        }
    }
    errors
}

/// A secret chest (`Chest.secret`) may not hold a route-critical reward, and its room may not lie
/// on [`main_route_rooms`] — a secret must never gate progress.
fn check_secrets(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    let main_route = main_route_rooms(world);
    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        for chest in &room.chests {
            if !chest.secret {
                continue;
            }
            if matches!(
                chest.contains,
                Reward::Sword | Reward::Lantern | Reward::Ember
            ) {
                errors.push(ContentError::SecretRouteCritical {
                    chest: chest.id.clone(),
                });
            }
            if main_route.contains(&room_idx) {
                errors.push(ContentError::SecretOnMainRoute {
                    chest: chest.id.clone(),
                });
            }
        }
    }
    errors
}

/// The union, over every route-critical target (the rooms holding `Reward::{Sword,Lantern,Ember}`
/// in a non-secret chest, plus `route.goal` and `route.home`), of every room lying on any
/// shortest path from `start.room` to that target in the plain room-adjacency graph (every door
/// traversable, ignoring locks). Union-of-all-shortest-paths, so there is no arbitrary tie-break.
pub fn main_route_rooms(world: &World) -> std::collections::BTreeSet<RoomIdx> {
    let mut on_route = std::collections::BTreeSet::new();
    let Some(start) = world.start_room() else {
        return on_route;
    };
    on_route.insert(start);

    let mut targets: std::collections::BTreeSet<RoomIdx> = std::collections::BTreeSet::new();
    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        let has_critical = room.chests.iter().any(|c| {
            !c.secret && matches!(c.contains, Reward::Sword | Reward::Lantern | Reward::Ember)
        }) || room.enemies.iter().any(|e| {
            e.kind == EnemyKind::Boss
                && matches!(
                    e.drops,
                    Some(Reward::Sword) | Some(Reward::Lantern) | Some(Reward::Ember)
                )
        });
        if has_critical {
            targets.insert(room_idx);
        }
    }
    if let Some(goal) = world.room_idx(&world.route.goal) {
        targets.insert(goal);
    }
    if let Some(home) = world.room_idx(&world.route.home) {
        targets.insert(home);
    }

    for target in targets {
        on_route.extend(shortest_path_rooms(world, start, target));
    }
    on_route
}

/// BFS over the plain room-adjacency graph from `start`; returns the union of every room on any
/// shortest path to `target` (empty if `target` is unreachable there).
fn shortest_path_rooms(
    world: &World,
    start: RoomIdx,
    target: RoomIdx,
) -> std::collections::BTreeSet<RoomIdx> {
    let mut dist: HashMap<RoomIdx, u32> = HashMap::new();
    dist.insert(start, 0);
    let mut queue = VecDeque::new();
    queue.push_back(start);
    while let Some(room_idx) = queue.pop_front() {
        let d = dist[&room_idx];
        for door in &world.room(room_idx).doors {
            let Some(next) = world.room_idx(&door.to_room) else {
                continue;
            };
            if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(next) {
                e.insert(d + 1);
                queue.push_back(next);
            }
        }
    }

    let mut on_path = std::collections::BTreeSet::new();
    let Some(&target_dist) = dist.get(&target) else {
        return on_path;
    };
    on_path.insert(target);

    // A room r (dist[r] < target_dist) is on some shortest path to target iff some door from r
    // leads to a room already known to be on the path, one step closer to target. Fixpoint
    // outward from target rather than a single backward walk, so every shortest path is found,
    // not just one arbitrary one.
    let mut changed = true;
    while changed {
        changed = false;
        for (&room_idx, &d) in &dist {
            if d >= target_dist || on_path.contains(&room_idx) {
                continue;
            }
            let reaches_path = world.room(room_idx).doors.iter().any(|door| {
                world.room_idx(&door.to_room).is_some_and(|next| {
                    on_path.contains(&next) && dist.get(&next) == Some(&(d + 1))
                })
            });
            if reaches_path {
                on_path.insert(room_idx);
                changed = true;
            }
        }
    }
    on_path
}

fn check_door_geometry(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        let mut door_counts: HashMap<Pos, usize> = HashMap::new();
        for d in &room.doors {
            *door_counts.entry(d.at).or_insert(0) += 1;
        }

        for y in 0..ROOM_H {
            for x in 0..ROOM_W {
                if room.tiles[y][x] == Tile::Door {
                    let at = Pos {
                        x: x as u8,
                        y: y as u8,
                    };
                    if door_counts.get(&at).copied().unwrap_or(0) != 1 {
                        errors.push(ContentError::DoorTileMismatch {
                            room: room.id.clone(),
                            at,
                        });
                    }
                }
            }
        }

        for d in &room.doors {
            if !in_bounds(d.at) {
                errors.push(ContentError::PositionOutOfBounds {
                    room: room.id.clone(),
                    what: format!("door '{}'", d.id),
                    at: d.at,
                });
            } else if room.tile_at(d.at) != Some(Tile::Door) {
                errors.push(ContentError::DoorTileMismatch {
                    room: room.id.clone(),
                    at: d.at,
                });
            }
        }
    }
    errors
}

fn orthogonally_adjacent(a: Pos, b: Pos) -> bool {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    (dx == 1 && dy == 0) || (dx == 0 && dy == 1)
}

fn check_reciprocity(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    for room in &world.rooms {
        for door in &room.doors {
            if !door.two_way {
                continue;
            }
            let Some(target_idx) = world.room_idx(&door.to_room) else {
                continue; // reported as UnknownRoom already
            };
            let target_room = world.room(target_idx);
            let Some(s_pos) = target_room.spawn_at(&door.to_spawn) else {
                continue; // reported as UnknownSpawn already
            };

            let reciprocal_ok = target_room.doors.iter().any(|b| {
                b.two_way
                    && b.to_room == room.id
                    && orthogonally_adjacent(s_pos, b.at)
                    && room
                        .spawn_at(&b.to_spawn)
                        .is_some_and(|bs| orthogonally_adjacent(bs, door.at))
            });

            if !reciprocal_ok {
                errors.push(ContentError::NonReciprocalDoor {
                    door: door.id.clone(),
                    to_room: door.to_room.clone(),
                });
            }
        }
    }
    errors
}

/// The spawn ids a hero could actually be standing on when they first set foot in `room_idx`: the
/// global start spawn if this is the start room, plus the `to_spawn` of every door in the world
/// that targets it. Static per world — independent of which locks are open — so it does not need
/// to be recomputed per reachability state.
fn entry_spawns_for_room(world: &World, room_idx: RoomIdx) -> Vec<String> {
    let mut ids = Vec::new();
    if world.start_room() == Some(room_idx) {
        ids.push(world.start.spawn.clone());
    }
    for room in &world.rooms {
        for door in &room.doors {
            if world.room_idx(&door.to_room) == Some(room_idx) {
                ids.push(door.to_spawn.clone());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn orthogonal_neighbours(pos: Pos) -> [Option<Pos>; 4] {
    [
        (pos.y > 0).then(|| Pos {
            x: pos.x,
            y: pos.y - 1,
        }),
        ((pos.y as usize) + 1 < ROOM_H).then(|| Pos {
            x: pos.x,
            y: pos.y + 1,
        }),
        (pos.x > 0).then(|| Pos {
            x: pos.x - 1,
            y: pos.y,
        }),
        ((pos.x as usize) + 1 < ROOM_W).then(|| Pos {
            x: pos.x + 1,
            y: pos.y,
        }),
    ]
}

/// One tile in `dir` from `pos`, or `None` off-grid — the same rule as `game::state::step_target`
/// (not reused directly: that function is `pub(super)` to `game`, and duplicating four lines here
/// is simpler than widening its visibility for one caller in a different module).
fn step(pos: Pos, dir: crate::game::entities::Facing) -> Option<Pos> {
    use crate::game::entities::Facing;
    match dir {
        Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
        Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
        Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
        Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
    }
}

/// The exact one-block push search `check_reachability_and_route`'s `BlockOnPlates` rule needs:
/// a BFS over `(block_pos, hero_pos)` seeded from every hero tile already reached in the room,
/// where an edge is either an ordinary hero step (refused onto the block's own tile) or a push of
/// the block through `game::puzzles::block_push_target` — the same predicate the simulation itself
/// uses, so the validator and the game can never disagree about what a player can do. Solved iff
/// some reachable `block_pos` is one of `target_plates`. Bounded by `(ROOM_W * ROOM_H)^2` states
/// (~147k, milliseconds), and exact because the schema forbids a second block in one puzzle.
fn block_puzzle_solvable(
    room: &Room,
    block_start: Pos,
    hero_starts: &HashSet<Pos>,
    target_plates: &HashSet<Pos>,
) -> bool {
    use crate::game::entities::Facing;
    use crate::game::puzzles::block_push_target;

    if target_plates.contains(&block_start) {
        return true;
    }

    let mut seen: HashSet<(Pos, Pos)> = HashSet::new();
    let mut queue = VecDeque::new();
    for &hero_pos in hero_starts {
        if hero_pos == block_start {
            continue;
        }
        let state = (block_start, hero_pos);
        if seen.insert(state) {
            queue.push_back(state);
        }
    }

    let no_enemies: HashSet<Pos> = HashSet::new();
    const DIRS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];
    while let Some((block_pos, hero_pos)) = queue.pop_front() {
        for dir in DIRS {
            let Some(next) = step(hero_pos, dir) else {
                continue;
            };
            if next == block_pos {
                let Some(target) =
                    block_push_target(room, &[block_pos], &no_enemies, block_pos, dir)
                else {
                    continue;
                };
                if target_plates.contains(&target) {
                    return true;
                }
                let state = (target, block_pos);
                if seen.insert(state) {
                    queue.push_back(state);
                }
                continue;
            }
            if room.tile_at(next).is_some_and(|t| t.is_walkable()) && room.object_at(next).is_none()
            {
                let state = (block_pos, next);
                if seen.insert(state) {
                    queue.push_back(state);
                }
            }
        }
    }
    false
}

/// Every position an authored torch or `StepPlates` puzzle can reveal, and every flag a reachable
/// NPC's dialogue can set, given which tiles are currently reached. Carried alongside `Items` in
/// the search state so both fixpoints — items and reveals/flags — settle together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RevealState {
    lit_torches: HashSet<(RoomIdx, u16)>,
    solved_puzzles: HashSet<(RoomIdx, u16)>,
    flags: HashSet<String>,
}

impl RevealState {
    fn is_revealed(&self, room: &Room, room_idx: RoomIdx, at: Pos) -> bool {
        let by_torch = room.torches.iter().enumerate().any(|(i, t)| {
            self.lit_torches.contains(&(room_idx, i as u16)) && t.reveals.contains(&at)
        });
        let by_puzzle = room.puzzles.iter().enumerate().any(|(i, p)| {
            self.solved_puzzles.contains(&(room_idx, i as u16)) && p.reveals.contains(&at)
        });
        by_torch || by_puzzle
    }
}

/// Item flags carried across the reachability search. `Lantern` and `Ember` are permanent,
/// non-consumable pickups; `Flag` locks are handled by [`RevealState::flags`] instead, since a
/// flag comes from a reachable NPC's dialogue rather than a chest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Items {
    lantern: bool,
    ember: bool,
}

fn traversable(
    door: &super::schema::Door,
    unlocked: u64,
    smallkey_bit: Option<u64>,
    items: Items,
    flags: &HashSet<String>,
) -> bool {
    match &door.lock {
        None => true,
        Some(LockKind::SmallKey) => smallkey_bit.is_some_and(|bit| unlocked & bit != 0),
        Some(LockKind::Lantern) => items.lantern,
        Some(LockKind::Flag(flag)) => flags.contains(flag),
    }
}

/// One BFS state: which `SmallKey` doors have been permanently opened (a bit per door, indexed by
/// `smallkey_doors`), plus which permanent items have been picked up so far.
type StateKey = (u64, bool, bool);

/// A place the hero can actually be standing: a room plus a tile position in it. Tile-level
/// (rather than component-level) so "is X reachable" is directly "some orthogonal neighbour of
/// X's tile is in the reached set", which is what makes an authored solid object's reachability
/// askable at all.
type Loc = (RoomIdx, Pos);

struct ReachabilitySearch<'w> {
    world: &'w World,
    smallkey_doors: Vec<(RoomIdx, Pos)>,
}

impl<'w> ReachabilitySearch<'w> {
    fn new(world: &'w World) -> Self {
        let mut smallkey_doors = Vec::new();
        for (i, room) in world.rooms.iter().enumerate() {
            for door in &room.doors {
                if matches!(door.lock, Some(LockKind::SmallKey)) {
                    smallkey_doors.push((RoomIdx(i as u16), door.at));
                }
            }
        }
        ReachabilitySearch {
            world,
            smallkey_doors,
        }
    }

    fn bit_for(&self, room_idx: RoomIdx, at: Pos) -> Option<u64> {
        self.smallkey_doors
            .iter()
            .position(|&(r, p)| r == room_idx && p == at)
            .map(|i| 1u64 << i)
    }

    fn walkable_for_search(
        &self,
        room: &Room,
        room_idx: RoomIdx,
        at: Pos,
        reveal: &RevealState,
    ) -> bool {
        let Some(tile) = room.tile_at(at) else {
            return false;
        };
        let tile_ok =
            tile.is_walkable() || (tile == Tile::Hidden && reveal.is_revealed(room, room_idx, at));
        // A block's authored reset position is solid to an ordinary hero step, exactly like
        // `GameState::walkable`'s `block_free` term — the flood otherwise treats it as plain
        // floor and could optimistically call a route reachable that a player cannot walk before
        // the block is pushed clear (round-1 review, minor). `block_puzzle_solvable` is the
        // mechanism that proves a block actually can be moved out of the way.
        let block_free = !room.blocks.iter().any(|b| b.at == at);
        tile_ok && room.object_at(at).is_none() && block_free
    }

    /// Flood-fills the tile graph from `start` (orthogonal steps within a room, doors across
    /// rooms), then repeatedly folds in newly-reachable items, lit torches, solved puzzles and
    /// dialogue flags until nothing new is reached — each only ever grows, so this terminates.
    fn settle(
        &self,
        start: Loc,
        unlocked: u64,
        mut items: Items,
    ) -> (HashSet<Loc>, usize, Items, RevealState) {
        let mut reveal = RevealState::default();
        loop {
            let reached = self.flood(start, unlocked, items, &reveal);

            let mut next_items = items;
            let mut keys_found = 0usize;
            // A chest is solid, so its own tile is never in `reached` (see `walkable_for_search`)
            // — it is opened by facing it from an orthogonal neighbour, exactly like a torch.
            for (i, room) in self.world.rooms.iter().enumerate() {
                let room_idx = RoomIdx(i as u16);
                for chest in &room.chests {
                    let reachable = orthogonal_neighbours(chest.at)
                        .into_iter()
                        .flatten()
                        .any(|n| reached.contains(&(room_idx, n)));
                    if !reachable {
                        continue;
                    }
                    match chest.contains {
                        Reward::SmallKey => keys_found += 1,
                        Reward::Lantern => next_items.lantern = true,
                        Reward::Ember => next_items.ember = true,
                        _ => {}
                    }
                }
            }

            let mut next_reveal = reveal.clone();

            // The validator cannot simulate the boss fight, so a boss spawn's reward/flag are
            // granted as soon as its arena is reachable (an orthogonal neighbour of its tile is in
            // `reached`) — asserting what it can (the arena is reachable and the reward/flag are
            // wired), while `tests/boss.rs`/`tests/playthrough.rs` prove the fight itself.
            for (i, room) in self.world.rooms.iter().enumerate() {
                let room_idx = RoomIdx(i as u16);
                for enemy in &room.enemies {
                    if enemy.kind != super::schema::EnemyKind::Boss {
                        continue;
                    }
                    let reachable = orthogonal_neighbours(enemy.at)
                        .into_iter()
                        .flatten()
                        .any(|n| reached.contains(&(room_idx, n)));
                    if !reachable {
                        continue;
                    }
                    match enemy.drops {
                        Some(Reward::SmallKey) => keys_found += 1,
                        Some(Reward::Lantern) => next_items.lantern = true,
                        Some(Reward::Ember) => next_items.ember = true,
                        _ => {}
                    }
                    if let Some(flag) = &enemy.defeat_flag {
                        next_reveal.flags.insert(flag.clone());
                    }
                }
            }
            if next_items.lantern {
                for (i, room) in self.world.rooms.iter().enumerate() {
                    let room_idx = RoomIdx(i as u16);
                    for (ti, torch) in room.torches.iter().enumerate() {
                        let key = (room_idx, ti as u16);
                        if next_reveal.lit_torches.contains(&key) {
                            continue;
                        }
                        if orthogonal_neighbours(torch.at)
                            .into_iter()
                            .flatten()
                            .any(|n| reached.contains(&(room_idx, n)))
                        {
                            next_reveal.lit_torches.insert(key);
                        }
                    }
                }
            }
            for (i, room) in self.world.rooms.iter().enumerate() {
                let room_idx = RoomIdx(i as u16);
                for npc in &room.npcs {
                    let satisfied = npc
                        .condition
                        .as_ref()
                        .is_none_or(|f| next_reveal.flags.contains(f));
                    if !satisfied {
                        continue;
                    }
                    let reachable = orthogonal_neighbours(npc.at)
                        .into_iter()
                        .flatten()
                        .any(|n| reached.contains(&(room_idx, n)));
                    if !reachable {
                        continue;
                    }
                    for node in &npc.dialogue {
                        if let Some(flag) = &node.sets_flag {
                            next_reveal.flags.insert(flag.clone());
                        }
                    }
                }
            }
            for (i, room) in self.world.rooms.iter().enumerate() {
                let room_idx = RoomIdx(i as u16);
                for (pi, puzzle) in room.puzzles.iter().enumerate() {
                    let key = (room_idx, pi as u16);
                    if next_reveal.solved_puzzles.contains(&key) {
                        continue;
                    }
                    let solved = match puzzle.kind {
                        PuzzleKind::StepPlates => {
                            !puzzle.plates.is_empty()
                                && puzzle.plates.iter().all(|id| {
                                    room.plate_index(id).is_some_and(|idx| {
                                        room.plates
                                            .get(idx as usize)
                                            .is_some_and(|p| reached.contains(&(room_idx, p.at)))
                                    })
                                })
                        }
                        PuzzleKind::TorchSequence => {
                            next_items.lantern
                                && !puzzle.torches.is_empty()
                                && puzzle.torches.iter().all(|id| {
                                    room.torches.iter().find(|t| &t.id == id).is_some_and(|t| {
                                        orthogonal_neighbours(t.at)
                                            .into_iter()
                                            .flatten()
                                            .any(|n| reached.contains(&(room_idx, n)))
                                    })
                                })
                        }
                        PuzzleKind::BlockOnPlates => {
                            puzzle.blocks.len() == 1
                                && !puzzle.plates.is_empty()
                                && room
                                    .block_index(&puzzle.blocks[0])
                                    .is_some_and(|block_idx| {
                                        let block = &room.blocks[block_idx as usize];
                                        let hero_starts: HashSet<Pos> = reached
                                            .iter()
                                            .filter(|(r, _)| *r == room_idx)
                                            .map(|(_, p)| *p)
                                            .collect();
                                        let target_plates: HashSet<Pos> = puzzle
                                            .plates
                                            .iter()
                                            .filter_map(|id| room.plate_index(id))
                                            .filter_map(|idx| room.plates.get(idx as usize))
                                            .map(|p| p.at)
                                            .collect();
                                        block_puzzle_solvable(
                                            room,
                                            block.at,
                                            &hero_starts,
                                            &target_plates,
                                        )
                                    })
                        }
                    };
                    if solved {
                        next_reveal.solved_puzzles.insert(key);
                    }
                }
            }

            if next_items == items && next_reveal == reveal {
                return (reached, keys_found, items, reveal);
            }
            items = next_items;
            reveal = next_reveal;
        }
    }

    fn flood(&self, start: Loc, unlocked: u64, items: Items, reveal: &RevealState) -> HashSet<Loc> {
        let mut seen = HashSet::new();
        seen.insert(start);
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some((room_idx, pos)) = queue.pop_front() {
            let room = self.world.room(room_idx);

            for next in orthogonal_neighbours(pos).into_iter().flatten() {
                let loc = (room_idx, next);
                if !seen.contains(&loc) && self.walkable_for_search(room, room_idx, next, reveal) {
                    seen.insert(loc);
                    queue.push_back(loc);
                }
            }

            if let Some(door) = room.door_at(pos) {
                let bit = self.bit_for(room_idx, pos);
                if traversable(door, unlocked, bit, items, &reveal.flags) {
                    if let Some(target_room) = self.world.room_idx(&door.to_room) {
                        if let Some(landing) = self.world.spawn_pos(target_room, &door.to_spawn) {
                            let loc = (target_room, landing);
                            if seen.insert(loc) {
                                queue.push_back(loc);
                            }
                        }
                    }
                }
            }
        }
        seen
    }

    /// Runs the full branching BFS and returns every state actually reached, each already
    /// settled (flood-filled and item/reveal-fixpointed).
    fn run(&self, start: Loc) -> Vec<(HashSet<Loc>, Items, u64, RevealState)> {
        let mut visited: HashSet<StateKey> = HashSet::new();
        let mut settled_states = Vec::new();
        let mut queue = VecDeque::new();

        let initial = (0u64, Items::default());
        queue.push_back(initial);
        visited.insert((0, false, false));

        while let Some((unlocked, items)) = queue.pop_front() {
            let (reachable, keys_found, settled_items, reveal) =
                self.settle(start, unlocked, items);
            let keys_spent = unlocked.count_ones() as usize;
            settled_states.push((reachable.clone(), settled_items, unlocked, reveal));

            if keys_found > keys_spent {
                for &(room_idx, at) in &self.smallkey_doors {
                    if !reachable.contains(&(room_idx, at)) {
                        continue;
                    }
                    let bit = self
                        .bit_for(room_idx, at)
                        .expect("door is in smallkey_doors");
                    if unlocked & bit != 0 {
                        continue;
                    }
                    let next_unlocked = unlocked | bit;
                    let key = (next_unlocked, settled_items.lantern, settled_items.ember);
                    if visited.insert(key) {
                        queue.push_back((next_unlocked, settled_items));
                    }
                }
            }
        }

        settled_states
    }
}

fn check_reachability_and_route(world: &World) -> Vec<ContentError> {
    let mut errors = Vec::new();
    let Some(start_room) = world.start_room() else {
        return errors; // UnknownRoom for `start` already reported by check_transitions
    };

    let search = ReachabilitySearch::new(world);
    if search.smallkey_doors.len() > MAX_SMALLKEY_DOORS {
        return vec![ContentError::TooManySmallKeyDoors {
            count: search.smallkey_doors.len(),
        }];
    }

    let Some(start_pos) = world.spawn_pos(start_room, &world.start.spawn) else {
        return errors; // UnknownSpawn for `start` already reported by check_transitions
    };
    if world.room(start_room).tile_at(start_pos).is_none() {
        return errors; // out of bounds — already reported elsewhere
    }

    let states = search.run((start_room, start_pos));

    let mut all_reachable: HashSet<Loc> = HashSet::new();
    let mut ever_unlocked: u64 = 0;
    let mut ever_lantern = false;
    let mut ever_flags: HashSet<String> = HashSet::new();
    let mut ember_states: Vec<&HashSet<Loc>> = Vec::new();
    let mut ever_solved_puzzles: HashSet<(RoomIdx, u16)> = HashSet::new();
    for (reachable, items, unlocked, reveal) in &states {
        all_reachable.extend(reachable.iter().copied());
        ever_unlocked |= unlocked;
        ever_lantern |= items.lantern;
        ever_flags.extend(reveal.flags.iter().cloned());
        ever_solved_puzzles.extend(reveal.solved_puzzles.iter().copied());
        if items.ember {
            ember_states.push(reachable);
        }
    }

    let all_reachable_rooms: HashSet<RoomIdx> = all_reachable.iter().map(|&(r, _)| r).collect();

    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        if !all_reachable_rooms.contains(&room_idx) {
            errors.push(ContentError::RoomUnreachable {
                room: room.id.clone(),
            });
            continue;
        }
        // The room is reached at some tile; a door whose own tile is never reached is exactly as
        // unreachable as one that does not exist (round-1 review, major) — reported directly here
        // rather than only surfacing as a downstream `RoomUnreachable` several doors later.
        for door in &room.doors {
            if !all_reachable.contains(&(room_idx, door.at)) {
                errors.push(ContentError::DoorUnreachableInRoom {
                    room: room.id.clone(),
                    door: door.id.clone(),
                    from_spawn: entry_spawns_for_room(world, room_idx).join(", "),
                });
            }
        }
        for (pi, puzzle) in room.puzzles.iter().enumerate() {
            if puzzle.kind == PuzzleKind::BlockOnPlates
                && !ever_solved_puzzles.contains(&(room_idx, pi as u16))
            {
                errors.push(ContentError::BlockPuzzleUnsolvable {
                    room: room.id.clone(),
                    puzzle: puzzle.id.clone(),
                });
            }
        }
    }

    for (i, &(room_idx, at)) in search.smallkey_doors.iter().enumerate() {
        let bit = 1u64 << i;
        if ever_unlocked & bit == 0 {
            let door = world
                .room(room_idx)
                .door_at(at)
                .expect("smallkey_doors only contains real doors");
            errors.push(ContentError::LockNeverUnlockable {
                door: door.id.clone(),
                lock: LockKind::SmallKey,
            });
        }
    }

    let mut lantern_lock_exists = false;
    let mut flag_locks: Vec<(&str, &LockKind)> = Vec::new();
    for room in &world.rooms {
        for door in &room.doors {
            match &door.lock {
                Some(LockKind::Lantern) => lantern_lock_exists = true,
                Some(LockKind::Flag(_)) => {
                    flag_locks.push((door.id.as_str(), door.lock.as_ref().unwrap()))
                }
                _ => {}
            }
        }
    }
    if lantern_lock_exists && !ever_lantern {
        for room in &world.rooms {
            for door in &room.doors {
                if matches!(door.lock, Some(LockKind::Lantern)) {
                    errors.push(ContentError::LockNeverUnlockable {
                        door: door.id.clone(),
                        lock: LockKind::Lantern,
                    });
                }
            }
        }
    }
    for (door_id, lock) in flag_locks {
        let LockKind::Flag(flag) = lock else {
            unreachable!("flag_locks only ever collects Flag locks");
        };
        if !ever_flags.contains(flag) {
            errors.push(ContentError::LockNeverUnlockable {
                door: door_id.to_string(),
                lock: lock.clone(),
            });
        }
    }

    let ember_authored = world.rooms.iter().any(|r| {
        r.chests.iter().any(|c| c.contains == Reward::Ember)
            || r.enemies
                .iter()
                .any(|e| e.kind == EnemyKind::Boss && e.drops == Some(Reward::Ember))
    });

    if ember_authored {
        if ember_states.is_empty() {
            errors.push(ContentError::EmberUnreachable);
        } else if let Some(home_idx) = world.room_idx(&world.route.home) {
            let home_reachable_with_ember = ember_states
                .iter()
                .any(|r| r.iter().any(|&(room, _)| room == home_idx));
            if !home_reachable_with_ember {
                errors.push(ContentError::HomeUnreachableWithEmber {
                    home: world.route.home.clone(),
                });
            }
        }
    } else if world.route.ember_required {
        errors.push(ContentError::EmberMissing);
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::loader::parse;

    #[allow(clippy::too_many_arguments)]
    fn room_ron(
        id: &str,
        map_index: (u8, u8),
        doors: &str,
        spawns: &str,
        chests: &str,
        extra_rows: [&str; 2],
    ) -> String {
        let mut rows = vec!["#".to_string() + &"#".repeat(ROOM_W - 2) + "#"];
        for _ in 0..(ROOM_H - 3) {
            rows.push("#".to_string() + &".".repeat(ROOM_W - 2) + "#");
        }
        // extra_rows[0] is the door row (declared doors use y: 2 in these fixtures).
        rows[1] = extra_rows[1].to_string();
        rows[2] = extra_rows[0].to_string();
        rows.push("#".to_string() + &".".repeat(ROOM_W - 2) + "#");
        rows.push("#".to_string() + &"#".repeat(ROOM_W - 2) + "#");
        let rows_ron = rows
            .iter()
            .map(|r| format!("                \"{r}\","))
            .collect::<Vec<_>>()
            .join("\n");
        // `check_beacon` requires exactly one beacon world-wide, in `route.home` — these fixtures
        // all use "room.a" as both start and home, so it carries the fixture beacon; nothing here
        // exercises beacon/boss placement itself.
        let beacons = if id == "room.a" {
            r#"[(id: "beacon.fixture", at: (x: 20, y: 12))]"#
        } else {
            "[]"
        };
        format!(
            r#"        (
            id: "{id}",
            name: "{id}",
            kind: Overworld,
            map_index: Some({map_index:?}),
            rows: [
{rows_ron}
            ],
            doors: [{doors}],
            spawns: [{spawns}],
            chests: [{chests}],
            beacons: {beacons},
        ),
"#
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn two_room_world(
        a_doors: &str,
        a_spawns: &str,
        a_chests: &str,
        b_doors: &str,
        b_spawns: &str,
        b_chests: &str,
        a_extra: [&str; 2],
        b_extra: [&str; 2],
    ) -> String {
        let a = room_ron("room.a", (0, 0), a_doors, a_spawns, a_chests, a_extra);
        let b = room_ron("room.b", (1, 0), b_doors, b_spawns, b_chests, b_extra);
        format!(
            r#"World(
    version: 1,
    start: (room: "room.a", spawn: "spawn.a.start"),
    route: (ember_required: false, home: "room.a", goal: "room.a"),
    rooms: [
{a}
{b}
    ],
)
"#
        )
    }

    fn floor_row() -> String {
        "#".to_string() + &".".repeat(ROOM_W - 2) + "#"
    }

    #[test]
    fn reciprocal_adjacent_spawn_pairing_is_accepted() {
        let src = two_room_world(
            r#"(id: "door.a.east", at: (x: 23, y: 2), to_room: "room.b", to_spawn: "spawn.b.west")"#,
            r#"(id: "spawn.a.start", at: (x: 22, y: 2))"#,
            "",
            r#"(id: "door.b.west", at: (x: 0, y: 2), to_room: "room.a", to_spawn: "spawn.a.start")"#,
            r#"(id: "spawn.b.west", at: (x: 1, y: 2))"#,
            "",
            [
                &("#".to_string() + &".".repeat(ROOM_W - 2) + "+"),
                &floor_row(),
            ],
            [
                &("+".to_string() + &".".repeat(ROOM_W - 2) + "#"),
                &floor_row(),
            ],
        );
        let world = parse(&src).expect("valid reciprocal world");
        assert!(check_reciprocity(&world).is_empty());
    }

    #[test]
    fn off_by_one_spawn_pairing_is_rejected() {
        // spawn.b.west sits two tiles from the door instead of one: not orthogonally adjacent.
        let src = two_room_world(
            r#"(id: "door.a.east", at: (x: 23, y: 2), to_room: "room.b", to_spawn: "spawn.b.west")"#,
            r#"(id: "spawn.a.start", at: (x: 22, y: 2))"#,
            "",
            r#"(id: "door.b.west", at: (x: 0, y: 2), to_room: "room.a", to_spawn: "spawn.a.start")"#,
            r#"(id: "spawn.b.west", at: (x: 2, y: 2))"#,
            "",
            [
                &("#".to_string() + &".".repeat(ROOM_W - 2) + "+"),
                &floor_row(),
            ],
            [
                &("+".to_string() + &".".repeat(ROOM_W - 2) + "#"),
                &floor_row(),
            ],
        );
        let errors = parse(&src).expect_err("expected a non-reciprocal door");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ContentError::NonReciprocalDoor { door, .. } if door == "door.a.east")),
            "{errors:?}"
        );
    }

    /// A two-room world with a `SmallKey` lock between them: the key chest sits in `room.a`
    /// (reachable before the lock) in the valid case, and in `room.b` (reachable only through the
    /// very door it should unlock) in the invalid case.
    fn key_lock_world(key_room_is_a: bool) -> String {
        let chest = r#"(id: "chest.key", at: (x: 5, y: 5), contains: SmallKey)"#;
        two_room_world(
            r#"(id: "door.a.east", at: (x: 23, y: 2), to_room: "room.b", to_spawn: "spawn.b.west", lock: Some(SmallKey), two_way: false)"#,
            r#"(id: "spawn.a.start", at: (x: 2, y: 2))"#,
            if key_room_is_a { chest } else { "" },
            r#"(id: "door.b.west", at: (x: 0, y: 2), to_room: "room.a", to_spawn: "spawn.a.start", two_way: false)"#,
            r#"(id: "spawn.b.west", at: (x: 1, y: 2))"#,
            if key_room_is_a { "" } else { chest },
            [
                &("#".to_string() + &".".repeat(ROOM_W - 2) + "+"),
                &floor_row(),
            ],
            [
                &("+".to_string() + &".".repeat(ROOM_W - 2) + "#"),
                &floor_row(),
            ],
        )
    }

    #[test]
    fn key_reachable_before_its_lock_validates() {
        let world = parse(&key_lock_world(true)).expect("key before the lock must validate");
        assert!(check_reachability_and_route(&world).is_empty());
    }

    #[test]
    fn key_reachable_only_behind_its_own_lock_is_rejected() {
        let errors = parse(&key_lock_world(false)).expect_err("key behind its own lock must fail");
        assert!(
            errors.iter().any(|e| matches!(
                e,
                ContentError::LockNeverUnlockable {
                    door,
                    lock: LockKind::SmallKey
                } if door == "door.a.east"
            )),
            "{errors:?}"
        );
    }

    /// Built directly from `schema` types (RON would be 65 lines of near-identical doors) so the
    /// `1u64 << i` bitmask in `bit_for`/the `LockNeverUnlockable` loop cannot overflow-panic on a
    /// world with more `SmallKey` doors than the mask has bits for (round-1 review, nit).
    #[test]
    fn more_than_64_smallkey_doors_is_rejected_without_panicking() {
        use std::collections::HashMap;

        use super::super::schema::{Door, RoomKind, Spawn, StartPoint};

        let door_count = MAX_SMALLKEY_DOORS + 1;
        let doors: Vec<Door> = (0..door_count)
            .map(|i| Door {
                id: format!("door.a.{i}"),
                at: Pos {
                    x: (i % ROOM_W) as u8,
                    y: (i / ROOM_W) as u8,
                },
                to_room: "room.a".to_string(),
                to_spawn: "spawn.a.start".to_string(),
                lock: Some(LockKind::SmallKey),
                two_way: false,
            })
            .collect();

        let room = Room {
            id: "room.a".to_string(),
            name: "A".to_string(),
            kind: RoomKind::Overworld,
            map_index: Some((0, 0)),
            rows: Vec::new(),
            tiles: [[Tile::Floor; ROOM_W]; ROOM_H],
            doors,
            spawns: vec![Spawn {
                id: "spawn.a.start".to_string(),
                at: Pos { x: 0, y: 0 },
            }],
            chests: Vec::new(),
            npcs: Vec::new(),
            enemies: Vec::new(),
            puzzles: Vec::new(),
            torches: Vec::new(),
            plates: Vec::new(),
            blocks: Vec::new(),
            beacons: Vec::new(),
            hint: None,
        };

        let world = World {
            version: 1,
            start: StartPoint {
                room: "room.a".to_string(),
                spawn: "spawn.a.start".to_string(),
            },
            route: super::super::schema::Route {
                ember_required: false,
                home: "room.a".to_string(),
                goal: "room.a".to_string(),
            },
            rooms: vec![room],
            room_index: HashMap::from([("room.a".to_string(), RoomIdx(0))]),
        };

        let errors = check_reachability_and_route(&world);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                ContentError::TooManySmallKeyDoors { count } if *count == door_count
            )),
            "{errors:?}"
        );
    }
}
