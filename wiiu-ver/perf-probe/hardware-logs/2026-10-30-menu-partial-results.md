# Partial menu redraws and touching hexagons: hardware result

Build `carnivores-menu-partial-v1.wuhb` (SHA-256
`90d8bab575d826ccbc7e4c0b5ee065af84bff7325d39e91ee585bbc22680ddfb`), AREA4
(Manya Jungle) at dawn, every animal and weapon, seed 4115460698.
[Raw capture](2026-10-30-menu-partial.csv),
[summary](2026-10-30-menu-partial-summary.txt),
[startup log](2026-10-30-menu-partial-startup.txt),
[lifecycle log](2026-10-30-menu-partial-lifecycle.txt).

Owner's report: the new menu "was awesome"; the game got really laggy once a
Stegosaurus came within the map circle.

Findings:

- The lag at 127.7 s is residency switched off (`resident_enabled` 1 to 0:
  the PLUS diagnostic, pressed looking for a pause or exit button), not the
  animal: every frame then rebuilt the scenery on the CPU (terrain ~13 ms,
  scenery ~77 ms, ~135 ms frames) for the rest of the capture. The next build
  makes PLUS the exit prompt and drops the toggle.
- Before that, dense jungle at dawn ran 43-50 ms frames, 29-34 ms of it
  waiting for the GPU with 150-235k scenery vertices in view (the earlier
  overlap capture saw ~33 ms in lighter views). The GPU is the floor; vsync
  was never switched off, so the true GPU time is still unmeasured.
- Menu redraws (startup log): TV 59-62 ms for a focus move, 130-185 ms for a
  pick (the content area redraws), 534 ms for the first full draw; were
  90-530 ms each.

## Next candidate: trophies, kill notice, survival, trophy room, exit prompt

`carnivores-trophies-v1.wuhb`, SHA-256
`d4f145664e33ad298bcae1fae5bb290dbac9c806b6690d57194163019757b5ab`, on the SD
with `TROPHY.MAP`, `TROPHY.RSC`, `TROPHY_G.TGA` and `TROPHY.TGA`.
