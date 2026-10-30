# Wii U hardware review and optimization worklist

Recorded 2026-10-29 after the first confirmed playable hunt on a physical Wii U.
This is an audit and work plan; the performance and gameplay fixes below have
not been implemented by this review.

## Hardware baseline and evidence

- Aroma, launched from `SD:/wiiu/apps/carnivores-probe/carnivores-probe.wuhb`.
- Working artifact: 438,932 bytes, SHA-256
  `9ddfdafa8157ed7b9d9cd320e9ac17aa67cebfdd37e681917a1e90fb63ff8cad`.
- Startup trace v3. This includes the display handoff fix and the temporary
  two-second blue TV / magenta GamePad check. The owner confirmed both colors,
  menu navigation, and playable gameplay. Preserve the `OSScreenEnableEx`
  calls after GX2 initialization when removing the temporary color check.
- All 102 SD assets were previously compared against `source/c2-original/HUNTDAT`
  and matched. The current log records successful menu reads/decodes, all 22
  CAR reads (15 animals, six weapons, hunter), calls, HUD pictures, and effects,
  ending at `Hunt loading complete; releasing SD client`.
- Owner reports gameplay **definitely below 30 FPS**. This is an observed
  estimate, not a captured frame-time measurement. Exact map, selected roster,
  TV output mode, GamePad view mode, and performance-overlay numbers were not
  recorded for this run.
- Preserved raw evidence: [working startup v3](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-playable-startup-v3.txt)
  and [black-screen v2](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-black-screen-v2.txt).
  The console clock dates on SD files are unreliable; these filenames use the
  workstation capture date.

The black-screen v2 log advanced past 9,000 menu frames with corresponding
swap/flip progress while both screens stayed black. The v3 change re-enabled
TV and GamePad after `WHBLogConsoleFree`/`OSScreenShutdown`; hardware then
displayed the color check and menu. This establishes the display handoff fix
as hardware-confirmed. Returning from a suspended/backgrounded app is a
separate lifecycle case and has not been established by this result.

## What the logs can and cannot tell us about performance

There are **no gameplay timing samples in the captured SD log**. In
[main.c](../wiiu-ver/perf-probe/main.c), `startup_trace_close` runs before the
gameplay loop and its `Perf` measurements. The existing 30-frame timing
averages are rasterized into the TV overlay by `perf_update`; they are not
written to SD. Menu frame/flip counts do not measure hunt performance.

Therefore, no terrain/scenery/AI/GPU time or ranked measured bottleneck can
honestly be reported from this run. The following candidates are established
code costs; their relative hardware impact needs measurement.

| Candidate | Evidence in the current implementation | First optimization to evaluate after capture |
| --- | --- | --- |
| Terrain CPU generation and uploads | `c2_terrain_vertices` in [lib.rs](../wiiu-ver/perf-probe/src/lib.rs) visits a 148×148 window (21,904 cells) every frame, calculates height bounds/culling, and projects/emits visible triangles on the CPU. Capacity is 131,424 vertices; position and attribute streams together can hold about 4 MiB. | Cache static terrain chunks; keep world positions/UVs/light resident and move camera projection into GX2, following the existing scenery transform approach. Retain culling and measure submitted vertices. |
| Static scenery CPU scan and copying | `c2_scenery_vertices` scans rings through 74 cells and copies cached visible triangle streams to dynamic buffers every frame. The 32 MiB cache avoids rebuilding object geometry, but does not make it GPU-resident. Scenery position/attribute buffers reserve 16 MiB combined. Cache eviction searches the chunk array. | Measure warm versus cold caches, copied bytes, misses and evictions. Submit resident chunk buffers and cull chunk bounds; avoid repeated full triangle copies and repeated eviction scans. |
| Whole-map CPU updates/cache maintenance | Each frame restores/saves/tints a 151×151 area (22,801 texels per pass), repaints radar, then invalidates the entire 1024×1024 RGBA map texture (4 MiB). This work runs even when the map is not shown. | Draw the view cone/player/radar as GPU overlays over an unchanged base map. Skip hidden-map work and use an independent map update cadence. A 4 MiB invalidation range at 30/60 FPS is nominally 120/240 MiB per second of covered ranges, not a measured memory-transfer rate. |
| CPU/GPU serialization | Explicit `GX2DrawDone` before rewriting geometry, another in the map branch, GX2R buffer locks/unlocks, plus `WHBGfxFinishRender`'s GPU completion/flush/presentation work. | Identify waits first, then use multiple dynamic buffers or resident geometry. Preserve synchronization where a resource is genuinely still in use. Measure whole-buffer versus used-range cache maintenance before changing it. |
| Simulation and actor posing | Fixed 16 ms steps can run multiple times per displayed frame, capped at eight catch-up steps. A 66-animal hunt remains active; far calm targets already use staggered updates. `c2_actor_vertices` allocates a visible list, sorts, poses visible animals, then poses the gun in a second call. | Record tick count and actor pose count. Reuse scratch allocations, cache/interpolate poses when safe, and measure scheduling. Rendering slowdown can increase catch-up work; do not mistake that secondary cost for the original bottleneck. |
| GPU rendering/output size | The world is drawn on TV, and also on GamePad when its gameplay view is enabled. WHB chooses TV render size from the system mode, including 1920×1080. Large linear RGBA atlases and scenery overdraw can be costly. | Log actual TV/DRC render sizes and view modes; compare map versus mirrored GamePad and 720p versus 1080p output. Profile before changing resolution, scene distance, or population. |

