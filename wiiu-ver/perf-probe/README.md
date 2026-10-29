## Status: Wii U port

### Done

- Rust runtime, SD access, shared parsers and VFS probes passed in Cemu.
- The Wii U hunt renders the selected map, terrain, water, sky, scenery and
  animals; movement, radar, calls, audio, blood, death flow, settings, menu,
  randomized populations and performance overlay are implemented.
- All six stock Carnivores 2 weapons are selectable and functional in the hunt.
  Y cycles carried weapons; ZL draws or holsters; ZR fires. Cemu gameplay has
  confirmed the weapons work, but weapon model textures still need visual
  correction.
- The GamePad now has a two-column, three-row weapon picker beside the map.
  Tap a carried weapon to equip it; tapping the equipped weapon hides it. A
  gold outline marks the active weapon. The touch picker change still needs a
  Cemu check.

### Remaining

- Diagnose and fix the incomplete/incorrect weapon model texture rendering,
  then visually review all six guns and their animations in Cemu.
- Confirm the GamePad weapon pictures display and touch selection works for
  every carried-weapon combination; check TV-only and GamePad-only modes too.
- Recheck weapon ammo/reload behavior, sounds, crossbow drop and sniper optic.
- Continue the broader Cemu pass for randomized population appearance, calls,
  radar, predator behavior, blood, death playback, performance and restarts.
- Test on Wii U hardware; all current runtime results are from Cemu.

Latest build: `build/c2-wiiu-stock-weapons.rpx` (will be refreshed with the
GamePad picker update).

The hunt setup supports the six stock Carnivores 2 weapons: pistol, shotgun,
double shotgun, crossbow, rifle, and sniper rifle. Select any combination in the
Weapons and Gear screen. Original weapon CAR models, animation data, firing
sounds, and six menu pictures are loaded from the owner's HUNTDAT files by the
staging script. The shared weapon handling supplies each stock weapon's ammo,
accuracy, power, spread, loudness, rate, crossbow drop, sniper optic, and reload
behavior. The user reports that the guns work in Cemu; texture rendering and
the new direct GamePad weapon picker remain under visual verification.

Latest test build: `build/c2-wiiu-population-calls.rpx`.

The GamePad call panel shows selected dinosaurs in two columns, six per page.
Tap a picture to select/use its call (normal call cooldown still applies); use
PREV/NEXT for selections larger than six. The highlighted picture is the current
call. D-pad Down cycles selected calls and follows their page. Existing SD assets
are sufficient.

The 66-animal population previously measured 47–52 FPS in Cemu. This build tries
four-way staggered updates for calm stock targets beyond view range plus 16 cells,
carrying elapsed simulation time into their next update. Nearby, alerted, wounded,
dead and ambient animals keep full-rate updates. The shared option defaults off
outside this Wii U setup. Population and render distance are unchanged.
The overlay now separates CPU SIM and ACTORS time; ACTORS includes animal and
weapon mesh generation/buffer updates. These are CPU timings, not GPU execution
timers. Cemu FPS gains, touch readability and real hardware remain unverified.

# Wii U stock Carnivores 2 roster probe

Run `build/c2-wiiu-random-population.rpx` (or the latest `c2-wiiu-perf-probe.rpx`)
in Cemu. Supply original Carnivores 2 data in `wiiu/apps/carnivores-probe/`
on the SD card. Game data is not bundled. To copy the exact required files:

```sh
python3 wiiu-ver/perf-probe/stage_assets.py /path/to/HUNTDAT /path/to/sdcard/wiiu/apps/carnivores-probe
```

The 3DS-style menu now runs before hunt loading. It offers all five daytime
hunting maps: Delphaeus Hills, Fort Ciskin, Vengar Fjords, Manya Jungle and
Mount Ravan. Pick any combination of the nine stock targets: Parasaurolophus,
Ankylosaurus, Stegosaurus, Allosaurus, Chasmosaurus, Velociraptor, Spinosaurus,
Ceratosaurus and T-Rex. The pistol is required and radar is optional.
Each hunt now starts with 66 animals: 48 targets split roughly evenly among
selected species, plus three each of the six ambient species (18 total).
Positions and headings are randomized across valid map terrain, with shared
water/slope/object collision checks and spacing between animals. Targets
start at least 40 cells from the hunter; ambient animals start at least eight
cells away. Flyers start airborne. The shared ambient AI may relocate distant
small animals toward the active area during play; hunt targets remain spread
across the map. This is a Wii U population tuning choice, not exact PC counts.

New hunts and restarts use a fresh timebase seed, logged in Cemu's log for
diagnostics. Settings and target selections survive restart, while the animal
layout changes. The target menu keeps its three-by-three picture grid.

Use GamePad touch or D-pad and A to select; B goes back. White shows
controller focus, gold marks selections. The 9999 points remain temporary;
profiles, pricing, saves and trophies are not implemented. The title's dark
panel covers the PC buttons embedded in the original artwork.

Open Settings from the title menu to toggle Blood and Kill Animations with
GamePad touch or D-pad/A. B or Back returns to the title. Both default to On.
Blood Off disables dinosaur-hit blood and ground trails. Kill Animations Off
skips death playback and immediately opens Restart Hunt / Main Menu.
Choices survive hunt restarts and returning to the title during this session;
they are not yet saved to SD across RPX launches. This update uses the same 92 assets as the stock-roster build; no new SD files are required.

