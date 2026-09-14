//! The single glyph table (spec §4). Glyphs never change per theme — only colour does
//! (see `theme.rs`), so a monochrome theme cannot lose information.

use crate::game::{EnemyKind, Tile};

/// Everything the scene can draw a glyph for: world tiles, the hero, enemies, and the sword/danger
/// projections (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Hero,
    Wall,
    Floor,
    Water,
    Bush,
    Door,
    Stairs,
    Pit,
    Slime,
    Bat,
    Guardian,
    Sword,
    Telegraph,
}

impl From<Tile> for Kind {
    fn from(t: Tile) -> Self {
        match t {
            Tile::Floor => Kind::Floor,
            Tile::Wall => Kind::Wall,
            Tile::Water => Kind::Water,
            Tile::Bush => Kind::Bush,
            Tile::Door => Kind::Door,
            Tile::Stairs => Kind::Stairs,
            Tile::Pit => Kind::Pit,
            // Deliberately indistinguishable from a wall until phase 4's reveal mechanic exists.
            Tile::Hidden => Kind::Wall,
        }
    }
}

impl From<EnemyKind> for Kind {
    fn from(k: EnemyKind) -> Self {
        match k {
            EnemyKind::Slime => Kind::Slime,
            EnemyKind::Bat => Kind::Bat,
            EnemyKind::Guardian => Kind::Guardian,
        }
    }
}

/// The ASCII glyph for a kind (spec §4/§7 table). Unicode is a phase-6 enhancement; this phase
/// always renders this table regardless of `Config::glyphs`.
pub fn glyph(kind: Kind) -> char {
    match kind {
        Kind::Hero => '@',
        Kind::Wall => '#',
        Kind::Floor => '.',
        Kind::Water => '~',
        Kind::Bush => '"',
        Kind::Door => '+',
        Kind::Stairs => '>',
        Kind::Pit => 'v',
        Kind::Slime => 'o',
        Kind::Bat => '^',
        Kind::Guardian => '&',
        Kind::Sword => '/',
        Kind::Telegraph => '!',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [Kind; 13] = [
        Kind::Hero,
        Kind::Wall,
        Kind::Floor,
        Kind::Water,
        Kind::Bush,
        Kind::Door,
        Kind::Stairs,
        Kind::Pit,
        Kind::Slime,
        Kind::Bat,
        Kind::Guardian,
        Kind::Sword,
        Kind::Telegraph,
    ];

    #[test]
    fn every_kind_has_a_glyph() {
        for kind in ALL_KINDS {
            assert_ne!(glyph(kind), '\0');
        }
    }
}
