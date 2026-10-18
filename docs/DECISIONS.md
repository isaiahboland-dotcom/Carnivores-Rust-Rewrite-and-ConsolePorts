# Decisions and deviations

Newest first. Each entry: what, why, and what it affects.

## 2026-10-18 — M6: cooker and first 3DS build
- **Layout**: `3ds-ver/` is one nightly workspace holding `c2-cook` (host
  tool) and `c2-3ds`, as the README tree says; `default-members` is the cooker.
  rust3ds crates come from git (not on crates.io), pinned by `Cargo.lock`.
- **Pack = original files + GPU textures.** The 3DS runs the same `c2-core`,
  which needs the `.MAP`/`.RSC`/`.CAR` data anyway, so the pack carries them
  unchanged plus textures pre-tiled for the PICA200 (RGB565 terrain atlas,
  RGBA5551 models, scaled per profile). After loading, the 3DS frees what
  only the PC uses (≈ heap 16.6 MiB in use for the default pack).
- **Partial species sets**: the pack keeps the full `_RES.TXT` (character and
  weapon indices drive placement and AI) but only the chosen `.CAR`s.
  `Herd::spawn` now skips a species with no animations instead of placing an
  invisible animal; with every `.CAR` present nothing changes.
- **Gameplay view range on 3DS** is 42 cells (`ctViewR` at the original's
  lowest view option); only drawing distance depends on the profile.
- **Fog in the vertex shader**: the GPU's fog table is indexed by depth-buffer
  values, which at ~10 000 units of view put the whole fog ramp into one
  table entry. The shader computes distance fog like the PC, and a second
  combiner stage mixes in the fog colour.
- **Texture orientation** (found in the emulator): with our tiling (bottom
  image row first), the hardware's `t` equals the image's `v` directly.
- `unsafe` is allowed in `c2-3ds` only (citro3d is a C API).
- The test-script hook (`autotest.txt`) and L+SELECT screenshots exist so
  builds can be checked in an emulator and bug reports can carry pictures.

