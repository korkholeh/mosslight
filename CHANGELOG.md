# Changelog

All notable user-visible changes to Mosslight are recorded here.

## 0.1.0 — 2026-09-14

First release. The game is completable start to finish and saves your progress.

### Breaking changes

- **`--unicode` has been withdrawn.** It is now a usage error (exit code 2), not a flag that
  silently does nothing. This only affects you if you ran a pre-release build that accepted it.
  Every Unicode glyph that would improve on ASCII risks silently rendering two columns wide under
  some locales and terminal settings, which would shear the fixed tile grid. ASCII is the one glyph
  table; `--ascii` remains as an explicit affirmation of it. Reasoning in `docs/user/cli.md`.

### The game

- **A complete adventure, start to victory.** New Game drops you in the lighthouse; the only exit
  leads to a room holding the sword and no enemies. From there: nine hand-authored overworld rooms
  in a 3x3 grid, an NPC keeper, a forester and a marsh warden, chests, and the old mill hiding the
  lantern behind a three-plate floor puzzle. A lantern-locked door at the marsh opens a six-room
  dungeon — a safe vestibule, a flooded hall, a block-puzzle chamber, a torch-sequence vault, a
  guardian-patrolled walk, and a boss arena. Beat the two-phase boss, take the ancient ember, carry
  it back to the lighthouse beacon and press `E` to relight it and end the run.
- **Combat.** Swing the sword (`J` or Space) at exactly the tile you face, on a cooldown, for a
  short active window. Slimes, bats and guardians populate six of the nine overworld rooms, each
  with its own patrol/chase/dart/telegraph-and-dash behaviour. Contact with a live enemy costs half
  a heart and knocks you back, with brief invulnerability afterward. A guardian only takes sword
  damage while recovering from its dash.
- **The boss** telegraphs every attack at least 600 ms before it lands and only takes damage in the
  brief window right after — hitting it any other time deflects harmlessly. It never deals contact
  damage; every hit it can land is one you can see coming.
- **Three kinds of puzzle**: stepping on pressure plates (any order), pushing a block onto a plate
  (walk into it), and lighting a room's torches in a hinted order. Lighting one out of turn resets
  that sequence at no cost — there is no dead end anywhere in the game.
- **Secrets and equipment.** Four secret caches sit off the road to the ember, each behind its own
  torch-lit passage: two heart containers, a small key and a lore note. None of them is needed to
  finish the game. Two small keys found in the dungeon open the two locked doors; the return trip
  through either is always free.
- **Two full-screen views**: `M` opens a map of the overworld (visited rooms only, marking your
  room, an unopened chest, a dungeon entrance, or an NPC) and `I` opens your inventory. Both pause
  the game, as does talking to an NPC — dialogue advances one line at a time on `E`, never on a
  timer.
- **Death is not a reset.** Falling to zero health opens a "You fell" screen; Enter retries from the
  last save, Esc returns to the main menu.
- A first playthrough estimates at roughly 33 minutes, inside the intended 30–45 minute band.

### Saving

- **Progress survives quitting.** One save slot, written automatically when you enter a new room,
  pick up something important, solve a puzzle, or defeat the boss — plus a manual save from the
  pause screen (`Enter`/`E`), refused during combat.
- **Dying can never cost you a save.** A save is never written at zero health, so falling never
  overwrites the last point the game actually saved.
- **A damaged or newer save is never silently destroyed.** The main menu's `Continue` reports the
  slot's real state (no save yet / present / damaged / from a newer version). A damaged save offers
  restoring the retained backup or starting fresh, and nothing is written until you choose. A save
  from a newer build is never overwritten by an older one. Starting `New Game` over an existing run
  asks for confirmation first. Where the file lives on each platform: `docs/user/cli.md`.

### Presentation

- **Three colour themes**: `gameboy` (four indexed greens), `ansi` (a 16-colour fallback for
  terminals without indexed colour), and `mono` (full monochrome). Glyphs are identical in every
  theme, so monochrome play never loses information a colour theme carries.
- `--color never` and a non-empty `NO_COLOR` stop colour reaching the terminal entirely.
- Below the 60x24 minimum terminal size the game pauses and shows the size it needs versus the size
  you have; on recovery it resumes paused rather than dropping you straight back into play.

### Terminal safety

- **The terminal is always left usable on exit** — normal quit, an error, a crash, SIGTERM or
  SIGHUP. `reset` is documented as the fallback for the cases nothing can catch (`kill -9`).
- No log line ever reaches the screen; diagnostics buffer and print to stderr after the terminal is
  restored.
- Every binding is a single key press. No key-release events, chords, or extended keyboard
  protocols anywhere, so the game plays identically over a plain SSH session.

### CLI

- `--ascii`, `--color auto|always|never`, `--theme gameboy|ansi|mono`, `--fps 10|20|30`,
  `--save-dir PATH`, `--seed NUMBER`, `--help`, `--version`.
- `--fps` caps drawing only — the simulation always runs at a fixed 30 Hz, so frame rate never
  changes game speed. An unsupported value is a usage error, not a silent clamp.
- The game refuses to start without an interactive terminal (stdin and stdout must both be a TTY,
  and `TERM` must not be unset or `dumb`), exiting with one line of explanation before touching raw
  mode.
- Esc pauses and resumes; `Q` asks for confirmation before quitting; Ctrl+C does the same. Below the
  minimum terminal size both quit immediately, since a confirmation dialog cannot be shown there.

### Under the hood

- The world is a single hand-authored RON file embedded at compile time and machine-validated at
  startup, in tests, and in CI: legal geometry, unique ids, door/spawn targets, two-way door
  reciprocity, object placement, and a reveal-aware reachability search proving every room and lock
  is reachable, no key is locked behind the door it opens, and no secret chest sits on the road to
  the ember. A world that fails to validate is refused before the terminal is touched — an
  unfinishable game cannot ship.
- One self-contained 2.3 MB binary with no runtime dependencies, no network, and no external asset
  files.
