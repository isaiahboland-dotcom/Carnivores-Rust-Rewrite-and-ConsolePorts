# Decisions and deviations

Newest first. Each entry: what, why, and what it affects.

## 2026-10-19 — Calls by menu slot; up to 118 huntable slots
- **Calls** are numbered `10 + menu slot` (the original's AI number, so the
  original game is unchanged) and play `CALL{slot+1}_A/B/C.WAV`. Dialect
  species follow the engine's `MakeCall`: an animal answers the call of
  its `callNo` slot (0 when not given, as the engine), hears it within
  `view * 400 * 2 * hearK`, and flees calls in its `fearCall` list (not
  flyers or the old brachiosaur). The original game's species still answer
  by AI. An add-on given a slot of its own gets `callNo` for it.
- Call sounds and icons load for the picked slots only (as the engine:
  with add-ons there can be a hundred).
- **Areas past 10**: a map's area is the whole number in its name
  (`AREA23` is 23; the engine reads one digit, any `area1x` being the
  tenth) and `areaN` conditions match whole numbers (`area0` still means
  10), so large combined games keep their areas apart.
- **128-bit target and weapon masks** (`HuntOptions::targets`,
  `mee::Hunt::picked`, `Weapons::new`'s carried set, the PC menu's picks):
  up to 118 huntable slots and 128 weapons, for large combined games. The
  3DS still picks from 32 bits.

