//! Colour only. Themes may never change a glyph (spec §4): colour is not the only distinction.

use ratatui::style::Color;

use super::tiles::Kind;
use crate::config::{ColorMode, ThemeName};

const GAMEBOY_LIGHTEST: Color = Color::Indexed(155);
const GAMEBOY_LIGHT: Color = Color::Indexed(149);
const GAMEBOY_DARK: Color = Color::Indexed(107);
const GAMEBOY_DARKEST: Color = Color::Indexed(22);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    name: ThemeName,
    color: ColorMode,
}

impl Theme {
    pub fn new(name: ThemeName, color: ColorMode) -> Self {
        Theme { name, color }
    }

    /// Foreground colour for a tile/hero kind. Under `ColorMode::Never` every kind resolves to
    /// `Color::Reset` — the terminal's own default foreground, i.e. no SGR colour is written at
    /// all — so `NO_COLOR`/`--color never` stop at the renderer, not merely in `Config`.
    pub fn color_for(self, kind: Kind) -> Color {
        if self.color == ColorMode::Never {
            return Color::Reset;
        }
        match self.name {
            ThemeName::Mono => mono_color(kind),
            ThemeName::Gameboy => gameboy_color(kind),
            ThemeName::Ansi => ansi_color(kind),
        }
    }
}

/// `White` for everything foreground-significant, `DarkGray` for floor/pit/opened-chest/unlit-torch
/// (spec §4's "повноцінний монохромний режим") — only white/black-family colours, so a monochrome
/// terminal stays fully legible.
fn mono_color(kind: Kind) -> Color {
    match kind {
        Kind::Floor | Kind::Pit | Kind::ChestOpen | Kind::Torch => Color::DarkGray,
        _ => Color::White,
    }
}

/// Four `Color::Indexed` greens (spec §4's "чотири відтінки зеленого"), assigned by *role* rather
/// than by object: hero and rewards get the lightest shade, live threats and interactables the
/// second, structure the third, background the darkest. Several kinds deliberately share a shade —
/// the glyph is the distinction (RISKS #10; `tiles::glyph` takes no theme argument, so it cannot
/// vary here). 256-colour indexed, not truecolor; `--theme ansi` is the documented fallback for a
/// 16-colour terminal.
fn gameboy_color(kind: Kind) -> Color {
    match kind {
        Kind::Hero | Kind::Chest | Kind::Sword | Kind::Beacon => GAMEBOY_LIGHTEST,
        Kind::Slime
        | Kind::Bat
        | Kind::Guardian
        | Kind::Boss
        | Kind::BossVulnerable
        | Kind::Telegraph
        | Kind::Npc
        | Kind::Torch
        | Kind::TorchLit
        | Kind::Plate
        | Kind::PlatePressed
        | Kind::Block
        | Kind::ChestOpen
        | Kind::Door
        | Kind::Stairs => GAMEBOY_LIGHT,
        Kind::Wall => GAMEBOY_DARK,
        Kind::Floor | Kind::Water | Kind::Bush | Kind::Pit => GAMEBOY_DARKEST,
    }
}

/// Only the 16 named ANSI colours (spec §4's "палітра ANSI 16 кольорів") — the documented fallback
/// for a terminal that cannot show `Gameboy`'s indexed greens.
fn ansi_color(kind: Kind) -> Color {
    match kind {
        Kind::Hero => Color::LightYellow,
        Kind::Wall => Color::Green,
        Kind::Floor => Color::DarkGray,
        Kind::Water => Color::Cyan,
        Kind::Bush => Color::LightGreen,
        Kind::Door => Color::Yellow,
        Kind::Stairs => Color::Gray,
        Kind::Pit => Color::DarkGray,
        Kind::Slime => Color::Green,
        Kind::Bat => Color::Magenta,
        Kind::Guardian => Color::Red,
        Kind::Boss => Color::Red,
        Kind::BossVulnerable => Color::LightYellow,
        Kind::Sword => Color::White,
        Kind::Telegraph => Color::LightRed,
        Kind::Npc => Color::LightCyan,
        Kind::Chest => Color::LightYellow,
        Kind::ChestOpen => Color::DarkGray,
        Kind::Torch => Color::DarkGray,
        Kind::TorchLit => Color::LightRed,
        Kind::Plate => Color::Gray,
        Kind::PlatePressed => Color::LightGreen,
        Kind::Block => Color::Yellow,
        Kind::Beacon => Color::LightRed,
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
    fn mono_theme_only_uses_white_or_black_family_colors() {
        let theme = Theme::new(ThemeName::Mono, ColorMode::Always);
        for kind in ALL_KINDS {
            let color = theme.color_for(kind);
            assert!(
                matches!(
                    color,
                    Color::White | Color::Black | Color::Gray | Color::DarkGray
                ),
                "{kind:?} resolved to {color:?} under Mono, breaking monochrome legibility"
            );
        }
    }

    #[test]
    fn gameboy_theme_only_uses_the_four_authored_greens() {
        let theme = Theme::new(ThemeName::Gameboy, ColorMode::Always);
        for kind in ALL_KINDS {
            let color = theme.color_for(kind);
            assert!(
                matches!(
                    color,
                    Color::Indexed(155)
                        | Color::Indexed(149)
                        | Color::Indexed(107)
                        | Color::Indexed(22)
                ),
                "{kind:?} resolved to {color:?} under Gameboy, outside the four authored greens"
            );
        }
    }

    #[test]
    fn ansi_theme_only_uses_the_sixteen_named_colors() {
        let theme = Theme::new(ThemeName::Ansi, ColorMode::Always);
        for kind in ALL_KINDS {
            let color = theme.color_for(kind);
            assert!(
                matches!(
                    color,
                    Color::Black
                        | Color::Red
                        | Color::Green
                        | Color::Yellow
                        | Color::Blue
                        | Color::Magenta
                        | Color::Cyan
                        | Color::Gray
                        | Color::DarkGray
                        | Color::LightRed
                        | Color::LightGreen
                        | Color::LightYellow
                        | Color::LightBlue
                        | Color::LightMagenta
                        | Color::LightCyan
                        | Color::White
                ),
                "{kind:?} resolved to {color:?} under Ansi, outside the 16 named ANSI colours"
            );
        }
    }

    #[test]
    fn color_mode_never_resolves_to_reset_regardless_of_theme() {
        for name in [ThemeName::Gameboy, ThemeName::Ansi, ThemeName::Mono] {
            let theme = Theme::new(name, ColorMode::Never);
            for kind in ALL_KINDS {
                assert_eq!(theme.color_for(kind), Color::Reset);
            }
        }
    }
}