The 66-animal population and 74-cell view distance are the baseline. First
seek savings from redundant CPU work, uploads, and waits. Any later quality
tradeoff should have a separate measured comparison instead of silently
changing the test workload.

## First performance task: persist meaningful hardware samples

Extend profiling before selecting the first optimization. Use a separate
`performance.csv` or equivalent bounded gameplay log; do not flush every frame
or route it through the startup console. Buffer samples and write batches at
a low cadence, while timing the logging overhead separately. SD I/O must not
be mistaken for simulation or renderer work. Store the latest useful samples
even when HOME exits; the current startup file closes too early for this.

Capture at least:

- Build identity, session/seed, map, selected animals/weapons, population,
  display sizes, GamePad map/gameplay mode and TV full-map state.
- Wall-clock interval, frame count, average FPS, average frame ms, p95/p99 or
  max frame ms, and number of simulation ticks per frame/sample.
- Existing CPU buckets: TERRAIN, SCENERY, WATER, SKY, MAP, SIM, ACTORS, DRAW,
  WAIT, PRESENT. Split animal posing from gun posing; split full-frame-map
  work from cache maintenance. Record unaccounted frame time too.
- Submitted terrain/scenery/animal/gun/water vertices, visible/posed animal
  count, bytes rebuilt/uploaded, scenery-cache misses/evictions/occupancy.
- Explicit wait locations, buffer-lock time, and profiler/SD-write overhead.

`WAIT` and `PRESENT` are CPU elapsed times around blocking APIs, not isolated
GPU execution times. PRESENT can include GPU completion and display pacing.
Do not add an overlapping GPU timer to CPU totals or conclude that a large
PRESENT value alone proves GPU saturation. Existing ACTORS combines animals
and gun geometry; its vertex count currently excludes the gun. The overlay
refresh and some HUD/input work are not separately bucketed.

Use a reproducible route/seed and record a stationary interval, rotation in
dense scenery, movement into uncached scenery, nearby animals, and gun
draw/fire/reload. Repeat with GamePad map and mirrored gameplay. Compare cold
and warm caches. Use equal workloads for before/after comparisons. For
reference, 30 FPS allows 33.33 ms per displayed frame and 60 FPS allows
16.67 ms; choose the achievable target from actual hardware measurements.

## Necessary gameplay and presentation fixes

### GamePad gun taps always select the handgun

**Code-confirmed input defect.** The touch handler encodes the carried slot as
`64 | (slot << 16)`. The pending-input path keeps `flags & 4334`, which includes
the select bit (64), but drops the slot bits (`0x70000`). Rust's
`c2_move_tick` then decodes slot zero. For example, a tap on slot two becomes
`64`, so the receiver selects the first carried gun. It selects the handgun
when that is the first carried weapon; it is not intrinsically hardcoded to
the handgun. The duplicate call-payload handling in that block does not
preserve the weapon payload.

Required fix: retain the weapon slot together with its pending selection
event, consume it once at the next simulation tick, and define last-selection
wins behavior for multiple taps before a tick. Prefer a shared flag definition
or typed pending event over another unexplained mask. Also align touch bounds
with `init_weapon_grid`: the drawn rows occupy approximately y=34..446 and
x=9..171 on the calibrated GamePad, while hit testing uses y/160 and x<187
with a column split at 94. Shared layout geometry should drive both paths.

Acceptance: every nonempty carried subset, correct icon-to-carried-slot
mapping, selecting/holstering the active gun, Y cycling, and taps retained
across frames with no simulation step. Check slot payloads through the C
pending queue as well as the Rust receiver.

### Sniper scope is a small square and does not zoom the world

