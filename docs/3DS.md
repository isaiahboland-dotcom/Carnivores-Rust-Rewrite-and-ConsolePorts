# 3DS port

> Status: M7 in progress. The whole game in one pack, a touch menu, hunts
> with sound, saves. See README §9 for status.

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
# 1. Cook your game into a pack (runs on the PC): every area, animal and
#    weapon (~140 MB); the 3DS loads only what you pick.
cargo run --release -p c2-cook -- --game ../source/c2-original --target o3ds --out c2rs.c2pk
#    options: --target n3ds|o3ds  --areas 1,2 (fewer areas, smaller pack)
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

## The menu
The title screen shows the original's main menu art with your hunter's
record; the bottom screen has **Play**, **Survival**, **Hunter**,
**Trophies** (the trophy room), **Settings** and **Quit**.

**Survival** is free and shows only the gun screen: the button where Tranq /
Observe usually are switches Survival / Hardcore (1–3 mods' animals join each
wave) / Custom (unlimited ammunition, the T-Rex from wave 15). The area, time
of day and up to three dangerous animals are picked at random; waves of them
come, bigger each time (at most 6), always after you, with health and
ammunition back between waves. **Classic** (with the mobile game's Survive
mode imported from your Carnivores 2 APK; launcher: Classic or Both) starts
at once: its waves on Mount Ravan, with the shotgun. No trophies; your kills
and best wave go in the record.

**Settings** (kept in `/3ds/c2rs/settings.txt`):
- **Dinosaur run speed**, 50–100%: every species' running speed times the
  same factor, so their differences stay; 100% is the original.
- **Ambient animals** on/off: the small animals that score nothing
  (Moschops, Gallimimus, the flyers, Dimetrodon, Brachiosaurus). On: at
  most 4 small ones placed near you (the original places 5 + density/80;
  more than a handful in view caused lag on Old 3DS). Off: none, and
  they aren't loaded at all (less memory). Play walks through three touch steps (A = Next, B = Back, START =
back to the title); the top screen sums up the picks, the points left and
whether the hunt fits in memory.
1. **Area**: a hexagon grid of the areas; the time of day as sunset (dawn),
   sun (day) or moon (night).
2. **Animals**: a hexagon grid of the huntable species; pick one or more.
3. **Weapons and gear**: weapons, camouflage, radar, cover scent, double
   ammo, **extra ammo**; Tranquilizer and Observer toggles; then **HUNT**.
   Extra ammo (3DS only) cycles x1 (10 points, 1 refill, kills score 5%
   less), x2 (50, 2 refills, -5%), x3 (150, 3 refills, no penalty), off.
   In the hunt, the **Ammo** button under the map (red number = refills
   left) refills every carried weapon.

