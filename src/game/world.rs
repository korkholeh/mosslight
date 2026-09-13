//! Phase-1 hard-coded room. Phase 2 replaces this with the RON content pipeline.

use super::entities::Pos;
use super::tuning::{ROOM_H, ROOM_W};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Tile {
    Floor,
    Wall,
    Water,
    Bush,
}

impl Tile {
    /// Whether the hero can step onto this tile (spec §6).
    pub fn is_walkable(self) -> bool {
        matches!(self, Tile::Floor)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Room {
    pub tiles: [[Tile; ROOM_W]; ROOM_H],
    pub spawn: Pos,
}

impl Room {
    pub fn tile_at(&self, pos: Pos) -> Option<Tile> {
        self.tiles
            .get(pos.y as usize)
            .and_then(|row| row.get(pos.x as usize))
            .copied()
    }
}

/// A hand-built 24x16 room: a wall ring, two interior wall stubs, a water pool and a bush patch.
pub fn debug_room() -> Room {
    let mut tiles = [[Tile::Floor; ROOM_W]; ROOM_H];

    for cell in tiles[0].iter_mut() {
        *cell = Tile::Wall;
    }
    for cell in tiles[ROOM_H - 1].iter_mut() {
        *cell = Tile::Wall;
    }
    for row in tiles.iter_mut() {
        row[0] = Tile::Wall;
        row[ROOM_W - 1] = Tile::Wall;
    }

    // Two interior wall stubs.
    for row in tiles.iter_mut().take(6).skip(3) {
        row[6] = Tile::Wall;
    }
    for cell in tiles[10].iter_mut().take(18).skip(14) {
        *cell = Tile::Wall;
    }

    // A water pool.
    for row in tiles.iter_mut().take(7).skip(4) {
        for cell in row.iter_mut().take(20).skip(16) {
            *cell = Tile::Water;
        }
    }

    // A bush patch.
    for row in tiles.iter_mut().take(13).skip(11) {
        for cell in row.iter_mut().take(7).skip(3) {
            *cell = Tile::Bush;
        }
    }

    Room {
        tiles,
        spawn: Pos { x: 12, y: 8 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_room_spawn_is_walkable() {
        let room = debug_room();
        assert_eq!(room.tile_at(room.spawn), Some(Tile::Floor));
    }

    #[test]
    fn debug_room_has_walls_water_and_bush() {
        let room = debug_room();
        assert_eq!(room.tile_at(Pos { x: 0, y: 0 }), Some(Tile::Wall));
        assert_eq!(room.tile_at(Pos { x: 16, y: 4 }), Some(Tile::Water));
        assert_eq!(room.tile_at(Pos { x: 3, y: 11 }), Some(Tile::Bush));
    }
}
