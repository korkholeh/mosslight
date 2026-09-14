//! The glyph tables (spec §4). Glyphs never change per theme — only colour does (see
//! `theme.rs`), so a monochrome theme cannot lose information. There are exactly two tables,
//! selected by `Config::glyphs` (`GlyphSet::Ascii`/`GlyphSet::Unicode`): ASCII is the default and
//! the one guaranteed to render correctly everywhere; Unicode is the optional enhancement spec §4
//! allows ("тільки символи з перевіреною шириною" — only characters of verified width).

use crate::config::GlyphSet;
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
    Boss,
    BossVulnerable,
    Sword,
    Telegraph,
    Npc,
    Chest,
    ChestOpen,
    Torch,
    TorchLit,
    Plate,
    PlatePressed,
    Block,
    Beacon,
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
    /// The boss's default (non-vulnerable) glyph; `scene::draw_scene` overrides this to
    /// `Kind::BossVulnerable` while the boss's `AiState` is `BossVulnerable`.
    fn from(k: EnemyKind) -> Self {
        match k {
            EnemyKind::Slime => Kind::Slime,
            EnemyKind::Bat => Kind::Bat,
            EnemyKind::Guardian => Kind::Guardian,
            EnemyKind::Boss => Kind::Boss,
        }
    }
}

/// The glyph for a kind under `set` (spec §4/§7 table). Dispatches to `ascii_glyph` or
/// `unicode_glyph`; every caller in `render/` goes through this rather than either table directly,
/// so `Config::glyphs` is the only switch.
pub fn glyph(kind: Kind, set: GlyphSet) -> char {
    match set {
        GlyphSet::Ascii => ascii_glyph(kind),
        GlyphSet::Unicode => unicode_glyph(kind),
    }
}

/// The ASCII glyph for a kind — the default table, guaranteed to render as exactly one column in
/// any terminal.
fn ascii_glyph(kind: Kind) -> char {
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
        Kind::Boss => 'W',
        Kind::BossVulnerable => 'w',
        Kind::Sword => '/',
        Kind::Telegraph => '!',
        Kind::Npc => 'N',
        Kind::Chest => 'C',
        Kind::ChestOpen => 'c',
        Kind::Torch => 't',
        Kind::TorchLit => 'T',
        Kind::Plate => '_',
        Kind::PlatePressed => '=',
        Kind::Block => 'O',
        Kind::Beacon => '*',
    }
}

/// The Unicode glyph for a kind — `--unicode` (spec §4's optional, verified-width enhancement).
/// Every character here is checked against Unicode's `East_Asian_Width` property (via Python's
/// `unicodedata`, which encodes the same data as `EastAsianWidth.txt`) and is `Neutral` or
/// `Narrow`, never `Ambiguous`/`Wide`/`Fullwidth` — the ambiguous blocks (Box Drawing, most of
/// Block Elements, most arrows and geometric shapes) are exactly what got `--unicode` withdrawn in
/// the first place (see DECISIONS.md), because a CJK locale or a terminal's "ambiguous characters
/// are wide" setting renders them two columns wide and shears the fixed tile grid. No
/// `unicode-width` dependency is needed because this table is fixed at compile time, not measuring
/// arbitrary text.
fn unicode_glyph(kind: Kind) -> char {
    match kind {
        Kind::Hero => '☺',
        Kind::Wall => '◼',
        Kind::Floor => '.',
        Kind::Water => '~',
        Kind::Bush => '✿',
        Kind::Door => '▯',
        Kind::Stairs => '⇗',
        Kind::Pit => '◌',
        Kind::Slime => '∾',
        Kind::Bat => '✢',
        Kind::Guardian => '◉',
        Kind::Boss => '☠',
        Kind::BossVulnerable => '◍',
        Kind::Sword => '⚔',
        Kind::Telegraph => '‼',
        Kind::Npc => '☻',
        Kind::Chest => '❐',
        Kind::ChestOpen => '▭',
        Kind::Torch => '☽',
        Kind::TorchLit => '☀',
        Kind::Plate => '⚬',
        Kind::PlatePressed => '⚉',
        Kind::Block => '◧',
        Kind::Beacon => '✦',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [Kind; 24] = [
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
        Kind::Boss,
        Kind::BossVulnerable,
        Kind::Sword,
        Kind::Telegraph,
        Kind::Npc,
        Kind::Chest,
        Kind::ChestOpen,
        Kind::Torch,
        Kind::TorchLit,
        Kind::Plate,
        Kind::PlatePressed,
        Kind::Block,
        Kind::Beacon,
    ];

    #[test]
    fn every_ascii_glyph_is_distinct() {
        // RISKS #10 (monochrome unreadable) rests on every kind being distinguishable by glyph
        // alone; `assert_ne!(glyph(kind), '\0')` over an exhaustive match can never fail (round-2
        // review, major) and says nothing about distinctness. A `HashSet` catches a copy-pasted
        // glyph directly.
        let glyphs: std::collections::HashSet<char> = ALL_KINDS
            .iter()
            .map(|&k| glyph(k, GlyphSet::Ascii))
            .collect();
        assert_eq!(
            glyphs.len(),
            ALL_KINDS.len(),
            "two Kinds share an ASCII glyph, breaking monochrome distinguishability"
        );
    }

    #[test]
    fn every_unicode_glyph_is_distinct() {
        let glyphs: std::collections::HashSet<char> = ALL_KINDS
            .iter()
            .map(|&k| glyph(k, GlyphSet::Unicode))
            .collect();
        assert_eq!(
            glyphs.len(),
            ALL_KINDS.len(),
            "two Kinds share a Unicode glyph, breaking monochrome distinguishability"
        );
    }

    #[test]
    fn every_unicode_glyph_is_a_verified_safe_codepoint() {
        // Regression gate for RISKS #15 / the Unicode decision: this is the exact set each
        // `unicode_glyph` character was checked against via `unicodedata.east_asian_width` before
        // being chosen (all `N` or `Na`, never `A`/`W`/`F`). It does not re-derive width — there is
        // no `unicode-width` dependency (Cargo.toml is fixed) — it only catches an edit to
        // `unicode_glyph` that swaps in an unverified character without re-running that check and
        // updating this list to match.
        const VERIFIED_SAFE: &[char] = &[
            '☺', '◼', '.', '~', '✿', '▯', '⇗', '◌', '∾', '✢', '◉', '☠', '◍', '⚔', '‼', '☻', '❐',
            '▭', '☽', '☀', '⚬', '⚉', '◧', '✦',
        ];
        for &kind in ALL_KINDS.iter() {
            let c = glyph(kind, GlyphSet::Unicode);
            assert!(
                VERIFIED_SAFE.contains(&c),
                "{kind:?}'s Unicode glyph {c:?} is not in the verified-width allow-list; check \
                 unicodedata.east_asian_width(c) is N or Na, then add it here"
            );
        }
    }
}
