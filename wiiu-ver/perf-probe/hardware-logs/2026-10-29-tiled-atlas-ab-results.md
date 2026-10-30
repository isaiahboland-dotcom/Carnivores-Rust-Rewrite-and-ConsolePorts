# Tiled/linear atlas hardware A/B and HOME-exit failure

Owner overlays at the same terrain/scenery counts (T 37,848 / S 71,760):

| Layout | Frame ms | Implied FPS | PRESENT ms | TERRAIN ms | SCENERY ms | SIM ms | ACTORS ms | Actor vertices |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Tiled | 85.6 | 11.68 | 26.0 | 16.6 | 25.2 | 5.7 | 7.8 | 21,144 |
| Linear after D-pad Left | 113.9 | 8.78 | 52.7 | 16.6 | 25.2 | 6.9 | 8.2 | 24,804 |

The tiled overlay has 24.8% lower frame time / 33.1% higher implied FPS. Actor
work differs, but terrain/scenery and their CPU buckets match, and most saving
appears in PRESENT. This is stronger same-session evidence than previous runs.

The owner also reports a hang on "returning to home menu", requiring forced
shutdown. This build is **not clean-exit validated**. No precise blocking call
was recorded by the old build, so the root cause is not established. Profiling
CSV on SD remains readable despite the forced shutdown.

SD WUHB matched the A/B candidate SHA-256
`33634c8d947d83dfe04fc8c8690c794c1ed8199bd59d81a3f3842795c1732641`.
[Raw CSV](2026-10-29-tiled-atlas-ab.csv),
[summary](2026-10-29-tiled-atlas-ab-summary.txt), and
[startup log](2026-10-29-tiled-atlas-ab-startup.txt) are archived.

Metadata: schema 4; all tiled conversions roundtrip-validated (`tiled_available=1`),
AREA4, target mask 23, all weapons (63), seed 1620581069, 66 animals, TV
1920x1080 / DRC 854x480. No death playback interval. 711 frames over 64.22 s.

| Group | Frames | Mean ms | Timed-loop FPS | p95 ms | PRESENT ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Tiled gameplay | 459 | 83.94 | 11.91 | 100.52 | 26.454 |
| Linear gameplay | 217 | 114.93 | 8.70 | 118.49 | 52.875 |
| Full map | 35 | 16.65 | 60.07 | 16.86 | 3.549 |

Gameplay terrain/scenery means are about 16.7/25.2 ms in either layout.
Animal means differ (5.78 tiled / 8.52 linear) and simulation differs because
slower display frames produce more fixed-step catch-up. This is not a perfectly
static total workload. Tiled is retained as the default; keep the reference
layout toggle for further comparisons. Full-map frames submit zero world
vertices and spend zero time generating world geometry, confirming that path.

## Exit investigation and candidate

The prior code used WHBProcIsRunning, whose documented implementation calls
ProcUIShutdown before returning false on EXITING, then performed the entire
SD/audio/GX2/simulation cleanup. It also returned a running state after the
foreground-release acknowledgment without explicitly gating the next render on
reacquisition. The [ProcUI lifecycle documentation](https://github.com/devkitPro/wut/blob/master/include/proc_ui/procui.h)
requires resource release before shutdown and forbids foreground-resource use
in the background. These are code-supported lifecycle issues, not a proven
identification of the owner's exact hang.

The candidate uses an explicit blocking ProcUI loop: it yields through release
and background states, stops audio on release and restores it (including ambient
loop) on reacquisition, and returns false on EXITING without shutting ProcUI down.
Session cleanup closes profiler, stops audio, frees renderer/simulation resources,
then closes the lifecycle trace and releases SD. Main clears stale callbacks and
shuts ProcUI down only after session teardown. Redundant post-loop GPU waits are
removed because each submitted frame already completed in WHBGfxFinishRender.
An existing weapon-audio leak is fixed by freeing all 26 clip categories.

A separate `lifecycle-<timebase>.log` flushes transition and teardown checkpoints,
so a repeated hang can be narrowed to profiler, audio, graphics or later cleanup.
The final SD checkpoint precedes trace close/FS-client removal/unmount; later
completion markers go to OSReport. Absence of the last marker cannot distinguish
trace close, FS cleanup or subsequent ProcUI shutdown by itself.

Static-view reuse was being developed when the crash was reported. Its default
is **off** in this exit-test build. The code can reuse unchanged static streams
using an exact eye/yaw/pitch/player-cell key; actors and simulation are dynamic.
D-pad Right toggles it, but first test HOME exit/resume without enabling it.
Schema 5 records requested reuse and actual hit status, separating rebuilt bytes
from submitted vertex counts. This optimization is not hardware-validated.

HOME-exit-v1 candidate packaged as `build/carnivores-home-exit-v1.wuhb`,
449,040 bytes, SHA-256
`094f9086739effac90bc29d9a154af42f35184a854111e90788afa4c8a577f7f`.
Installed to the standard SD app path, `sync -f` completed, read-back hash matched.
Rust release and RPX builds pass. Lifecycle/static-view/map-view sanitizer checks,
stream-range checks and schema-5 CSV/close/limit host check pass. Tiled sampling
is enabled if roundtrip checks pass; static reuse defaults off. Successful
HOME overlay/resume/system-menu exit and the cause of any remaining hang still
require physical-console verification.
