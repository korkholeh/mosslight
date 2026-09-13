//! Top bar: health, active item, key count (spec §4).

use ratatui::buffer::Buffer;
use ratatui::style::Style;

use super::scene::Layout;
use crate::game::GameState;

pub fn draw_hud(buf: &mut Buffer, layout: Layout, state: &GameState) {
    let hp_text = format!(
        "HP {}/{}  Item: none  Keys: {}",
        state.hero.health_halves, state.hero.max_health_halves, state.hero.keys
    );
    buf.set_string(layout.x0, layout.hud_row(), hp_text, Style::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{debug_room, GameState as GS};
    use ratatui::layout::Rect;

    #[test]
    fn hud_contains_hp_label() {
        let state = GS::new(1, debug_room());
        let layout = Layout::compute(60, 24);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 24));
        draw_hud(&mut buf, layout, &state);
        let row: String = (0..60)
            .map(|x| {
                buf[(x, layout.hud_row())]
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(row.contains("HP"));
    }
}
