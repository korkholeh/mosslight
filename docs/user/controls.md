# Controls

Every binding is a single key press — no key-release events, key chords, or extended keyboard
protocols are used anywhere, so this works identically over a plain SSH session.

| Action | Keys | Available in |
|---|---|---|
| Move (4-directional, no diagonals) | Arrow keys, or W/A/S/D | Playing |
| Move the menu cursor (Up/Down only; Left/Right do nothing here) | Arrow keys, or W/S | Main menu |
| Attack (sword) | J or Space | Playing |
| Use lantern | K | Playing |
| Interact | E or Enter | Playing |
| Confirm (menu selection, save the game, retry after death, advance/close a dialogue line, or leave the victory screen) | E or Enter | Main menu, Paused, Confirm-quit, Help, Game over, Dialogue, Victory |
| Map (opens from Playing, M or Esc closes it) | M | Playing, Map |
| Inventory (opens from Playing, I or Esc closes it) | I | Playing, Inventory |
| Pause / back | Esc | Playing (pauses), Paused (resumes), Help/Map/Inventory (closes), Dialogue (closes early), Confirm-quit (cancels), Game over (returns to main menu), Victory (returns to main menu) |
| Help | ? | Main menu, Playing, Paused |
| Quit (asks for confirmation) | Q or Ctrl+C | Main menu, Playing, Paused, Game over, Victory |
| Quit immediately (terminal too small to show a confirmation) | Q or Ctrl+C | Below the 60x24 minimum size |

Notes:

- An attempted move always turns the hero to face that direction, even when the way is blocked by
  a wall, water, a bush, or the edge of the room.
- The hero takes one step per press once the step cooldown has elapsed; holding a key down relies
  on your terminal's own key-repeat, and a burst of queued movement (e.g. after your terminal
  catches up from a stall) is coalesced to at most one step — it never fast-forwards through a
  backlog of old moves.
- Closing a menu, dialogue, or overlay drops any game action that was queued behind the key that
  closed it, so a movement key pressed while a menu was open cannot fire the instant it closes.
- **Save**: from the pause screen (Esc, then Enter/E), the game writes its one save slot — refused
  with a message if a live enemy is nearby, a sword swing is in flight, or the hero is still
  invulnerable from a recent hit. The game also saves on its own at four points: entering a new
  room, picking up the sword/lantern/a small key/a heart container/the ember, solving a puzzle, and
  defeating the boss. A save is never written at zero health, so dying can never overwrite the
  last point the game actually saved. See `docs/user/cli.md` for what happens when the save file is
  missing, damaged, or from a newer version.
- Main menu items: **Continue** (its label reflects the save slot: no save yet, present, damaged,
  or from a newer version), **New Game**, **Help**, **Quit**. Choosing **New Game** over an
  existing save asks for confirmation first, since it overwrites that progress.
- Falling to zero health opens a "You fell" screen. Confirm retries from the last save (not
  necessarily the last room — see "Save" above); Esc abandons the run and returns to the main
  menu.
- Facing an NPC and pressing Interact opens a dialogue window if the NPC has something to say;
  Confirm advances one line at a time (never on a timer), and closes the window past the last
  line. Facing a chest opens it once; facing it again reports it is empty. Facing an unlit torch
  and pressing Use Lantern lights it (if the hero has one) — this can reveal a hidden passage
  elsewhere in the room.
- The Map shows every room visited so far in its 3x3 grid position: `@` for the room the hero is
  in, `C` for a room with an unopened chest, `>` for a room holding a door to the dungeon, `N` for
  a room with an NPC, `.` otherwise; a room never visited stays blank. The Inventory lists the
  sword/lantern/ember, the key count, current hearts, and any story flags learned so far.
- Opening the Map, Inventory, or a dialogue pauses the simulation exactly like Esc does — nothing
  moves and no damage lands while one is open.
- Moving into a movable block (`O`) pushes it one tile in that direction instead of blocking the
  step, as long as the tile ahead of the block is clear floor — the same step cooldown applies, so
  pushing costs no more than an ordinary step. A block cannot be pushed through a door, onto
  another block, an enemy, or any other solid object.
- Facing an unlit torch that is part of a lighting puzzle and pressing Use Lantern lights it only
  if it is next in the puzzle's order; lighting one out of order snuffs every torch in that puzzle
  back out (no penalty beyond retrying). Solving a puzzle — every plate held down, every block on
  its plate, or every torch lit in order — is permanent; the room's transient state (pressed
  plates, block positions, an in-progress torch order) resets if the hero leaves and returns
  without finishing it.
- Locked doors marked `SmallKey` in the dungeon cost one key to open; returning through the same
  doorway afterward is always free.
- Once the hero carries the ancient ember, facing the lighthouse's beacon (`*`) and pressing
  Interact relights it and ends the run. Without the ember, the beacon only reports that it is
  cold.

Running over SSH or under tmux/screen: see `docs/user/ssh.md`.
