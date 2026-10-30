# Resident terrain/water hardware result

Build `carnivores-resident-terrain-v1.wuhb` (SHA-256
`e1245536f93dac17b7fe75743f0f0c382c0705c06bfd78d6bd061e27e19da725`), AREA4
(Manya Jungle), all targets, all weapons, seed 2046267735.
[Raw capture](2026-10-30-resident-terrain.csv),
[summary](2026-10-30-resident-terrain-summary.txt),
[startup log](2026-10-30-resident-terrain-startup.txt),
[lifecycle log](2026-10-30-resident-terrain-lifecycle.txt).

Owner's report: about 30 ms standing, ~45 ms turning and walking, 35-50 ms
chasing an Ankylosaurus, then ~100 ms and staying around 80+ after emptying
a magazine at it.

Findings (resident gameplay frames, 1,948):

- TERRAIN 0.2 ms, SCENERY 0.1-0.3 ms, WATER 0 per frame (were ~13, ~0.1 and
  ~4 while turning). The CPU world cost is gone.
- PRESENT (GPU) is now the floor: 21-40 ms per frame, average 30.3.
- ANIMALS reaches 8-10 ms with a large animal close (30k skinned vertices).
- The ~118 ms stretch at 84.9-87.3 s is residency switched off (TCB, PLUS)
  together with the GamePad mirror (MINUS), not the animal: the old CPU
  path. Back on at 87.3 s.
- Full scenery rebuilds every ~13-15 s (54.3, 67.5 and 82.6 s; 330-430k
  vertices, ~100 ms scenery, 130-157 ms frames): the buffer filled with the
  gaps of departed objects. Measuring every cell found a third of AREA4's
  windows (up to 742,053 vertices) never fitted the 524,288-vertex buffer.

## Next candidate: gap reuse, larger scenery buffer, weapon taps

`carnivores-scenery-gaps-v1.wuhb`, SHA-256
`a3f51b3b4de757904420be01c7fd03983257d5c1601369d7573919fa4f52e1dd`, on the SD.
Entering scenery reuses the gaps of departed objects; the buffer holds
1,048,576 vertices, so every original window fits (host test: 6,400 jungle
crossings, one rebuild). Tapping a weapon on the GamePad now selects that
weapon (the slot bits were dropped; it always drew the pistol).
