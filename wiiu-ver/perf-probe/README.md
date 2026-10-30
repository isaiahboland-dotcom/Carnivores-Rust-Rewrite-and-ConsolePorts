## Status: Wii U port

### First playable hardware result, 2026-10-29

The owner confirmed the trace-v3 WUHB displays the blue TV/magenta GamePad
check, then the menu and a playable hunt on a real Wii U under Aroma. Keep
the display re-enable calls after the loading console shuts down.

Gameplay was reported below 30 FPS. The SD log contains loading results but
no gameplay timing samples, so measured bottlenecks have not been ranked yet.
Scope zoom is not connected to rendering. (GamePad weapon taps, the 512x256
menu and the gun drawn without depth, which showed the shotgun shells and the
pistol arm through the gun, were fixed on 2026-10-30.)

See the [hardware review](../../docs/WIIU-HARDWARE-REVIEW.md) for evidence,
optimization candidates, required profiling, and the prioritized fix list.
The installed baseline is `build/carnivores-probe.wuhb` (trace v3); this review
does not replace it with another build.

### Hardware startup diagnostics

The playable build now displays each menu file read/decode before switching
from the red loading console to GX2. It writes flushed startup checkpoints to
`SD:/wiiu/apps/carnivores-probe/startup.log`, including graphics initialization,
shader loading, controller setup, and the first frame presentation. The file is
replaced when a session starts; after a hang, return the SD card to the PC and
read its last line before launching again. Failure to open the log is reported
on the loading console. Game assets and the complete playable hunt are retained.

This diagnoses a reported hardware black screen after the initial two loading
lines. All 102 SD assets were verified against the original game, and switching
the same RPX into WUHB did not resolve the report. The trace-v3 handoff fix
described below subsequently resolved the black screen on hardware.

The first hardware log reached `Menu: first frame presented PASS`. Trace v2
therefore adds a two-second clear-only display check (blue TV, magenta GamePad)
before the textured menu, explicit menu color-write/channel-mask state, and
frame/swap/flip progress in the log. A returned presentation call does not
prove that either display showed the intended pixels. This build distinguishes
clear-only scanout problems from textured drawing and subsequent-frame stalls.

Hardware trace v2 recorded more than 9,000 menu frames with advancing swap/flip
counters, despite both displays staying black even during clear-only frames.
Trace v3 explicitly calls `OSScreenEnableEx` for TV and GamePad after
`WHBGfxInit`: the preceding `WHBLogConsoleFree` calls `OSScreenShutdown`, which
disables both outputs. The owner subsequently confirmed both colors and a
playable hunt: the handoff change is now confirmed on hardware.

### Done

- Rust runtime, SD access, shared parsers and VFS probes passed in Cemu.
- The Wii U hunt renders the selected map, terrain, water, sky, scenery and
  animals; movement, radar, calls, audio, blood, death flow, settings, menu,
  randomized populations and performance overlay are implemented.
- All six stock Carnivores 2 weapon assets and shared handling are integrated.
  Y cycles carried weapons; ZL draws or holsters; ZR fires. Cemu gameplay has
  confirmed basic firing, but hardware exposed selection, depth/occlusion,
  scope and performance defects; complete weapon correctness is pending.
- The GamePad now has a two-column, three-row weapon picker beside the map.
  Tap a carried weapon to equip it; tapping the equipped weapon hides it. A
  gold outline marks the active weapon. Hardware exposed a dropped slot
  payload in the touch input queue; taps currently select the first carried gun.

### Remaining

- Diagnose and fix the incomplete/incorrect weapon model texture rendering,
  then visually review all six guns and their animations in Cemu.
- Confirm the GamePad weapon pictures display and touch selection works for
  every carried-weapon combination; check TV-only and GamePad-only modes too.
- Recheck weapon ammo/reload behavior, sounds, crossbow drop and sniper optic.
- Continue the broader Cemu pass for randomized population appearance, calls,
  radar, predator behavior, blood, death playback, performance and restarts.
