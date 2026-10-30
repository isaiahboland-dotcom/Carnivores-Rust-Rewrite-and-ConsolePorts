# Static-scene hardware result and confirmed HOME exit

Owner confirms returning to the system HOME menu now works. Reported lowest
30-frame average 33.4 ms (~29.94 FPS), occasional roughly 60 ms averages while
turning quickly. Other reported buckets: WAIT 0.0, TERRAIN/SCENERY/WATER/SKY
0.0, MAP 2.8, DRAW 0.9, PRESENT 25.9, SIM 3.7, ACTORS 0.0. Submitted vertices
T 40,596 / S 72,873 / A 0; TC mode. Zero actor vertices means none submitted in
that sample, not that simulation/population was disabled.

[Raw capture](2026-10-29-static-scene.csv),
[summary](2026-10-29-static-scene-summary.txt), and
[lifecycle log](2026-10-29-static-scene-lifecycle.txt) are archived. The SD WUHB
matches static-scene-v1 SHA-256
`b1d87cb21f099740675852aea830fe8a56d6274558b7bde668b1be06e7f9c758`.

Metadata: schema 6, AREA2, all targets (511), all weapons (63), radar enabled,
blood and kill animation disabled, population 66, seed 3401612258, 1920x1080
TV / 854x480 DRC, tiled/cache enabled. 613 live gameplay frames over 28.70 s;
whole-capture wall FPS 21.36, timed-loop mean 46.68 ms, p95 81.05 / p99 83.01 /
max 84.33. The moving/rebuild samples are slower than the transcribed low window.

| Group | Frames | Average frame ms | TERRAIN ms | SCENERY ms | PRESENT ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exact static-view reuse | 428 | 33.477 | 0.000 | 0.000 | 25.653 |
| Rebuild view | 185 | 77.224 | 13.593 | 22.020 | 24.770 |

No GamePad mirrored-gameplay, full-map or death interval in this capture. The
lifecyle trace shows foreground release, audio stop, EXITING, profiler close,
geometry/texture release, graphics shutdown, and simulation release, reaching
the last SD checkpoint before trace close/unmount. Successful system-menu return
is owner-confirmed. Resume after HOME remains a separate unverified case.

## Next candidate: resident scenery window

Keep all nearby static-object triangles in the existing scenery GPU buffers
while the player stays in the same map cell. Rotation only reculls object bounds
and submits visible spans. Merge only contiguous visible spans, preserving
near-first order, exact geometry and culling. A cell crossing/restart/mode switch
rebuilds the resident window. If all directions cannot fit the existing 524,288
vertex allocation, reject before writing and retain the prior visible-stream
renderer until the next anchor change. Dense regions can therefore still have
legacy rebuilding costs. Population, radius and resolution remain unchanged.

No extra GPU geometry buffers are allocated. Metadata/range arrays add under
1 MiB at the maximal 149x149 candidate count. Initial and cell-crossing uploads
can cost more than a warm rotation. More draw calls can offset CPU-copy savings;
this requires hardware measurement, and no moving-view speedup is claimed yet.
PLUS toggles residency for comparisons; D-pad Right controls exact-view caching,
and D-pad Left controls atlas layout. The overlay suffix R means resident active,
F means dense-window fallback, B means residency bypassed.

Schema 7 records requested/active residency, ranges per view, and actually uploaded
scenery vertices. Rebuilt stream bytes include resident preparation uploads,
not the visible vertex extent merely resubmitted by a warm resident frame.

Original-map resident scenery streams match the previous CPU-cache renderer
bit-for-bit across 100 views on all five maps. Terrain's separate 100-view
regression still passes, as do adjacent-visible-range merging, zero geometry,
exact/oversized/overflowing capacity checks, Rust release build and Wii U RPX
build. CSV schema/close/limit host check passes. Hardware behavior is pending.

Resident-scenery-v1 candidate packaged and copied to the standard SD app path.
Local package `build/carnivores-resident-scenery-v1.wuhb`, 454,292 bytes, SHA-256
`911841c6d0be6488bc1ca62c8012fd0cab194560db62d38a6089551c8f101411`.
SD `sync -f` completed and read-back hash matched. The working static-scene-v1
package remains available for rollback. No resident-renderer hardware result yet.
