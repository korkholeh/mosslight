//! Pure projection of `App` state into a frame. Never mutates the game (spec §4/§9).

pub mod hud;
pub mod overlays;
pub mod scene;
pub mod theme;
pub mod tiles;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;

use crate::app::{App, Mode, MIN_COLS, MIN_ROWS};
pub use theme::Theme;

const HINT_TEXT: &str = "Esc: pause/back  Q: quit  ?: help";

pub fn draw(frame: &mut Frame, app: &App, theme: Theme) {
    let area = frame.area();

    // `app.mode` only becomes `TooSmall` once a resize event (or the startup probe) has told
    // `App` the real size; guarding on the frame's actual area as well means a mismatch between
    // `app.size` and what the terminal really handed us can never index outside the buffer.
    if app.mode == Mode::TooSmall || is_too_small(area) {
        overlays::draw_too_small(
            frame.buffer_mut(),
            area,
            (MIN_COLS, MIN_ROWS),
            (area.width, area.height),
        );
        return;
    }

    let layout = scene::Layout::compute(area.width, area.height);
    let buf = frame.buffer_mut();

    hud::draw_hud(buf, layout, &app.state);
    scene::draw_scene(buf, layout, &app.state, theme);
    draw_message_row(buf, layout, &app.message);
    draw_hint_row(buf, layout);
    overlays::draw_overlay_for_mode(buf, area, app);
}

fn draw_message_row(buf: &mut ratatui::buffer::Buffer, layout: scene::Layout, message: &str) {
    buf.set_string(layout.x0, layout.message_row(), message, Style::default());
}

fn draw_hint_row(buf: &mut ratatui::buffer::Buffer, layout: scene::Layout) {
    buf.set_string(layout.x0, layout.hint_row(), HINT_TEXT, Style::default());
}

/// Whether a size is at least the minimum playable terminal (spec §4).
pub fn is_too_small(area: Rect) -> bool {
    area.width < MIN_COLS || area.height < MIN_ROWS
}
