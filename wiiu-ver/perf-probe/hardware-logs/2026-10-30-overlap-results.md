# CPU/GPU overlap hardware result

Build `carnivores-overlap-v1.wuhb` (SHA-256
`dbab43d1cf679684bdf255357eaf39ca6c66387cb21a376b1e4c0bef8d6605fd`), AREA4 at
dawn. [Capture](2026-10-30-overlap.csv) ([summary](2026-10-30-overlap-summary.txt)),
[startup log](2026-10-30-overlap-startup.txt) with the menu redraw times.

Owner: smoother (fewer drops), no big gain. The capture agrees:

- 1,733 of 2,642 gameplay frames take 32-36 ms: vsync now holds a steady 30
  FPS; ~30% take 36-52 ms (just missing a vsync), a few 64-104 ms.
- CPU work per frame (frame minus the wait for the GPU) median 11.8 ms; the
  wait median 25.8 ms. Beyond 30 FPS the GPU must get faster.
- `gpu_tv_ms`/`gpu_drc_ms` read 0.2/0.1 ms: GX2's bottom-of-pipe timestamps
  are written as the command processor reaches them, not when drawing ends.
  Unusable; the next build turns vsync off on demand instead (D-pad Left).
- Menu redraws on the console: TV 90-530 ms, GamePad 17-105 ms (the PC
  draws the TV in 2.5 ms). Memory-bound: a full 1080p redraw writes and
  blends ~16 MB. The next build redraws only what changed.
- D-pad Left (then the atlas layout) was pressed by accident: 42 frames
  with the linear atlases.

## Next candidate

`carnivores-menu-partial-v1.wuhb`, SHA-256
`90d8bab575d826ccbc7e4c0b5ee065af84bff7325d39e91ee585bbc22680ddfb`: menus
redraw only changed areas (host test: identical to full redraws); touching
hexagons; gear as on/off buttons; the GamePad panel shows the weapon and its
ammo above the guns and the kills above the calls; D-pad Left turns vsync
off/on (overlay V/N, capture column `vsync`, schema 9).
