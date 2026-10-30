# Map-stream hardware capture

Owner reported starting overlay FRAME 114.2 ms, WAIT 0.0, TERRAIN 16.5,
SCENERY 25.0, WATER 0.6, SKY 0.0, MAP 3.5, DRAW 0.3, PRESENT 58.0,
SIM 7.9, ACTORS 1.9; vertices T 37,176 / S 70,665 / A 5,640.
After a T. rex attack, reported FRAME 152.8, WAIT 0.0, TERRAIN 18.7,
SCENERY 26.3, WATER 2.1, SKY 0.0, MAP 0.0, DRAW 0.4, PRESENT 78.0,
SIM 20.9, ACTORS 6.3; vertices T 47,184 / S 63,795 / A 7,656.
These imply approximately 8.76 and 6.54 FPS, respectively.

The SD executable matched the map-stream candidate SHA-256
`213c654c6770ab669c4414e8aa15a5745e99770f2321712567470ef8d25af821`.
Raw [CSV](2026-10-29-map-streams.csv),
[summary](2026-10-29-map-streams-summary.txt), and
[startup log](2026-10-29-map-streams-startup.txt) are archived.

Metadata: schema 3, AREA1, target mask 257, weapon mask 63, radar on, blood
and kill animation enabled, population 66, seed 4112966559, TV 1920x1080,
DRC 854x480. This differs from the prior run's all-target mask and seed.
620 frames across 70.95 seconds, 8.74 wall FPS. No full-map or GamePad
gameplay-toggle intervals were captured; hidden-full-map savings remain untested.

| Group | Frames | Average ms | Timed-loop FPS | p95 ms | p99 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Live play | 555 | 112.18 | 8.91 | 124.68 | 127.31 |
| Death playback | 65 | 130.79 | 7.65 | 162.75 | 168.59 |

Live-play averages: PRESENT 52.723 ms, SCENERY 23.384, TERRAIN 17.007,
SIM 8.467, animals 4.912, gun 0.432, map paint 2.718, map cache 0.747.
Map wait is zero; geometry wait 0.006 ms. Stream lock/cache subsets average
0.003/1.725 ms and cover 3.71 MiB. Death playback: PRESENT 66.170 ms,
SIM 16.875, SCENERY 22.990, TERRAIN 17.668, animals 5.313; map work is
essentially zero. This confirms the map synchronization wait was removed and
hidden-map work is skipped during death playback.

Both displays render the world during death playback, unlike TV gameplay plus
GamePad map during ordinary play. This is consistent with higher blocking in
PRESENT; it is not an isolated GPU measurement or proof of one GPU bottleneck.
Simulation catch-up averages 6.96 ticks/live frame and 7.25 ticks/death frame;
nearby animal behavior and the different camera/geometry also change costs.
There is no evidence here of a persistent post-restart slowdown: this capture
ends during death playback. The earlier run's different selection, seed and
missing death-state labels prevent a controlled cross-build FPS claim.

Next experiment: retain linear atlases and create tiled terrain/scenery/actor
copies using GX2CopySurface. Compare layouts within one session using D-pad Left,
with schema 4 grouping by tiled state, death state and view toggles. Copy each
tiled atlas back to temporary linear storage at load time and compare all texels;
if validation or allocation fails, retain the linear renderer. This tests whether
layout improves frame/PRESENT time without reducing output size, detail or geometry.
Dual reference/candidate atlases add approximately 40 MiB persistent memory for
this experiment, with a temporary readback allocation during loading. No speedup
or correct hardware sampling is established until the next console run.

Tiled-atlas A/B candidate built and installed to the standard SD app path;
`sync -f` completed and read-back hash matched. Package
`build/carnivores-tiled-atlas-ab.wuhb`, 445,260 bytes, SHA-256
`33634c8d947d83dfe04fc8c8690c794c1ed8199bd59d81a3f3842795c1732641`.
RPX build, schema-4 CSV/close/limit host check, map-view sanitizer check and
stream-range host check pass. GPU roundtrip validation runs on the next launch;
physical-console sampling/appearance/performance are still pending.