- Capture gameplay timing samples on Wii U hardware, then prioritize measured
  optimization work and the fixes in the hardware review.

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

### The menus (HD, 2026-10-30)

The menus are drawn in Rust (`src/menu.rs`, `src/ui.rs`) at each display's
own resolution: 1920x1080 for the TV and 854x480 for the GamePad, from a
1280x720 layout, with the console's system font (`OSGetSharedData`; the 8x8
bitmap font if there is none) and the original pictures at their full size.
Each is redrawn on the CPU only when something changes; scaled pictures are
cached between redraws.

- Title: Hunt, Settings, Exit (back to the Wii U Menu).
- Hunt setup, the 3DS design: 1. the five areas as hexagons, and dawn, day or
  night; 2. the nine animals; 3. the six weapons, camouflage, radar, cover
  scent, double ammo, tranquilizer and observer mode (no weapons). The
  focused item's original picture and description (`MENU/TXT`) are shown on
  the right; the bottom line sums up the picks.
- Settings: dinosaur run speed (50-100%), ambient animals, blood, death
  camera, animal descriptions in metric or US units, the map look (classic
  or mobile), teleport (tap the GamePad map) and button hints.
- Controls: D-pad or left stick moves, A picks, B goes back, L/R change the
  setup page, + starts the hunt; or tap the GamePad.

The picks and settings are saved to `settings.txt` beside the app when the
menu closes and read at start. Night hunts use the night sky, light map and
the original's night vision (every world texel becomes (G/8, G, G/8)).
The title also offers survival (the setup's second page picks the hunters
that come in waves and normal, hardcore or custom), the trophy room
(`TROPHY.MAP`) and the hunter record (score, trophies, kills, best wave,
longest shot, the last kills). Prices are still to come.

In a hunt, + asks to leave (A yes, B no, Y restart unsaved) and D-pad Right
pauses. Leaving saves the trophies and score to `trophy00.sav` (the
original's format, with its character numbers, so it moves to and from the PC
and the 3DS) and every kill to `trophy00.rec`; dying keeps only the kills, as
on the PC. A kill shows the PC's notice (bottom right) or, for a trophy, its
plaque (`TROPHY_G.TGA`); survival shows the wave. These are drawn by
`src/overlay.rs` into a 1024x1024 texture and blended over the view with the
menu shader (which now passes alpha). In the trophy room ZL / ZR move the
trophy in front to the previous or next free stand, X removes it, Y locks it,
and + leaves (saving moves and removals). The install now has 148 files (the
trophy room's map, the two plaques and the PC map's frame `MAPFRAME.TGA`).

The world's three atlases are tiled with five mip levels (`atlas_mips.h`),
as the original mipmaps its textures in the distance; the linear copies are
freed once every level reads back intact. R3 turns the levels off for an A/B
(overlay line 6 starts `VM`/`VF`; capture `tiled_atlases` 2/1, schema 10).

The TV's world is drawn at 1280x720 and scaled up to the 1080p TV buffer,
with the HUD, overlay and performance text drawn on top at full resolution
(`tv_target.h`; the 720p buffers reuse the memory of the 1080p TV depth
buffer). L3 draws it at the TV's own resolution for an A/B (overlay line 6,
5th letter `S`/`N`; capture `tv_world`, schema 11). Terrain and water use
`terrain_opaque.frag` (no `discard`, so early depth testing), the world
atlases are R5G5B5A1, and the fog is a per-draw uniform: two 16-byte vertex
streams instead of three.

R shows the PC's map over the TV view (the overlay's map region; the dots
and the mobile look's cone repaint as they move). The GamePad map has a zoom
button at its top right ("-" shows the whole area, "+" goes back), and with
teleport on a tap elsewhere on it teleports there after 5 s. With button
hints on, the controls run along the bottom of the TV view. Dawn is tinted
warmer than the original's (a Wii U-only look; see `docs/DECISIONS.md`).