## 2026-10-19 — M4: loose add-ons, hunter merging, base overlays
Real add-ons turned out to be loose files plus instructions, not
`HUNTDAT`-shaped folders; most are for the Modder's Edition (MEE).
- **Loose add-ons** (`c2_mods::loose`) are placed by rules standing in for
  the instructions (docs/MODDING.md): files by name and kind, entries
  pulled from text files, a slot takeover inferred from `DINOn` / `WEAPONn`
  menu files, variant folders chosen by the base. Fix-ups a person would
  make are made and reported (a misnamed model, a plain entry keeping the
  replaced slot's MEE spawn tables). A no-entry add-on (skin pack) only
  adds files, so it can't silently replace call sounds.
- **Hunter models are merged** (`c2_mods::hunter`): every add-on hunter
  checked so far is the base hunter (same body, same first animations)
  plus its kill animations. Extras are appended once (identical ones
  shared) and each add-on's `hunterAnim` remapped, instead of the last
  add-on's hunter winning and breaking the others' kills. Texture: the
  base's (some add-on hunters are reskins). A different body is left out
  with a warning.
- **Base overlays** (`--overlay DIR`, repeatable): the MEE release ships
  only changed files; it is layered over the game as part of the *base*
  (its `_RES.TXT` replaces the original's), and add-ons diff against that.
- Recipes don't cover loose add-ons yet (no hash pinning for them).
- A loose add-on's model always goes to `HUNTDAT/<name>`, where its entry
  looks; matching a game file by name is for menu pictures and sounds, and
  never a file another game keeps in its own folder (`HUNTDAT/MODS/...`).
- **Add, don't replace** (owner's call): an add-on made for a menu slot
  gets a new slot alongside the original (handed out at build time in
  load order; menu files renumbered, `overwrite charN` conditions and
  `callNo` / `pic` moved, `_MENU.TXT` extended, price copied). Animals on
  the original game's data still take the slot over: its engine places by
  AI number. Slot conditions past `char9` match by whole number (the
  engine's substring test would let `char1` match `char10`); the PC hunt
  passes up to 22 picked slots (it masked them to 10), and the menu lists
  scroll.

## 2026-10-19 — 3DS: slow-tick report; catch-up capped, tick timings
Owner's Old 3DS report: after a Stegosaurus came into view (and was
killed) the frame rate fell to 40 with "sim" at 93 ms. That is about 12
fixed 10 ms ticks per frame at about 8 ms each, against well under 1 ms
normally: one tick got slower, and the catch-up loop (up to 25 ticks per
frame) multiplied it. The same scene on PC (`c2-parity tick-bench`: hunter
beside a Stegosaurus, the kill, the ship's pickup) never exceeds 0.12 ms
a tick, so the cause is not found yet.
- The 3DS runs at most 4 ticks a frame (`MAX_TICKS`); below 25 fps the
  game slows down instead of spiralling. Deviation from the PC build,
  which keeps catching up.
- `Hunt::prof` times the tick's parts when given a clock; the 3DS pause
  screen shows ticks last frame and per part mean / worst ms (`p` player,
  `w` weapon, `d` dinos, `e` events, `f` effects, `a` CheckAfraid, `s`
  ship), to find the slow part on hardware.

## 2026-10-19 — Jumping needs a fresh press
The original jumps whenever the jump key is down and the hunter is not
rising (`Hunt2.cpp: ProcessPlayerMovement`, `YSpeed == 0`), and walking
onto higher ground zeroes `YSpeed` as a landing. Holding jump therefore
relaunches the hunter every frame on a slope: on PC and 3DS you could
climb mountains in the air (owner's report). Deviation, at the owner's
request: a jump needs the key released and pressed again
(`Player::jump_held`). Holding it still keeps a swimmer afloat.

## 2026-10-19 — M9: the dialect's shared AI (`AnimateHuntable`)
- **Source**: ported from the upstream Modder's Edition release the mods
  target (`Characters.cpp: AnimateHuntable`, `AnimateDeadCommon`,
  `SetNewTargetPlaceRegion`, `ReplaceCharacterForward`, `MakeNoise`,
  `CheckAfraid`; `Resources.cpp: ReadCharacterLine`, the `AIInfo` table;
  `Game.cpp: registerDamage`), not the modernized fork's redesigned
  awareness. Read and cited only (the upstream code has no licence).
- **Who runs it**: dialect species with AI 10-17 and 19-27 (the original's
  herbivores and hunters, Pachycephalosaurus, the Ice Age animals). The
  others still run their interim family (previous entry). Original data
  never reaches this code: the original's AIs are unchanged.
- **Logical phases**: a dialect species' phase is a position in its
  `animIndex` table (`AddAnim`), mapped to a model animation for timing,
  sounds and posing (`Species::anims`, `pose::pose(c, car, anims)`). A
  missing key leaves its phase at 0, as in the original. We number from the
  *resolved* block, so an `overwrite` replaces an entry in place instead of
  appending a new phase; only a species missing a key *and* overwriting the
  first listed one could differ.
- **Awareness**: `awareHunter`, `heardShot`, `damaged`, packs (shared alarm
  and attack, leader following, a dead leader replaced), kill types (eat
  animation, body offset, `dontloop`, silent kills), death types, idle
  groups, weaving, the region-aware target picking and the forward respawn
  only for `moveForward` groups (resetting the animal, as the original).
  For dialect species `MakeNoise` reaches `2 * hearK` times as far,
  `CheckAfraid` asks the AI table which animals look (and keeps renewing
  alarmed ones), and neither restarts an alarm already running.
- **T-Rex** (`AnimateTRex`, dialect AI 18): never flees; once it notices
  the hunter (2: seen or heard, 3: smelt) it turns, plays a `lookAnim` /
  `smellAnim` (else its roar), roars, then chases; obstacles only steer it
  while calm unless `TRexObjCollide`. Unlike the original's, it does not
  carry the hunter's body (the dialect disabled that), and `MakeNoise` only
  wakes a calm one. Roars are picked per animal at reset.
- **Ambients** (`AnimateClassicAmbient`, dialect AI 1, 2, 5, 6): bolt
  from a hunter within 812 units, zig-zag away, settle beyond 4096; idle
  groups, packs, swimming, `dash`. A killed animal of a `moveForward` spawn
  group is replaced in its group's regions (`Characters_AddSecondaryOne`;
  any species, as the dialect does). Dialect animals always leave blood
  trails (the original exempts its AI 6, the brachiosaur; the dialect's 6
  is the pig).
- **Flyers** (`AnimateDimor`, dialect AI 3, 4): flap up toward
  `1.5 * maxalt`, glide down from `maxalt`, flap again below `minalt`,
  follow packs, call now and then; a forward respawn starts at `minalt`.
  Shot, they fall (`AnimateIcthDead`: the death type's `fallAnim`, a
  splash on water, then `dieAnim` or a water death).
- **Brachiosaurs** (`AnimateBrahiOld` 30, `AnimateBrahi` 34 / 35): wade
  (or, 35, walk) between spots 200-400 deep (`SetNewTargetPlace_Brahi`),
  idle in groups, follow packs, and move without obstacle checks (the
  original's call is commented out). 34 / 35 with `aggress` charge an
  aware hunter in water no deeper than 550, kill within `killdist`,
  otherwise flee; deeper than 550 they head back. Kept: the dangerous
  wader's path test also runs the common land check 64 further on (a
  missing `else` upstream), and turning while alarmed scales by
  `runrotatespeed`, which the manual makes required (0 freezes the turn).
- **Fish and Mosasaurus** (`AnimateFish`, `AnimateDeadFish`, 32 / 33):
  swim at a depth between `spcDepth / 2` from surface and bottom, in water
  `mindepth`-`maxdepth` deep, pitching toward a target depth; `DangerFish`
  hunt a hunter standing in water within `view * aggress` (jumpers leap
  at one near the surface), others swim off; idles (`lookAnim` /
  `fishIdleAnim`) near the surface; dead ones sink. Kept: the target
  range is 5024 for both (a `switch` without `break`); the depth retry
  compares an `int` pitch difference; `spcdm` is an `int`, so 0 while
  idling. Not done: leap spray particles (`partCnt`), and the vertical
  bend `bdepth` (computed upstream, never drawn). Shots are not heard by
  them (`Aquatic`). Their target picking can retry 3024 times: watch on
  Old 3DS.
- **Ichthyornis** (`AnimateIcth`, 31): walks the shore and swims (land
  and water idle groups, shaking itself dry every eighth walk cycle),
  takes off when the hunter comes within 1050 (or after a minute on the
  ground), flies up, glides down and lands. Kept: `waterNear` ignores its
  position and tests map cell (0, 0); `swimmingAnim` is true whenever the
  species lists a `shakeWAnim` (an index tested as a flag). Deviation:
  `ProcessPrevPhase` runs once per step, not once per `goto TBEGIN`.
- **Not ported** (no game folder we have uses them): Titan (28) and Micro
  (29) still run as the interim Ceratosaurus / Velociraptor; the wary
  Dimorphodon (36) is not placed; poacher (8) and hunting dog (9) are
  multiplayer-only upstream.
- **Kept quirks**: clearing the water flag clears all of `StateF`; the
  herbivore turn tests `State` (not `StateF`) for water; `morphTime`
  defaults to 1 (the manual; the released code leaves it 0, which would
  never finish a blend).
- **Deviations**: `goto TBEGIN` repeats at most 16 times per step (the
  original could loop forever on a bad target); a pack forms only when its
  leader was actually placed; the evacuation ship's `DShipPickup` and
  survival mode are not modelled yet; roar and water-death animations are
  not rolled at reset (their AIs are not ported).
- **Checked**: synthetic tests (fleeing, a wolf's kill type, defensive
  animals, idle groups, packs, death types, phase numbering), and
  `c2-parity mee-sim` over every dialect game folder we have: no
  non-finite positions, slowest step under 0.5 ms with 147 animals.

## 2026-10-19 — M9: dialect hunts on PC (menus, spawning; interim AIs)
- **Menu**: `_MENU.TXT` / `.c2map` lists (`c2_formats::menu_txt`); a
  huntable's menu slot, not its AI, is what a hunt passes on.
- **Resolution**: `c2_formats::mee::resolve` / `Tables::read` apply a
  hunt's `overwrite` / `addition` blocks with the engine's substring rules
  (so `area10` also matches area 1, and in the trophy room every block
  applies, as in `ReadCharacters`). The area number comes from the map name
  the way the engine reads its launch path (`area_of`).
- **Placement**: `Herd::place_mee` follows the modernized fork's
  `PlaceCharacters` / `spawnMapAmbient` (pack types, ratio picking in turn
  or at random, regions, avoid areas, the 40-cell distance, water depths,
  the density option), capped at the engine's 256 characters. Deviation:
  the engine halts past 256; we stop placing.
- **Targets**: `Hunt::is_target` uses a species' `radar` flag when the data
  sets it, else the chosen-AI bits (scoring halves non-targets).
- **Interim AIs** (until step 4 implements the families): the dialect's
  AIs run as the closest original one (`c2_core::mee::family`: Ice Age
  grazers as Parasaurolophus/Stegosaurus/Ankylosaurus, wolf and Micro as
  Velociraptor, Smilodon and bear as Allosaurus, Titan as Ceratosaurus,
  Ichthyornis as Pteranodon, the brachiosaur variants as the brachiosaur);
  fish, Mosasaurus, poacher and dog are not placed. Per-species animation
  slots and speeds (`runAnim`, `runspd`, `deathtype`, the first
  `idlegroup`) override the family's (`dino::Tuning`).
- **Weapons**: the renamed keys (`land_power`, `land_prec`, `land_fall`,
  `pic1`) are read as the original ones; the dialect's extra weapon
  features are not implemented yet.

## 2026-10-19 — Modder's Edition Engine data in scope (M9)
Owner's decision. Many full-game mods are made for the community Modder's
Edition Engine (MEE) rather than the original executable; they need its
`_RES.TXT` dialect and features. They are now supported in the public
crates as a dialect of the game data, reimplemented in Rust:
- Reference only, never committed, under gitignored `source/`: the
  upstream engine (`Carnivores-CPE`, branch `Map-Amb-Demo-2`, no licence
  file, so read and cite only, like the original source) with its
  *Modders Edition 1.2 Manual*, and the MIT "Modernized" fork. Code
  comments cite their file / function as they do for the original.
- No game or mod data in fixtures; tests stay synthetic.
- The vanilla path stays as the original: the dialect only applies to
  data that uses it.

## 2026-10-18 — Reading more modded game folders
Found by scanning a batch of mods (all game folders now parse):
- `_RES.TXT`: a `{ ... }` block outside any section is kept as an unnamed
  section and ignored by `to_data`, instead of failing. The original's
  `LoadResourcesScript` only looks for lines containing `weapons` /
  `characters` and skips everything else, so this matches it. Modded
  engines put their own tables there.
- `.CAR`: bytes after the `Anifx` table (some modding tools leave a few)
  are kept in `Car::trailing`, so rewrites are byte-identical.
- PC file lookup: a `_RES.TXT` file name may be a relative path with
  either slash (`MODELS\x.car`); each part is matched ignoring case, as
  on Windows.
- Many full-game mods target community-modified engines whose `_RES.TXT`
  is a different dialect (spawn / area / trophy tables, per-area and
  per-time `overwrite` sections, new AI kinds, new weapon keys). They
  load and render; their menus, spawning and AIs need that dialect
  implemented, which is not done yet.

## 2026-10-18 — M7: 3DS settings: animal run speed, ambient animals
Owner's request, because the game handles awkwardly with 3DS controls
and 10+ ambient animals nearby caused lag spikes on Old 3DS:
- `HuntOptions::dino_run_k` / `Ctx::run_k` multiplies the running gait's
  speed of carnivores, herbivores, small animals and the T-Rex (not walking,
  swimming or flying), so species keep their relative speeds. The 3DS
  setting is 50–100% in 5% steps; the PC passes 1.
- `PlaceOptions::max_ambient` caps the small ambient animals (`None` on PC
  = the original count). The 3DS uses 4, or 0 with ambient animals off,
  which also leaves their models and sounds unloaded (brachiosaurs and
  flyers included).
- Settings live in `settings.txt`, not the save, which keeps the
  original's format.

## 2026-10-18 — M7: kill notice, radar on the 3DS map
- `GameEvent::Scored { ctype, score }` when a kill adds to the account
  (with the tranquilizer too, which keeps no trophy). The 3DS shows the
  species and points for 4 s in the top screen's bottom right (owner's
  idea; the original shows nothing until the trophy plaque). The PC
  ignores the event.
- Radar: the 3DS map shows the hunted species everywhere, as the
  original's radar blips do; without it, only animals in view range (the
  owner's always-on map).

## 2026-10-18 — M7: title screen, extra ammo, longer Old 3DS view
- **Title screen** (owner): Play / Trophies / Quit over the original's
  `MENUM.TGA` (its painted buttons covered by the hunter's record). The
  trophy room left the area grid for its own button.
- **Extra ammo gear** (owner's idea, not in the original): x1 10 points,
  1 refill, kills score x0.95; x2 50, 2 refills, x0.95; x3 150, 3 refills,
  no penalty. A refill is `Weapons::resupply` (every carried weapon back to
  its starting ammo); the penalty is `HuntOptions::score_factor` (1.0 on
  PC, so nothing changes there).
- **Old 3DS view distance 26 -> 34 cells, objects 18 -> 24**: the owner's
  Old 3DS ran 60 fps with ~8 ms of CPU work per frame (sim 1.5, world 2.3,
  2D 4.4, waiting 11.4). More if the next measurement allows.
- Screenshots (L+SELECT, test script) now save both screens
  (`<name>.tga`, `<name>_b.tga`).

## 2026-10-18 — M7: one pack for the whole game, and the 3DS menu
- **Packs hold the whole game** (pack version 3): every area (plus the
  trophy room), animal and weapon. The menu picks, and a hunt loads only its
  area, the hunter, the ambient animals (always placed, as in the
  original), the targets and the carried weapons
  (`load::species_with` / `weapons_with`). Vanilla o3ds: 136 MiB on the SD
  card.
- **Night vision on the GPU** (a texture-combiner stage) instead of in
  the cooker, so one set of textures serves every time of day.
- **Linear memory 20 → 24 MiB.** With every ambient animal's sounds, a
  hunt needs 12–17.5 MiB; the heap still fits the 23 MiB loading peak.
  The cooker writes each piece's cost and the menu refuses a hunt that
  doesn't fit ("Too much for this console") instead of risking a crash.
- **Menu (owner's design, after the mobile game)**: touch steps on the
  bottom screen (area hexagons + sun/sunset/moon; animal hexagons; weapon
  and gear hexagons with Tranquilizer / Observer), a summary on the top.
  Prices, points and what is saved follow the PC menu (`Menu.cpp`). One
  hunter for now (`trophy00.sav`, named "Hunter"); name entry and more
  hunters come later.
- Dying goes back to the menu without saving, as from the PC menu.

## 2026-10-18 — M7: 3DS buttons
Owner's request, for the Old 3DS / 2DS, which have no C-Stick: the face
buttons look (X/B up/down, Y/A turn) and the D-Pad does what they did:
down next weapon, left binoculars, right show/hide weapon. Then (owner):
L jumps and D-Pad up toggles running. A/B still answer the leave prompt.

## 2026-10-18 — M7: 3DS sound
- `c2_core::sound` now holds what used to be the PC's `play_sounds` and
  `lr_gains`: which `SoundId` an event plays (`commands`) and
  `CalcLRVolumes`. Both front ends map ids to their own samples.
- Packs carry sounds as raw 16-bit mono at 22050 Hz (`C2RS/SND/*.PCM`,
  names in `c2-cook`'s `Cooker::car` / `wav` calls). They take the samples
  out of the slimmed `.CAR`/`.RSC`. They load into linear memory, which
  went from 16 to 20 MiB. The o3ds heap budget went from 40 to 38 MiB, the
  linear budget from 12 to 16 MiB. The vanilla test pack needs 7 MiB for
  sounds.
- No DSP firmware means no sound and a message, not an error.

## 2026-10-18 — M7: 3DS bottom screen
The owner's design: a **touch weapon picker on the left, the map in the
middle, a call picker on the right**, replacing the PC's number keys,
change-call key and Tab map. The map is always on, which the original
isn't. It shows **the animals in view range** (huntable ones bright),
whatever the radar option says. That's a deviation the owner asked for. In
the original, blips need the radar. Tapping the selected call calls; a
touch that moves more than 6 px is a look drag, so the Old 3DS can still
look by touch. The font is font8x8 (as on PC), cooked into the pack.
Pictures are the game's own `MENU/PICS/WEAPONn` / `CALLn`, resized by the
cooker.

## 2026-10-18 — M6: Old 3DS memory (first hardware test)
The first build crashed with an out-of-memory fatal error on the owner's Old
3DS and Old 2DS while loading. The emulator didn't catch it, since it gives
3DSX apps 96 MiB on either model. On hardware an Old 3DS app has 64 MiB,
libctru reserved 32 MiB of it as linear (GPU) memory, and loading the area
needed over 30 MiB of heap.
- **Linear heap 16 MiB** (`__ctru_linear_heap_size` in `c2-3ds`). The GPU
  data is 5–7 MiB, so the Old 3DS heap goes from ~30 to ~47 MiB.
- **Packs carry slimmed `.RSC`/`.CAR` files** (pack version 2): no terrain or
  model textures, which are in the `.TX` files, and no sound samples. Sounds
  stay as empty entries so indices hold; M7 will cook sound separately.
  The `.MAP` stays whole (fixed layout), but the 3DS parses it with
  `map::parse_lean`, keeping one light map and no `texture2`. The area loads
  first, while the heap is still empty. Packs went from 31 to 17–18 MiB.
- **The cooker measures instead of estimating**: it repeats the 3DS load
  steps under a counting allocator and checks the peak against the target
  (o3ds 40 MiB, n3ds 96 MiB), and checks the GPU total against 12 MiB.
  Vanilla AREA1 + 2 animals + 2 weapons peaks at 23 MiB.
- Running out of memory now prints a message and waits for START (nightly
  `alloc_error_hook`) instead of the system crash screen and reboot.
- Side effect, unchanged from before: the effects code's terrain colours
  (`texture_mid_color`) see no textures on the 3DS, which already happened
  after `trim()`.

## 2026-10-18 — M6: cooker and first 3DS build
- **Layout**: `3ds-ver/` is one nightly workspace holding `c2-cook` (host
  tool) and `c2-3ds`, as the README tree says; `default-members` is the cooker.
  rust3ds crates come from git (not on crates.io), pinned by `Cargo.lock`.
- **Pack = original files + GPU textures.** The 3DS runs the same `c2-core`,
  which needs the `.MAP`/`.RSC`/`.CAR` data anyway, so the pack carries them
  unchanged plus textures pre-tiled for the PICA200 (RGB565 terrain atlas,
  RGBA5551 models, scaled per profile). After loading, the 3DS frees what
  only the PC uses (≈ heap 16.6 MiB in use for the default pack). (Since
  the hardware test above, the pack's `.RSC`/`.CAR` are slimmed.)
- **Partial species sets**: the pack keeps the full `_RES.TXT` (character and
  weapon indices drive placement and AI) but only the chosen `.CAR`s.
  `Herd::spawn` now skips a species with no animations instead of placing an
  invisible animal; with every `.CAR` present nothing changes.
- **Gameplay view range on 3DS** is 42 cells (`ctViewR` at the original's
  lowest view option); only drawing distance depends on the profile.
- **Fog in the vertex shader**: the GPU's fog table is indexed by depth-buffer
  values, which at ~10 000 units of view put the whole fog ramp into one
  table entry. The shader computes distance fog like the PC, and a second
  combiner stage mixes in the fog colour.
- **Texture orientation** (found in the emulator): with our tiling (bottom
  image row first), the hardware's `t` equals the image's `v` directly.
- `unsafe` is allowed in `c2-3ds` only (citro3d is a C API).
- The test-script hook (`autotest.txt`) and L+SELECT screenshots exist so
  builds can be checked in an emulator and bug reports can carry pictures.

## 2026-10-18 — Zip mods
Owner approved the `zip` crate (9.0.5, deflate only via pure-Rust zlib-rs;
no bzip2/lzma/zstd/AES, which old Carnivores mods don't use). It sits behind
`c2-vfs`'s `zip` feature, so the 3DS build doesn't carry it. zip 9 needs Rust
1.88, so the workspace MSRV went 1.85 → 1.88. A zip's `HUNTDAT` may be at the
top or one folder down, like a mod folder; a zip's `mod.toml` is read from
beside `HUNTDAT`. Files from a zip are copied (not linked) into a built pack.
`recipe verify` looks at `.zip` files in `--mods` as well as folders, and a
zip's name without `.zip` counts as its folder name.

## 2026-10-18 — M5: recipes

**A recipe pins exactly the files it uses** (`sha256:` per file) and the
build takes only those from each mod, so the same recipe produces the same
game for everyone, whatever else their download contains. `recipe new`
pins a mod's changed files plus its `_RES.TXT` when that changes anything.

**Finding mods:** `folder` hint, then a folder whose name slugs to the id,
then the folder with the most hash matches. Download folder names vary;
hashes don't.

**Base check only warns.** `recipes/bases/c2-vanilla.toml` holds the SHA-256
of the owner's 192 vanilla `HUNTDAT` files (hashes, no content). Other
releases of the game will differ in places; mods still apply.

**Schema additions** (README §6 updated): optional `folder` per mod, and
`[[resolve]] entry = "section:name"` for `_RES.TXT` conflicts (the README
only had file paths). Patches are unified diffs, matched without `\r` and
allowed to drift from their header line; they must live inside the recipe
folder. Our own small applier, not a crate (≈100 lines, tested).

**Dependency:** `sha2` (RustCrypto) for SHA-256.

## 2026-10-18 — M4: mod system

**Combined games are materialised, not read through the VFS at run time.**
`c2-mods` layers vanilla and the mods in a `c2-vfs` VFS (provenance, merge
report), then writes the result as a folder of symlinks (hard links or copies
where symlinks fail) plus generated files (the merged `_RES.TXT`) in the data
dir. The engine keeps loading from a plain game folder, the folder is easy to
inspect, and the 3DS cooker (M6) can start from the same VFS. A folder is only
ever replaced if it carries our `.c2rs-pack` marker.

**Only `HUNTDAT/` counts as mod data**, minus logs/backups and map-editor
side files. Files byte-identical to vanilla are dropped, which turns
"full copy of the game" mods into their real changes.

**`_RES.TXT` is merged by entry**: blocks matched by `name`; a renamed block
in a slot vanilla no longer has by name replaces that slot (slots matter to
saves and the menu); otherwise appended. `prices` by key + occurrence.
Conflicting changes: later mod wins, reported. Identical changes from two mods
are not conflicts.

**Manifest extensions:** `[res]` gains `adds_weapons` / `modifies_weapons`
(README §5 updated). `[files]` empty means detect.

**Dependencies:** `serde` + `toml` for `mod.toml` (the format the spec
names). Zip support is not in yet: it needs the `zip` crate (with its
deflate dependencies), which per §8 waits for the owner's OK.

## 2026-10-18 — M3: menus and the trophy room

**The menus are reconstructed.** The released source is only the game; the
original menu was a separate front end that started it with command-line
arguments (`prj= din= wep= dtm= -double -observ`, see `Game.cpp`). We rebuild
the screens from the shipped art: each picture, its `*_ON.TGA` lit version and
its `*_MAP.RAW` hit map (`docs/FORMATS.md`). Player registration, the waiver,
deleting a player, main menu, statistics, hunt setup, options, credits and the
quit prompt all follow the pictures. List layout, fonts (the 8x8 font, scaled)
and wording of the statistics are our own.

**Prices are a budget, not a payment.** `_RES.TXT` `prices` gives `start`
(a new hunter's score) and a price per area, dinosaur, weapon and accessory.
The account shows the score and what is left after the current choices; a
choice that would go below zero is refused. Nothing is deducted: hunting a
5-point Parasaurolophus with a 10-point licence could never pay off otherwise,
and the shipped saves (scores 104/114) fit this reading. Observer mode is free
and needs no targets or weapons. No rank locks.

**Menu mapping.** Dawn/Day/Night = `TimeOfDay::Dusk/Day/Night` (the code
calls the first time "dusk", the menu "dawn"); dinosaur *n* = AI `9 + n`;
equipment = Camouflage, Radar, Cover Scent, Double Ammo (`EQUIP1-4`, the
`.NFO` texts). Camouflage, radar, scent and tranquilizer are kept in the save
as the game does (`CamoMode` etc.); other choices start fresh each visit.
Info texts are re-flowed to the panel width.

**Players.** Each registered hunter is a `trophy0N.sav` (N = 0..9) in our
data dir; on first run every `trophy0N.sav` in the game folder is copied
there (the game folder is never written). Key bindings, mouse sensitivity,
reverse mouse, units, aggressiveness, density, sensitivity and view range come
from the save and can be changed in Options; `ctViewR = 42 + OptViewR/8*2`.
Options the port doesn't use (resolution, textures, fog, shadows,
renderer, sound device) are kept in the file but not shown.

**Trophy room** is the original's `TrophyMode` (`TROPHY.MAP`): trophies stand
on the landing ring in their first animation frame, the hunter collides with
them, a plaque shows when 2.5 cells in front of one, `R` removes it, `Esc`
saves and leaves at once; no weapons, calls, map or AI. **Deviation:** the
original's exit path adds `Last` to `Total` again when leaving the room
(stats inflate on every visit); we leave the stats untouched. After a kill
in a hunt the plaque (`TROPHY_G.TGA`) shows for 20 s, as in the original.

**Menu logic lives in `c2-pc` for now** (README §4 puts the menus' state
machine in `c2-core`). It is tied to the PC art and mouse; the parts the 3DS
needs (budget, choices, player list) move to `c2-core` in M6/M7, when a
second front end shows what is actually shared.

**Brightness** is `BrightenTexture` done in the shader: each 5-bit channel
times `(OptBrightness + 128) / 256`, floored and capped, plus the same factor
on the sky/fog colour. **Pause** is `VK_PAUSE`, and `P` unless a binding uses
it (many keyboards have no Pause key). **Health recovery** (`MyHealth +=
TimeDt * 4` while alive and not paused) was missing until now; without it
drowning was 1.5x too fast.

**Death returns to the menu** (discarding the hunt's trophies, as the
original's `LoadTrophy` on exit does) when started from the menu; `c2-pc hunt`
still restarts.

## 2026-10-18 — Field of view, binoculars, scope, HUD text

**FOV matches the original:** `CameraW = VideoCX * 1.25` with its 4:3
correction gives a vertical half-angle tangent of 0.6 (61.9 degrees); the
binoculars (1.5x-3x) and scope (3x) divide it. On screens wider than 4:3 the
binocular and scope frames are stretched horizontally to cover the view (they
were drawn for 4:3); the zoom itself is unchanged.

**HUD text** uses the public-domain 8x8 font from the `font8x8` crate (MIT);
the original used Windows GDI fonts, which the game data doesn't include.
Units follow the save's `OptSys` (imperial when set).

## 2026-10-18 — Audio

**kira 0.12** (owner's choice), default features off except `cpal` (we only
play raw PCM; no file decoders needed). We don't use kira's spatial model:
the original's mixer (`Audio.cpp`) gives each voice left/right gains from
`CalcLRVolumes` (falloff `1224/(1224+d)` past 512 units, fading out between
6000 and 10000, its own pan law) and positioned voices stay where they were
emitted. We compute those gains and map them exactly onto kira's volume and
constant-power panning, re-panning every frame. 16-voice limit (extra sounds
dropped), ambience crossfades over ~0.74 s, as the original's 16 x 1024-sample
ramp. The jump and water in/out sounds are never loaded in the released
source, so they stay silent. Ambience zones and random sounds are game logic
in `c2-core`.

## 2026-10-18 — M3: weapons, detection, death

`c2-core::game::Hunt` holds a whole hunt (player, herd, weapons, wind,
health) so every frontend runs identical logic. Ported: `ProcessShoot`,
`MakeShot` (incl. the crossbow's three falling segments), `TraceShot`
(ground, water, objects, posed characters; `sfMortal` faces kill outright),
`MakeNoise`, `CheckAfraid` (sight with `TraceLook` and cover, smell with
wind), the weapon state machine from `DrawPostObjects`, `AddDeadBody`,
`AnimateHuntDead` and `ProcessDemoMovement` (death camera).

Quirks kept: `TraceModel` rotates objects by eighth turns (`(FMap>>2)&7`)
though they render in quarter turns; `ObjectsOnLook` carries over between
animals as the original's global does.

Deviations: weapon animation advances in the simulation tick (the original
advances it while drawing). Key bindings default to the owner's saved
`KeyMap` (W/S/A/D, LMB fire, E weapon, Shift run toggle, X crouch, Space
jump). The loadout menu doesn't exist yet, so every weapon is carried.

## 2026-10-18 — M3: dinosaur AI

All 15 `Animate*` routines from `Characters.cpp` are ported, plus
`PlaceCharacters`, `ReplaceCharacterForward`, `CreateChMorphedModel` and the
death animations. Where routines are copies with different constants they share
one parameterized function (`herbivore`: Par/Anky/Steg/Chasmo, `carnivore`:
Allo/Velo/Spino/Cerato, `small`: Mosh/Dimet/Gall); the parameter tables list
the original's constants and phase numbers per species.

**`MoveCharacter` runs its body.** In the released source its first line is
`return;`, so no dinosaur would ever move — clearly a debugging leftover, as
every dinosaur walks in the retail game. We port the body as written below it.

**Kept original quirks** (they affect behaviour subtly but faithfully):
- `StateF &= (!csONWATER)` (logical not) clears the whole field, not one bit.
- Herbivores test `State & csONWATER` (never set) instead of `StateF`.
- `PlaceCharacters` lifts Dimetrodon (not Dimorphodon) by 2048 at spawn.
- Raw phase-index comparisons (`_Phase <= 2`) differ in meaning per species.
- T-Rex and Ceratosaurus only spawn when chosen as hunt targets.

`AnimatePac` (Pachycephalosaurus) is not dispatched by `AnimateCharacters`;
it's a Carnivores 1 leftover and isn't ported.

The AI uses an exact port of MSVC's `rand()` (same LCG), so random decisions
come from the same kind of sequence as the original's.

`c2-parity dinos <AREA.MAP> [seconds]` places and runs the herd headlessly.
`c2-pc hunt ... --sim MS,AI,DIST,AFTER --shot x.png` renders a chosen animal.

## 2026-10-18 — M3: hunter movement

**Fixed 10 ms simulation tick** instead of the original's variable `TimeDt`.
The movement code is a direct port (it already scales by `TimeDt` and
sub-steps collision every 16 ms), so behaviour is the same at any frame rate,
and a fixed step makes runs reproducible for the planned logic-parity harness.
Rendering interpolates the eye position between ticks.

**Functions that read the global `PlayerY` take it as a parameter**
(`land_qh`, `land_ceil_h`, `check_collision`) so they can serve dinosaurs too.

`c2-parity walk <AREA.MAP> [seconds]` runs the hunter headlessly over real
terrain and prints the path: a quick regression check for movement/collision.

## 2026-10-18 — M2 viewer

**Stack: `winit` 0.30 + `wgpu` 30** (owner's choice), plus `glam` (maths),
`bytemuck`, `pollster`. Versions picked at least two weeks old. No image crate:
screenshots use a ~60-line stored-deflate PNG writer.

**Colour maths stay in the original's space.** The original lights
(`D3DTBLEND_MODULATE` with a grey vertex colour) and fogs raw 16-bit colour
values. We use non-sRGB textures and surfaces so `texel * light` gives the same
numbers. The owner's saves use `OptBrightness = 128`, which makes
`BrightenTexture` a no-op, so it's skipped for now.

**Terrain is generated in the vertex shader** from a per-cell storage buffer
(height, light, water, texture, flags) instead of building meshes on the CPU.
Each cell is 6 vertices; triangulation follows `fmReverse`, and the 4
`TDirection` UV tables in `DrawTPlane` reduce to rotating the corner, which
is how the shader does it. The PC draws a square window around the camera;
the 3DS will need cooked meshes instead (M6).

**Load-time fixups live in `c2-core::area`** (`CreateTMap` hardware path,
`GetLandH`, `GetObjectH`, landing list, trophy ring), so the cooker can reuse them.

**Sky, sun and lighting** follow `RenderSkyPlane`, `RenderSun`, `CalcLights`
and the cloud-shadow code in `Hunt2.cpp`. Night is rendered as the original
does it: `BrightenTexture` maps every texel to `(G/8, G, G/8)`, i.e. night
vision. The sun/moon is drawn additively (the original's black quad corners
are invisible in-game).

**Known approximations** (to check against the retail game): no `RandomMap`
light jitter; ground-lit objects use one light value per object instead of
per vertex; `sfTransparent` faces blend at 50%; object quarter-turn direction
is unverified; no distant billboards (`bmpmodel`) yet; sun is not occluded
by terrain (`SkyTraceK` = 1).

## 2026-10-17 — TGA/WAV: read the header instead of assuming it

The original TGA loader assumes 16-bit uncompressed bottom-up and ignores the
header; the WAV loader assumes 16-bit mono 22050 Hz and ignores `fmt `. For
every vanilla file our output is identical to the original's (checked by
`c2-parity scan`). For files in other encodings (seen in some mods: 24/32-bit,
RLE and ID-field TGAs, a stereo 16 kHz WAV) we decode correctly instead of
reproducing the original's garbage, and report the format so the cooker can
convert it. This only changes behaviour for files the original misread.

## 2026-10-17 — Add-on scope; MIT license

**The public build targets add-on mods**: small, data-only mods that layer
on vanilla and merge cleanly. Whole-game total conversions are out of scope.

**License: MIT** (`LICENSE`, `license = "MIT"` in the workspace). Covers our
code only; game and mod data are excluded.

## 2026-10-17 — M0/M1 kickoff

**Repository layout differs from README §3 (owner's request).**
- `pc-ver/` is the Cargo workspace for all shared crates plus the PC backend.
- `3ds-ver/` will hold the 3DS backend as its own workspace (nightly toolchain,
  `armv6k-nintendo-3ds` target) that depends on `../pc-ver/crates/*` by path.
  A separate workspace keeps the nightly pin and devkitPro requirements from
  leaking into the stable PC build.
- `source/` (gitignored) replaces `reference/`:
  - `source/c2-original/` — the user's Carnivores 2 **install** (README called
    this `reference/game/`).
  - `source/c2-source/` — original C++ source (README's
    `reference/c2-original/`), cloned from
    `github.com/videogamepreservation/carnivores2` (mirror of the 2013 Assembla release).
  - `source/mods/` — mod installs.

**ReShade is out of scope.** The user's install includes ReShade
(`reshade-shaders/`, `dxgi.dll/.ini`, `d3d9.dll/.ini`, `opengl32.dll/.ini`).
These are third-party post-processing wrappers, not game data; they are
ignored by every tool and will not be ported to PC or 3DS.

**The released C++ source is not the retail 1.0.4 build.** It has no menu
code (`Interface.cpp` is a fragment) and never reads the `prices` section that
every `_RES.TXT` contains. Where the source is missing, behaviour will be
matched against the retail `carn2.exe` by observation, and noted here.

**`_RES.TXT` keys are matched exactly, not by substring.** The original uses
`strstr(line, "ai")` etc., so any line containing the substring sets the field
— e.g. a character named `'Maiasaura'` would have `ai` reset to 0. We match
whole keys, plus the aliases the substring rule actually made work in shipped
data (`hearK`, `lookK`). Verified: `c2-parity` carries a faithful `strstr`
copy of the original reader and diffs every typed field; no `_RES.TXT` we
have produces any difference, and none would trigger the original's halt. Unknown keys are preserved (`extra`) so mod data survives merge/write.

**Parsers return on-disk values; transforms live in the engine.** Vertex
`*2`/z-flip, `BrightenTexture`, face re-sorting, `TraceC = trace - 1`, and
`_RES.TXT` defaults are applied by `c2-core`, not `c2-formats`. This is what
makes byte-exact round-tripping possible, which the cooker and mod merger need.

**Counts are bounded by input size, not engine limits.** The original has
fixed arrays (1024 vertices/faces, 256 objects, 64 animations…). We reject a
count only if the remaining bytes can't possibly hold it, so mods that exceed
the old limits still load and no malformed file can trigger a huge allocation.

**Synthetic fixtures are generated inside the tests** (`c2-formats/tests/`)
rather than by a separate tool in `tools/`. Same guarantee (no real game data
in the repo), less machinery. A standalone generator can be added if the
parity shim needs files on disk.

**Format parity is checked by byte-exact round-trip instead of a C++ JSON
shim (for now).** Building the original requires MSVC 6-era DirectX/Glide
SDKs. Parse-then-write reproducing every real file byte-for-byte, with the
parser consuming the whole file, is a strong check of the layouts. A shim that
compiles just the loader functions with modern GCC remains an option if a
field's *meaning* (rather than layout) needs confirming.

**License:** superseded — MIT, see the entry above.
