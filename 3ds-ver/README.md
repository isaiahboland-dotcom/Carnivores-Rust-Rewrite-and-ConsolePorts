# 3ds-ver — Nintendo 3DS port

A separate Cargo workspace (nightly, see `rust-toolchain.toml`) that reuses the shared crates in
`../pc-ver/crates/` by path:

- `c2-cook/` runs on your PC. It turns your game (plus add-on mods or a recipe) into one `.c2pk`
  pack for the 3DS.
- `c2-3ds/` is the 3DS app.

The full guide (the menu, controls, emulator testing, how it draws, sound) is
[`../docs/3DS.md`](../docs/3DS.md).

## Quick commands

Needs devkitPro (devkitARM, libctru, citro3d), `cargo-3ds` (`cargo install cargo-3ds`) and
libclang. Run from this folder:

```sh
export DEVKITPRO=/opt/devkitpro DEVKITARM=/opt/devkitpro/devkitARM
export PATH="$DEVKITARM/bin:$DEVKITPRO/tools/bin:$PATH"

# 1. Cook a pack. --target o3ds for an Old 3DS / 2DS (works on every model), n3ds for New 3DS only.
cargo run --release -p c2-cook -- --game PATH/TO/GAME --target o3ds --out GAME.c2pk
#    add-ons:     --overlay "PATH/Carnivores 2" --mod ADDON --mod ADDON ...
#    a recipe:    --recipe ../recipes/NAME.toml --mods DOWNLOADS   (each recipe's header has its command)
#    over budget: --force writes it anyway; the 3DS menu then refuses picks that don't fit

# 2. Build the app: target/armv6k-nintendo-3ds/release/c2-3ds.3dsx (and c2-3ds.elf for .cia builds).
cargo 3ds build --release -p c2-3ds

# 3. Check a pack on the PC the way the 3DS loads it (see "What the checks mean" below).
cargo run --release -p c2-cook -- --check GAME.c2pk
```

On the SD card, put `c2-3ds.3dsx` in `/3ds/` and the packs in `/3ds/c2rs/`, then start it from the
Homebrew Launcher. With more than one pack, it asks which one to play. Each pack keeps its own
saves.

**The launcher does all of this for you:** its **Build > 3DS** tab cooks the pack, builds the
app and installs under a name of your choosing (`sdmc:/3ds/<name>/`), so several installs can sit
side by side. It can also make a `.cia` for the home menu (see below). The 3DS side alone is the
`c2-3ds-builder` binary in `../pc-ver/`.

### `.cia` builds (optional)
A `.cia` is installed with FBI or a similar title manager, so the game shows on the home menu.
It needs two tools that are **not part of this project**; download them yourself:
- **makerom**, from Project_CTR: <https://github.com/3DSGuy/Project_CTR/releases>
- **bannertool**: <https://github.com/Steveice10/bannertool/releases>

Put both beside the launcher, or in the tools folder set in the launcher's Settings (working from
the source tree, its top folder works too; the files are ignored by git). On Linux and macOS, make
them executable (`chmod +x makerom bannertool`). Then use **Build > 3DS** in the launcher, or the
command line (from `../pc-ver/target/release/`):

```sh
c2-3ds-builder --cia NAME --tools DIR --out SD_DIR [--target o3ds|n3ds] [--icon PNG] [--banner PNG] [--sound WAV]
```

The `.cia` is built from `c2-3ds.elf`. Each install name gets its own title ID and reads its packs
and saves from `sdmc:/3ds/NAME/`. The home-menu icon (48x48 PNG), banner (256x128 PNG) and banner
sound (WAV/OGG, up to 3 s) are your own files. Without them, they're made from the game's menu
picture.

## Performance

**Old 3DS first.** Every pack listed here is cooked for the Old 3DS (`--target o3ds`), and the
Old 3DS is the console we measure on. An o3ds pack or `.cia` plays unchanged on a New 3DS / New
2DS, with more headroom:

- **Same memory.** An o3ds build gets the same 64 MiB on both (the `.cia` installs in the
  original memory mode), so a pick that fits on the Old 3DS fits on the New.
- **More speed.** On a New 3DS the app turns on the 804 MHz clock and the extra cache (it has no
  effect on an Old 3DS), so the same hunt runs at least as fast.

So **if it runs on the Old 3DS, it runs on the New 3DS.** The reverse doesn't hold: an n3ds pack
(longer view, bigger textures, 96 MiB budget) is for New 3DS models only.

