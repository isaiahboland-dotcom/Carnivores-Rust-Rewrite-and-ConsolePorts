# Physical Wii U baseline: owner-transcribed overlay

Received 2026-10-29. These are one reported 30-frame overlay window, not an
SD timing capture. No route, map, selections, output sizes or view modes were
recorded. Counts are latest-frame values rather than 30-frame averages.

| CPU bucket | Average ms |
| --- | ---: |
| FRAME | 180.9 |
| WAIT | 75.3 |
| TERRAIN | 18.2 |
| SCENERY | 68.2 |
| WATER | 1.1 |
| SKY | 0.0 |
| MAP | 3.5 |
| DRAW | 0.4 |
| PRESENT | 0.0 |
| SIM | 11.4 |
| ACTORS | 2.3 |

Vertices: terrain 41,094; scenery 194,508; actors 1,830 (gun excluded).
Implied average FPS: approximately 5.53. WAIT + SCENERY cover approximately
79.3% of FRAME. WAIT remains CPU blocking time; it does not independently
measure GPU execution or establish GPU saturation. SCENERY includes buffer
locks/unlocks, cache lookup/build/eviction, scanning/culling and geometry copies.

Reinserted SD artifact: 438,932 bytes; SHA-256
`9ddfdafa8157ed7b9d9cd320e9ac17aa67cebfdd37e681917a1e90fb63ff8cad`.
It matches the original hardware-confirmed startup-v3 WUHB, predating CSV
instrumentation. No performance CSV files were present. Startup log archived as
`2026-10-29-performance-baseline-startup-v3.txt`. WUHB archived locally as
`build/hardware-playable-baseline.wuhb` (generated artifacts are ignored by Git).

First test change: range-based dynamic geometry cache maintenance, preserving
explicit GPU completion waits, population, draw distance and vertex generation.
Scenery position/attribute ranges at this vertex count total 6,224,256 bytes
(5.94 MiB), versus 16 MiB allocated. This is a reduction in cache-covered ranges,
not a measured transfer saving or established FPS improvement.

Candidate: `build/carnivores-range-upload.wuhb`, 443,696 bytes; SHA-256
`beb5f62804b1f0c381dbfa1a11a94a24f0c7f2825bf780c70a621a5bf512e83c`.
Wii U C/RPX build and host stream-boundary tests (range and legacy paths) pass.
Hardware result remains pending.

Candidate copied to `SD:/wiiu/apps/carnivores-probe/carnivores-probe.wuhb`,
flushed with `sync -f`, and read-back SHA-256 matched the local candidate above.
Only the executable was replaced; the 102 SD assets were retained.
