# Predation performance

What predation (`c2-core/src/dino/predation*`) costs per step at 20, 40 and
60 animals, how it is measured, and the results per platform. Background:
DECISIONS "Predation phase 3: one scan per animal".

## What is measured

`Predators::think` re-picks every animal's goal every 300 ms. It used to walk
the herd up to five times per animal (being charged, predator, rival, prey,
body), with a square root per pair in `senses()`: about 5·n² pair checks per
think. It now makes one pass per animal (`predation/scan.rs`). Each pair's
relations are checked once, squared distances pick the nearest per kind,
and the square root is taken only for animals within reach of the senses.
The goals picked are the same (see Determinism).

The bench (`predation/bench.rs`, shared by every platform):
- `fill` grows or trims the main population (AI 10 and up) to a count,
  copying the species mix already placed, at spots chosen as
  `PlaceCharacters` chooses them (map centre, at least 40 cells from the
  hunter).
- `measure` steps the herd headless with the hunter away (20 ms steps) and
  times every `Herd::step`. Every 300 ms it also times `think()` alone, on a
  copy of the herd so the run itself is not disturbed.
- Each count runs twice, predation on and off. With predation on, the mean
  step is *lower* than with it off: killed animals cost almost nothing to
  step, and a 60-animal herd loses 20–30 of them in 5 minutes.

Worst-step figures on a desktop OS include scheduler noise (single outliers
of 0.5–1.2 ms with or without predation). The `think` worst is the figure
that tracks predation.

## Determinism

`c2-parity predation` prints the kill log and a digest of every animal's
position, goal and health at the end. Stock Areas 1–5, both priorities:
- 600 s at the stock population: kill logs identical to the previous build
  (10 of 10).
- 300 s at 20, 40 and 60 animals: kill logs and digests identical before
  and after the single pass (30 of 30).

## Results

### PC (desktop x86-64, release build)

`c2-parity predation AREAn.MAP 300 [prey] --count 20,40,60`, Areas 1–5,
both priorities (10 runs per row). Microseconds: mean of the run means,
median of the run worsts.

| animals | think before (mean / worst) | think after (mean / worst) | step, predation on (mean / worst) |
|--------:|-----------:|-----------:|------------:|
| 20 | 4.1 / 8  | 2.0 / 5  | 47 / 187  |
| 40 | 14.8 / 24 | 7.4 / 13 | 90 / 264  |
| 60 | 26.6 / 48 | 14.5 / 26 | 133 / 449 |

The single pass halves `think()`. At 60 animals it is about 10% of a
mean herd step, and its worst is well under the step's.

### Android, 3DS

Not measured yet. The Old 3DS's ARM11 at 268 MHz is roughly 20–30 times
slower per step than the PC above, which would put a 60-animal `think()`
near 0.5–0.8 ms after the change (1–1.5 ms before) against a 33 ms frame.
These are estimates to be replaced with measured numbers.

## Running it

PC:
```
cargo run --release -p c2-parity -- predation source/c2-original/HUNTDAT/AREAS/AREA1.MAP 300 --count 20,40,60
```

Android (device on adb; nothing is installed, files go to
`/data/local/tmp/c2bench`):
```
android-ver/bench-predation.sh source/c2-original "1 3 5" 300 20,40,60
```

3DS: start a hunt (observer mode, so gathered carnivores cannot end it),
press SELECT to pause, then X. Over about three minutes the hunt runs at 20,
40 and 60 animals, first placed as the game places them, then gathered
within 12 cells of the player (to load the drawing), then the herd alone
(30 s per count, predation on and off). Results are appended to
`bench.txt` beside the packs (`sdmc:/3ds/c2rs/bench.txt` for the `.3dsx`).
The frame lines give mean / worst ms for the whole frame and for sim,
world, 2D and wait. The steps are also test-script commands (`animals`,
`prof`, `bench`; see `docs/3DS.md`). If the pack has no `_PREDATION.TXT`,
the herd bench uses default rules (carnivore fights on) and says so.
