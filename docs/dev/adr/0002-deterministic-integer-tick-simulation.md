# 0002. The simulation is deterministic: integer tick time, integer tile positions, and a seeded in-crate PRNG

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§8 requires that the simulation not depend on the terminal, that randomness be driven by a controlled seed,
and that combat rules and AI never read the system clock — time must be passed in. §13 requires a test
proving that the same seed and the same action sequence produce the same simulation, and a headless
playthrough from start to victory driven only through ordinary game actions.

§6 defines object positions as integer tile coordinates and health in whole half-heart units. §6 also lists
tuning values in milliseconds (step 120 ms, sword cooldown 300 ms, active swing 100 ms, invulnerability
800 ms, telegraph ≥ 600 ms) and calls them starting values to tune, and requires all combat values to be
kept in one place.

## Decision

- **Time is a `Tick` (u64) counter** advanced exactly once per 1/30 s simulation step. Every duration in the
  game is stored as a tick count in `game::tuning`, with the millisecond value in a comment:
  step 4 ticks (133 ms), sword cooldown 9 (300 ms), active swing 3 (100 ms), invulnerability 24 (800 ms),
  telegraph 18 (600 ms). The public entry point is
  `update(&mut GameState, &[Action], tick: Tick) -> Vec<GameEvent>`; the interface in §8 takes a `Duration`,
  and passing a tick is the same contract with the ambiguity removed.
- **Positions are integer tile coordinates.** No floating-point arithmetic anywhere in `game`.
- **Randomness is a SplitMix64 generator implemented in-crate**, seeded from `--seed` (default: a fixed
  constant so that an unseeded run is also reproducible), stored inside `GameState`, and advanced only
  inside `update`.
- **`game` contains no I/O, no clock access, and no terminal type.** This is enforceable by inspection
  because `game` does not import `std::time`, `std::io`, or `ratatui`.
- A `GameState::state_hash()` gives the determinism test something cheap to compare.

## Alternatives considered

- **`Duration`/float `dt` inside the simulation** — rejected: floating-point accumulation differs across
  platforms and optimization levels, and it couples simulation speed to frame timing.
- **The `rand` crate** — rejected: it makes no cross-version guarantee that a given seed yields a given
  sequence, so a dependency bump could silently break the determinism test. SplitMix64 is ~15 lines, is a
  published algorithm, and is frozen by being ours.
- **Sub-tile positions for smooth movement** — rejected: §6 specifies tile coordinates, and a terminal at 20
  fps with two columns per tile has no sub-tile resolution to show anyway.
- **Storing durations in milliseconds and converting at use sites** — rejected: rounding at each use site
  is exactly how two timers drift apart. Rounding happens once, in `tuning`.

## Consequences

- The 120 ms step becomes 133 ms because 120 ms is not a multiple of the 33.3 ms tick. §6 calls these
  starting values for tuning, so this is within the spec; it is recorded in `DECISIONS.md`.
- Replaying an action log reproduces a bug exactly, which is the debugging tool this project would otherwise
  lack entirely (there is no logging on screen and no telemetry).
- Every test in §13's list — hitbox, cooldown, invulnerability, key consumption, puzzle reset, death and
  restore, both boss phases — can be written as "feed actions, assert state", with no terminal and no sleeps.
- The cost is that all tuning must be expressed on a 33 ms grid. For a game with a 120 ms step and a 600 ms
  telegraph this is far below perceptible.
- We would revisit only if the game needed smooth sub-tile motion, which the fixed 24×16 character grid
  cannot display.
