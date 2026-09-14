# Running over SSH and under tmux/screen

Mosslight is a normal terminal program: SSH (or tmux/screen) only carries keystrokes in and
rendered frames out. Nothing about the game is SSH-aware.

## Why `-t`

```sh
ssh -t user@host 'mosslight --ascii --fps 10'
```

The game refuses to start unless both stdin and stdout are a TTY (spec §3) — a plain `ssh
user@host mosslight` pipes stdout through SSH's own machinery without allocating one, and the
game exits immediately with a one-line refusal and code 2 rather than doing anything with a
non-interactive stream. `-t` forces SSH to allocate a real pseudo-terminal on the remote host, the
same kind a local terminal emulator gives a locally-run program.

## `--fps` on a slow link

`--fps` caps how often a frame is drawn; the simulation itself always runs at a fixed 30 Hz
regardless of this flag (spec §9), so lowering it changes bandwidth and perceived latency, never
gameplay speed or determinism. On a slow or high-latency link, `--fps 10` halves the frame rate
`--fps 20`'s default sends, at no cost to how responsive an individual keypress feels — input is
still read every loop iteration, only drawing is throttled.

## What the output volume measurement actually was

`tests/metrics.rs` drives the real `ratatui`/`crossterm` write path (the same
`CrosstermBackend` `main.rs` uses) over a byte-counting writer, at a fixed 80x24 viewport and 20
fps, with the hero moving continuously through a room with live enemies for 60 simulated seconds.
Measured on the host this phase was implemented on (`aarch64-apple-darwin`, rustc 1.98.1): **about
31 KB/min during continuous, uninterrupted play** — well under the 200 KB/min budget
`.autodev/ARCHITECTURE.md` assumes. A paused game or a static main menu write **zero bytes** after
their first frame (`tests/metrics.rs::a_paused_game_with_no_input_writes_nothing_after_the_first_frame`,
`::a_static_main_menu_writes_nothing_after_the_first_frame`) — the dirty-flag draw gate
(`app::draw_due`) means idle time costs nothing on the wire, which matters more than the active-play
number for a long SSH session where the player is reading a dialogue line or thinking about a
puzzle. See `docs/dev/verification-report.md` for the exact numbers and how they were produced.

This is a measurement of bytes written locally to the pty, not of what actually crosses a real
network — see "What was not checked" below.

## tmux and screen

Starting the game inside `tmux`/`screen` on the remote host lets play survive a dropped SSH
session — detach (`Ctrl+b d` in tmux), reconnect later, and `tmux attach` picks the game back up
mid-run:

- **`TERM` inside tmux** is usually `screen` or `tmux-256color`, not whatever the outer terminal
  reports. Mosslight only reads `TERM` to refuse `dumb` and to decide default colour-on behaviour
  (`--color auto`, see `docs/user/cli.md`) — both `screen` and `tmux-256color` are treated as an
  ordinary non-`dumb` terminal, so nothing about being inside tmux changes what the game does.
- **Resize on reattach**: tmux delivers a resize event to the game when the attaching client's
  window size differs from the size the session was created at (the same as any other resize —
  spec §9's live-resize handling applies unchanged, including dropping into `Mode::Paused` on
  recovery from a too-small size rather than resuming straight into play).
- **The `gameboy` theme's 256-colour caveat**: `gameboy` paints four indexed colours from the
  256-colour palette (`docs/user/cli.md`). Whether tmux forwards 256-colour codes depends on its
  own `terminal-overrides`/`default-terminal` configuration and the *outer* terminal's own
  capability — a tmux session started under a terminal that does not advertise 256-colour support
  can render `gameboy` with wrong or degraded colours even though the game itself never detects
  this (there is no reliable, portable way to probe 256-colour support from inside a pty). If
  `gameboy` looks wrong under tmux, pass `--theme ansi` (the 16-colour fallback, safe under any
  terminal tmux is likely configured for) or `--theme mono` (no colour dependency at all — glyphs
  are identical in every theme, spec §4, so monochrome never loses information).

## What was not checked

This host cannot exercise a real network SSH session, so the following are **not verified**, only
expected from the design (see `docs/dev/verification-report.md`'s *Not verified* section for the
full list and why):

- **The ~150 ms round-trip-time playability check** was not run. `--fps 10`'s bandwidth argument
  above is a measurement of local write volume, not a claim about how the game feels over a real
  high-latency link.
- A genuine SSH session over an actual network (as opposed to a local pty that stands in for one).
- Linux as an SSH server (CI builds and tests it; nobody has played over it interactively here).
