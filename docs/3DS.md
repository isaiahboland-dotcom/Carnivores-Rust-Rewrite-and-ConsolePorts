# 3DS port

> Status: M6 first test build. One area, the animals and weapons chosen
> when cooking, hunt mode, no sound or menus yet. See README §9 for status.

## What you need
- devkitPro with devkitARM, libctru and citro3d (`$DEVKITPRO=/opt/devkitpro`;
  `picasso`, `3dsxtool`, `smdhtool` come with it)
- Rust nightly with `rust-src` (`3ds-ver/rust-toolchain.toml` selects it;
  tested with nightly 2026-10-16) and `cargo-3ds` (`cargo install cargo-3ds`)
- `libclang` (bindgen builds the citro3d bindings)
- The rust3ds crates (`ctru-rs`, `ctru-sys`, `citro3d-sys`) come from git;
  `3ds-ver/Cargo.lock` pins the commits.

## Build and run
From `3ds-ver/`:
```
# 1. Cook your game into a pack (runs on the PC).
cargo run --release -p c2-cook -- --game ../source/c2-original --out c2rs.c2pk
#    options: --area N  --time day|dusk|night  --dinos Velociraptor,T-Rex
#             --weapons Pistol,Shotgun  --ambient  --target n3ds|o3ds
#             --mod DIR... or --recipe R.toml --mods DIR  (add-on mods)

# 2. Build the app.
export DEVKITPRO=/opt/devkitpro DEVKITARM=/opt/devkitpro/devkitARM
cargo 3ds build --release -p c2-3ds
#    -> target/armv6k-nintendo-3ds/release/c2-3ds.3dsx
```
3. On the SD card: `c2-3ds.3dsx` in `/3ds/`, the pack in `/3ds/c2rs/`
   (any name ending `.c2pk`; the first one alphabetically is used).
4. Start it from the Homebrew Launcher. To send just the app over Wi-Fi:
   press Y in the Homebrew Launcher, then `3dslink target/.../c2-3ds.3dsx`.

The pack is made from your own game files and stays on your SD card; never
share it (README §2).

## Controls
| | |
|---|---|
| Circle Pad | move / strafe |
| D-Pad | turn (left/right), look (up/down) |
| C-Stick (New 3DS), touch-drag | look |
| R | fire |
| A | show / hide weapon (and "yes" at the leave prompt) |
| X | next weapon |
| Y | binoculars |
| L | run on/off |
| B | jump / swim up (and "no" at the leave prompt) |
| ZL / ZR (New 3DS) | crouch / call |
| SELECT | pause |
| L + SELECT | screenshot to `/3ds/c2rs/shots/` |
| START | leave the hunt |

## Testing in an emulator
Azahar runs the `.3dsx` directly (`azahar c2-3ds.3dsx`); put the pack in its
SD folder (`~/.local/share/azahar-emu/sdmc/3ds/c2rs/`). An
`autotest.txt` in the same folder drives the app without input and saves
screenshots (format in `c2-3ds/src/shot.rs`):
```
wait 120
shot start
hold left 40
press a
wait 150
shot weapon
quit
```

## How it draws
- The cooker converts textures to the PICA200's tiled 16-bit formats
  (terrain RGB565 in one atlas, models RGBA5551 with 1-bit alpha), scaled to
  the target profile, so the 3DS uploads them as they are.
- Terrain is meshed on the 3DS in 16x16-cell chunks around the hunter (same
  triangulation and texture rotation as `terrain.wgsl`), built as they come
  into view and dropped two seconds after leaving it.
- One vertex shader (`c2-3ds/shaders/world.v.pica`) for everything; light is
  per vertex (light map for terrain, `CalcLights` for objects), fog is the
  GPU's fog table. The view model's depth is squeezed into the front 2% so
  the world never covers it.
- Animals and the view model are posed on the CPU each frame (as on PC) and
  written to per-character GPU buffers.

## Memory budget
- Old 3DS ~64 MB app RAM, New 3DS ~124 MB (extended mode) — less under the
  Homebrew Launcher, depending on how it was started.
- `c2-cook` prints an estimate per pack and refuses packs over the target's
  budget (n3ds 96 MiB, o3ds 44 MiB) unless `--force`.
- After loading, the app frees what only the PC needs (terrain and model
  textures now on the GPU, unused light maps, sounds).
- Vanilla AREA1, two hunted species + hunter, two weapons, n3ds: pack
  31 MiB, estimate 36 MiB. The bottom screen shows free heap and linear
  memory; real numbers from hardware go here.

## Not yet (M7)
Sound, the map and other bottom-screen UI, menus and saves, sky and sun,
animated objects, particles/blood, stereo 3D, gyro aim, the pack picker,
an Old 3DS performance pass.