The host test `pc-ver/crates/c2-core/tests/wiiu_menu.rs` drives every page;
with `C2_MENU_SHOTS=DIR` (and `C2_MENU_FONT=FILE.ttf`, `C2_ORIGINAL_HUNTDAT`)
it writes them as PNGs at both sizes.

Each hunt starts with 66 animals: 48 targets split roughly evenly among the
picked species, plus three each of the six ambient species (18, none with
ambient animals off). Positions and headings are randomized across valid map
terrain, with shared water/slope/object collision checks and spacing between
animals. Targets start at least 40 cells from the hunter; ambient animals
start at least eight cells away. Flyers start airborne. This is a Wii U
population tuning choice, not exact PC counts. New hunts and restarts use a
fresh timebase seed; restarts keep every option.

L stick moves, R stick looks, A jumps, B toggles sprint, D-pad up toggles
crouch, ZL draws/holsters the pistol and ZR fires. D-pad down cycles through
selected targets' calls; X or the GamePad call picture uses that call. The
GamePad picture updates to match. The HUD shows ammo with no health meter
or crosshair. Tapping the pistol picture toggles it. R toggles the map over
the TV view; minus switches the GamePad between the map and gameplay.

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
update. R toggles the map over the TV view; minus
switches the GamePad between map and gameplay. Movement is left stick, look is
right stick, A jumps, B toggles sprint, and D-pad up toggles crouch. This death-screen update
has not yet been visually verified in Cemu or tested on hardware.

Build with the same Rust and devkitPro procedure as the [fog probe](../fog-probe/README.md),
substituting `perf-probe` for `fog-probe` in the paths. Shader and CMake changes
must be rebuilt before testing.

