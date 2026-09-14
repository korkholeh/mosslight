//! The full §7 validity list: structural checks (part A) plus the `(unlocked, items)`
//! reachability search (part B, see ADR 0005 / DECISIONS.md).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::game::entities::Pos;
use crate::game::tuning::{ROOM_H, ROOM_W};

use super::error::{ContentError, IdKind};
use super::schema::{LockKind, Reward, Room, RoomIdx, Tile, World};

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

/// Partitions a room's walkable tiles into connected components (4-directional). The door graph
/// alone (spec §7's reachability rule as originally implemented) proves nothing about whether the
/// hero can actually walk from the tile they land on to a given door — two tiles in the same room
/// can be mutually unreachable if a wall splits them (round-1 review, major). Component ids are
/// arbitrary and only meaningful for equality within the same room.
fn compute_components(room: &Room) -> HashMap<Pos, usize> {
    let mut component = HashMap::new();
    let mut next_id = 0usize;
    for y in 0..ROOM_H {
        for x in 0..ROOM_W {
            let start = Pos {
                x: x as u8,
                y: y as u8,
            };
            if component.contains_key(&start) || !room.tile_at(start).is_some_and(Tile::is_walkable)
            {
                continue;
            }
            let mut queue = VecDeque::new();
            queue.push_back(start);
            component.insert(start, next_id);
            while let Some(pos) = queue.pop_front() {
                for neighbour in orthogonal_neighbours(pos).into_iter().flatten() {
                    if room.tile_at(neighbour).is_some_and(Tile::is_walkable)
                        && !component.contains_key(&neighbour)
                    {
                        component.insert(neighbour, next_id);
                        queue.push_back(neighbour);
                    }
                }
            }
            next_id += 1;
        }
    }
    component
}

/// Item flags carried across the reachability search. Only `Lantern` and `Ember` are permanent,
/// non-consumable pickups this phase; `LockKind::Flag` has no authored way to become true yet
/// (phases 4-5 add the mechanism), so a `Flag`-locked door is always `LockNeverUnlockable`.
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
) -> bool {
    match &door.lock {
        None => true,
        Some(LockKind::SmallKey) => smallkey_bit.is_some_and(|bit| unlocked & bit != 0),
        Some(LockKind::Lantern) => items.lantern,
        Some(LockKind::Flag(_)) => false,
    }
}

/// One BFS state: which `SmallKey` doors have been permanently opened (a bit per door, indexed by
/// `smallkey_doors`), plus which permanent items have been picked up so far.
type StateKey = (u64, bool, bool);

/// A place the hero can actually be standing: a room plus which of that room's tile-connectivity
/// components (see [`compute_components`]) they are in. Two doors in the same room are not
/// necessarily mutually reachable, so the search branches on this instead of on `RoomIdx` alone.
type Loc = (RoomIdx, usize);

