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
| C-Stick (New 3DS), drag on the touch screen | look |
| R | fire |
| A | show / hide weapon (and "yes" at the leave prompt) |
| X | next weapon |
| Y | binoculars |
| L | run on/off |
| B | jump / swim up (and "no" at the leave prompt) |
| ZL / ZR (New 3DS) | crouch / call |
| SELECT | pause (shows fps and memory) |
| L + SELECT | screenshot to `/3ds/c2rs/shots/` |
| START | leave the hunt |

### Bottom screen (touch)
```
+--------+----------------------+--------+
| weapon |                      |  call  |
| weapon |   map: you, your     |  call  |
|  ...   |   view range, the    |  ...   |
|        |   animals in range   |        |
|        +----------------------+        |
|        |  status / message    |        |
|        | [Binocs]   [Pause]   |        |
+--------+----------------------+--------+
```
- Tap a weapon to draw it. Its card shows the picture, name and shots
  left.
- Tap a call to select it; tap the selected one to call (ZR also calls).
- Tap the map to switch between close-up (follows you) and the whole area.
  Huntable animals in view range are bright green, others pale.
- A drag anywhere looks around instead (for the Old 3DS, which has no
  C-Stick).

## Testing in an emulator
Azahar runs the `.3dsx` directly (`azahar c2-3ds.3dsx`); put the pack in its
SD folder (`~/.local/share/azahar-emu/sdmc/3ds/c2rs/`). Azahar gives a
`.3dsx` 96 MiB even in Old 3DS mode, so it won't show Old 3DS memory
problems; the cooker's measured peak is the check for those. An
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
  per vertex (light map for terrain, `CalcLights` for objects). Fog is worked
  out in the shader from distance to the eye and blended in by a second
  texture-combiner stage (the GPU's fog table works on depth, which is too
  uneven here). The view model's depth is squeezed into the front 2% so the
  world never covers it.
- Animals and the view model are posed on the CPU each frame (as on PC) and
  written to per-character GPU buffers.

## Memory budget
- An Old 3DS app gets 64 MiB, a New 3DS app 124 MiB. The app keeps 16 MiB as
  linear (GPU) memory and the rest is heap: about 47 MiB on Old, 107 on New.
  Start the Homebrew Launcher through a game or Download Play (title
  takeover). Started as an applet, it has much less memory.
- `c2-cook` loads each pack the way the 3DS does, measures the peak heap,
  and refuses packs over the target's budget (heap: o3ds 40 MiB, n3ds 96;
  GPU: 12 MiB) unless `--force`.
- Packs carry the `.RSC`/`.CAR` without textures or sounds. The app reads
  one light map from the `.MAP` and frees the rest of what only the PC uses
  after loading.
- Vanilla AREA1, two hunted species + hunter, two weapons: pack 17 MiB (o3ds)
  / 18 MiB (n3ds), loading peak 23 MiB. In the emulator the heap then sits
  at 15 MiB; GPU memory used is 4–7 MiB.
- **Hardware (owner, 2026-10-18)**: Old 3DS / Old 2DS, o3ds pack: loads,
  ~58 fps, heap 15.1 MiB in use while hunting.
- Out of memory shows a message (press START) instead of crashing.
- The loading screen and the bottom screen show heap used/total and free
  linear memory.

## Not yet (M7)
Sound, menus and saves, scope cross-hairs, stereo 3D, gyro aim, the pack
picker, an Old 3DS performance pass.
