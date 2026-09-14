//! Presentation (spec §4, §12, RISKS #10): the same scene renders identical characters under
//! every theme, each theme stays inside its documented colour family, and `NO_COLOR`/`--color`
//! reach all the way from `Config` through to the rendered buffer.
//!
//! `tests/render.rs` already proves glyph identity for the hero/enemy/sword/telegraph/npc/beacon
//! combination (`scene_renders_identical_characters_under_every_theme`,
//! `object_kind_glyphs_render_identically_under_every_theme`); this file adds the one kind that
//! combination is missing (`Chest`/`ChestOpen`), plus the colour-family and `ColorMode`
//! end-to-end checks that belong at the rendered-buffer level rather than `Theme::color_for`
//! alone (which `src/render/theme.rs`'s own unit tests already cover in isolation).

mod common;

use mosslight::app::App;
use mosslight::config::{ColorMode, Config, Env, GlyphSet, ThemeName};
use mosslight::game::{Action, AiState, Enemy, EnemyId, EnemyKind, Facing, Pos};
use mosslight::render::scene::Layout;
use mosslight::render::{self, Theme};
use mosslight::save::MemorySaveIo;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;

const W: u16 = 60;
const H: u16 = 24;

/// Enters `room.crossroads` (wall/floor/water/bush/door/chest, per `assets/world.ron`) and adds a
/// synthetic enemy, so the rendered scene covers hero, wall, bush, chest and enemy in one frame.
fn crossroads_scene() -> App {
    let mut app = App::new(
        &common::cfg(1),
        common::world(),
        Box::new(MemorySaveIo::new()),
    );
    app.apply(&[Action::Confirm]);
    let crossroads = app
        .state
        .world
        .room_idx("room.crossroads")
        .expect("room.crossroads exists");
    app.state.room = crossroads;
    app.state.enter_room();
    app.state.hero.pos = Pos { x: 12, y: 1 };
    app.state.enemies = vec![Enemy {
        id: EnemyId(0),
        kind: EnemyKind::Slime,
        pos: Pos { x: 20, y: 10 },
        facing: Facing::South,
        hp: 2,
        ai: AiState::SlimeIdle { until: 1_000_000 },
        patrol: Vec::new(),
        move_ready_at: 1_000_000,
        alive: true,
    }];
    app.on_resize(W, H);
    app
}

fn render_text(app: &App, theme: Theme) -> String {
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).expect("TestBackend never fails");
    term.draw(|f| render::draw(f, app, theme, GlyphSet::Ascii))
        .unwrap();
    buffer_text(term.backend().buffer())
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

/// Every scene-tile cell's foreground colour, for the 24x16 grid `scene::draw_scene` actually
/// paints with `theme.color_for` — deliberately excludes the HUD/message/hint rows and the scene
/// border, which are drawn with `Style::default()` and stay `Color::Reset` regardless of theme.
fn scene_tile_colors(buf: &Buffer) -> Vec<Color> {
    let layout = Layout::compute(W, H);
    let mut colors = Vec::new();
    for ty in 0..16u8 {
        for tx in 0..24u8 {
            let (col, row) = (layout.tile_col(tx), layout.tile_row(ty));
            colors.push(buf[(col, row)].fg);
        }
    }
    colors
}

#[test]
fn the_same_scene_renders_identical_characters_under_every_theme() {
    let app = crossroads_scene();
    let mono = render_text(&app, Theme::new(ThemeName::Mono, ColorMode::Always));
    let gameboy = render_text(&app, Theme::new(ThemeName::Gameboy, ColorMode::Always));
    let ansi = render_text(&app, Theme::new(ThemeName::Ansi, ColorMode::Always));
    assert_eq!(mono, gameboy, "mono vs gameboy glyphs differ");
    assert_eq!(gameboy, ansi, "gameboy vs ansi glyphs differ");
}

