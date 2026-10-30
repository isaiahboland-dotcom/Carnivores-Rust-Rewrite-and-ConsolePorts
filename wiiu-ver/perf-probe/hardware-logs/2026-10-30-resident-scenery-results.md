# Resident-scenery hardware result

Owner-reported overlay sample: 33.4 ms average, WAIT 0.0, TERRAIN/SCENERY/WATER/SKY
0.0, MAP 2.8, DRAW 3.8, PRESENT 22.9, SIM 3.4, ACTORS 0.4; TCR (resident
active), vertices T 38,256 / S 56,367 / A 1,830. This is the standing-still
(exact-view reuse) window, the same 33.4 ms the static-scene build reported.

[Raw capture](2026-10-30-resident-scenery.csv),
[summary](2026-10-30-resident-scenery-summary.txt),
[startup log](2026-10-30-resident-scenery-startup.txt) and
[lifecycle log](2026-10-30-resident-scenery-lifecycle.txt) are archived. SD WUHB
SHA-256 `911841c6d0be6488bc1ca62c8012fd0cab194560db62d38a6089551c8f101411`.

Metadata: schema 7, build Oct 30 2026 02:31:16, AREA2, 511 targets, 63
weapons, population 66, seed 2087490667, 1920x1080 TV / 854x480 DRC. 5,773
frames over 256 s; HOME exit trace reaches the last SD checkpoint again.

Gameplay frames (TV map off, GamePad mirror off), split by what the frame did:

| Group | Frames | Avg ms | p95 ms | TERRAIN | SCENERY | WATER | PRESENT |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Resident, view reused | 2223 | 30.9 | 40.9 | 0.0 | 0.0 | 0.0 | 20.0 |
| Resident, turning (no upload) | 2470 | 52.3 | 67.2 | 12.9 | 0.1 | 3.8 | 22.2 |
| Resident, cell crossing (upload) | 398 | 106.1 | 118.7 | 13.8 | 48.2 | 4.4 | 24.9 |
| Old path (PLUS bypass), reused | 25 | 33.0 | 33.7 | 0.0 | 0.0 | 0.0 | 22.7 |
| Old path (PLUS bypass), rebuilt | 21 | 73.6 | 77.4 | 13.5 | 19.7 | 7.0 | 22.1 |
| Previous build (10-29), rebuilt | 185 | 77.2 | 82.6 | 13.6 | 22.0 | 1.7 | 24.8 |

Findings:

- Residency works for rotation: scenery while turning drops from ~20-22 ms
  to 0.1 ms, and turning frames go from ~75 ms to ~52 ms (~13 to ~19 FPS).
- Turning is now bound by TERRAIN (~13 ms) plus WATER (~4 ms) on top of the
  ~22 ms PRESENT floor. Terrain is the next residency candidate.
- Cell crossings rebuild the whole resident window (~190k vertices, max
  223k): 48 ms scenery, 106 ms frames. 351 crossing events, 300 single-frame
  and 51 two-frame, so walking produces a visible hitch every crossing.
  Spreading the window rebuild over several frames, or shifting only the
  entering/leaving cell rows, would remove most of it.
- PRESENT (~20-25 ms) is unchanged and caps standing-still frames near 30 FPS;
  it remains the floor no matter what the CPU stages do.
- No dense-region fallback (F) frames occurred in this capture.
- GamePad mirrored gameplay intervals were short (14 and 27 frames); not
  enough to draw conclusions.
