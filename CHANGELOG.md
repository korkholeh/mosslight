# Changelog

All notable user-visible changes to Mosslight are recorded here.

## Unreleased

- The game is now completable end to end. Behind the marsh's lantern-locked door, a six-room
  dungeon (a safe vestibule, a flooded hall, a block-puzzle chamber, a torch-sequence vault, a
  guardian-patrolled walk, and a boss arena) leads to a two-phase telegraphed boss. Defeating it
  grants the ancient ember; bringing it back to the lighthouse's beacon and facing it (E) ends the
  run. Two small keys, found in the dungeon, open the two locked doors along the way — the return
  trip through either is always free.
- Two new puzzle kinds join the mill's step-plates: pushing a block (walk into it) onto a plate,
  and lighting a room's torches in a hinted order (lighting one out of turn resets the sequence at
  no cost — there is no dead end). Leaving a room always resets its unsolved puzzles to their
  authored start; a solved puzzle and what it revealed stay that way.
- The boss telegraphs every attack at least 600ms before it lands, and only takes sword damage in
  the brief window right after — hitting it any other time deflects the blow harmlessly. It never
  deals contact damage; every hit it can land is a telegraphed strike you can see coming.
- The overworld is fully authored: an NPC keeper starts the lighthouse cul-de-sac, whose single
  exit leads to a sword-holding room with no enemies. Chests, NPCs and torches occupy their tile
  and are opened/talked to/lit by facing them (E to interact, K for the lantern); a chest opens
  once. Talking to the lighthouse keeper unlocks a conversation with a marsh warden later on. The
  old mill hides its lantern behind a three-plate floor puzzle (step on all three, in any order);
  three secret chests (a heart container, a small key, and a lore note), each behind its own
  torch-lit passage, sit off the road to the ember and grant no progress. A lantern-locked door at
  the marsh leads to the dungeon's entrance. Attacking without the sword now does nothing but say
  so.
- Two new full-screen views: `M` opens a map of the overworld (visited rooms only, marking the
  current one, a chest, a dungeon entrance, or an NPC); `I` opens an inventory listing the sword,
  lantern, ember, keys, hearts, and any story flags learned. Both pause the game like Esc's pause
  screen does. Talking to an NPC opens a dialogue window that advances one line at a time on E,
  never on a timer.
- Combat: the hero can swing a sword (J or Space) that hits exactly the tile they face, on a
  cooldown, for a short active window. Slimes, bats and a guardian now populate six of the nine
  overworld rooms (the starting lighthouse and the crossroads stay clear), each with its own
  patrol/chase/dart/telegraph-and-dash behaviour. Contact with a live enemy costs half a heart and
  knocks the hero back, with a brief invulnerability window afterward. A guardian only takes sword
  damage while recovering from its dash — hits at any other time are deflected.
- Falling to zero health opens a "You fell" screen; Enter retries from the last room the hero
  entered (health included), Esc returns to the main menu.
- The overworld is now real content: nine hand-authored rooms in a 3x3 grid
  (`assets/world.ron`), connected by two-way doors, plus a one-room dungeon vestibule the lantern
  opens. New Game drops the hero into the lighthouse, whose only exit leads to the sword; walking
  through a door moves the hero into the neighboring room at its entry spawn.
- A content validator (`content::validate`) checks the world at startup, in tests, and in CI:
  legal geometry, unique ids, door/spawn targets, spawn placement, door-tile correspondence,
  two-way door reciprocity, object placement, and a reveal-aware reachability search that proves
  every room and every lock (including the lantern and story-flag locks) is reachable/satisfiable,
  no key is locked behind the door it opens, and no secret chest sits on the road to the ember or
  holds a reward the main route needs. A world that fails to validate is refused before the
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