#[test]
fn gameboy_uses_only_the_four_authored_greens() {
    let app = crossroads_scene();
    let text = render_text(&app, Theme::new(ThemeName::Gameboy, ColorMode::Always));
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render::draw(
            f,
            &app,
            Theme::new(ThemeName::Gameboy, ColorMode::Always),
            GlyphSet::Ascii,
        )
    })
    .unwrap();
    for color in scene_tile_colors(term.backend().buffer()) {
        assert!(
            matches!(
                color,
                Color::Indexed(155)
                    | Color::Indexed(149)
                    | Color::Indexed(107)
                    | Color::Indexed(22)
            ),
            "found {color:?} outside the four authored greens; scene was:\n{text}"
        );
    }
}

#[test]
fn ansi_uses_only_the_sixteen_named_ansi_colors() {
    let app = crossroads_scene();
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render::draw(
            f,
            &app,
            Theme::new(ThemeName::Ansi, ColorMode::Always),
            GlyphSet::Ascii,
        )
    })
    .unwrap();
    for color in scene_tile_colors(term.backend().buffer()) {
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
            "found {color:?} outside the 16 named ANSI colours"
        );
    }
}

#[test]
fn mono_uses_only_white_family_colors() {
    let app = crossroads_scene();
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render::draw(
            f,
            &app,
            Theme::new(ThemeName::Mono, ColorMode::Always),
            GlyphSet::Ascii,
        )
    })
    .unwrap();
    for color in scene_tile_colors(term.backend().buffer()) {
        assert!(
            matches!(
                color,
                Color::White | Color::Black | Color::Gray | Color::DarkGray
            ),
            "found {color:?} outside the mono white/black family"
        );
    }
}

#[test]
fn color_mode_never_writes_no_color_at_all() {
    let app = crossroads_scene();
    for theme_name in [ThemeName::Gameboy, ThemeName::Ansi, ThemeName::Mono] {
        let backend = TestBackend::new(W, H);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render::draw(
                f,
                &app,
                Theme::new(theme_name, ColorMode::Never),
                GlyphSet::Ascii,
            )
        })
        .unwrap();
        for color in scene_tile_colors(term.backend().buffer()) {
            assert_eq!(
                color,
                Color::Reset,
                "{theme_name:?} under ColorMode::Never must write no colour at all"
            );
        }
    }
}

fn env() -> Env {
    Env {
        no_color: None,
        term: Some("xterm-256color".to_string()),
        stdout_tty: true,
        xdg_data_home: None,
        home: Some("/home/tester".to_string()),
        os_is_macos: false,
    }
}

/// Drives the full `Config` precedence (already unit-tested in `tests/config.rs`) through to a
/// rendered buffer, so `NO_COLOR`/`--color` are proved to reach the renderer end to end rather
/// than only `resolve_color`'s return value.
#[test]
fn no_color_env_disables_color_and_an_explicit_color_flag_overrides_it() {
    let app = crossroads_scene();

    let no_color_env = Env {
        no_color: Some("1".to_string()),
        ..env()
    };
    let cfg = Config::from_args(["mosslight"], &no_color_env).expect("parses");
    assert_eq!(cfg.color, ColorMode::Never);
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render::draw(f, &app, Theme::new(cfg.theme, cfg.color), cfg.glyphs))
        .unwrap();
    for color in scene_tile_colors(term.backend().buffer()) {
        assert_eq!(color, Color::Reset, "NO_COLOR must disable colour");
    }

    let overridden =
        Config::from_args(["mosslight", "--color", "always"], &no_color_env).expect("parses");
    assert_eq!(
        overridden.color,
        ColorMode::Always,
        "an explicit --color flag must override NO_COLOR"
    );
    let backend = TestBackend::new(W, H);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render::draw(
            f,
            &app,
            Theme::new(overridden.theme, overridden.color),
            overridden.glyphs,
        )
    })
    .unwrap();
    assert!(
        scene_tile_colors(term.backend().buffer())
            .iter()
            .any(|c| *c != Color::Reset),
        "--color always must override NO_COLOR and actually write colour"
    );
}
