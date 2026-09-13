# Profile: Mosslight — terminal game in Rust

Corrected against the real project on 2026-09-13. Nothing here is aspirational: every command below is the
command this project uses, and the layout is the layout `.autodev/ARCHITECTURE.md` specifies. Phase 1 is
responsible for making the skeleton match.

## Toolchain

- Pinned in `rust-toolchain.toml`: **Rust 1.98.1** stable, components `rustfmt` and `clippy`.
- Host used by this run: `aarch64-apple-darwin`, rustc 1.98.1.
- `Cargo.lock` is committed. **Every** build/test command passes `--locked`.
- Targets: macOS (aarch64, x86_64) and Linux (x86_64, aarch64). **Windows is out of scope** (spec §2).

## Dependencies (fixed — see ADR 0003, 0006, and RISKS #16)

| Crate | Version | Why |
|---|---|---|
| `ratatui` | `0.30.2` | Rendering (spec §8). Default features include the crossterm backend. |
| — `crossterm` | **not declared** | Used exclusively through `ratatui::crossterm` (crossterm 0.29 underneath). Declaring it directly is the §8 version-skew hazard. |
| `clap` | `4.6` (derive) | CLI (spec §12). |
| `serde` | `1` (derive) | Content and save serialization. |
| `ron` | `0.12` | `assets/world.ron`, hand-authored content. |
| `serde_json` | `1` | Save file; two-pass version probe. |
| `signal-hook` | `0.4` | SIGTERM/SIGHUP as atomic flags (spec §11); crossterm does not cover these. |

Adding anything else requires a line in `.autodev/DECISIONS.md` naming the concrete requirement it serves.

## Layout

```
Cargo.toml                 single package, lib + bin (bin name: mosslight)
rust-toolchain.toml        pinned 1.98.1
src/main.rs                argv -> Config -> TerminalGuard -> loop -> exit code (only terminal owner)
src/lib.rs                 module tree; the public surface tests use
src/config.rs              clap parser, Config, TTY / TERM / NO_COLOR probing
src/terminal.rs            raw mode, alternate screen, RAII guard, panic hook, signal flags, size probe
src/input.rs               KeyEvent -> Action, per-mode maps, coalescing, event cap
src/app.rs                 screen/mode state machine, pause semantics, GameEvent -> effect, autosave triggers
src/render/                pure: theme.rs, tiles.rs, hud.rs, scene.rs, overlays.rs
src/game/                  pure simulation: state.rs world.rs entities.rs combat.rs ai.rs puzzles.rs tuning.rs rng.rs
src/content/               RON schema types, include_str! loader, validator
src/save.rs                versioned JSON, path resolution, atomic write, backup rotation
assets/world.ron           the entire world, embedded at compile time
tests/                     integration tests (see Testing layers)
docs/dev/                  architecture summary, ADRs
docs/user/                 README-level user docs: controls, CLI, save locations, SSH
.github/workflows/ci.yml   ubuntu-latest + macos-latest matrix
```

The split that makes this stack testable: **`src/game/` is a pure function
`update(&mut GameState, &[Action], tick) -> Vec<GameEvent>`, and `src/render/` is a pure projection of state
into a frame.** `game/` imports neither `std::io`, nor `std::time`, nor `ratatui`. Every terminal call lives
in `main.rs` and `terminal.rs`.

## Commands

| Key | Command |
|---|---|
| install | `cargo fetch --locked` |
| build | `cargo build --release --locked` |
| run | `cargo run --release` |
| test | `cargo test --locked` |
| lint | `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings` |
| format | `cargo fmt` |
| gate (end of every phase) | `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked` |
| e2e up | `-` |
| e2e | `-` |
| e2e down | `-` |

**There is no separate e2e layer** (intake decision). The spec's end-to-end requirements — the headless
start-to-victory playthrough and the scene-placement checks — are ordinary Rust tests under `tests/`, run by
`cargo test`. Do not add a PTY harness (`expectrl`, `portable-pty`, `rexpect`).

## Testing layers

- **Simulation tests** (most cases live here) — build a `GameState`, feed a `&[Action]` sequence into
  `game::update`, assert the resulting state and the emitted `GameEvent`s. No terminal, no clock, no sleeps.
- **Render tests** — render into `ratatui::backend::TestBackend` at a fixed size (60×24 and 80×24) and assert
  the buffer: which characters appear, where, and what is highlighted. This is the deterministic equivalent
  of a screenshot. Assert on characters, not on escape sequences.
- **Content tests** — run `content::validate` over the real `assets/world.ron`, and over deliberately broken
  fixture worlds to prove the validator actually catches a missing key, a non-reciprocal door, and an
  out-of-bounds spawn.
- **Headless playthrough** — `tests/playthrough.rs` drives the game from the main menu to victory using only
  ordinary `Action`s. It must never set a story flag directly (spec §13).
- **Determinism** — the same seed plus the same action sequence must produce the same `state_hash()`, and the
  result must be identical at every `--fps` setting.
- **Save tests** — round-trip, corrupt file, incompatible (newer) version, and the rule that a death state
  never overwrites a usable save.

The §13 mandatory list is the test plan: movement and collision, room transitions, sword facing and hitbox,
cooldown and invulnerability, a door consuming a key exactly once, puzzles and their reset, death and
restore, both boss phases and the final victory, save round-trip, corrupt and incompatible saves, content
validity, and determinism.

## Running the game by hand

```sh
cargo run --release
cargo run --release -- --ascii --color never --fps 10
cargo run --release -- --theme mono --save-dir /tmp/mosslight-scratch
```

Always pass `--save-dir` to a scratch directory when testing so a manual run cannot clobber a real save.
The game refuses to start without a TTY, so it cannot be driven from a pipe — use `TestBackend` tests instead.

## Documentation

- `docs/user/` — the key map (every binding, every mode), the full CLI surface, where the save file lives on
  each OS, what happens when it is missing or corrupt, and how to run over SSH and under tmux.
- `docs/dev/` — the architecture summary, `docs/dev/adr/` for the ADRs, the loop and mode state machine, and
  how to add a room, an enemy kind, or a screen.
- English, per the intake, even though `docs/spec.md` is Ukrainian.

## Pitfalls specific to this project

- **Never declare `crossterm` in `Cargo.toml`.** Use `ratatui::crossterm`. Two crossterm versions in the lock
  file is the failure spec §8 warns about.
- **Never read a clock inside `src/game/`.** Time arrives as a `Tick`. This is what the determinism test
  protects.
- **Never handle key-release or modifier-chord input.** Spec §5 requires the game to be fully playable with
  single key presses over SSH; there is no key-release path in the codebase and there must not be one.
- **Never let a log line reach the screen.** Diagnostics are buffered and flushed to stderr after the terminal
  guard drops.
- **Never `unwrap` on the save path or the content path.** Both are fallible inputs with explicit outcome
  enums.
- **Do not clear the whole screen each frame**, and do not draw when nothing changed. ratatui already diffs
  the buffer; the dirty flag is what makes a paused game emit ~0 bytes.
- **Colour is never the only distinction.** The theme layer changes colour only; glyphs are identical in
  every theme, so monochrome legibility cannot regress.
- **Do not claim an unverified environment.** Linux, a real PTY SSH session, the ~150 ms RTT check, Intel
  macOS, and aarch64 Linux are not reachable from this host. CI covers the Linux build and tests; everything
  else is labelled expected-but-unverified in the report.
