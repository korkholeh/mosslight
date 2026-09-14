# Controls

Every binding is a single key press — no key-release events, key chords, or extended keyboard
protocols are used anywhere, so this works identically over a plain SSH session.

| Action | Keys | Available in |
|---|---|---|
| Move (4-directional, no diagonals) | Arrow keys, or W/A/S/D | Playing |
| Attack (sword) | J or Space | Playing |
| Use lantern | K | Playing |
| Interact | E or Enter | Playing |
| Confirm (menu selection, retry after death, or advance/close a dialogue line) | E or Enter | Main menu, Paused, Confirm-quit, Help, Game over, Dialogue |
| Map (opens from Playing, M or Esc closes it) | M | Playing, Map |
| Inventory (opens from Playing, I or Esc closes it) | I | Playing, Inventory |
| Pause / back | Esc | Playing (pauses), Paused (resumes), Help/Map/Inventory (closes), Dialogue (closes early), Confirm-quit (cancels), Game over (returns to main menu) |
| Help | ? | Main menu, Playing, Paused |
| Quit (asks for confirmation) | Q or Ctrl+C | Main menu, Playing, Paused, Game over |
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
- Main menu items: **Continue** (present, but reports "no save yet" until phase 6 adds saving),
  **New Game**, **Help**, **Quit**.
- Falling to zero health opens a "You fell" screen. Confirm retries from the last room the hero
  entered, with the health they had on entering it; Esc abandons the run and returns to the main
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
