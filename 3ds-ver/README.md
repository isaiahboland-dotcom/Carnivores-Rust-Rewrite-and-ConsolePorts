# 3ds-ver — Nintendo 3DS port

A separate Cargo workspace (nightly, see `rust-toolchain.toml`) reusing the
shared crates in `../pc-ver/crates/` by path:

- `c2-cook/` — runs on your PC: turns your game (plus add-on mods or a
  recipe) into one `.c2pk` pack for the 3DS.
- `c2-3ds/` — the 3DS app (`cargo 3ds build --release -p c2-3ds`).

Build steps, controls, testing in an emulator and memory notes:
`../docs/3DS.md`.
