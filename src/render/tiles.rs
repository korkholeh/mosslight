//! The single glyph table (spec §4). Glyphs never change per theme — only colour does
//! (see `theme.rs`), so a monochrome theme cannot lose information.

use crate::game::Tile;

/// Everything the scene can draw a glyph for: world tiles plus the hero.
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [Kind; 8] = [
        Kind::Hero,
        Kind::Wall,
        Kind::Floor,
        Kind::Water,
        Kind::Bush,
        Kind::Door,
        Kind::Stairs,
        Kind::Pit,
    ];

    #[test]
    fn every_kind_has_a_glyph() {
        for kind in ALL_KINDS {
            assert_ne!(glyph(kind), '\0');
        }
    }
}