struct ReachabilitySearch<'w> {
    world: &'w World,
    smallkey_doors: Vec<(RoomIdx, Pos)>,
    /// Per room (indexed like `RoomIdx`), the tile -> connected-component-id map.
    components: Vec<HashMap<Pos, usize>>,
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
        let components = world.rooms.iter().map(compute_components).collect();
        ReachabilitySearch {
            world,
            smallkey_doors,
            components,
        }
    }

    fn component_of(&self, room: RoomIdx, at: Pos) -> Option<usize> {
        self.components[room.0 as usize].get(&at).copied()
    }

    fn bit_for(&self, room_idx: RoomIdx, at: Pos) -> Option<u64> {
        self.smallkey_doors
            .iter()
            .position(|&(r, p)| r == room_idx && p == at)
            .map(|i| 1u64 << i)
    }

    /// Flood-fills the room/component graph from `start`, then repeatedly folds in any
    /// newly-reachable permanent item until the item set stops growing (lantern, ember: at most
    /// two iterations).
    fn settle(&self, start: Loc, unlocked: u64, mut items: Items) -> (HashSet<Loc>, usize, Items) {
        loop {
            let reachable = self.flood(start, unlocked, items);
            let mut next = items;
            let mut keys_found = 0usize;
            for &(room, comp) in &reachable {
                for chest in &self.world.room(room).chests {
                    if self.component_of(room, chest.at) != Some(comp) {
                        continue; // chest sits in a different, currently-unreached part of the room
                    }
                    match chest.contains {
                        Reward::SmallKey => keys_found += 1,
                        Reward::Lantern => next.lantern = true,
                        Reward::Ember => next.ember = true,
                        _ => {}
                    }
                }
            }
            if next == items {
                return (reachable, keys_found, items);
            }
            items = next;
        }
    }

    /// A door only crosses to another room if the hero can actually walk to that door's tile from
    /// the component they are currently in (round-1 review, major).
    fn flood(&self, start: Loc, unlocked: u64, items: Items) -> HashSet<Loc> {
        let mut seen = HashSet::new();
        seen.insert(start);
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some((room, comp)) = queue.pop_front() {
            for door in &self.world.room(room).doors {
                if self.component_of(room, door.at) != Some(comp) {
                    continue;
                }
                let bit = self.bit_for(room, door.at);
                if !traversable(door, unlocked, bit, items) {
                    continue;
                }
                let Some(target_room) = self.world.room_idx(&door.to_room) else {
                    continue;
                };
                let Some(landing) = self.world.spawn_pos(target_room, &door.to_spawn) else {
                    continue;
                };
                let Some(target_comp) = self.component_of(target_room, landing) else {
                    continue;
                };
                let loc = (target_room, target_comp);
                if seen.insert(loc) {
                    queue.push_back(loc);
                }
            }
        }
        seen
    }

    /// Runs the full branching BFS and returns every state actually reached, each already
    /// settled (flood-filled and item-fixpointed).
    fn run(&self, start: Loc) -> Vec<(HashSet<Loc>, Items, u64)> {
        let mut visited: HashSet<StateKey> = HashSet::new();
        let mut settled_states = Vec::new();
        let mut queue = VecDeque::new();

        let initial = (0u64, Items::default());
        queue.push_back(initial);
        visited.insert((0, false, false));

        while let Some((unlocked, items)) = queue.pop_front() {
            let (reachable, keys_found, settled_items) = self.settle(start, unlocked, items);
            let keys_spent = unlocked.count_ones() as usize;
            settled_states.push((reachable.clone(), settled_items, unlocked));

            if keys_found > keys_spent {
                for &(room_idx, at) in &self.smallkey_doors {
                    let Some(comp) = self.component_of(room_idx, at) else {
                        continue;
                    };
                    if !reachable.contains(&(room_idx, comp)) {
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
    let Some(start_comp) = search.component_of(start_room, start_pos) else {
        return errors; // start spawn not walkable — already reported as SpawnNotWalkable
    };

    let states = search.run((start_room, start_comp));

    let mut all_reachable: HashSet<Loc> = HashSet::new();
    let mut ever_unlocked: u64 = 0;
    let mut ever_lantern = false;
    let mut ember_states: Vec<&HashSet<Loc>> = Vec::new();
    for (reachable, items, unlocked) in &states {
        all_reachable.extend(reachable.iter().copied());
        ever_unlocked |= unlocked;
        ever_lantern |= items.lantern;
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
        // The room is reached in at least one tile-connectivity component; a door outside every
        // reached component is exactly as unreachable as one that does not exist (round-1 review,
        // major) — reported directly here rather than only surfacing as a downstream
        // `RoomUnreachable` on some other room several doors later.
        for door in &room.doors {
            let Some(door_comp) = search.component_of(room_idx, door.at) else {
                continue; // not on a walkable tile — reported elsewhere as DoorTileMismatch
            };
            if !all_reachable.contains(&(room_idx, door_comp)) {
                errors.push(ContentError::DoorUnreachableInRoom {
                    room: room.id.clone(),
                    door: door.id.clone(),
                    from_spawn: entry_spawns_for_room(world, room_idx).join(", "),
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
        errors.push(ContentError::LockNeverUnlockable {
            door: door_id.to_string(),
            lock: lock.clone(),
        });
    }

    let ember_authored = world
        .rooms
        .iter()
        .any(|r| r.chests.iter().any(|c| c.contains == Reward::Ember));

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
    route: (ember_required: false, home: "room.a"),
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
