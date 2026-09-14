//! RON parse, row decode and id interning (spec §7/§8 stages 1-2). Stage 3 (`validate`) runs from
//! `parse` too, so the public entry point is one call that either succeeds or returns every error
//! found, in the stable order documented on `parse`.

use std::collections::HashMap;

use crate::game::entities::Pos;
use crate::game::tuning::{ROOM_H, ROOM_W};

use super::error::ContentError;
use super::schema::{RoomIdx, Tile, World};
use super::validate::collect_errors;

/// The embedded world, baked into the binary at compile time (spec §8).
pub const EMBEDDED: &str = include_str!("../../assets/world.ron");

/// Parses and validates the embedded world.
pub fn load() -> Result<World, Vec<ContentError>> {
    parse(EMBEDDED)
}

/// Runs all three stages and collects rather than short-circuits:
///
/// 1. **RON parse** — a syntax error is fatal, returned alone (nothing to continue with).
/// 2. **Decode** — per room: row count must be `ROOM_H`, every row exactly `ROOM_W` chars, every
///    char in the tile table. A room that fails to decode is dropped before stage 3.
/// 3. **Validate** — structural checks always run; the reachability search is skipped if stage 2
///    found any error (a missing room would otherwise cascade into unrelated route errors).
///
/// The returned errors are ordered: parse, then decode (by room, then position), then validation
/// (by check, then id) — stable, so tests can assert on the list rather than a set.
pub fn parse(src: &str) -> Result<World, Vec<ContentError>> {
    let mut world: World = ron::from_str(src).map_err(|e| {
        vec![ContentError::Parse {
            message: e.to_string(),
        }]
    })?;

    let mut errors = Vec::new();
    let mut decode_failed = false;

    let rooms = std::mem::take(&mut world.rooms);
    let mut kept = Vec::with_capacity(rooms.len());
    for mut room in rooms {
        let mut ok = true;
        if room.rows.len() != ROOM_H {
            let found_w = room.rows.first().map_or(ROOM_W, |r| r.chars().count());
            errors.push(ContentError::RoomDimensions {
                room: room.id.clone(),
                expected: (ROOM_W, ROOM_H),
                found: (found_w, room.rows.len()),
            });
            ok = false;
        } else {
            let mut grid = [[Tile::Wall; ROOM_W]; ROOM_H];
            for (y, row_str) in room.rows.iter().enumerate() {
                let chars: Vec<char> = row_str.chars().collect();
                if chars.len() != ROOM_W {
                    errors.push(ContentError::RoomDimensions {
                        room: room.id.clone(),
                        expected: (ROOM_W, ROOM_H),
                        found: (chars.len(), ROOM_H),
                    });
                    ok = false;
                    continue;
                }
                for (x, ch) in chars.into_iter().enumerate() {
                    match Tile::from_char(ch) {
                        Some(t) => grid[y][x] = t,
                        None => {
                            errors.push(ContentError::IllegalTile {
                                room: room.id.clone(),
                                at: Pos {
                                    x: x as u8,
                                    y: y as u8,
                                },
                                ch,
                            });
                            ok = false;
                        }
                    }
                }
            }
            if ok {
                room.tiles = grid;
            }
        }

        if ok {
            kept.push(room);
        } else {
            decode_failed = true;
        }
    }
    world.rooms = kept;

    world.room_index = intern_room_ids(&world.rooms);

    errors.extend(collect_errors(&world, !decode_failed));

    if errors.is_empty() {
        Ok(world)
    } else {
        Err(errors)
    }
}

