# Wii U hardware findings

These findings summarize physical-console experiments with a native WUT/GX2
frontend linked to shared Rust game logic. Raw captures and session metadata
are intentionally excluded. Frame times below are elapsed durations, not
capture timestamps. Results describe experimental builds rather than a
performance guarantee for the current port.

## Measurement limits

CPU section timers include submission, synchronization, and resource-management
costs. A presentation or completion wait is not a direct measurement of GPU
execution. Work can move between wait buckets without becoming faster.
Compare total frame time, actual display modes, geometry counts, and simulation
work together. Different scenes and animal activity prevent treating successive
builds as a controlled benchmark series. Emulator results do not establish
physical-console performance.

## Display handoff and lifecycle

A black screen persisted even though rendering and presentation counters kept
advancing. Re-enabling both displays after the loading console shut down and
GX2 initialized restored visible output. Successful submission alone did not
prove scanout was enabled.

A later build hung when returning to the system menu. Code review identified
resource teardown after ProcUI shutdown and insufficient gating of foreground
resource use. An explicit lifecycle loop moved resource cleanup before ProcUI
shutdown and suspended foreground work while released. System-menu return was
subsequently confirmed. The exact original blocking call was not established;
resume after foreground release requires its own validation.

## Map buffers and synchronization

Separate immutable map positions and per-display UV buffers eliminated an
explicit map completion wait. Preparing both display buffers before submission
removed the write-after-use hazard. The measured map-wait bucket fell to zero;
remaining waits could appear in presentation instead. Required protection of
in-flight resources was retained.

Skipping world generation when both displays showed maps eliminated submitted
world geometry on that path. Skipping hidden map updates also removed map work
during death playback. Benchmark grouping must use the actual rendered views,
since death playback can override normal map/gameplay selections.

## Tiled texture atlases

A same-session comparison measured approximately 84 ms per gameplay frame with
tiled atlases and 115 ms with linear atlases. Terrain/scenery CPU work was
similar, while presentation blocking was approximately 26 ms versus 53 ms.
Animal and simulation costs differed, so the comparison does not isolate a
perfectly fixed workload. It supports tiled storage as the preferred layout,
without establishing a universal speedup. Conversion roundtrip checks provided
a separate check of stored texel correctness.

## Static geometry and residency

Exact-view reuse measured approximately 33 ms per frame versus 77 ms for
rebuilt views in one experiment. This benefits an unchanged camera; it does
not establish movement performance.

Keeping scenery resident reduced its CPU cost while turning from roughly
20–22 ms to approximately 0.1 ms. Whole-window regeneration at cell crossings
still produced roughly 106 ms frames. Keeping terrain and water in world-space
GPU buffers and updating only entering rows/columns removed most of their
per-frame CPU generation cost: terrain measured approximately 0.2 ms and water
approximately zero in a subsequent experiment. These measurements came from
different workloads and are not a controlled aggregate speedup.

Appending entering scenery without reusing departed objects' gaps eventually
filled the buffer and triggered expensive full rebuilds. Some dense windows
also exceeded the earlier capacity. Gap reuse and a larger buffer addressed
those failure modes; host traversal tests supported the allocation strategy.
Those host results alone do not prove stutter-free console gameplay.

## Frame overlap and remaining rendering costs

Allowing CPU simulation and input work to overlap the preceding GPU frame
produced many frames near 33 ms, with slower dense views. CPU work excluding
the GPU wait had a median near 12 ms in one experiment. Residual blocking and
dense-scene costs motivated GPU-side investigation, but vsync and presentation
must be separated before assigning a precise GPU execution budget.

Attempted pass timestamps returned implausibly short intervals and were not
accepted as reliable GPU draw timings. Large nearby animated models also
increased CPU skinning cost; world residency does not eliminate actor work.

## Menu redraws

Full-resolution CPU menu redraws caused noticeable input-to-display latency.
Caching glyphs, avoiding repeated per-pixel square roots, and redrawing only
changed regions reduced work. Measured TV focus updates were approximately
59–62 ms, selection updates 130–185 ms, and an initial full redraw roughly
534 ms in one build. Partial redraws improved responsiveness but did not make
all menu operations fit a single display frame. Host comparisons checked that
partial redraw output matched full redraw output.

## Apparent regressions and gameplay defects

Severe slowdowns initially associated with nearby animals coincided with a
diagnostic control disabling geometry residency; another interval also enabled
mirrored gameplay. Rebuilding scenery every frame explained those intervals
better than attributing the entire regression to an animal. Diagnostic mode
changes must be recorded and should not conflict with normal controls.

Investigation also exposed a dawn/day slot mismatch and missing weapon-slot
bits in touch selection. Those were code defects rather than performance
limits. A crouch-to-sprint freeze did not reproduce in extensive host ticking;
additional panic diagnostics and revised state transitions did not by themselves
establish its original cause or prove a console fix.

## Follow-up validation

Compare stationary, turning, walking, dense-scene, nearby-animal, and weapon
workloads with consistent settings. Exercise TV-only, GamePad-only, map, mirrored
views, death playback, restart, and lifecycle transitions separately. Validate
texture filtering and mip levels visually as well as measuring performance.
Retain raw evidence privately and publish only aggregate technical conclusions.
