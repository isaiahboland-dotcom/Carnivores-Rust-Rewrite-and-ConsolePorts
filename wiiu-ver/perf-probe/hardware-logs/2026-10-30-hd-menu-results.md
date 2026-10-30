# HD menu hardware result

Build `carnivores-hd-menu-v1.wuhb` (SHA-256
`72a36e4f14e5dd419991c3bf6608cf9df03cdf439de80aaaed766a370b4c5d88`).

Owner's report: the menu "looked amazing"; button presses took about a
quarter to half a second to show. Gameplay felt much smoother. Night was
solid; dawn looked no different. Going from crouch to sprint, the game
froze and needed a forced shutdown.

- [Night run](2026-10-30-hd-menu-night.csv) ([summary](2026-10-30-hd-menu-night-summary.txt)):
  AREA2, all animals, every weapon and gear.
- [Dawn run](2026-10-30-hd-menu-dawn-crash.csv) ([summary](2026-10-30-hd-menu-dawn-crash-summary.txt),
  [lifecycle](2026-10-30-hd-menu-dawn-crash-lifecycle.txt)): AREA5, four
  herbivores, blood and death camera off; 2,316 resident frames at 35.3 ms
  (28.4 FPS), p95 50.7. The capture stops mid-row at 176 s and the
  lifecycle trace never reached shutdown: a halt (a Rust panic stops in
  `OSFatal`). [Startup log](2026-10-30-hd-menu-startup.txt).

Findings and changes:

- Dawn: `OptDayNight` 0 is dawn (low sun, light maps ~150 against ~225 for
  slot 1, warm skies) but the core numbered it `Day`, so every platform's
  "Dawn" showed full day and "Day" showed dawn. Swapped (DECISIONS).
- The crash did not reproduce on the host (100,000 ticks of crouch+sprint
  over five seeds, including both of the session's). Panics now write their
  place and message to the lifecycle log and the capture is flushed first.
- Sprint from a crouch now stands up and sprints; crouching ends a sprint.
- Menu: no hardware square root, so hexagon edges no longer call `sqrtf`
  per pixel; glyphs are rasterized once and cached. Each redraw's time is
  logged to `startup.log` (`Menu: redraw N took TV ... us`).

## Next candidate: CPU/GPU overlap and GPU timing (schema 8)

`carnivores-overlap-v1.wuhb`, SHA-256
`dbab43d1cf679684bdf255357eaf39ca6c66387cb21a376b1e4c0bef8d6605fd`, on the SD
(also includes the dawn fix, sprint from crouch, panic logging and the
faster menu). `WHBGfxFinishRender` waited for the GPU after every frame, so
CPU (~11 ms) and GPU work ran one after the other. Hunt frames are now
submitted without that wait; the CPU reads input and runs the simulation
while the GPU draws, and waits (WAIT) only before writing GPU data. The
capture adds `gpu_tv_ms` and `gpu_drc_ms` (GPU timestamps of the previous
frame's passes), to tell GPU time from vsync waiting before deciding on the
GPU-side work (a 720p TV render target is the main candidate).
