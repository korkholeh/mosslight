# Intake — Mosslight

- **Date:** 2026-09-13
- **Spec:** docs/spec.md
- **Profile:** rust-tui

## Answers

- **Target and delivery:** Answered by the spec (§3, §8) — a single-binary terminal game in Rust stable,
  built and run on macOS (Apple Silicon + Intel) and Linux (x86_64 + aarch64), including over an
  interactive SSH session with a PTY, and under tmux/screen. No question asked.
- **Stack inside that target:** Answered by the spec (§8) — ratatui for rendering, crossterm for input and
  terminal control, serde + RON or JSON for content and saves, clap for the CLI. No game engine, no async
  runtime, no full ECS. Single-threaded. No question asked.
- **Data and persistence:** Answered by the spec (§10) — one save slot on the local filesystem, written via
  a temp file in the same directory plus an atomic rename, keeping the previous valid copy. Paths:
  `$XDG_DATA_HOME/mosslight` (fallback `~/.local/share/mosslight`) on Linux,
  `~/Library/Application Support/mosslight` on macOS, overridable with `--save-dir`. The save carries a
  format version; a corrupt file must not panic or be silently overwritten, and an unknown newer format
  version must not be overwritten. No question asked.
- **Identity and access:** Answered by the spec (§1, §2) — single-player, offline, no sign-in, no roles,
  no multi-user data. No question asked.
- **Docs language:** **English.** README, architecture description, and the verification report are written
  in English even though the spec is Ukrainian. Code, identifiers, comments, and commit messages are also
  English.
- **Phase gate command:** **Full gate**, the spec's own §13 readiness list:
  `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked`.
  Every phase must leave all three green.
- **Web access:** **On.** Sessions may look up current ratatui/crossterm releases and their compatibility
  matrix, and ratatui API documentation. This targets the specific hazard the spec names in §8 —
  mismatched Ratatui and Crossterm versions producing incompatible event types.
- **Run ceiling:** **`--max-hours 8`.** The run stops after roughly eight hours rather than working into the
  next usage window.
- **GitHub:** Push to the existing `korkholeh/mosslight`, on an `autodev/…` branch, with a draft PR carrying
  the live PROGRESS.md. Commits authored as Oleh Korkh <78351+korkholeh@users.noreply.github.com>.

## Defaults the developer accepted

- English docs, because it is the standard for a Rust crate and matches the code, commit messages, and CI
  output that surround them.
- The full fmt + clippy + test gate, because it is exactly the bar the spec sets in §13, and because
  deferring lint work leaves an unattended run to discover a large lint pile in the middle of the night
  rather than one warning at a time.
- Web access on, because §8 calls out Ratatui/Crossterm version skew as a concrete failure mode and a wrong
  version pin chosen offline costs a whole phase to discover and undo.
- An eight-hour ceiling, because each of the six stages in §14 is defined to end on a working build, so a
  stop at the boundary lands cleanly rather than mid-feature.

## Left open on purpose

- **End-to-end layer is off (`--e2e off`).** The spec's end-to-end requirement (§13) is a headless
  start-to-victory playthrough driven through ordinary game actions, plus scene-placement checks against a
  test terminal backend. Both belong in `cargo test`, and neither needs a service started or torn down.
  The run should implement them as ordinary Rust tests under `tests/`, not as a separate e2e surface.
- **Unicode rendering mode.** The spec (§4) makes ASCII the base mode and Unicode an optional enhancement
  restricted to characters of verified width. Whether `--unicode` ships as a complete second tile set in
  this run is left to the roadmap; ASCII and the monochrome mode are the ones that must be fully legible.
  If Unicode is cut, say so in the verification report rather than shipping a half-populated tile set.
- **Manual verification claims.** Spec §13 forbids claiming a platform or scenario was verified when it was
  not actually run. Unattended sessions on this Mac can run macOS, local-terminal, tmux, 60×24 and 80×24,
  resize, monochrome, held-key, save/quit/continue, and clean-exit-after-panic checks. Linux, real SSH with
  a PTY, and the ~150 ms RTT check are not reachable from here — mark them explicitly unverified in the
  report instead of asserting them, and let CI cover the Linux build.
- **Content scale versus the clock.** If the eight hours run out, §2's mandatory content table
  (9 overworld rooms, 6 dungeon rooms, 3 NPCs, 3 enemy types, a two-phase boss, 3+ secrets) is the part that
  must not be silently trimmed. Record any shortfall in HANDOFF.md as remaining work; do not leave stubs on
  the main route (§14).