| | o3ds pack (every model) | n3ds pack (New 3DS only) |
|---|---|---|
| App memory | 64 MiB | 124 MiB |
| Heap budget while loading | 34 MiB | 96 MiB |
| Linear (GPU/DSP) budget | 22 MiB | 22 MiB |
| View distance | 56 cells (full detail to 34) | 76 cells (full detail to 42) |
| Textures | terrain 32 px, models 64 px | terrain 64 px, models 128 px |
| Animations | thinned above 2 MiB | thinned above 6 MiB |

`c2-cook` measures every area's loading peak and what each animal and weapon costs. The 3DS menu
adds up the picks and refuses a hunt that doesn't fit ("Too much for this console"). Running out
of memory shows a message instead of crashing. More: [Memory budget](../docs/3DS.md#memory-budget).

**Measured so far:** vanilla Carnivores 2, single-area o3ds pack, about 58 fps and 15.1 MiB heap
in use while hunting (Old 3DS / Old 2DS, 2026-10-18). Per-game readings from 2026-11-08 are in
[Hardware results](#hardware-results-2026-11-08). Press SELECT while hunting to pause and show
fps, memory and the time each part of a frame takes.

## Tested games and mods

Old 3DS packs built on 2026-11-08 (commit `936aba7`, the same code as v0.5.2). Each was checked
with `c2-cook --check` and the cook's memory estimate. The **Old 3DS fps** column is the range
seen while playing on an original 2DS and an Old 3DS XL (2026-11-08). These are spot readings,
not averages; the details are in [Hardware results](#hardware-results-2026-11-08).

### What the checks mean

- **The cook's memory estimate.** These are real console limits:
  - *Over the budget with every ambient animal: AREAn.* That area with all its ambient animals
    doesn't fit. The menu refuses the hunt; fewer ambient animals or picks may fit.
  - *The costliest picks don't fit together.* Each animal fits alone, but the most expensive
    ones together don't. The menu says when picks are too much.
- **`c2-cook --check`** loads the pack the way the 3DS does and simulates 60 s of every menu
  animal and every roamer alone on each area, plus the trophy room and both survival modes. Its
  last line, "N hunts with missing files", counts every failed item, survival included.

| Check finding | What it means on the 3DS | Real limit? |
|---|---|---|
| Survival: `MISSING: its area` | Survival is played on AREA5 with the shotgun. A game without an AREA5 has no survival on the 3DS. | Yes: game feature |
| Survival: a roster but no wave | The survival starts, but no wave reached the hunter in 5 simulated minutes. | Not diagnosed: may be a bug |
| `not a hunt target` on a roamer (`(roams)`) | Modder's Edition roamers are hidden from the 3DS menu unless **all huntable** is on. Picked that way, they spawn but show blue on the map instead of green. Scoring is unaffected. | No: cosmetic |
| `not a hunt target` on a menu animal | The pick is placed, but none of the placed animals counts as a target (it isn't drawn green). | Not diagnosed |
| `speeds of ... it can't move` | The mod gives that entry zero speeds. In every case below, it is a prop the mod placed on purpose. | No: the mod's data |

### Summary

| Game / pack | Check | Memory (o3ds) | Survival | Old 3DS fps |
|---|---|---|---|---|
| Carnivores 2 + Android (`c2-android`) | ok | costliest picks don't fit together | yes | 30 |
| Carnivores 2 + every add-on (`c2-everything`) | 35, roamers only | costliest picks don't fit together | yes | to measure |
| Dinosaur Hunter APK (`c2-apk`) | ok | costliest picks don't fit together | yes | 30–31 |
| Ice Age + Android (`ice-age-android`) | ok | fits | yes | 26–30 |
| Ice Age APK (`ice-age-apk`) | ok | fits | yes | 30–45 |
| Carnivores Triassic | ok | fits | yes | to measure |
| Desperation | ok | fits | yes | to measure |
| Gobi Desert | ok | fits | yes | 27–30 |
| Reborn | ok | costliest picks don't fit together | yes | 24–30 |
| Mandibles | 1 | fits | modded: no wave | to measure |
| Jurassic Park Hunter 3 | 2 | fits | none (no AREA5) | to measure |
| JPHL Special Edition | 2 | AREA4 to AREA6 over budget with every ambient | no wave | 17–35; AREA4–5 won't start |
| Genesis Redux | 9 | costliest picks don't fit together | modded: no wave | won't start a hunt |
| C2 De-Extinct | 12 | fits | none (no AREA5) | to measure |
| Anachronous | 78, roamers only | fits | yes | 30–57 |
| CClassic MEE | 152, roamers only | fits | yes | 24–30 |
| Triassic MEE | 135 | costliest picks don't fit together | yes | to measure |
| Fallen Kings | 153 | costliest picks don't fit together | yes | to measure |

### Hardware results (2026-11-08)

Played on an original 2DS and an Old 3DS XL with o3ds packs. Each figure is one reading from the
SELECT overlay at a spot on the map, not an average, and spots and animal counts differ between
rows. Sim, world, 2D and wait are milliseconds per frame. **Heap** is heap in use over heap size
(MiB). **cmd** is how full the GPU command buffer is, not memory. Most hunts used blood off, kill
animations off and animals at 50% running speed; those settings change the workload, so later
records should state them.

**Overall:** the APK-built bases are the steadiest on the Old 3DS (about 30 fps with animals in
view, 40–45 fps on Ice Age APK with none). PC-based packs and the larger mods play, but vary more
with location. Two packs fail to start a hunt.

| Pack | Area, setting, scene | fps | Heap | cmd | Sim | World | 2D | Wait |
|---|---|---|---|---|---|---|---|---|
| Dinosaur Hunter APK | 1, 1 animal, 1 gun | 30 | 17.6/38.4 | | 7.4 | 15.8 | 5.3 | 5.9 |
| | 1, 2 animals in range | 30 | | | 8.7 | 13.2 | 8.4 | 7.0 |
| C2 + Android | 1, sprites on, 2 in range | 30 | | 2% | 11.8 | 17.8 | 4.9 | 6.0 |
| | 1, sprites off, 2 in range | 30 | | 7% | 12.7 | 14.9 | 5.5 | 8.4 |
| Ice Age APK | sprites on, none visible | 45 | | | 5.0 | 3.3 | 4.0 | 5.9 |
| | sprites on, 1 visible | 30 | | | 9.9 | 6.9 | 7.6 | 10.3 |
| | sprites off, none visible | 40 | | | 11.3 | 7.4 | 5.8 | 9.0 |
| Ice Age + Android | 2, 1 animal, ambients on | 29 | 15.5/38.4 | | 16.4 | 18.3 | 4.7 | 10.5 |
| | 4, observer, 1 visible | 26 | | | 20.3 | 15.2 | 7.6 | 9.1 |
| JPHL SE | 1, 2DS, none in range | 30 | 21.4/38.4 | | 6.1 | 12.8 | 8.7 | 5.1 |
| | 1, 3DS XL, 3 in range | 24 | | | 8.5 | 15.1 | 8.6 | 6.9 |
| | 2, 3 herbivores, ambients on | 33–35 | | | 6.6 | 7.0 | 8.2 | 10.9 |
| | 3, T. rex, ambients on | 17 | 24.3/38.4 | | 12.3 | 37.4 | 10.3 | 7.5 |
| Anachronous | 1, start | 57 | 30.3/38.8 | | 40.9 | 2.9 | 8.2 | 9.1 |
| | 1, 3 in range | 30 | | | 36.6 | 6.3 | 8.7 | 8.3 |
| CClassic MEE | 1, sprites on, 2 visible | 30 | 25/38.4 | 2% | 29.3 | 14.9 | 10.1 | 11.4 |
| | 1, sprites off, start | 24 | | 12% | 12.0 | 9.2 | 3.4 | 2.6 |
| | 1, sprites off, 2 in range | 30 | | 10% | 30.0 | 20.2 | 10.0 | 8.2 |
| Reborn | 1, sprites on, start | 29 | 19.7 used | 3% | 6.2 | 10.3 | 4.2 | 6.1 |
| | 1, sprites off, start | 24 | | 8% | 6.3 | 12.0 | 4.8 | 6.9 |
| | 1, sprites on, 1 visible | 30 | | 3% | 8.8 | 16.0 | 8.4 | 12.6 |
| | 1, sprites off, 1 visible | 27 | | 10% | 9.2 | 18.0 | 8.3 | 10.3 |
| Gobi Desert | 1, sprites on, start | 30 | | 2% | 9.2 | 10.9 | 8.0 | 6.7 |
| | 1, sprites off, start | 27 | | 4% | 12.7 | 14.8 | 6.8 | 6.5 |

What the readings show:
- **Animals in view cost the most.** Most packs hold 30 fps with a few animals nearby and run
  well above it with none (Ice Age APK 45, Anachronous 57).
- **Trees as sprites helped wherever it was compared.** On first load fps went up 5 (CClassic
  MEE, Reborn) and 3 (Gobi Desert), and cmd fell from 8–12% to 2–3%. The spots weren't
  identical, so this needs a repeat at fixed coordinates before it's counted as proven.
- **Anachronous is CPU-bound.** Sim is about 37–46 ms whatever is on screen, while world is
  under 13 ms. Its heap (30.3 of 38.8 MiB) is the highest seen.
- **JPHL AREA3 depends on location.** It reads 30 fps in one place and 17 fps near a T. rex, with
  world at 37.4 ms.
- **JPHL AREA4 and AREA5 won't start,** even with one animal, one weapon and ambients off. The
  cook flags AREA4–6 as over budget only with every ambient animal, so its estimate misses this
  case. Survival is played on AREA5, so JPHL has no survival on the 3DS until this is fixed.
- **Genesis Redux won't start a hunt.** The cause is unknown (parsing, loading, memory or world
  set-up). Its hunts pass `--check`, so it isn't the same case as JPHL.

### Per game

**Carnivores 2 + every add-on (`c2-everything`).** The 35 findings are the add-on roamers
(Achillobator, Agathaumas, Anatotitan, Lifranyx, Majungasaurus, Manospondylus, Suchomimus) on each
area: cosmetic.

**Reborn.** Passes the check. The costliest picks don't fit together.

**Mandibles.** Every hunt passes. The modded survival starts (an all-arthropod roster), but no
wave came in 5 minutes: not diagnosed.

**Jurassic Park Hunter 3.** Every hunt passes. Four areas and no AREA5, so no survival (both
findings).

**JPHL Special Edition.** Every hunt passes. AREA4, AREA5 and AREA6 are over the Old 3DS budget
with every ambient animal; fewer ambients may fit. Classic and modded survival both start but no
wave came (both findings); survival is played on AREA5, one of the tight areas.

**Genesis Redux.** The costliest picks don't fit together. The Utahraptor roamer isn't a target
on all 8 areas (cosmetic). The modded survival had no wave (the 9th finding).

**C2 De-Extinct.** Two areas and no AREA5, so no survival (2 findings). The Amargasaurus,
Dimetrodon, Iguanodon, Oviraptor and Pachycephalosaurus roamers aren't targets (cosmetic).

**Anachronous.** All 78 findings are roamers (Alvarezsaurus, Andalgalornis, Bicentenaria,
Carnotaurus, Genyodectes, Gualicho, Pampaphoneus, Provelosaurus, Rauisuchus), some in more than
one roamer entry: cosmetic.

**CClassic MEE.** All 152 findings are 15 roamers on 8 areas (Allosaurus, Coelurus, Laelaps,
Lifranyx, Agathaumas, Seismosaurus, Therizinosaurus and others): cosmetic.

**Triassic MEE.** The costliest picks don't fit together. Twelve roamers aren't targets on all 9
areas (cosmetic). The "can't move" T-Rex, Velociraptor, Stegosaurus, Parasaurolophus and
Allosaurus are the mod's corpse props (`FX/Corpses/*.car`, zero speeds): meant to lie still.

**Fallen Kings.** The largest pack (469 MiB). The costliest picks don't fit together. Ten roamers
aren't targets (cosmetic). Five menu animals, mostly the mod's bosses, aren't counted as targets
(not diagnosed): Alpha Tanycolagreus, Beast of the Everglades, Foghorn Monster, King
Iguanosaurus and Small Carnivores. The "can't move" Foghorn is the Foghorn Monster's sound object
(`FOGHORNSFX.car`), meant to stay put.

### Open items
- Survival with no wave: JPHL (both modes), Mandibles and Genesis Redux (modded).
- Fallen Kings' boss menu animals not counted as targets.
- `--check` labels survival and trophy-room failures as "hunts with missing files".
- JPHL AREA4–5 don't start even with minimal picks; Genesis Redux doesn't start a hunt. Both
  need further testing and possibly specific optimziations
- Repeatable 3DS runs (fixed spawn, view direction, animal count and settings) so builds can be
  compared; see the 3DS performance plan in [`../docs/ROADMAP.md`](../docs/ROADMAP.md).
