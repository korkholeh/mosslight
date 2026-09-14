# Mosslight

A single-player, top-down terminal adventure in Rust. The forest lighthouse has gone dark; the hero finds
the ancient ember in a flooded sanctuary and relights it. 9-room overworld, 6-room dungeon, three enemy
kinds, a two-phase boss, one save slot. Runs locally and over an interactive SSH session with a PTY.
Target playthrough: 30–45 minutes.

Specification: `docs/spec.md` (Ukrainian — it is the source of truth; all docs and code are English).

## Stack

Rust stable 1.98.1, pinned in `rust-toolchain.toml`; `Cargo.lock` is committed and every command passes
`--locked`. One package, `lib` + `bin` (`mosslight`). macOS and Linux only — Windows is out of scope.

Dependencies are fixed: `ratatui 0.30.2`, `clap 4.6`, `serde 1`, `ron 0.12`, `serde_json 1`,
`signal-hook 0.4`. Adding anything else needs a line in `.autodev/DECISIONS.md`.

## Commands

| | |
|---|---|
| install | `cargo fetch --locked` |
| test | `cargo test --locked` |
| lint | `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings` |
| format | `cargo fmt` |
| build | `cargo build --release --locked` |
| run | `cargo run --release` |
| e2e | none — see below |
| full gate | `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked` |

There is **no separate e2e layer**. The headless start-to-victory playthrough and the scene-placement
checks are ordinary Rust tests under `tests/`, run by `cargo test`. Do not add a PTY harness.

`scripts/terminal-restore-check.sh`, `scripts/manual-checks.sh` and `scripts/measure-cpu.sh` are
manual, host-run scripts (not part of `cargo test`) that drive the release binary through a real PTY
via `expect`: `stty -a` before/after for terminal restoration, the spec §13 manual-check list, and
%cpu/RSS sampling across a minute of play and a minute of pause, respectively. None is part of any
automated gate; see `docs/dev/testing.md` for how to run them and
`docs/dev/verification-report.md` for the last recorded results.

When running by hand, always pass `--save-dir /tmp/mosslight-scratch` so a manual run cannot clobber a real
save. The game refuses to start without a TTY, so it cannot be driven from a pipe — use `TestBackend`.

## Layout

```
src/main.rs     argv -> Config -> TerminalGuard -> loop -> exit code (the only terminal owner)
src/lib.rs      module tree; the surface tests use
src/config.rs   clap parser, Config, TTY / TERM / NO_COLOR probing
src/terminal.rs raw mode, alternate screen, RAII guard, panic hook, signal flags, size probe
src/input.rs    KeyEvent -> Action, per-mode maps, coalescing, event cap
src/app.rs      screen/mode machine, pause semantics, GameEvent -> effect, autosave triggers
src/render/     pure projection: theme.rs tiles.rs hud.rs scene.rs overlays.rs
src/game/       pure simulation: state.rs world.rs entities.rs combat.rs ai.rs puzzles.rs tuning.rs rng.rs
                balance.rs (run-length estimate, never called by update)
src/content/    RON schema, include_str! loader, validator
src/save.rs     versioned JSON, path resolution, atomic write, backup rotation
assets/world.ron  the entire world, embedded at compile time
tests/          integration tests
```

## Conventions

- `src/game/` is a pure function `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>`. It imports
  neither `std::io`, nor `std::time`, nor `ratatui`. Time arrives as an integer `Tick` (30 Hz).
- `src/render/` only reads state. Rendering never mutates the game.
- **Never declare `crossterm` in `Cargo.toml`** — use `ratatui::crossterm`. Two crossterm versions in the
  lock file is the failure spec §8 warns about.
- **Never handle key-release, chords, or extended keyboard protocols.** Single presses only (spec §5).
- Every combat/movement constant lives in `game/tuning.rs`, expressed in ticks.
- Content ids are stable namespaced strings (`room.lighthouse`, `chest.forest_sword`) and are part of the
  save-format compatibility surface.
- **Never `unwrap`** on the save path or the content path; both return explicit outcome enums.
- Themes may change colour only — glyphs are identical in every theme, so monochrome cannot regress.
- No log line may reach the screen: diagnostics buffer and flush to stderr after the guard drops.
- Do not clear the whole screen per frame and do not draw when nothing changed — `app::draw_due` is
  the one draw gate; every caller (`main.rs`, tests, the metrics harness) goes through it.
- Do not claim an environment was verified unless it was actually run.
- English everywhere: code, identifiers, comments, commit messages, docs.

Autodev docs: .autodev/ (ARCHITECTURE.md, RISKS.md, ROADMAP.md, PROGRESS.md, DECISIONS.md, phases/NN-*/PLAN.md)
Project docs: docs/dev/ (architecture, development, testing, loop-and-modes, troubleshooting,
verification-report, adr/), docs/user/ (cli, controls, ssh), CHANGELOG.md, HANDOFF.md
