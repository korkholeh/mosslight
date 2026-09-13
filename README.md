# Mosslight

A single-player, top-down terminal adventure. The forest lighthouse has gone dark; find the
ancient ember in the flooded sanctuary and relight it. Runs locally and over an interactive SSH
session with a PTY. macOS and Linux only.

## Build

```sh
cargo build --release --locked
```

Toolchain is pinned in `rust-toolchain.toml` (Rust 1.98.1); `Cargo.lock` is committed and every
command should pass `--locked`.

## Run

```sh
cargo run --release
mosslight --ascii --color never --fps 10
mosslight --theme mono --save-dir /tmp/mosslight-scratch
```

The game refuses to start without an interactive terminal (stdin and stdout must both be a TTY,
and `TERM` must not be unset or `dumb`) and exits with a one-line explanation before touching raw
mode. When testing by hand, pass `--save-dir` to a scratch directory so a manual run cannot
overwrite a real save.

Full CLI reference: `docs/user/cli.md`. Full key map: `docs/user/controls.md`.

## Controls

| Action | Keys |
|---|---|
| Move | Arrows or WASD |
| Attack | J or Space |
| Lantern | K |
| Interact / confirm | E or Enter |
| Map | M |
| Inventory | I |
| Pause / back | Esc |
| Help | ? |
| Quit (with confirmation) | Q |

No key-release events, chords, or extended keyboard protocols are ever used, so the game is fully
playable over a plain SSH session — single key presses only.

## Running over SSH

```sh
ssh -t user@host 'mosslight --ascii --fps 10'
```

The `-t` flag forces a PTY, which the game requires. Everything runs on the remote host; SSH only
carries keystrokes and terminal output. If the connection should survive a dropped SSH session,
start the game inside `tmux` or `screen` on the remote host first.

## If the terminal looks broken after a crash

The game restores raw mode, the alternate screen, the cursor and text styles on every exit path,
including a panic. If something still looks wrong (e.g. after a killed process), run:

```sh
reset
```

## Development

```sh
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked
```

This is the gate every phase must pass; CI (`.github/workflows/ci.yml`) runs it on
`ubuntu-latest` and `macos-latest`. See `docs/dev/development.md` for setup/build/debug,
`docs/dev/testing.md` for the test layers, `docs/dev/loop-and-modes.md` for the loop and
mode-machine architecture, `docs/dev/troubleshooting.md` for symptom-to-fix, and
`.autodev/ARCHITECTURE.md` for the full module layout and design rationale.

`scripts/terminal-restore-check.sh` drives a real PTY (via `expect`) through the normal-exit,
`--debug-panic`, and live-resize paths and prints `stty -a` before/after each, for manually
confirming terminal restoration on a given host; see `docs/dev/troubleshooting.md` if it hangs or
misbehaves in a sandboxed environment.

See `CHANGELOG.md` for user-visible changes.

## Known limitations (phase 1)

This phase delivers the terminal skeleton, the mode machine, and one hard-coded playable room —
not the full game. See `.autodev/ROADMAP.md` for what each later phase adds, and
`.autodev/PROGRESS.md` for current status.
