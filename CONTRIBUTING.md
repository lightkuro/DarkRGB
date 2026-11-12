# Contributing to darkRGB

Thank you for looking. This is a young project with one maintainer, so the most useful things right now are
reports from real hardware and real music, and small, focused changes.

## The most useful reports

- **Does it drive your LEDs?** Run `darkrgb-cli ddp-test` followed by your WLED device's IP address and open
  an issue with what it prints, the WLED version, the LED count, and whether the device is on WiFi or
  Ethernet.
- **Does it hear your computer?** Run `darkrgb-cli audio-test` while something is playing and report what it
  prints, with your operating system.
- **Does it follow your music?** Say which starter scene, what kind of music, and what looked wrong: late,
  too jumpy, missing the drops, changing scene at odd moments.

## Building and testing

The README has the build steps. Before sending a change, these should all pass:

```
cd ui && pnpm install && pnpm build && cd ..
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo test` does not need LED hardware or a sound card: the end-to-end tests drive a simulated WLED device
(`crates/sim`).

## How the code is laid out

`docs/architecture.md` explains the design. The short version:

- `crates/core` has no input or output. Effects, the renderer and the show model live there, and it should
  stay that way.
- Every change to a show goes through a command in `crates/engine/src/commands.rs`, which is where input is
  checked. A randomised test applies tens of thousands of awkward commands and checks the show after each
  one; a new command should be added to it.
- The UI (`ui/`) builds an effect's controls from what the engine says about it, so a new effect needs no UI
  code.

## Style

- Code and comments in plain English. A comment says why, not what.
- User-facing text: short sentences, no jargon, no exclamation marks. Say what a thing does.
- Simple first. A new setting needs a default that works untouched, and belongs in the full layout unless
  most people need it.
- Anything that can flash the whole picture fast must respect the flash limit. See "The flash limit" in
  `docs/architecture.md`.

## Licence

darkRGB is licensed under GPL-3.0-or-later. By contributing you agree that your contribution is licensed the
same way.