**Code-confirmed missing renderer integration.** Shared `Hunt::zoom()` in
[game.rs](../pc-ver/crates/c2-core/src/game.rs) supplies stock 3× magnification,
and `scope_cross()` describes the scope crosshair. The Wii U renderer does
not consume either. Terrain/water/sky and the weapon use fixed projection
factors `0.75` and `1.333`; GPU scenery/animals use matching fixed factors in
[terrain.vert](../wiiu-ver/perf-probe/terrain.vert). The PC weapon path also
widens the optic model for a widescreen display; the Wii U path does not.

Required fix: carry scope/zoom state to both world projection paths and their
culling tests, implement the scope frame/mask and crosshair, and handle the
view model's aspect/overlay projection separately. Keep aim/shot direction
aligned with the zoomed view. Merely scaling the small square will not zoom
the world. Validate draw, ready, shot, reload, holster, weapon switching,
death, and TV/GamePad gameplay views against shared stock behavior.

### Gun interiors and reload shells overlap the exterior

**Strong code-supported explanation; visual confirmation still required.**
Every gun face is emitted in mesh order, GPU culling is disabled, and the
weapon pass explicitly disables depth testing/writing. Its projected vertices
also all use clip Z=0. Consequently triangles have no reliable self-occlusion:
internal/reload geometry can paint over the visible barrel/receiver. This
matches the owner's double-barrel example. The successful CAR reads do not
support treating this as missing gun files.

Required fix: provide meaningful view-model depth, clear or otherwise separate
weapon depth from world depth, and depth-test/write opaque gun surfaces.
Simply enabling depth while retaining identical Z=0 is insufficient. Respect
face sidedness, cutout/transparent flags, winding and near clipping rather
than enabling global backface culling for every model. Audit texture UV/alpha
handling and pose/animation selection separately; those may contribute, but
this run does not establish bad textures or bad source animation frames.

Acceptance: compare all six guns at idle/draw/fire/reload/holster, especially
the double-barrel's shells hidden during idle and visible at the proper reload
phase. Check that nearby world geometry does not occlude the view model.

### TV menu is grainy

**Code-confirmed resolution limit.** All 22 pictures are resampled into
512×256 RGBA buffers by `c2_menu_picture`. `run_menu` uses a 512×256 texture,
the same software rasterization for both displays, and point sampling. The
original `MENUM.TGA` is 800×600 (16-bit); the stock weapon/equipment menu
pictures are 357×208. Thus title detail is discarded before TV upscaling.
The Wii U menu is based on the 3DS-style layout, but 512×256 is this frontend's
own render surface, not evidence that the TV output itself is running at a
3DS screen resolution.

Required fix: separate logical layout coordinates from render resolution.
Preserve source artwork detail/aspect, render TV text/UI at a suitable HD
resolution (initial candidate 1280×720), and scale/map GamePad touch coordinates
consistently for its 854×480 display. Filtering can improve scaling but cannot
recover detail discarded during decoding. Original art is not native HD;
larger buffers alone cannot invent source detail. Redraw menus on state
changes or use GPU composition so HD software rasterization does not add a
new per-frame CPU bottleneck. Keep TV UI work separate from 3D render-size
tuning.

## Implementation order and completion criteria

1. Keep the confirmed display handoff fix and archive this working baseline.
   Remove the temporary color check only after preserving its diagnostic role
   in a separate debug option.
2. Add persistent, bounded hardware profiling and capture a repeatable hunt.
   Rank measured costs; current source costs are hypotheses about priority.
3. Optimize the largest measured cost first, then repeat the same capture.
   Likely opportunities are map overlays, resident terrain/scenery buffers,
   reduced copying/cache maintenance and fewer pipeline stalls. Do not remove
   synchronization or reduce game population blindly.
4. Fix the touch selection payload and hit bounds, gun depth/sidedness,
   and complete scope integration, each with focused regression coverage.
5. Upgrade TV menu rendering with appropriate source-art scaling and a cheap
   redraw strategy; measure its menu frame cost independently from hunt FPS.

The current deployed WUHB remains the hardware-confirmed playable baseline.
This review changes documentation and preserves evidence; it does not install
an unmeasured optimization or claim the newly reported weapon issues are fixed.

## Implementation progress after this review

