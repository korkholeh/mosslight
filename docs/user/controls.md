# Controls

Every binding is a single key press — no key-release events, key chords, or extended keyboard
protocols are used anywhere, so this works identically over a plain SSH session.

| Action | Keys | Available in |
|---|---|---|
| Move (4-directional, no diagonals) | Arrow keys, or W/A/S/D | Playing |
| Attack (sword) | J or Space | Playing |
| Use lantern | K | Playing |
| Interact | E or Enter | Playing |
| Confirm (menu selection) | E or Enter | Main menu, Paused, Confirm-quit, Help |
| Map | M | Playing |
| Inventory | I | Playing |
| Pause / back | Esc | Playing (pauses), Paused (resumes), Help (closes), Confirm-quit (cancels) |
| Help | ? | Main menu, Playing, Paused |
| Quit (asks for confirmation) | Q or Ctrl+C | Main menu, Playing, Paused |
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
