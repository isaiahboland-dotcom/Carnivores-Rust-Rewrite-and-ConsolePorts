# First range-upload hardware capture

Owner overlay transcription: FRAME 145 ms, WAIT 0.0, TERRAIN 18.7, SCENERY
36.7, WATER 6.3, SKY 0.0, MAP 3.5, DRAW 0.4, PRESENT 56.5, SIM 17.6,
ACTORS 6.0; latest vertices T 44,016 / S 109,536 / A 8,028.
Approximately 6.90 FPS versus the earlier overlay's 5.53 FPS, but vertex counts
and workload differ, so this is not a controlled 25% optimization result.

SD WUHB SHA-256 matched the range-upload candidate:
`beb5f62804b1f0c381dbfa1a11a94a24f0c7f2825bf780c70a621a5bf512e83c`.
Raw [CSV](2026-10-29-range-upload.csv),
[summary](2026-10-29-range-upload-summary.txt), and
[startup log](2026-10-29-range-upload-startup.txt) are archived.

Capture metadata: schema 2, AREA1, target mask 511, weapon mask 63, radar on,
blood and kill animations on, population 66, seed 1501663636, TV 1920x1080,
DRC 854x480. 484 frames across 50.42 seconds, 9.60 wall FPS including both
map modes. No mirrored-gameplay toggle interval was captured.

347 frames with TV full-map toggle off averaged 121.28 ms (8.25 timed-loop FPS),
p95 154.40 / p99 161.39 / max 166.37 ms. Average buckets: explicit map wait
35.239 ms, scenery 26.146, terrain 17.712, present 15.994, simulation 14.323,
animals 4.350, water 3.260, map painting 2.722, map cache maintenance 0.746.
Dynamic stream locks average 0.004 ms; stream cache work 1.893 ms, covering
4.19 MiB. These lock/cache numbers are subsets of geometry time.

137 frames with the TV full-map toggle on averaged 59.90 ms (16.70 timed-loop
FPS). Terrain 17.296 and scenery 21.335 ms were still generated despite both
displays showing maps (except if overridden by death playback).

Schema 2 records view *toggles*, not the actual death-playback override. It
cannot reliably separate normal play from death playback (which renders the
world on both screens). Thus the toggle-off group's average is not guaranteed
to be exclusively live hunt gameplay. The next schema adds `death_playback`.
WAIT and PRESENT can move between locations; neither independently measures
GPU execution. Their sum and total frame time matter when comparing versions.

Next candidate: independent TV/DRC map UV buffers prepared before render
submission, eliminating the map branch's explicit completion wait. Previous-frame
resource protection and final WHB completion remain. Skip world generation when
neither display draws the world; skip map repaint/cache work when neither draws
the map. No change to population, terrain range, projection, or output resolution.
This removes a buffer hazard rather than simply deleting required synchronization.
Hardware impact remains to be measured; time may move from WAIT to PRESENT.

Map-stream candidate built and deployed to the same SD app path, flushed and
read-back SHA-256 verified: 444,048 bytes,
`213c654c6770ab669c4414e8aa15a5745e99770f2321712567470ef8d25af821`.
Local package: `build/carnivores-map-streams.wuhb`. The prior range-upload
package remains preserved. RPX build, map-view address/undefined sanitizer
checks and stream-upload host checks pass. Physical-console result pending.