The first profiling increment adds bounded, buffered per-frame SD CSV capture
and a host summary tool; see the [capture instructions](../wiiu-ver/perf-probe/README.md#persistent-hardware-timing-capture-schema-1).
It records CPU buckets, separate animal/gun and map-paint/cache work, both
explicit wait sites, simulation steps, vertex/stream counts, seed/settings and
render sizes. The Wii U C/RPX build and a host CSV/close/size-limit check pass.
Scenery-cache statistics, visible/posed counters and isolated buffer-lock timing
remain outstanding. Hardware capture and measured bottleneck ranking remain
outstanding; this is not an optimized or hardware-validated replacement for the
preserved playable WUHB.

The owner subsequently supplied a hardware overlay sample: **180.9 ms/frame
(~5.53 FPS), WAIT 75.3 ms, SCENERY 68.2 ms, TERRAIN 18.2 ms**. The
[full transcription and artifact identity](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-performance-baseline-overlay.md)
are preserved. The reinserted SD still contained the original playable WUHB,
so no CSV evidence was available. These measurements prioritize scenery/buffer
work and explicit waits, without proving GPU saturation. The first candidate
reduces dynamic-stream cache maintenance to written vertex ranges and adds
separate lock/cache subset timings. Explicit waits, workload and visibility
remain unchanged; hardware before/after measurement is pending.

The range-upload candidate now has a [physical-console capture and analysis](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-range-upload-results.md).
The reported overlay improved from 180.9 to 145 ms/frame with different vertex
counts; this is not a controlled before/after result. The capture establishes
1080p TV output, low stream lock/cache costs, substantial scenery generation and
map-branch waits, plus world generation even during full-map frames. Schema 2
cannot separate death playback from ordinary gameplay flags. The next candidate
uses independent map UV buffers to eliminate the shared-buffer synchronization
hazard, skips geometry hidden on both displays and map work hidden on both,
and records death playback separately. Required previous-frame/final completion
is retained. New hardware results are pending.

The map-stream build now has a [schema-3 hardware capture](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-map-streams-results.md):
555 live frames average 112.18 ms, 65 death-playback frames average 130.79 ms.
Map wait is zero, and hidden-map work drops to zero during death playback.
PRESENT is the largest elapsed bucket (52.72 ms live / 66.17 ms death), while
scenery and terrain remain substantial CPU costs. The different target mask and
seed prevent a controlled comparison with the earlier capture. A tiled-atlas
A/B candidate now tests memory layout without a quality reduction; both layouts
are retained for same-session toggling and each conversion is roundtrip-checked
at load time. Its hardware performance and appearance remain unverified.

A [same-session tiled/linear capture](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-tiled-atlas-ab-results.md)
now supports tiled sampling: gameplay averaged 83.94 ms tiled versus 114.93 ms
linear; terrain/scenery workload is stable and PRESENT approximately halves.
Actor/simulation work still differs, so this is not a perfectly fixed workload.
Full-map frames average 16.65 ms with zero world generation. **The owner also
reported a HOME-exit hang requiring forced shutdown.** Further optimization is
secondary to lifecycle repair. The next candidate defers ProcUI shutdown until
resource cleanup, gates rendering on foreground ownership, stops/restores audio
across foreground transitions and records persistent teardown checkpoints.
Static-view reuse is implemented for subsequent testing but defaults off in this
exit-test build. The exact hang cause and successful fix remain hardware-unverified.

The owner explicitly deferred further console trips; HOME-exit-v1 remains
untested. A combined schema-6 candidate now enables exact unchanged-view reuse,
precomputes terrain bounds and rotated UVs, projects four unique cell corners,
and caches occupied scenery cells until the player crosses a cell boundary.
It retains the lifecycle repair/tracing and tiled sampling. The bounds add
8 MiB; no population, visibility or output-resolution reduction is involved.
An original-data regression verifies bit-identical terrain streams against the
previous renderer across 100 views on all five maps and verifies the original
near-first scenery order independently. Host checks and release/RPX builds pass;
performance, appearance during movement and successful HOME shutdown are still
hardware-unverified. See the [combined candidate instructions](../wiiu-ver/perf-probe/README.md#combined-static-scene-optimization-candidate-schema-6).

The [static-scene hardware result](../wiiu-ver/perf-probe/hardware-logs/2026-10-29-static-scene-results.md)
now confirms working system-menu exit and 33.48 ms average on 428 reused-view
frames. 185 rebuilt frames average 77.22 ms, including 13.59 ms terrain and
22.02 ms scenery. Resume after HOME remains separate from confirmed exit.
The next candidate keeps a whole scenery window resident in existing GPU buffers
and culls/submits visible ranges without recopying geometry during rotation.
It preserves culling/order and falls back before upload if the window exceeds
capacity. An original-data regression matches all visible scenery floats/counts
across 100 views on five maps; capacity/range tests and release/RPX builds pass.
Cell-crossing uploads, fallback performance and extra draw-call overhead still
need hardware comparison; no new speedup is asserted.
