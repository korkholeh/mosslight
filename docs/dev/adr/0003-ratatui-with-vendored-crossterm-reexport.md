# 0003. Depend on ratatui only and use its `ratatui::crossterm` re-export, pinned as a verified pair

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§8 names ratatui for rendering and crossterm for input and terminal control, and explicitly warns to align
the two versions to avoid incompatible event types. The intake enabled web access specifically to resolve
this hazard and asks for the verified pair to be recorded.

This is a real, recurring failure in the ratatui ecosystem: ratatui re-exports crossterm types in its
backend API, so a manifest that also depends on `crossterm` directly can resolve two semver-incompatible
crossterm versions. The result is a `KeyEvent` from one that the other will not accept — a type error that
reads as nonsense until you see the duplicated crate in the lock file.

Checked on 2026-09-13 against crates.io: ratatui `0.30.2` (released 2026-06-19) depends on
`ratatui-crossterm 0.1.x`, which uses crossterm `0.29.0` and re-exports it; ratatui enables the `crossterm`
backend by default and re-exports it as `ratatui::crossterm`. crossterm `0.29.0` is the current stable
release. ratatui 0.30 also exposes `crossterm_0_xx` feature flags for selecting among crossterm majors.

## Decision

- Pin **`ratatui = "0.30.2"`** with default features (which include the crossterm backend).
- **Do not list `crossterm` in `Cargo.toml` at all.** Every crossterm item is used through
  `ratatui::crossterm::…` — events, raw mode, the alternate screen, cursor control, queue/execute.
- Commit `Cargo.lock` and pass `--locked` in every gate and CI command, so the resolved pair is the pair that
  was tested.
- A test asserts the invariant mechanically: the lock file contains exactly one `crossterm` entry. A future
  dependency that drags in a second crossterm fails the gate instead of failing to compile three files later.
- `signal-hook` is the one exception where a terminal-adjacent crate is added directly, because crossterm
  does not deliver SIGTERM/SIGHUP (see ADR 0007); it does not expose crossterm types, so it cannot cause skew.

## Alternatives considered

- **`ratatui` + a direct `crossterm` dependency pinned to the same version** — rejected: it works until
  either crate's next release, and then it is exactly the §8 hazard. Two declarations of one truth is the
  bug, not the version numbers.
- **A different backend (termion, termwiz)** — rejected: §8 names crossterm, and termion is Unix-only in a
  way that buys nothing here since Windows is already out of scope.
- **An older, more settled ratatui (0.29)** — rejected: 0.30 has been stable for nine months with two patch
  releases, and starting a new project on a superseded major means a migration later for no benefit now.
- **Writing to the terminal directly without ratatui** — rejected by §8, and ratatui's buffer diffing is
  precisely the "differential screen update, no full clear" behaviour §9 demands.

## Consequences

- Version skew between ratatui and crossterm is structurally impossible: there is one version requirement,
  owned by ratatui.
- Upgrading ratatui may change the crossterm version underneath us. The event-handling code is confined to
  `input.rs` and `terminal.rs`, so the blast radius is two files, and the single-crossterm lock-file test
  catches the mistake early.
- We give up the ability to use a crossterm feature ratatui does not enable. `event-stream` is the notable
  one, and ADR 0001 rules it out anyway.
- The re-export path makes the dependency visible in the source (`ratatui::crossterm::event::KeyCode`), which
  is a small readability cost paid for a large class of avoided breakage.