Each hexagon shows its price; picks can't cost more than your score, as in
the original. Leaving a hunt saves your score and trophies to
`/3ds/c2rs/trophy00.sav` (the original's format; a new hunter starts with
`_RES.TXT`'s `prices.start` points). Dying returns to the menu without
saving.

## Controls (hunting)
| | |
|---|---|
| Circle Pad | move / strafe |
| X / B | look up / down |
| Y / A | turn left / right |
| C-Stick (New 3DS), drag on the touch screen | look |
| R | fire |
| D-Pad up | run on/off |
| D-Pad down | next weapon |
| D-Pad left | binoculars |
| D-Pad right | show / hide weapon |
| L | jump / swim up |
| ZL / ZR (New 3DS) | crouch (hold; with stealth crouch on: press to toggle) / call |
| crouch button (Settings) | ZL (New 3DS default); Hold L, tap L jumps (Old 3DS default); D-Pad Up cycles walk, run, crouch |
| turning the console (stealth crouch, while crouched) | aim (gyroscope) |
| A / B at the leave prompt | yes / no |
| SELECT | pause (shows fps, memory, and ms per part of a frame) |
| L + SELECT | screenshot to `/3ds/c2rs/shots/` |
| START | leave the hunt (A = yes saves and returns to the menu) |

### Bottom screen (touch)
```
+--------+----------------------+--------+
| weapon |                      |  call  |
| weapon |   map: you, your     |  call  |
|  ...   |   view range, the    |  ...   |
|        |   animals in range   |        |
|        +----------------------+        |
|        |  status / message    |        |
|        | [Ammo][Binocs][Pause]|        |
+--------+----------------------+--------+
```
- Tap a weapon to draw it. Its card shows the picture, name and shots
  left.
- Tap a call to select it; tap the selected one to call (ZR also calls).
- Ammo: use an extra ammo refill (the red number is how many are left).
- Tap the map to switch between close-up (follows you) and the whole area.
  Huntable animals in view range are bright green, others pale. With the
  radar, the hunted species show everywhere on the map.
- A kill shows a small box in the top screen's bottom right: the species
  and the points it earned.
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
- One vertex shader (`c2-3ds/shaders/world.v.pica`) for the world; light is
  per vertex (light map for terrain, `CalcLights` for objects). Fog is worked
  out in the shader from distance to the eye and blended in by the last
  texture-combiner stage (the GPU's fog table works on depth, which is too
  uneven here). Night vision (`BrightenTexture`: `(G/8, G, G/8)`) is the
  first combiner stage, so packs carry one set of textures for all times of
  day. The view model's depth is squeezed into the front 2% so the world
  never covers it.
- A second shader (`flat.v.pica`) draws coloured shapes: particles, blood,
  the map and menu shapes. Text is the font8x8 font, cooked into the pack.
- Animals and the view model are posed on the CPU each frame (as on PC) and
  written to per-character GPU buffers.

## Memory budget
- An Old 3DS app gets 64 MiB, a New 3DS app 124 MiB. The app keeps 24 MiB as
  linear memory (what the GPU and the DSP read: textures, vertex buffers,
  sounds) and the rest is heap: about 39 MiB on Old, 99 on New.
  Start the Homebrew Launcher through a game or Download Play (title
  takeover). Started as an applet, it has much less memory.
- `c2-cook` measures the loading peak of each area with every animal and
  weapon (heap budget o3ds 34 MiB, n3ds 96) and estimates linear memory for
  a typical hunt (budget 22 MiB). It writes what each area, animal and
  weapon takes (`cost.*` in `C2RS/INFO.TXT`); the menu adds up the picks
  and won't start a hunt that doesn't fit.
- Packs carry the `.RSC`/`.CAR` without textures or sounds. The app reads
  one light map from the `.MAP` and frees the rest of what only the PC uses
  after loading.
- Vanilla, o3ds: pack 136 MiB. Loading peak 23 MiB in every area. Linear
  for a hunt with 2 animals and 2 weapons: 12 to 17.5 MiB, mostly sounds
  (the area's ambience, and every ambient animal's).
- **Hardware (owner, 2026-10-18)**: Old 3DS / Old 2DS, single-area o3ds
  pack: loads, ~58 fps, heap 15.1 MiB in use while hunting.
- Out of memory shows a message (press START) instead of crashing.

## Sound
Sounds play on the DSP, which needs the console's DSP firmware dump at
`sdmc:/3ds/dspfirm.cdc`. Most homebrew setups have it; if not, run the
**DSP1** homebrew app once. Without it the game runs silently and says so
on the bottom screen. In Azahar, any file at that path works (it emulates
the DSP).

The cooker writes every sound as 16-bit mono at 22050 Hz: the animals',
weapons' and ship's `.CAR` sounds, each area's random and ambient sounds
(`C2RS/SND/AREAn/`), steps, screams and calls. A hunt loads its area's,
its loaded animals' and carried weapons' sounds, and the targets' calls. Which
event plays what, and at what left/right level, is `c2_core::sound`, shared
with the PC. 16 voices as the original, plus two channels to crossfade the
ambience and one for the ship's engine.

## Not yet (M7)
Hunter name entry and more than one hunter, the options screen
(aggressiveness, density, ...), scope cross-hairs, stereo 3D, gyro aim, the
pack picker, an Old 3DS performance pass.
