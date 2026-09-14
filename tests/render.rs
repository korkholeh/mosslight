//! Scene placement, HUD/hint rows, the too-small notice and the main-menu overlay, rendered
//! through `TestBackend` — the deterministic equivalent of a screenshot (spec §4, §13).

mod common;

use std::path::PathBuf;
use std::rc::Rc;

use mosslight::app::{App, MenuCursor, Mode};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::{
    Action, AiState, BossPattern, Enemy, EnemyId, EnemyKind, Facing, ObjectRef, Pos, Swing, World,
};
use mosslight::render::{self, Theme};
use mosslight::save::{FileSaveIo, MemorySaveIo};
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    setup(&mut app);
    app.on_resize(w, h);
    let theme = Theme::new(cfg().theme, ColorMode::Always);
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
        let app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
        let theme = Theme::new(cfg().theme, ColorMode::Always);
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
            // Away from npc.keeper's tile (5, 5) — the scene paints objects before enemies, so an
            // overlapping enemy would silently hide the NPC glyph from every assertion below
            // (round-2 review, major).
            pos: Pos { x: 5, y: 10 },
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

    // Slime glyph at tile (5, 10): col = 5 + 1 + 2*5 = 16, row = 1 + 2 + 10 = 13.
    assert_eq!(cell(&buf, 16, 13), "o");
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    combat_scene_setup(&mut app);
    app.on_resize(60, 24);

    let texts: Vec<String> = [ThemeName::Mono, ThemeName::Gameboy, ThemeName::Ansi]
        .into_iter()
        .map(|theme_name| {
            let theme = Theme::new(theme_name, ColorMode::Always);
            let backend = TestBackend::new(60, 24);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| render::draw(f, &app, theme)).unwrap();
            buffer_text(term.backend().buffer())
        })
        .collect();

    assert_eq!(texts[0], texts[1], "mono vs gameboy glyphs differ");
    assert_eq!(texts[1], texts[2], "gameboy vs ansi glyphs differ");
}

/// Drops the hero into `room_id`, rebuilding its enemies exactly as a real door transition would.
fn enter_room(app: &mut App, room_id: &str) {
    let room_idx = app
        .state
        .world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    app.state.room = room_idx;
    app.state.enter_room();
    app.state.hero.pos = Pos { x: 1, y: 1 };
}

/// One scene per new object kind and its stateful variant (`Chest`/`ChestOpen`,
/// `Torch`/`TorchLit`, `Plate`/`PlatePressed`, `Npc`), each checked for identical rendered text
/// under every theme — the RISKS #10 mitigation this phase claims (only a glyph distinguishes an
/// object; colour must never be the only cue). `combat_scene_setup`'s room (`room.lighthouse`)
/// already exercises `Npc` above now that the slime no longer sits on `npc.keeper`'s tile.
type SceneSetup = (&'static str, fn(&mut App));

#[test]
fn object_kind_glyphs_render_identically_under_every_theme() {
    let setups: Vec<SceneSetup> = vec![
        // chest.forest_sword, unopened -> Kind::Chest.
        ("room.crossroads", |_app: &mut App| {}),
        // One of three plates pressed (Plate + PlatePressed together) and its chest opened
        // (Kind::ChestOpen).
        ("room.old_mill", |app: &mut App| {
            app.state.puzzle.pressed.insert(0);
            let room_idx = app.state.room;
            app.state.progress.opened_chests.insert(ObjectRef {
                room: room_idx,
                index: 0,
            });
        }),
        // torch.ridge, unlit -> Kind::Torch.
        ("room.north_ridge", |_app: &mut App| {}),
        // torch.shore, lit -> Kind::TorchLit.
        ("room.south_shore", |app: &mut App| {
            let room_idx = app.state.room;
            app.state.progress.lit_torches.insert(ObjectRef {
                room: room_idx,
                index: 0,
            });
        }),
    ];

    for (room_id, mutate) in setups {
        let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
        app.apply(&[Action::Confirm]);
        enter_room(&mut app, room_id);
        mutate(&mut app);
        app.on_resize(60, 24);

        let texts: Vec<String> = [ThemeName::Mono, ThemeName::Gameboy, ThemeName::Ansi]
            .into_iter()
            .map(|theme_name| {
                let theme = Theme::new(theme_name, ColorMode::Always);
                let backend = TestBackend::new(60, 24);
                let mut term = Terminal::new(backend).unwrap();
                term.draw(|f| render::draw(f, &app, theme)).unwrap();
                buffer_text(term.backend().buffer())
            })
            .collect();

        assert_eq!(
            texts[0], texts[1],
            "{room_id}: mono vs gameboy glyphs differ"
        );
        assert_eq!(
            texts[1], texts[2],
            "{room_id}: gameboy vs ansi glyphs differ"
        );
    }
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

#[test]
fn npc_glyph_and_room_hint_render_in_the_start_room_60x24() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]); // New Game -> Playing, in room.lighthouse
    });
    // npc.keeper sits at tile (5, 5): col = 5 + 1 + 2*5 = 16, row = 1 + 2 + 5 = 8.
    assert_eq!(cell(&buf, 16, 8), "N");

    let message_row: String = (0..60).map(|x| cell(&buf, x, 20)).collect();
    assert!(
        message_row.contains("Face the keeper"),
        "the start room's hint must show on entry: {message_row:?}"
    );
}