fn intern_room_ids(rooms: &[super::schema::Room]) -> HashMap<String, RoomIdx> {
    let mut map = HashMap::with_capacity(rooms.len());
    for (i, room) in rooms.iter().enumerate() {
        map.insert(room.id.clone(), RoomIdx(i as u16));
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal two-room RON source. `row6_a`/`row7_a` override room.a's rows 6 and 7 so
    /// tests can deliberately plant a bad length and an illegal character independently.
    fn two_room_src(row6_a: &str, row7_a: &str) -> String {
        let wall = "#".repeat(ROOM_W);
        let floor_row = format!("#{}#", ".".repeat(ROOM_W - 2));
        let mut rows_a = vec![wall.clone()];
        for _ in 0..5 {
            rows_a.push(floor_row.clone());
        }
        rows_a.push(row6_a.to_string());
        rows_a.push(row7_a.to_string());
        for _ in 0..7 {
            rows_a.push(floor_row.clone());
        }
        rows_a.push(wall.clone());
        let rows_a_ron = rows_a
            .iter()
            .map(|r| format!("                \"{r}\","))
            .collect::<Vec<_>>()
            .join("\n");

        let door_row_b = format!("+{}#", ".".repeat(ROOM_W - 2));
        let mut rows_b = vec![wall.clone()];
        for _ in 0..6 {
            rows_b.push(floor_row.clone());
        }
        rows_b.push(door_row_b);
        for _ in 0..7 {
            rows_b.push(floor_row.clone());
        }
        rows_b.push(wall);
        let rows_b_ron = rows_b
            .iter()
            .map(|r| format!("                \"{r}\","))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "World(\n\
             \x20   version: 1,\n\
             \x20   start: (room: \"room.a\", spawn: \"spawn.a.start\"),\n\
             \x20   route: (ember_required: false, home: \"room.a\", goal: \"room.a\"),\n\
             \x20   rooms: [\n\
             \x20       (\n\
             \x20           id: \"room.a\",\n\
             \x20           name: \"A\",\n\
             \x20           kind: Overworld,\n\
             \x20           map_index: Some((0, 0)),\n\
             \x20           rows: [\n{rows_a_ron}\n            ],\n\
             \x20           doors: [(id: \"door.a.east\", at: (x: 23, y: 7), to_room: \"room.b\", to_spawn: \"spawn.b.west\", two_way: false)],\n\
             \x20           spawns: [(id: \"spawn.a.start\", at: (x: 2, y: 2))],\n\
             \x20       ),\n\
             \x20       (\n\
             \x20           id: \"room.b\",\n\
             \x20           name: \"B\",\n\
             \x20           kind: Overworld,\n\
             \x20           map_index: Some((1, 0)),\n\
             \x20           rows: [\n{rows_b_ron}\n            ],\n\
             \x20           doors: [(id: \"door.b.west\", at: (x: 0, y: 7), to_room: \"room.a\", to_spawn: \"spawn.a.start\", two_way: false)],\n\
             \x20           spawns: [(id: \"spawn.b.west\", at: (x: 1, y: 7))],\n\
             \x20       ),\n\
             \x20   ],\n\
             )\n"
        )
    }

    fn valid_floor_row() -> String {
        format!("#{}#", ".".repeat(ROOM_W - 2))
    }

    fn valid_row7_a() -> String {
        format!("#{}+", ".".repeat(ROOM_W - 2))
    }

    #[test]
    fn a_two_room_source_parses() {
        let src = two_room_src(&valid_floor_row(), &valid_row7_a());
        let world = parse(&src).expect("valid two-room world");
        assert_eq!(world.rooms.len(), 2);
        assert_eq!(world.room_idx("room.a"), Some(RoomIdx(0)));
        assert_eq!(world.room_idx("room.b"), Some(RoomIdx(1)));
    }

    #[test]
    fn a_bad_row_length_and_an_illegal_char_are_both_reported_from_one_source() {
        // Row 6: an illegal 'x', correct length. Row 7: one character short.
        let illegal_char_row = format!("#{}x#", ".".repeat(ROOM_W - 3));
        let short_row = format!("#{}+", ".".repeat(ROOM_W - 3));
        let bad = two_room_src(&illegal_char_row, &short_row);
        let errors = parse(&bad).expect_err("expected decode errors");
        assert!(
            errors.iter().any(
                |e| matches!(e, ContentError::RoomDimensions { room, .. } if room == "room.a")
            ),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ContentError::IllegalTile { room, ch, .. } if room == "room.a" && *ch == 'x')),
            "{errors:?}"
        );
    }
}
