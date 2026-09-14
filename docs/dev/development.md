# Development

## Set up

Install the pinned toolchain (`rust-toolchain.toml` selects it automatically if you use `rustup`)
and fetch dependencies:

```sh
cargo fetch --locked
```

## Build and run

```sh
cargo build --release --locked
cargo run --release -- --save-dir /tmp/mosslight-scratch
```

Always pass `--save-dir` to a scratch directory when running by hand. The game autosaves at four
points during play (`docs/dev/loop-and-modes.md`), so a manual session started without it writes to
the real per-user save slot and can overwrite a run you cared about. The game refuses to start
outside a real TTY, so it cannot be driven from a pipe or redirected input — use
`ratatui::backend::TestBackend` in tests instead (see `docs/dev/testing.md`).

To put the binary on `PATH` instead of running it out of `target/release/`:

```sh
cargo install --path . --locked
```

## Format and lint

```sh
cargo fmt
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings
```

Clippy runs with `-D warnings`: no warning is ever left for later, in this phase or any other.

## Debugging

- `--debug-panic` (hidden, not in `--help`) panics deliberately once the terminal guard is up, so
  the restore-before-panic path can be exercised against a real terminal instead of only asserted
  by `tests/terminal_guard.rs`.
- Diagnostics (a failed write, discarded input overflow, a failed signal registration) are buffered
  in memory and printed to stderr only after the terminal has been restored — never during play, so
  they never corrupt the screen. If something seems to fail silently, check stderr after the
  process exits.
- `RUST_BACKTRACE=1 cargo run --release -- --debug-panic --save-dir /tmp/mosslight-scratch` gives a
  full backtrace for the deliberate panic path.
- `--debug-content PATH` (hidden, not in `--help`) validates `PATH` instead of the embedded
  `assets/world.ron`, so the content startup refusal (spec §7/§15) can be exercised against a real
  broken file. See `docs/dev/content.md` for the RON schema and the validator's checks.
- If a manual run leaves the terminal in a bad state (rare — see
  `docs/dev/troubleshooting.md`), run `reset`.

## Where things live

See the `Layout` table in `CLAUDE.md` for the module map, and
`.autodev/ARCHITECTURE.md` for the full design and its rationale. `docs/dev/loop-and-modes.md`
covers the main loop and mode machine in more detail than either.
