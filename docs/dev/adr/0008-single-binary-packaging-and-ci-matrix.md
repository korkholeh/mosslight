# 0008. Ship one self-contained binary built by Cargo, with a Linux+macOS CI matrix as the portability check

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§3 requires the game to run on macOS (Apple Silicon and Intel) and Linux (x86_64 and aarch64), locally and
over SSH, and to need no internet, no graphics server, no browser, no special fonts, and no external asset
files after installation. The launch examples are `cargo run --release`, a bare `mosslight` on `PATH`, and
`ssh -t user@host 'mosslight --ascii --fps 10'` — so copying one file to a server has to be enough.

§15 requires a pinned toolchain, a committed `Cargo.lock`, a README covering build, controls and SSH use, and
CI for Linux and macOS.

The intake constrains what can honestly be claimed: this run executes on an Apple Silicon Mac. Linux, a real
SSH session with a PTY, and the ~150 ms RTT check are not reachable from here, and §13 forbids claiming a
platform was verified when it was not.

## Decision

- **One Cargo package**, `lib` + `bin`, producing a single executable named `mosslight`. Content is embedded
  (ADR 0005), so `cargo build --release` is the whole pipeline — there is no asset step, no installer, and no
  runtime file layout to get wrong.
- **Toolchain pinned** in `rust-toolchain.toml` to `1.98.1` with `rustfmt` and `clippy`; `Cargo.lock`
  committed; every gate and CI command passes `--locked`.
- **Install paths:** `cargo install --path .`, or `cargo build --release` and copy
  `target/release/mosslight` anywhere on `PATH`. No `sudo`, no system directories, no post-install step.
- **CI (GitHub Actions)** runs on `ubuntu-latest` and `macos-latest`:
  `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`,
  `cargo build --release --locked`. CI is the project's Linux evidence — it is the reason the Linux claim in
  the verification report is "builds and tests pass in CI" rather than "verified by hand".
- **Cross-architecture policy:** Apple Silicon and x86_64 Linux are built and tested by CI directly. Intel
  macOS and aarch64 Linux are **not** exercised; the code contains no architecture-specific constructs (no
  SIMD, no `usize` width assumptions, integer-only simulation per ADR 0002), so they are expected to work and
  are labelled *expected, unverified* in the report rather than claimed.
- **No release archives or package-manager recipes in v1.** Distribution is source plus `cargo`; a tagged
  release with per-triple archives is a later, additive decision.

## Alternatives considered

- **musl static builds for a maximally portable Linux binary** — rejected for v1: it adds a cross-toolchain
  and a target to the matrix for a portability problem nobody has reported yet. The glibc floor is documented
  instead, and this is the cheapest decision to revisit, since nothing in the code depends on it.
- **`cross` / QEMU emulation to test aarch64 Linux and Intel macOS in CI** — rejected: it roughly doubles CI
  time and complexity to raise confidence on architectures where integer-only, dependency-light Rust has no
  plausible divergence. Reconsider if a real bug appears.
- **A Homebrew formula or a `.deb`** — rejected for v1 by scope; both need a published release artifact to
  point at, which does not exist yet.
- **A workspace with separate `core` / `tui` crates** — rejected: the `lib` + `bin` split inside one package
  already gives the testability boundary (the library never touches a terminal), and a workspace would add
  manifest overhead with no additional guarantee.
- **Claiming Linux and SSH as verified on the strength of CI and reasoning** — rejected by §13 and by the
  intake. CI proves build and tests on Linux; it does not prove a PTY session behaves, and the report must
  say exactly that.

## Consequences

- Deployment is `scp` plus `ssh -t`, which is what §3's example implies and what a user actually wants.
- Nothing to migrate on upgrade except the save file, whose compatibility rules live in ADR 0006.
- The set of honestly verifiable claims is fixed in advance, so the verification report cannot drift into
  overstatement late in the run.
- Publishing to crates.io, release archives, and packaging are deliberately left open and cost little to add
  later, since none of them changes the code.
