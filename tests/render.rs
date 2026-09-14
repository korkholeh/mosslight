//! Scene placement, HUD/hint rows, the too-small notice and the main-menu overlay, rendered
//! through `TestBackend` — the deterministic equivalent of a screenshot (spec §4, §13).

use std::path::PathBuf;
use std::rc::Rc;

use mosslight::app::App;
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::{Action, AiState, Enemy, EnemyId, EnemyKind, Facing, Pos, Swing, World};
use mosslight::render::{self, Theme};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn cfg() -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps: Fps::F20,
        save_dir: PathBuf::from("/tmp"),
        seed: 1,
        debug_panic: false,
        debug_content: None,
    }
}

fn cell(buf: &Buffer, x: u16, y: u16) -> String {
    buf[(x, y)].symbol().to_string()
}

fn buffer_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

fn render_at(w: u16, h: u16, setup: impl FnOnce(&mut App)) -> Buffer {
    let mut app = App::new(&cfg(), world());
    setup(&mut app);
    app.on_resize(w, h);
    let theme = Theme::new(cfg().theme);
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render::draw(f, &app, theme)).unwrap();
    term.backend().buffer().clone()
}

#[test]
fn scene_placement_60x24() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]); // New Game -> Playing
    });

    // Hero spawns at tile (12, 8): col 30, row 11.
    assert_eq!(cell(&buf, 30, 11), "@");
    // Room corners: wall ring at tile (0,0)/(23,0)/(0,15)/(23,15).
    assert_eq!(cell(&buf, 6, 3), "#");
    assert_eq!(cell(&buf, 52, 3), "#");
    assert_eq!(cell(&buf, 6, 18), "#");
    assert_eq!(cell(&buf, 52, 18), "#");

    let text = buffer_text(&buf);
    let hud_row: String = (0..60).map(|x| cell(&buf, x, 1)).collect();
    assert!(hud_row.contains("HP"));
    let hint_row: String = (0..60).map(|x| cell(&buf, x, 21)).collect();
    assert!(hint_row.contains("Esc"));
    assert!(hint_row.contains('Q'));
    let _ = text;
}

#[test]
fn scene_centred_80x24() {
    let buf = render_at(80, 24, |app| {
        app.apply(&[Action::Confirm]);
    });
    // x0 = (80-50)/2 = 15; hero tile (12,8) -> col = 15+1+24 = 40, row = 1+2+8 = 11.
    assert_eq!(cell(&buf, 40, 11), "@");
    // Top-left wall corner: col = 15+1 = 16, row = 3.
    assert_eq!(cell(&buf, 16, 3), "#");
}

#[test]
fn too_small_notice_shows_required_and_current_size() {
    let buf = render_at(59, 23, |_app| {});
    let text = buffer_text(&buf);
    assert!(text.contains("60x24"));
    assert!(text.contains("59x23"));
}

#[test]
fn draw_never_panics_on_a_too_small_frame_even_without_a_prior_resize() {
    // `App::new` leaves `app.size` at the 60x24 minimum and `app.mode` at `MainMenu` — the state
    // before any `Resize` event (or the startup size probe) has ever run. If `render::draw` only
    // trusted `app.mode`/`app.size` here instead of the frame it was actually given, this would
    // index outside the buffer instead of drawing the too-small notice.
    for (w, h) in [(100u16, 20u16), (59u16, 23u16)] {
        let app = App::new(&cfg(), world());
        let theme = Theme::new(cfg().theme);
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render::draw(f, &app, theme)).unwrap();
        let text = buffer_text(term.backend().buffer());
        assert!(
            text.contains("60x24"),
            "expected the too-small notice at {w}x{h}, got:\n{text}"
        );
    }
}

fn combat_scene_setup(app: &mut App) {
    app.apply(&[Action::Confirm]); // New Game -> Playing
    app.state.enemies = vec![
        Enemy {
            id: EnemyId(0),
            kind: EnemyKind::Slime,
            pos: Pos { x: 5, y: 5 },
            facing: Facing::South,
            hp: 2,
            ai: AiState::SlimeIdle { until: 1000 },
            patrol: Vec::new(),
            move_ready_at: 1000,
            alive: true,
        },
        Enemy {
            id: EnemyId(1),
            kind: EnemyKind::Guardian,
            pos: Pos { x: 8, y: 5 },
            facing: Facing::East,
            hp: 4,
            ai: AiState::GuardianTelegraph {
                until: 1000,
                facing: Facing::East,
            },
            patrol: Vec::new(),
            move_ready_at: 1000,
            alive: true,
        },
    ];
    app.state.hero.attack = Some(Swing {
        started_at: 0,
        facing: Facing::South,
        at: Some(Pos { x: 12, y: 9 }),
        hit: Vec::new(),
    });
}

#[test]
fn enemy_sword_and_telegraph_glyphs_land_on_expected_cells_60x24() {
    let buf = render_at(60, 24, combat_scene_setup);

    // Slime glyph at tile (5, 5): col = 5 + 1 + 2*5 = 16, row = 1 + 2 + 5 = 8.
    assert_eq!(cell(&buf, 16, 8), "o");
    // Guardian glyph at tile (8, 5): col = 22, row = 8. Drawn after the telegraph lane, so its
    // own tile is never hidden by the danger cue.
    assert_eq!(cell(&buf, 22, 8), "&");
    // The telegraph lane starts one tile east of the guardian, at (9, 5): col = 24, row = 8.
    assert_eq!(cell(&buf, 24, 8), "!");
    // Sword glyph at the swing's target tile (12, 9): col = 30, row = 12.
    assert_eq!(cell(&buf, 30, 12), "/");
}

#[test]
fn scene_renders_identical_characters_under_every_theme() {
    let mut app = App::new(&cfg(), world());
    combat_scene_setup(&mut app);
    app.on_resize(60, 24);

    let texts: Vec<String> = [ThemeName::Mono, ThemeName::Gameboy, ThemeName::Ansi]
        .into_iter()
        .map(|theme_name| {
            let theme = Theme::new(theme_name);
            let backend = TestBackend::new(60, 24);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| render::draw(f, &app, theme)).unwrap();
            buffer_text(term.backend().buffer())
        })
        .collect();

    assert_eq!(texts[0], texts[1], "mono vs gameboy glyphs differ");
    assert_eq!(texts[1], texts[2], "gameboy vs ansi glyphs differ");
}

#[test]
fn main_menu_overlay_lists_all_items() {
    let buf = render_at(60, 24, |_app| {});
    let text = buffer_text(&buf);
    assert!(text.contains("Continue"));
    assert!(text.contains("New Game"));
    assert!(text.contains("Help"));
    assert!(text.contains("Quit"));
}