#[test]
fn map_overlay_hides_unvisited_rooms_and_marks_the_current_one() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        app.apply(&[Action::ToggleMap]);
    });
    let text = buffer_text(&buf);
    assert!(text.contains("Map"));
    assert!(text.contains("Lighthouse"), "current room's name: {text:?}");
    assert!(text.contains("you") && text.contains("chest") && text.contains("dungeon"));

    // The 3x3 grid sits inside a `centered_box(60x24 frame, 42, 11)` (x=9, y=6, bordered), so the
    // grid's own text starts at (10, 7); a cell for map_index (mx, my) lands at column 10+2*mx,
    // row 7+my. room.lighthouse's map_index (1, 2) -> (12, 9).
    assert_eq!(
        cell(&buf, 12, 9),
        "@",
        "the current room must be marked on the grid"
    );

    // room.crossroads (map_index (1, 1) -> (12, 8)) is unvisited: its cell must stay blank, not
    // merely unmentioned — the whole point of `draw_map`'s visited-gate is that an unvisited room
    // renders nothing, distinguishing "hidden" from "rendered but empty".
    assert_eq!(
        cell(&buf, 12, 8),
        " ",
        "an unvisited neighbour must render blank"
    );
}

#[test]
fn map_overlay_reveals_a_visited_non_current_room_with_its_unopened_chest() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        let crossroads = app
            .state
            .world
            .room_idx("room.crossroads")
            .expect("room.crossroads exists");
        // Simulate having already visited crossroads (it holds `chest.forest_sword`, unopened)
        // without moving the hero out of the current room, isolating the visited-marking rule
        // from door traversal.
        app.state.progress.visited.insert(crossroads);
        app.apply(&[Action::ToggleMap]);
    });

    // Same (10, 7)-anchored grid as above; room.crossroads' map_index (1, 1) -> (12, 8).
    assert_eq!(
        cell(&buf, 12, 8),
        "C",
        "a visited room holding an unopened chest must show 'C' once revealed"
    );
}

#[test]
fn map_overlay_does_not_mark_a_room_whose_only_chest_is_secret() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        // room.south_shore's only chest (`chest.shore_key`) is secret and behind an unlit torch —
        // the map must not point at it just because the room has been visited.
        let south_shore = app
            .state
            .world
            .room_idx("room.south_shore")
            .expect("room.south_shore exists");
        app.state.progress.visited.insert(south_shore);
        app.apply(&[Action::ToggleMap]);
    });

    // room.south_shore's map_index (2, 2) -> column 10+2*2=14, row 7+2=9.
    assert_eq!(
        cell(&buf, 14, 9),
        ".",
        "a room whose only chest is secret must not render 'C'"
    );
}

