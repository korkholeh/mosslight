# Changelog

All notable user-visible changes to Mosslight are recorded here.

## Unreleased

- Combat: the hero can swing a sword (J or Space) that hits exactly the tile they face, on a
  cooldown, for a short active window. Slimes, bats and a guardian now populate six of the nine
  overworld rooms (the starting lighthouse and the crossroads stay clear), each with its own
  patrol/chase/dart/telegraph-and-dash behaviour. Contact with a live enemy costs half a heart and
  knocks the hero back, with a brief invulnerability window afterward. A guardian only takes sword
  damage while recovering from its dash — hits at any other time are deflected.
- Falling to zero health opens a "You fell" screen; Enter retries from the last room the hero
  entered (health included), Esc returns to the main menu.
- The overworld is now real content: nine hand-authored rooms in a 3x3 grid
  (`assets/world.ron`), connected by 24 two-way doors with no dead ends. New Game drops the hero
  into the lighthouse; walking through a door moves the hero into the neighboring room at its
  entry spawn.
- A content validator (`content::validate`) checks the world at startup, in tests, and in CI:
  legal geometry, unique ids, door/spawn targets, spawn placement, door-tile correspondence,
  two-way door reciprocity, and a reachability search that proves every room is reachable and no
  key is locked behind the door it opens. A world that fails to validate is refused before the
  terminal is touched.
- First playable build: a main menu (Continue / New Game / Help / Quit), and New Game drops the
  hero into one 24x16 room that can be walked around with the arrow keys or WASD.
- Esc pauses and resumes; Q asks for confirmation before quitting; Ctrl+C does the same as Q. Below
  the 60x24 minimum terminal size, Q/Ctrl+C quit immediately instead, since a confirmation dialog
  cannot be shown at that size.
- The terminal is always left in a usable state on exit — normal quit, an error, or a crash — so a
  broken shell after playing should not happen; `reset` is documented as the fallback if it ever
  does.
- New CLI flags: `--ascii`/`--unicode`, `--color auto|always|never`, `--theme gameboy|ansi|mono`,
  `--fps 10|20|30`, `--save-dir PATH`, `--seed NUMBER`, `--help`, `--version`. `--unicode` and
  `--theme ansi` are accepted but not yet visually distinct from the ASCII/gameboy defaults.
- Continue is present in the main menu but reports "No save yet" — the save subsystem has not
  landed yet.