The world's shaders: `terrain.vert` with `terrain.frag` (cut-out) and with
`terrain_opaque.frag` (terrain and water). If editing them, run:

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler -vs wiiu-ver/perf-probe/terrain.vert -ps wiiu-ver/perf-probe/terrain.frag -o wiiu-ver/perf-probe/terrain.gsh
wiiu-ver/tools/cafeglsl/bin/glslcompiler -vs wiiu-ver/perf-probe/terrain.vert -ps wiiu-ver/perf-probe/terrain_opaque.frag -o wiiu-ver/perf-probe/terrain_opaque.gsh
```

The startup menu uses its own `menu.vert`/`menu.frag` pair compiled into
`menu.gsh`, independently of the world’s camera/fog shader. If editing it, run:

```sh
wiiu-ver/tools/cafeglsl/bin/glslcompiler --uniform-mode register -vs wiiu-ver/perf-probe/menu.vert -ps wiiu-ver/perf-probe/menu.frag -o wiiu-ver/perf-probe/menu.gsh
```

Then rebuild the RPX; CMake embeds both shader files.

### Persistent hardware timing capture (schema 1)

Gameplay now retains the mounted SD card/FS client until session cleanup and
writes `performance-<timebase>-<session>.csv` beside `startup.log`. Each hunt
session gets a separate file; restarts keep the file and change the seed column.
The startup trace still closes before gameplay. The confirmed display handoff
and temporary diagnostic colors are unchanged.

Capture is bounded to 16 MiB per session. Rows accumulate in a 64 KiB aligned
buffer, written/flushed when full or after five seconds. Normal HOME exit and
return to the main menu save pending rows and close the file before releasing
SD. An abrupt power loss or GPU hang can lose the current batch. A write failure
disables capture and reports through OSReport; gameplay continues. Move captures
off SD between tests to avoid accumulating files across sessions.

Each row records seed, frame start time, view modes, simulation tick count,
CPU timing buckets, and emitted vertex counts including the gun. Metadata records
compile date/time, selection/settings, baseline population, and actual TV/DRC
render-buffer sizes. Animal/gun buffer work, map painting/invalidation, and the
two explicit wait sites are separate. `geometry_stream_bytes` counts emitted
position/attribute data (32 bytes/vertex, which is now the whole vertex), not measured GPU transfers or the
cache ranges flushed by GX2R. Buffer locks remain inside geometry buckets;
scenery-cache misses/evictions/occupancy and visible/posed animal counters are
not yet captured. These are follow-up instrumentation, not zero-valued metrics.

Frame timings exclude CSV formatting/writes; `previous_profiler_ms` reports
that work on the following row. Logging can still affect the next frame's
simulation catch-up. The last profiler operation and final close are not in a
subsequent row. Frame start intervals reveal additional elapsed time, including
logging, menus and suspension. WAIT/PRESENT remain CPU elapsed time, not GPU
timers. To summarize a copied capture:

```sh
python3 wiiu-ver/tools/summarize-performance.py performance-*.csv
```

The tool groups by seed and view mode, reports timed-loop FPS, mean/p95/p99/max
frame time, average CPU buckets and geometry bytes. Grouped wall spans can include
intervening modes/menus; use contiguous intervals for comparisons. Archive the
built RPX/WUHB and its hash alongside each capture. For hardware comparisons,
follow the stationary/dense rotation/cold movement/nearby animals/weapon route in
[the hardware review](../../docs/WIIU-HARDWARE-REVIEW.md), testing both GamePad
modes with the same selections and output mode. The first physical-console capture is now archived under `hardware-logs`; see
the range-upload results below.

### Range-upload hardware experiment

The owner reported 180.9 ms/frame, with WAIT 75.3 ms and SCENERY 68.2 ms on the
preserved playable build. The first experiment keeps all explicit GPU waits and
geometry generation but replaces whole-buffer GX2R cache maintenance with an
explicit `GX2Invalidate(CPU_ATTRIBUTE_BUFFER)` for each written vertex prefix.
The prior frame must complete before any stream is locked or rewritten. GX2R
allocation/binding remains in use; lock/unlock automatic invalidation is disabled
only for these dynamic streams. Static buffers and menu/map/UI buffers retain
their existing maintenance. Zero-vertex streams need no cache operation.

`C2_RANGE_UPLOAD` is a CMake option, default ON; configure OFF for the legacy
whole-buffer path with the same instrumentation. Schema 2 metadata records the
option. New `stream_lock_ms` and `stream_cache_ms` columns are subsets of the
existing geometry buckets; do not add them to frame totals. `stream_cache_bytes`
is the sum of requested cache range sizes (including dynamic sky fog), not a
measured bus transfer count. The summary prints these subsets separately.

Host checks for exact, empty and full stream ranges and the legacy path:

```sh
cc -std=c11 -Wall -Wextra -Werror wiiu-ver/perf-probe/tests/stream_upload_test.c -o /tmp/wiiu-stream-test
/tmp/wiiu-stream-test
cc -std=c11 -Wall -Wextra -Werror -DC2_RANGE_UPLOAD=0 wiiu-ver/perf-probe/tests/stream_upload_test.c -o /tmp/wiiu-stream-legacy-test
/tmp/wiiu-stream-legacy-test
```

The range-upload WUHB is `build/carnivores-range-upload.wuhb`. Build/host checks
pass and the first physical-console timing capture is archived below.
The SD's prior playable WUHB is archived as `build/hardware-playable-baseline.wuhb`.

### Map synchronization and hidden work experiment (schema 3)

The [first range-upload hardware capture](hardware-logs/2026-10-29-range-upload-results.md)
now provides physical-console evidence. The next candidate uses immutable map
positions and separate six-vertex UV buffers for TV and DRC. Both UV buffers are
written after the previous-frame completion wait and before either display
submits draws. The old post-map `GX2DrawDone` is removed because no display
rewrites the other's submitted UVs. Final WHB completion and the previous-frame
resource wait remain. Some WAIT time may move to PRESENT; assess total frame time.

World geometry generation and dynamic stream maintenance are skipped when both
displays show maps. Simulation continues. Map painting and texture maintenance
are skipped when both displays render gameplay; cached overlay restoration is
retained for the next time the map opens. Death playback forces world rendering
on both displays as before. Schema 3 adds `death_playback`, and the summary groups
it separately. Older captures report death state as unknown. View flags alone
must not be treated as proof of the actual rendered views in older captures.

Host layout/policy checks cover both views, death/full-map/gameplay combinations,
map-edge UV clamping, triangle continuity and independent TV/DRC UVs:

```sh
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined wiiu-ver/perf-probe/tests/map_view_test.c -o /tmp/wiiu-map-test
ASAN_OPTIONS=detect_leaks=0 /tmp/wiiu-map-test
```

Leak checking is disabled for this allocation-free test because the sandbox's
ptrace environment prevents LeakSanitizer from running. Address/undefined checks
remain enabled. Wii U RPX and host checks pass; the new candidate still needs a
physical-console check of both map modes, mirrored gameplay, death playback,
and repeated map toggles. Population, visibility range and resolution are unchanged.

### Tiled atlas A/B experiment (schema 4)

The [map-stream hardware capture](hardware-logs/2026-10-29-map-streams-results.md)
separates live play (112.18 ms/frame) from death playback (130.79 ms/frame).
The map wait is eliminated; PRESENT is now the largest CPU elapsed bucket.
It still includes completion/presentation and is not a pure GPU timer.

The next candidate retains linear terrain, scenery and actor atlases and creates
2D tiled copies with `GX2CopySurface` during loading. Texture dimensions, UVs,
RGBA8 data, alpha, filtering, fog, geometry and output sizes are unchanged.
For validation, each tiled atlas is copied back to temporary linear storage and
all RGBA texels are compared to the original. Allocation/roundtrip failure
falls back to linear sampling. CPU cache lines are flushed before GPU writes,
GPU completion precedes readback, and CPU readback cache is invalidated before
comparison. The temporary readback surface is freed after checking. Keeping
both layouts adds approximately 40 MiB persistent memory for this A/B test.

D-pad **Left** on GamePad or Pro Controller switches between tiled and linear
layouts when all three conversions validate. The last overlay row starts with
**T** (tiled) or **L** (linear); it refreshes with the usual 30-frame averages,
so allow 30 frames after switching before transcribing overlay values. The
candidate starts tiled when validation succeeds. The map/UI/sky remain linear.

Schema 4 metadata adds `tiled_available`; each frame adds `tiled_atlases`.
The host summary groups by layout as well as view toggles, seed and death state.
For a comparable capture, stand still for 15–20 seconds in each layout, switch
back and repeat, then rotate/move with both. Keep the same GamePad view mode.
Record visual corruption immediately; D-pad Left returns to the original atlas
layout. Compare total frame/PRESENT time, vertex counts and live/death groups.
Do not compare unrelated locations/seeds or interpret all PRESENT as GPU time.

### HOME lifecycle repair candidate and static-view work (schema 5)

The [tiled-atlas A/B capture](hardware-logs/2026-10-29-tiled-atlas-ab-results.md)
supports tiled textures as the default (83.94 ms tiled / 114.93 ms linear in
one session). The owner also reported a forced-shutdown HOME-exit hang. Clean
exit is not validated; fixing it takes priority over further performance tests.

All frame/load/menu polling now uses an explicit ProcUI loop. It does not return
a drawable frame during release/background, and does not shut ProcUI down on
EXITING until session resources have been released. Audio is shut down before
foreground release and reconstructed, with its ambient loop, on return. Graphics
foreground buffers remain managed by WHB's release/acquire callbacks. After all
session teardown, callbacks are cleared and ProcUI is shut down once. The HBL
HOME-denied callback also requests the same cleanup path. Full HBL/background
compatibility and successful Aroma exit still need hardware verification.

`lifecycle-<timebase>.log` beside the CSV records flushed lifecycle/cleanup
checkpoints, without per-frame logging. Preserve it if HOME hangs again. The
last persistent message announces trace close and SD release; final cleanup and
ProcUI-shutdown messages go to OSReport because the SD file is already closed.
The exact cause of the reported hang is not established by existing evidence.

The in-progress static-world reuse feature defaults **off** for this lifecycle
candidate. D-pad Right toggles it after exit/resume works; the last overlay row
starts `TB` (tiled, reuse bypassed) or `TC` (tiled, reuse enabled). D-pad Left still
changes texture layout. Allow 30 frames for the averaged overlay to settle.
Reuse requires an exact match of eye XYZ, yaw/pitch and player cell XZ. Changing
any key component rebuilds terrain/scenery/water/sky. Restart invalidates the
key. Actors, weapon and simulation always update normally. This only saves work
for unchanged views; continuous movement/rotation still regenerates geometry.
Any future dynamic terrain, lighting or projection needs a new key/invalidation.

Schema 5 adds `static_cache_enabled` and `static_reused`. The summary groups by
requested mode and reports actual hits. Vertex columns count submitted geometry;
`geometry_stream_bytes` counts rebuilt position/attribute streams, excluding reused
static buffers. Requested cache ranges remain in `stream_cache_bytes`.

Host checks (disable LeakSanitizer for the sandbox; ASan/UBSan remain active):

```sh
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined wiiu-ver/perf-probe/tests/lifecycle_test.c -o /tmp/wiiu-lifecycle-test
ASAN_OPTIONS=detect_leaks=0 /tmp/wiiu-lifecycle-test
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined wiiu-ver/perf-probe/tests/static_view_test.c -lm -o /tmp/wiiu-static-view-test
ASAN_OPTIONS=detect_leaks=0 /tmp/wiiu-static-view-test
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined wiiu-ver/perf-probe/tests/atlas_mips_test.c -o /tmp/wiiu-atlas-mips-test
/tmp/wiiu-atlas-mips-test
cc -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined wiiu-ver/perf-probe/tests/tv_layout_test.c -o /tmp/wiiu-tv-layout-test
/tmp/wiiu-tv-layout-test
```

Lifecycle checks exercise foreground → release → background → reacquisition →
exit, audio stop/restore, deferred cleanup, no repeated polling after exit, and
HBL HOME-denied handling. Reuse checks exercise every key component (including
one-ULP changes), nonfinite keys, bypass, restart invalidation and empty buffers.

### Combined static-scene optimization candidate (schema 6)

The owner requested more local optimization work without another hardware trip.
The HOME-exit-v1 candidate has **not been tested on hardware**; no new lifecycle
log exists yet. Its fixes and tracing are retained, and successful shutdown is
still pending. The combined candidate enables static-view reuse by default.

Changes apply without reducing population, view distance, output size or detail:

- Unchanged camera/player-window keys reuse terrain/scenery/water/sky GPU streams.
  Animated actors, weapon and simulation still update. D-pad Right toggles reuse;
  `TC` means tiled/cache enabled and `TB` means tiled/cache bypassed.
- Terrain conservative bounds are precomputed for the static map at load time.
  A 256-entry height-span radius table avoids repeated square roots. Bounds cost
  8 MiB per loaded map and are retained across hunt restarts on the same map.
- Rotated terrain atlas UVs are precomputed per texture/quarter-turn/corner.
  Visible cells project four unique corners once, then emit the original six
  triangle vertices with the same topology, UVs, lighting and clip projection.
- Scenery scans cache the occupied cells in the current 149x149 window in the
  original near-first order. The candidate list rebuilds when the player crosses
  a map-cell boundary. Per-object culling/cache use remains camera-dependent.
  This avoids revisiting every empty cell while turning or moving within a cell.
  The list contains at most 22,201 indices (under 87 KiB on the Wii U).

Schema 6 metadata identifies terrain precomputation and scenery candidate lists;
frame columns are unchanged from schema 5. The summary supports schemas 1–6.
No new physical-console FPS result is claimed. Static-view reuse mainly helps
unchanged views; terrain/candidate precomputation also applies during motion.
D-pad Left still toggles tiled versus linear atlases for reference comparisons.

Original-data regression coverage compares every emitted float/count against
hashes captured from the previous renderer at 20 views on each of five maps
(100 views total, including edges, landing, rotations and pitch extremes).
It also checks output capacity sentinels and independently verifies scenery
candidate ordering against rectangular enumeration sorted by ring/row/column.
Run with your original data so this test actually exercises all maps:

```sh
C2_ORIGINAL_HUNTDAT="$PWD/source/c2-original/HUNTDAT" cargo test --offline --release --manifest-path pc-ver/Cargo.toml -p c2-core --no-default-features --features portable-hunt --test wiiu_static_scene
```

These checks, C sanitizer checks, Rust release build and Wii U RPX build pass.
Expected terrain-stream hashes depend on the original stock maps/resources.
Generated CAR/RSC/MAP data is not bundled with the tests.

### Resident scenery window candidate (schema 7)

The [static-scene hardware capture](hardware-logs/2026-10-29-static-scene-results.md)
confirms working system-menu exit and an average 33.48 ms for unchanged views.
Rebuilt views still average 77.22 ms, including 22.02 ms scenery generation/copy.
The next candidate targets this rotation-dependent scenery cost.

Scenery geometry for all directions within the current 149x149 window is written
once into the existing position/attribute GPU streams. Every changed view culls
object bounds exactly as before, then submits visible contiguous ranges in the
same near-first order. Invisible gaps are not merged or drawn. Vertex data and
cutout ordering match the old stream. No shader, population, view-distance or
resolution change is involved.

The resident window is rebuilt at cell changes, restart and residency toggles.
It uses the scenery buffers, 1,048,576 vertices since 2026-10-30 (every original
window fits; AREA4 peaks at 742,053). Preflight rejects an
oversized or overflowing total before writing anything; dense windows use the
old visible-stream path for that anchor. Initial/cell-crossing costs and extra
draw calls need measurement. Resident metadata and the bounded range list add
under 1 MiB. No additional GPU geometry allocation is made. Terrain continues
using the precomputed CPU path and exact-view cache.

**PLUS** on GamePad/Pro Controller toggles resident scenery. The overlay starts
`TCR` for tiled/cache/resident, `TCF` for tiled/cache/dense-window fallback, or
`TCB` with residency bypassed. D-pad Left and Right retain their atlas/cache A/B
controls. To compare residency, leave atlas and exact-view cache on, rotate in
place in each PLUS mode, then move across cell boundaries. Stationary views
already benefit from exact-view reuse in both modes. Do not reduce population
or distance to compare versions.

Schema 7 adds `resident_enabled`, `resident_active`, `scenery_ranges` (per view),
and `scenery_uploaded_v`. The summary groups by requested residency and reports
active frames, range counts and uploaded vertices. Submitted scenery vertices
still count visible geometry; rebuilt-stream bytes count actual resident uploads
or legacy writes. Capture ranges that fall back as well as resident windows.

Additional original-map regression (100 scenery views plus the existing 100
terrain views) compares all emitted position/UV/light floats and vertex counts
to the old renderer and checks range coalescing and capacity boundaries:

```sh
C2_ORIGINAL_HUNTDAT="$PWD/source/c2-original/HUNTDAT" cargo test --offline --release --manifest-path pc-ver/Cargo.toml -p c2-core --no-default-features --features portable-hunt --test wiiu_resident_scenery --test wiiu_static_scene
```

Rust release/RPX builds and these checks pass. Physical-console rendering,
rotation/cell-crossing costs, fallback behavior and draw-call overhead are not
yet validated for this candidate. The working static-scene-v1 package is retained.