#[test]
fn inventory_overlay_shows_equipment_keys_and_hearts() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        app.apply(&[Action::ToggleInventory]);
    });
    let text = buffer_text(&buf);
    assert!(text.contains("Inventory"));
    assert!(text.contains("Sword: no"));
    assert!(text.contains("Lantern: no"));
    assert!(text.contains("Ember: no"));
    assert!(text.contains("Keys: 0"));
    assert!(text.contains("Hearts: 6/6"));
    assert!(text.contains("Flags: none"));
}

#[test]
fn dialogue_overlay_shows_the_current_node_and_the_continue_prompt() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        // npc.keeper sits at (5, 5); stand one tile south and face it.
        app.state.hero.pos = Pos { x: 5, y: 6 };
        app.state.hero.facing = Facing::North;
        let sim_actions = app.apply(&[Action::Interact]);
        app.tick(&sim_actions);
    });
    let text = buffer_text(&buf);
    assert!(text.contains("npc.keeper"));
    assert!(text.contains("Welcome, traveler"));
    assert!(
        text.contains("[E] continue"),
        "node 0 of 2 is not the last: {text:?}"
    );
}

#[test]
fn the_boss_arena_renders_boss_and_telegraph_glyphs_at_60x24() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        enter_room(app, "room.boss_arena");
        for e in app.state.enemies.iter_mut() {
            if e.kind == EnemyKind::Boss {
                e.ai = AiState::BossWindup {
                    phase: 1,
                    until: 1000,
                    pattern: BossPattern::Slam,
                };
            }
        }
    });
    // The boss spawns at (18, 8): col = 6 + 2*18 = 42, row = 3 + 8 = 11. Drawn after the
    // telegraph, so its own tile shows the boss glyph, not the danger cue.
    assert_eq!(cell(&buf, 42, 11), "W");
    // A `Slam` telegraphs the boss's tile and its four orthogonal neighbours; (19, 8) -> col 44.
    assert_eq!(cell(&buf, 44, 11), "!");
}

#[test]
fn the_boss_vulnerable_glyph_differs_from_its_default() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        enter_room(app, "room.boss_arena");
        for e in app.state.enemies.iter_mut() {
            if e.kind == EnemyKind::Boss {
                e.ai = AiState::BossVulnerable {
                    phase: 1,
                    until: 1000,
                };
            }
        }
    });
    assert_eq!(cell(&buf, 42, 11), "w");
}

#[test]
fn the_victory_overlay_renders_at_60x24() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]);
        app.mode = Mode::Victory;
    });
    let text = buffer_text(&buf);
    assert!(text.contains("relit"));
}

#[test]
fn confirm_new_game_overlay_renders_over_a_usable_slot() {
    let buf = render_at(60, 24, |app| {
        app.apply(&[Action::Confirm]); // New Game -> Playing
        app.apply(&[Action::Cancel]); // -> Paused
        app.apply(&[Action::Confirm]); // manual save -> slot becomes Usable
        app.mode = Mode::MainMenu;
        app.menu = MenuCursor::NewGame;
        app.apply(&[Action::Confirm]); // New Game over a Usable slot -> confirmation
    });
    let text = buffer_text(&buf);
    assert!(text.contains("Start a new game?"));
    assert!(text.contains("overwrite"));
}

#[test]
fn save_problem_overlay_offers_a_backup_restore_over_a_corrupt_slot() {
    let scratch = common::ScratchDir::new("render-save-problem");
    std::fs::write(mosslight::save::save_path(scratch.path()), b"not json").expect("write fixture");

    let mut app = App::new(
        &cfg(),
        world(),
        Box::new(FileSaveIo::new(scratch.path().to_path_buf())),
    );
    app.menu = MenuCursor::Continue;
    app.apply(&[Action::Confirm]); // Continue over a Corrupt slot -> Mode::SaveProblem
    app.on_resize(60, 24);

    let backend = TestBackend::new(60, 24);
    let mut term = Terminal::new(backend).unwrap();
    let theme = Theme::new(cfg().theme, ColorMode::Always);
    term.draw(|f| render::draw(f, &app, theme)).unwrap();
    let text = buffer_text(term.backend().buffer());
    assert!(text.contains("Save damaged"));
    assert!(text.contains("Restore backup"));
    assert!(text.contains("New game"));
}