## 2026-10-18 — Zip mods
Owner approved the `zip` crate (9.0.5, deflate only via pure-Rust zlib-rs;
no bzip2/lzma/zstd/AES, which old Carnivores mods don't use). It sits behind
`c2-vfs`'s `zip` feature, so the 3DS build doesn't carry it. zip 9 needs Rust
1.88, so the workspace MSRV went 1.85 → 1.88. A zip's `HUNTDAT` may be at the
top or one folder down, like a mod folder; a zip's `mod.toml` is read from
beside `HUNTDAT`. Files from a zip are copied (not linked) into a built pack.
`recipe verify` looks at `.zip` files in `--mods` as well as folders, and a
zip's name without `.zip` counts as its folder name.

## 2026-10-18 — M5: recipes

**A recipe pins exactly the files it uses** (`sha256:` per file) and the
build takes only those from each mod, so the same recipe produces the same
game for everyone, whatever else their download contains. `recipe new`
pins a mod's changed files plus its `_RES.TXT` when that changes anything.

**Finding mods:** `folder` hint, then a folder whose name slugs to the id,
then the folder with the most hash matches. Download folder names vary;
hashes don't.

**Base check only warns.** `recipes/bases/c2-vanilla.toml` holds the SHA-256
of the owner's 192 vanilla `HUNTDAT` files (hashes, no content). Other
releases of the game will differ in places; mods still apply.

**Schema additions** (README §6 updated): optional `folder` per mod, and
`[[resolve]] entry = "section:name"` for `_RES.TXT` conflicts (the README
only had file paths). Patches are unified diffs, matched without `\r` and
allowed to drift from their header line; they must live inside the recipe
folder. Our own small applier, not a crate (≈100 lines, tested).

**Dependency:** `sha2` (RustCrypto) for SHA-256.

## 2026-10-18 — M4: mod system

**Combined games are materialised, not read through the VFS at run time.**
`c2-mods` layers vanilla and the mods in a `c2-vfs` VFS (provenance, merge
report), then writes the result as a folder of symlinks (hard links or copies
where symlinks fail) plus generated files (the merged `_RES.TXT`) in the data
dir. The engine keeps loading from a plain game folder, the folder is easy to
inspect, and the 3DS cooker (M6) can start from the same VFS. A folder is only
ever replaced if it carries our `.c2rs-pack` marker.

**Only `HUNTDAT/` counts as mod data**, minus logs/backups and map-editor
side files. Files byte-identical to vanilla are dropped, which turns
"full copy of the game" mods into their real changes.

**`_RES.TXT` is merged by entry**: blocks matched by `name`; a renamed block
in a slot vanilla no longer has by name replaces that slot (slots matter to
saves and the menu); otherwise appended. `prices` by key + occurrence.
Conflicting changes: later mod wins, reported. Identical changes from two mods
are not conflicts.

**Manifest extensions:** `[res]` gains `adds_weapons` / `modifies_weapons`
(README §5 updated). `[files]` empty means detect.

**Dependencies:** `serde` + `toml` for `mod.toml` (the format the spec
names). Zip support is not in yet: it needs the `zip` crate (with its
deflate dependencies), which per §8 waits for the owner's OK.

## 2026-10-18 — M3: menus and the trophy room

**The menus are reconstructed.** The released source is only the game; the
original menu was a separate front end that started it with command-line
arguments (`prj= din= wep= dtm= -double -observ`, see `Game.cpp`). We rebuild
the screens from the shipped art: each picture, its `*_ON.TGA` lit version and
its `*_MAP.RAW` hit map (`docs/FORMATS.md`). Player registration, the waiver,
deleting a player, main menu, statistics, hunt setup, options, credits and the
quit prompt all follow the pictures. List layout, fonts (the 8x8 font, scaled)
and wording of the statistics are our own.

**Prices are a budget, not a payment.** `_RES.TXT` `prices` gives `start`
(a new hunter's score) and a price per area, dinosaur, weapon and accessory.
The account shows the score and what is left after the current choices; a
choice that would go below zero is refused. Nothing is deducted: hunting a
5-point Parasaurolophus with a 10-point licence could never pay off otherwise,
and the shipped saves (scores 104/114) fit this reading. Observer mode is free
and needs no targets or weapons. No rank locks.

**Menu mapping.** Dawn/Day/Night = `TimeOfDay::Dusk/Day/Night` (the code
calls the first time "dusk", the menu "dawn"); dinosaur *n* = AI `9 + n`;
equipment = Camouflage, Radar, Cover Scent, Double Ammo (`EQUIP1-4`, the
`.NFO` texts). Camouflage, radar, scent and tranquilizer are kept in the save
as the game does (`CamoMode` etc.); other choices start fresh each visit.
Info texts are re-flowed to the panel width.

**Players.** Each registered hunter is a `trophy0N.sav` (N = 0..9) in our
data dir; on first run every `trophy0N.sav` in the game folder is copied
there (the game folder is never written). Key bindings, mouse sensitivity,
reverse mouse, units, aggressiveness, density, sensitivity and view range come
from the save and can be changed in Options; `ctViewR = 42 + OptViewR/8*2`.
Options the port doesn't use (resolution, textures, fog, shadows,
renderer, sound device) are kept in the file but not shown.

**Trophy room** is the original's `TrophyMode` (`TROPHY.MAP`): trophies stand
on the landing ring in their first animation frame, the hunter collides with
them, a plaque shows when 2.5 cells in front of one, `R` removes it, `Esc`
saves and leaves at once; no weapons, calls, map or AI. **Deviation:** the
original's exit path adds `Last` to `Total` again when leaving the room
(stats inflate on every visit); we leave the stats untouched. After a kill
in a hunt the plaque (`TROPHY_G.TGA`) shows for 20 s, as in the original.

**Menu logic lives in `c2-pc` for now** (README §4 puts the menus' state
machine in `c2-core`). It is tied to the PC art and mouse; the parts the 3DS
needs (budget, choices, player list) move to `c2-core` in M6/M7, when a
second front end shows what is actually shared.

**Brightness** is `BrightenTexture` done in the shader: each 5-bit channel
times `(OptBrightness + 128) / 256`, floored and capped, plus the same factor
on the sky/fog colour. **Pause** is `VK_PAUSE`, and `P` unless a binding uses
it (many keyboards have no Pause key). **Health recovery** (`MyHealth +=
TimeDt * 4` while alive and not paused) was missing until now; without it
drowning was 1.5x too fast.

**Death returns to the menu** (discarding the hunt's trophies, as the
original's `LoadTrophy` on exit does) when started from the menu; `c2-pc hunt`
still restarts.

## 2026-10-18 — Field of view, binoculars, scope, HUD text

**FOV matches the original:** `CameraW = VideoCX * 1.25` with its 4:3
correction gives a vertical half-angle tangent of 0.6 (61.9 degrees); the
binoculars (1.5x-3x) and scope (3x) divide it. On screens wider than 4:3 the
binocular and scope frames are stretched horizontally to cover the view (they
were drawn for 4:3); the zoom itself is unchanged.

**HUD text** uses the public-domain 8x8 font from the `font8x8` crate (MIT);
the original used Windows GDI fonts, which the game data doesn't include.
Units follow the save's `OptSys` (imperial when set).

## 2026-10-18 — Audio

**kira 0.12** (owner's choice), default features off except `cpal` (we only
play raw PCM; no file decoders needed). We don't use kira's spatial model:
the original's mixer (`Audio.cpp`) gives each voice left/right gains from
`CalcLRVolumes` (falloff `1224/(1224+d)` past 512 units, fading out between
6000 and 10000, its own pan law) and positioned voices stay where they were
emitted. We compute those gains and map them exactly onto kira's volume and
constant-power panning, re-panning every frame. 16-voice limit (extra sounds
dropped), ambience crossfades over ~0.74 s, as the original's 16 x 1024-sample
ramp. The jump and water in/out sounds are never loaded in the released
source, so they stay silent. Ambience zones and random sounds are game logic
in `c2-core`.

## 2026-10-18 — M3: weapons, detection, death

`c2-core::game::Hunt` holds a whole hunt (player, herd, weapons, wind,
health) so every frontend runs identical logic. Ported: `ProcessShoot`,
`MakeShot` (incl. the crossbow's three falling segments), `TraceShot`
(ground, water, objects, posed characters; `sfMortal` faces kill outright),
`MakeNoise`, `CheckAfraid` (sight with `TraceLook` and cover, smell with
wind), the weapon state machine from `DrawPostObjects`, `AddDeadBody`,
`AnimateHuntDead` and `ProcessDemoMovement` (death camera).

Quirks kept: `TraceModel` rotates objects by eighth turns (`(FMap>>2)&7`)
though they render in quarter turns; `ObjectsOnLook` carries over between
animals as the original's global does.

Deviations: weapon animation advances in the simulation tick (the original
advances it while drawing). Key bindings default to the owner's saved
`KeyMap` (W/S/A/D, LMB fire, E weapon, Shift run toggle, X crouch, Space
jump). The loadout menu doesn't exist yet, so every weapon is carried.

## 2026-10-18 — M3: dinosaur AI

All 15 `Animate*` routines from `Characters.cpp` are ported, plus
`PlaceCharacters`, `ReplaceCharacterForward`, `CreateChMorphedModel` and the
death animations. Where routines are copies with different constants they share
one parameterized function (`herbivore`: Par/Anky/Steg/Chasmo, `carnivore`:
Allo/Velo/Spino/Cerato, `small`: Mosh/Dimet/Gall); the parameter tables list
the original's constants and phase numbers per species.

**`MoveCharacter` runs its body.** In the released source its first line is
`return;`, so no dinosaur would ever move — clearly a debugging leftover, as
every dinosaur walks in the retail game. We port the body as written below it.

**Kept original quirks** (they affect behaviour subtly but faithfully):
- `StateF &= (!csONWATER)` (logical not) clears the whole field, not one bit.
- Herbivores test `State & csONWATER` (never set) instead of `StateF`.
- `PlaceCharacters` lifts Dimetrodon (not Dimorphodon) by 2048 at spawn.
- Raw phase-index comparisons (`_Phase <= 2`) differ in meaning per species.
- T-Rex and Ceratosaurus only spawn when chosen as hunt targets.

`AnimatePac` (Pachycephalosaurus) is not dispatched by `AnimateCharacters`;
it's a Carnivores 1 leftover and isn't ported.

The AI uses an exact port of MSVC's `rand()` (same LCG), so random decisions
come from the same kind of sequence as the original's.

`c2-parity dinos <AREA.MAP> [seconds]` places and runs the herd headlessly.
`c2-pc hunt ... --sim MS,AI,DIST,AFTER --shot x.png` renders a chosen animal.

## 2026-10-18 — M3: hunter movement

**Fixed 10 ms simulation tick** instead of the original's variable `TimeDt`.
The movement code is a direct port (it already scales by `TimeDt` and
sub-steps collision every 16 ms), so behaviour is the same at any frame rate,
and a fixed step makes runs reproducible for the planned logic-parity harness.
Rendering interpolates the eye position between ticks.

**Functions that read the global `PlayerY` take it as a parameter**
(`land_qh`, `land_ceil_h`, `check_collision`) so they can serve dinosaurs too.

`c2-parity walk <AREA.MAP> [seconds]` runs the hunter headlessly over real
terrain and prints the path: a quick regression check for movement/collision.

## 2026-10-18 — M2 viewer

**Stack: `winit` 0.30 + `wgpu` 30** (owner's choice), plus `glam` (maths),
`bytemuck`, `pollster`. Versions picked at least two weeks old. No image crate:
screenshots use a ~60-line stored-deflate PNG writer.

**Colour maths stay in the original's space.** The original lights
(`D3DTBLEND_MODULATE` with a grey vertex colour) and fogs raw 16-bit colour
values. We use non-sRGB textures and surfaces so `texel * light` gives the same
numbers. The owner's saves use `OptBrightness = 128`, which makes
`BrightenTexture` a no-op, so it's skipped for now.

**Terrain is generated in the vertex shader** from a per-cell storage buffer
(height, light, water, texture, flags) instead of building meshes on the CPU.
Each cell is 6 vertices; triangulation follows `fmReverse`, and the 4
`TDirection` UV tables in `DrawTPlane` reduce to rotating the corner, which
is how the shader does it. The PC draws a square window around the camera;
the 3DS will need cooked meshes instead (M6).

**Load-time fixups live in `c2-core::area`** (`CreateTMap` hardware path,
`GetLandH`, `GetObjectH`, landing list, trophy ring), so the cooker can reuse them.

**Sky, sun and lighting** follow `RenderSkyPlane`, `RenderSun`, `CalcLights`
and the cloud-shadow code in `Hunt2.cpp`. Night is rendered as the original
does it: `BrightenTexture` maps every texel to `(G/8, G, G/8)`, i.e. night
vision. The sun/moon is drawn additively (the original's black quad corners
are invisible in-game).

**Known approximations** (to check against the retail game): no `RandomMap`
light jitter; ground-lit objects use one light value per object instead of
per vertex; `sfTransparent` faces blend at 50%; object quarter-turn direction
is unverified; no distant billboards (`bmpmodel`) yet; sun is not occluded
by terrain (`SkyTraceK` = 1).

## 2026-10-17 — TGA/WAV: read the header instead of assuming it

The original TGA loader assumes 16-bit uncompressed bottom-up and ignores the
header; the WAV loader assumes 16-bit mono 22050 Hz and ignores `fmt `. For
every vanilla file our output is identical to the original's (checked by
`c2-parity scan`). For files in other encodings (seen in some mods: 24/32-bit,
RLE and ID-field TGAs, a stereo 16 kHz WAV) we decode correctly instead of
reproducing the original's garbage, and report the format so the cooker can
convert it. This only changes behaviour for files the original misread.

## 2026-10-17 — Add-on scope; MIT license

**The public build targets add-on mods**: small, data-only mods that layer
on vanilla and merge cleanly. Whole-game total conversions are out of scope.

**License: MIT** (`LICENSE`, `license = "MIT"` in the workspace). Covers our
code only; game and mod data are excluded.

## 2026-10-17 — M0/M1 kickoff

**Repository layout differs from README §3 (owner's request).**
- `pc-ver/` is the Cargo workspace for all shared crates plus the PC backend.
- `3ds-ver/` will hold the 3DS backend as its own workspace (nightly toolchain,
  `armv6k-nintendo-3ds` target) that depends on `../pc-ver/crates/*` by path.
  A separate workspace keeps the nightly pin and devkitPro requirements from
  leaking into the stable PC build.
- `source/` (gitignored) replaces `reference/`:
  - `source/c2-original/` — the user's Carnivores 2 **install** (README called
    this `reference/game/`).
  - `source/c2-source/` — original C++ source (README's
    `reference/c2-original/`), cloned from
    `github.com/videogamepreservation/carnivores2` (mirror of the 2013 Assembla release).
  - `source/mods/` — mod installs.

**ReShade is out of scope.** The user's install includes ReShade
(`reshade-shaders/`, `dxgi.dll/.ini`, `d3d9.dll/.ini`, `opengl32.dll/.ini`).
These are third-party post-processing wrappers, not game data; they are
ignored by every tool and will not be ported to PC or 3DS.

**The released C++ source is not the retail 1.0.4 build.** It has no menu
code (`Interface.cpp` is a fragment) and never reads the `prices` section that
every `_RES.TXT` contains. Where the source is missing, behaviour will be
matched against the retail `carn2.exe` by observation, and noted here.

**`_RES.TXT` keys are matched exactly, not by substring.** The original uses
`strstr(line, "ai")` etc., so any line containing the substring sets the field
— e.g. a character named `'Maiasaura'` would have `ai` reset to 0. We match
whole keys, plus the aliases the substring rule actually made work in shipped
data (`hearK`, `lookK`). Verified: `c2-parity` carries a faithful `strstr`
copy of the original reader and diffs every typed field; no `_RES.TXT` we
have produces any difference, and none would trigger the original's halt. Unknown keys are preserved (`extra`) so mod data survives merge/write.

**Parsers return on-disk values; transforms live in the engine.** Vertex
`*2`/z-flip, `BrightenTexture`, face re-sorting, `TraceC = trace - 1`, and
`_RES.TXT` defaults are applied by `c2-core`, not `c2-formats`. This is what
makes byte-exact round-tripping possible, which the cooker and mod merger need.

**Counts are bounded by input size, not engine limits.** The original has
fixed arrays (1024 vertices/faces, 256 objects, 64 animations…). We reject a
count only if the remaining bytes can't possibly hold it, so mods that exceed
the old limits still load and no malformed file can trigger a huge allocation.

**Synthetic fixtures are generated inside the tests** (`c2-formats/tests/`)
rather than by a separate tool in `tools/`. Same guarantee (no real game data
in the repo), less machinery. A standalone generator can be added if the
parity shim needs files on disk.

**Format parity is checked by byte-exact round-trip instead of a C++ JSON
shim (for now).** Building the original requires MSVC 6-era DirectX/Glide
SDKs. Parse-then-write reproducing every real file byte-for-byte, with the
parser consuming the whole file, is a strong check of the layouts. A shim that
compiles just the loader functions with modern GCC remains an option if a
field's *meaning* (rather than layout) needs confirming.

**License:** superseded — MIT, see the entry above.
