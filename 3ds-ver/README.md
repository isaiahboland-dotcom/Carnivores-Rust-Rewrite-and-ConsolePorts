# 3ds-ver — Nintendo 3DS port

A separate Cargo workspace (nightly, see `rust-toolchain.toml`) reusing the
shared crates in `../pc-ver/crates/` by path:

- `c2-cook/` — runs on your PC: turns your game (plus add-on mods or a
  recipe) into one `.c2pk` pack for the 3DS.
- `c2-3ds/` — the 3DS app (`cargo 3ds build --release -p c2-3ds`).

Build steps, controls, testing in an emulator and memory notes:
`../docs/3DS.md`.

## `.cia` builds (optional)
The 3DS builder can also make a `.cia` (installed with FBI or a similar title manager, so the game shows
on the home menu). It needs two tools that are **not part of this project**; download them yourself:
- **makerom** — from Project_CTR: <https://github.com/3DSGuy/Project_CTR/releases>
- **bannertool** — <https://github.com/Steveice10/bannertool/releases>

Put both beside the launcher, or in the tools folder set in the launcher's Settings (working from the
source tree, its top folder works too; the files are ignored by git). On Linux and macOS make them
executable (`chmod +x makerom bannertool`). The `.cia` is built from `c2-3ds.elf`, which comes with
`c2-3ds.3dsx`. Each install name gets its own title ID and keeps using `sdmc:/3ds/<name>/` for its packs
and saves. The home-menu icon (48x48 PNG), banner (256x128 PNG) and banner sound (WAV/OGG, up to 3 s)
are your own files, picked in the 3DS tab; until you pick them, plain placeholders are used.