L stick moves, R stick looks, A jumps, B toggles sprint, D-pad up toggles
crouch, ZL draws/holsters the pistol and ZR fires. D-pad down cycles through
selected targets' calls; X or the GamePad call picture uses that call. The
GamePad picture updates to match. The HUD shows ammo with no health meter
or crosshair. Tapping the pistol picture toggles it. R toggles the full map;
minus switches the GamePad between the map and gameplay.

AX audio now includes pistol and animal sounds, all nine target calls,
area loops/random sounds, dry/water footsteps, landing sounds, underwater
ambience and hunter death sounds. Positional gain is calculated at sound
start; it does not track moving voices yet. Loops switch without crossfading.
An animal's death stops its current voices. Hunter death plays the shared death camera and original HUNTER1.CAR body
animation on both displays for 12 seconds, then opens a dark green kill screen with shots/kills and Restart Hunt / Main Menu.
Use D-pad and A (GamePad or Pro Controller), or tap either GamePad button.
Gameplay and ambient sounds stop while the hunter death sound can finish.
A short input guard prevents the killing moment's button press from selecting
an option immediately. No health meter or crosshair is added.

Restart rebuilds the hunt on the same map with the same targets and radar,
restoring the player, animals and ammunition while keeping loaded assets.
Main Menu releases the current hunt and returns to the title, retaining the
previous selections for editing. This is a death menu, not an in-hunt pause menu. The weapon is hidden and
hunt actions are suppressed during the sequence. All world layers use the
death camera. The hunter occupies a separate species/model slot so it is
not rendered or animated as a Moschops. Base C2 behavior is used. Predator attacks and death playback are covered
by host tests; visual verification in Cemu and mod-specific kill sequences
remain separate validation work.

Blood bursts from both lethal and nonlethal dinosaur hits, and blood trails,
are now drawn through the bounded actor geometry buffer. This first pass uses
solid red particle diamonds and ground marks; it is not yet the PC renderer's
blended particle appearance. Ground marks shrink during their final 20 seconds.
The actor atlas is now 2048 by 1024, adding 4 MiB over the previous build.
The staging script includes all required stock models, pictures and sounds.

Animated animals now undergo conservative distance/frustum checks before posing.
Bounds cover every original animation frame. Visible animals are drawn nearest
first, with whole-model capacity checks and space reserved for blood geometry.
All animals still simulate; this reduces rendering work, not AI updates.
The renderer retains the PC-style 74-cell distance, GPU scenery transforms
and fog, precomputed object lighting, and 32 MiB lazy scenery cache. The
terrain atlas now holds 256 textures (2048 square), because three original
maps exceed the old 64-texture limit. The actor atlas holds all fifteen animal
models, the hunter and the pistol. The larger terrain atlas adds 12 MiB over the old
one; performance across all maps and real hardware remains unmeasured.

Host checks exercise the exact Wii U setup on every map with each target
individually and all nine selected: AI, poses, calls, geometry capacity,
Chasmosaurus/predator contact deaths, 12-second playback and restart. They
also cover both settings across restarts and all 27 call WAVs. Menu sanitizer
checks cover all 511 target combinations, navigation, touch and raster bounds.
Population tests additionally verify counts, selected-species balance, valid
terrain, spacing, landing clearance, map-wide spread, repeatable fixed seeds,
different layouts for different seeds, and visibility boundaries.
The randomized population still needs Cemu checks for appearance, calls/audio,
radar, predator behavior, blood, death playback and performance, followed by
restart and return-to-menu. Real hardware is untested.

The times are milliseconds averaged over 30 frames, measured with the Wii U
CPU clock. Cemu emulates that clock, so these are diagnostic measurements,
not guaranteed real-hardware timings. Vertex counts are from the latest frame.

- **FRAME**: whole CPU loop iteration, including waits and presentation.
- **WAIT**: time blocked in the explicit `GX2DrawDone` calls, including the
  map buffer synchronization. This is a CPU wait for submitted GPU work; it
  is not the GPU's execution time and can include emulator overhead.
- **TERRAIN**, **SCENERY**, **WATER**, **SKY**: CPU buffer generation,
  fog calculation, and associated buffer locks/unlocks.
- **MAP**: CPU map texture restoration, overlay painting, and invalidation.
- **DRAW**: CPU time in render setup and draw submissions for both displays,
  excluding the explicit map `GX2DrawDone` wait. It is not GPU draw time.
- **PRESENT**: CPU time in `WHBGfxFinishRender`; it may include presentation,
  synchronization, and frame pacing. It is not pure GPU time.

The overlay itself costs some time and the frame average includes its periodic
update. It disappears when the TV full map is open. R toggles that map; minus
switches the GamePad between map and gameplay. Movement is left stick, look is
right stick, A jumps, B toggles sprint, and D-pad up toggles crouch. This death-screen update
has not yet been visually verified in Cemu or tested on hardware.

Build with the same Rust and devkitPro procedure as the [fog probe](../fog-probe/README.md),
substituting `perf-probe` for `fog-probe` in the paths. Shader and CMake changes
must be rebuilt before testing.

The startup menu uses its own `menu.vert`/`menu.frag` pair compiled into
`menu.gsh`, independently of the world’s camera/fog shader. If editing it, run:

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler --uniform-mode register -vs wiiu-ver/perf-probe/menu.vert -ps wiiu-ver/perf-probe/menu.frag -o wiiu-ver/perf-probe/menu.gsh
```

Then rebuild the RPX; CMake embeds both shader files.
