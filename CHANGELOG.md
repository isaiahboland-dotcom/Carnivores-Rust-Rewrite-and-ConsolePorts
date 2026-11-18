# Changelog

## 0.7.0 — 2026-11-17

Survival switch, poisonous fog, scuba gear, advanced binoculars, bounded
fleeing and flyer spawns, and the DHC OS field manual, island rules and CRT
pass. Notes: `docs/releases/v0.7.0.md`. Hardware and by-eye checks are
listed in `testing-needed.md` and remain pending.

### Survival

- Survival is offered only when the game asks for it: a `survival` line in
  `_RULES.TXT`, `c2-pc play --offer-survival`, or a game built from the APK
  (always on). Without it, the original menu's Survival button, the DHC OS
  SURVIVAL entry and the Wii U title entry are hidden.
- Launcher: Survival mode is set on the instance editor's Base game page (off
  by default), saved with the instance's settings. `c2-cook --offer-survival`
  writes the rules line into 3DS and Wii U packs.
- Survival starts on Mount Ravan when the stock animals are present, else on
  the last listed area (standalone games included), with the shotgun or else
  the third weapon (`survival_area`, `survival_weapon`).
- The arena is fitted to each map at survival start: the ravine where it fits,
  else one found on the map. Genesis Redux, Fallen Kings and JPHL had no waves
  before this, as their ravine was off their land.
- Wave movement routes round objects; Stegosaurus waves no longer stall among
  trees.
- `c2-cook --check` follows the menus' survival pick and skips survival when a
  pack does not offer it.

### Poisonous fog and gear

- Mortal fog zones (AREA5's volcanic gas in the vanilla game) drain health and
  kill the hunter while the eye is under the zone's top and above water. This
  applies whether or not fog zones are drawn.
- Advanced binoculars (new gear, 40 points, PC only): each living huntable
  animal gets a green outline through the binoculars, red while it is after
  the hunter. Kill score is 60% in the hunt and the DHC terminal. Command line:
  `c2-pc hunt --advanced-binoculars`.
- Scuba gear (new gear, 30 points): offered only when a recipe sets
  `[settings.rules] scuba_gear` (launcher: the instance's Advanced page, Extra
  equipment). Diving is faster (top speed 0.5 instead of 0.3, surface swimming
  0.4 instead of 0.25), the underwater fog reaches twice as far and the oxygen
  tank makes drowning take twice as long. Kill score is 85%. Command line:
  `c2-pc hunt --scuba`.
- The modern underwater fog now ends at a fraction of the hunt's view range
  rather than the drawn distance, so at long render distances water no longer
  looks nearly clear. The default render distance is unchanged.
- Launcher instance boxes have a yellow border and a yellow Play button, and
  paths are white.

### Animals

- APK-based games: an animal past the PC's AI range (AI 21, running as a PC
  kind) keeps its original-engine AI when a dialect add-on joins it. The
  nightmare-run copy's Oviraptor stood still before this fix.
- Bounded fleeing: a scared animal that has fled and still sees or smells the
  hunter waits out a lockout before fleeing again: 15 s when walking beside it,
  25 s standing still, 45 s moving away, and at once when running at it (PC,
  Wii U and VR). On 3DS the lockout is a fixed 15 s. Shots and calls keep their
  noise lockout. See `docs/DECISIONS.md` (2026-11-17).
- Flyers spawn halfway through their altitude band, at least 256 above the
  ground, and never on the ground.

### DHC OS

- Advanced DHC OS UI (PC, experimental): a launcher switch in the instance
  editor, "Advanced DHC OS UI: Always On". HUNT SETUP opens a map carousel
  with the map's relief, a survey panel (permit, status, issued animals and
  weapons), the hunter's record and the points. Off by default; with it off
  the terminal and original menus are as before.
- Island rules from `_DHC.TXT` apply in the original menus (PC, Android, the
  3DS hexagon menu, Wii U): barred items are dimmed and refused, picks are
  pruned on an area change, and a forbidden hunt start is refused.
- PC launcher: DHC OS is recommended once per instance with advanced settings
  (at import review, or before the first Play).
- UI beeps (the "DHC OS sounds" option) and a CRT pass (scanlines, vignette,
  noise; `dhc_crt_layers`).
- Field manual (PC, Android): lists every animal of the hunt setup with its
  picture. A file gives its diet, size, estimated height and weight, permit
  price, kill score, the areas it is restricted to, and the hunter's kills and
  largest kill for it. The "DHC OS 3D specimens" option (off by default) turns
  the model instead of the picture.
- Field manual areas and armoury: an area's survey (permit, times, issued
  animals and weapons, TOTAL DINOSAURS UNKNOWN, hunts completed there), a
  weapon's file (rounds, restrictions, kills and its longest) and 3D models of
  the area's terrain and of each weapon. Listed names link to their files.
- Kill records keep each kill's size (older records still load), and hunts
  left alive are counted per map.
- Weapon hands: the first-person weapon models in the armoury lose their
  arms. The hand rig is recognised only by its full signature. A mod can
  override it with `HUNTDAT/_HANDS.TXT`. 149 of 842 local weapon files are
  recognised and none is emptied. Hunts draw weapons as before.
- PC VR: the four original guns are prepared as holdable. Guns are not drawn in
  VR yet.
- File notes read as paragraphs.

## 0.6.0 — 2026-11-16

Spawn ecosystem and awareness rules, bounded fleeing, the DHC OS terminal and
launcher look, modern graphics options, memory-planned 3DS hunts and Play
Vanilla. Notes: `docs/releases/v0.6.0.md`. Hardware and by-eye checks are
listed in `testing-needed.md` and remain pending.

### Modern graphics (PC, VR shaders; all off by default except where noted)

- Options > Video > Modern graphics: water waves, depth-tinted water, sky glow
  around the sun and moon, water reflections (the world drawn a second time at
  half size), sun through fog, underwater effects (blue underwater fog and
  tint) and desaturated night. Screen drops after surfacing are held back.
- Fog zones from the map's own fog list fill the ground floor, so a game's
  hollows and swamps carry their own fog colours (`c2_core::fog`). Original
  underwater fog thickens with depth and distance as in the original.
- Night: with night vision off, "Pitch black night" (on by default) darkens
  the world; off, the night keeps its own colours. This was part of dynamic
  time before; dynamic time itself looks as before.
- `c2-pc hunt --modern` turns every Modern graphics option on for one run;
  `--dive MS` places the hunter under deep water or wets the screen for
  captures.
- Fix the APK fog colour byte order (`0x00BBGGRR`).

### Trophy room, terminal, launcher and 3DS

- The trophy room's saved layout (`TROPHY.SPOT`) names the map it was made
  for and is ignored, with a log line, when a mod replaced that map.
- PC DHC OS terminal: pad navigation (D-pad or left stick with key repeat,
  A/B/Start, shoulders), a records page and layout geometry shared by pages.
- Launcher: follows the DHC OS look by default; Settings > Look > Legacy
  restores the earlier look. The missing-file marker is red under DHC.
- 3DS: the DHC OS terminal is on by default (`dhc_terminal = 0` keeps the
  original menus); settings and hunt-options screens are rebuilt in the
  terminal look, with rows re-spaced so the secret-animals line and "Bounded
  fleeing" stay readable.
- Hardcore hunt: Esc at the pickup zone calls the ship.
- Far-animal stagger covers dialect animals and is enabled on the 3DS.
- VR: failed constructors release what they made, frame fences are reset
  just before submit and the exit waits for the device, so an error no longer
  hangs the exit.

### Rules, 3DS memory plan, ecosystem and other changes

- 3DS: a hunt's memory is planned instead of checked pass or fail. The
  picked animals come first (one of each at least), then secret animals,
  ambient animals and, new on the 3DS, aquatic animals (fish, sea animals
  and waders of Modder's Edition games), each as far as memory goes. The
  menu shows "Dinos up to N, ambient M" and names the pick to drop when a
  hunt does not fit. Packs must be cooked again for the plan (pack 5);
  older packs load as before.
- 3DS: a "Secret animals" hunt option for Modder's Edition games; ambient
  animals read AUTO/OFF.
- `c2-cook --plan PACK.c2pk` prints the 3DS memory plan for each area.
- 3DS packs hold an area's repeated sound once.
- Wii U: a Modder's Edition weapon with a muzzle flash (`muzzflash`) shows it
  (`MUZZ4.CAR`, as on the PC and Android) while firing.
- Recipes can carry `[settings.world]`: zones (circles and polygons), player
  starts and extraction points on the areas' maps, in map cells. Builds write
  them to `HUNTDAT/_WORLD.TXT`; the spawn ecosystem uses the zones. Play Vanilla leaves
  the section out.
- Spawn ecosystem (experimental, PC and Android): `[settings.world.ecosystem]`
  sets, per zone drawn in the map editor, which animals appear there, how
  readily, how many at once, and whether they stay inside (hard or soft
  containment); no-spawn zones; caps per species and per map. The rules steer
  the game's own placement and never remove an animal. Edited in the launcher
  (recipe editor > Advanced > Spawn ecosystem); off by default; Play Vanilla
  leaves it out. See MODDING.md.
- Spawn ecosystem: rules by time of day (`dawn`, `day`, `night` override the
  main rule key by key, following dynamic time); spawn safety (distance from
  the hunter at the start, for replacements and for extra animals, a larger
  distance per species, replacements out of view); `avoid` zones that
  wandering and fleeing animals keep out of; replacements that find no room
  or no spot are retried instead of dropped.
- Awareness (experimental, PC and Android): `[settings.behavior.awareness]`
  gives animals states between calm and alarmed. One that almost notices the
  hunter grows suspicious and comes to look; a carnivore that loses him runs
  to where it last sensed him, searches round it and gives up after a set
  time. Stealth crouch helps only until the hunter is noticed. Edited in the
  launcher (recipe editor > Advanced > Awareness); off by default; Play
  Vanilla leaves it out. See MODDING.md.
- Bounded fleeing also covers the stock carnivores' retreat and, in Modder's
  Edition games, shy fish, wary flyers, brachiosaurs and ambient animals:
  each keeps to the map and to the ground or water it lives in.
- Wii U: a "Bounded fleeing (experimental)" row in Settings.
- VR: `c2-vr play GAME --bounded-flee`.
- `c2-regress`: the behaviour script can judge a bounded flight
  (`[behaviours.flee]`; `scenarios/flee_bounds.toml`).
- Launcher: a map editor on the recipe editor's Advanced page draws those
  zones and points on an area's overview picture. A polygon can be moved
  whole, given a corner by dragging from an edge, and lose one by a
  right-click.
- Recipes can carry `[[settings.world.missions]]`: fixed challenges with an
  area, time of day, animals, weapons, equipment, entry and exit points, kill
  requirements, an optional time limit and unlock requirements. They are
  checked and built into `_WORLD.TXT`; no game lists or plays them yet.
- Launcher: the recipe editor's Advanced page gains a mission editor, the DHC
  OS terminal's rules (discovery, licence costs, what each area offers) and
  per-weapon restrictions (tranquilizer darts, trophies). The mission editor
  warns when the DHC OS terminal is off, since missions will be listed there
  only.
- Read rules written for older Modder's Edition releases (`clone`, ambient
  sections, per-animal spawn info, `idleAnim`), in games and in add-on
  entries. Their map ambients, brachiosaurs and sea animals now spawn.
- Read an add-on's entry from a readme beside its `HUNTDAT` folder.
- Keep fish and waders to suitable water when replaced, respawned or added by
  rules; never place a fish on dry ground.
- Observer mode: dangerous fish and the Giant deer no longer attack, and an
  observer cannot be killed.
- Add the PC setting "Water animals on radar" (off by default).
- Carnivores: Ice Age: the Diatryma's kill is synchronized as in the original.
- `c2-regress` runs a game with the rules play generates for it, so Ice Age
  scenarios use Ice Age's AIs; clips are recorded as model clips. Earlier Ice
  Age baselines are superseded.

- Rebuild the regression audit's master report as one filterable page: coverage
  matrix by game, scenario and engine, derived and declared findings, gameplay
  comparisons, matched original/Rust screenshots, compact and full sweeps, full
  or compact views, a blink-comparison viewer, and review statuses stored
  separately in `reviews.json`. Add `--report`, `--serve` and `--only` to
  `tools/regression_audit.py`.
- `c2-regress` rejects unknown flags and flags without a value, never reuses a
  run directory within the same second, and fails native runs whose executable
  lacks a required hook import.

- Make binocular identification follow configured draw distance up to 1,000 m,
  with tighter zoom-sensitive aiming and a shared aim/terrain-visibility point.

- Fix imported animals with a shared run/swim clip using swimming speed on dry
  ground, including APK Diatryma add-ons. Preserve configured speeds and clips.

## 0.5.3 — 2026-11-10

Desktop release bundles, texture import fixes and broader original-engine
regression coverage. Notes: `docs/releases/v0.5.3.md`.

- Add exact-tag Windows/Linux bundle builds and release attachment automation,
  with the launcher, host engines/tools, recipes, base hashes and file manifests.
  Game/mod assets and personal instances are excluded. Console payloads and
  third-party packaging tools remain separate.
- Windows x64 test builds use the static MSVC runtime; launcher startup is
  confirmed in a VM. Bundle hunt acceptance, clean-machine checks and physical
  Windows performance testing remain pending.
- Support more than 254 PC map object types through `.OBX`/`.RSX` side files and
  `c2-pc area-layers`; console packs retain their existing limits.
- Keep phone-menu sprites aligned when base parts are skipped.
- Extend original-engine tours and spawn diagnostics to Modder's Edition games,
  correct the initial tour camera, and test 200 MEE character entries.
- Preserve opaque near-black texels when importing APK models, and fall back to
  internal model names for texture lookup. Add synthetic import/runtime tests.
- Add deterministic texture-review captures, original-engine comparisons, local
  HTML reports and verified lossless capture archives.
- Extend walking/running, death and settled-body regression checks and record
  engine revision provenance. Human review is mandatory.
- Document Steam Deck updates, Desktop Mode setup and controller limitations.
- Add platform comparison screenshots and document bundle validation.

## 0.5.2 — 2026-11-08

Launch protection, predation and regression-tool fixes, and finally some o3ds performance numbers. 
Notes: `docs/releases/v0.5.2.md`.

### Launcher and command line

- An instance whose base game hash list is missing, broken or empty no longer plays unchecked.
  The recipe page explains how to fix it.
- **Use this game anyway** is remembered per instance for that game folder, so it isn't asked
  again on every launch.
- `c2-pc play --recipe`, `recipe build` and `recipe verify` stop when the game differs from the
  recipe's base; `--accept-base` allows another version of the same game.

### Hunt (PC, Android)

- With predation on, a carnivore that finished (or was interrupted in) a meal no longer stands
  eating for the rest of the hunt; it walks off and its AI takes over again.
- With predation on, a carnivore shot dead next to its prey can no longer kill it or eat from a
  body in the moment before it stops hunting.
- With predation on, an animal killed by another one comes back as a new animal of its kind,
  out of the hunter's sight, a minute later, so predators no longer empty the map. The hunter's
  own kills still don't come back, and a predator's kill still scores nothing.
- With a pick limit set (`--max-picks`, used by Android), a refused pick shows **Limit reached**
  instead of doing nothing.

### Gameplay regression scenarios (PC; developer tool)

- Comparing with a baseline fails when the scenario file's text differs, not only its id or
  version.
- Repeats that disagree fail the run. `publish` requires agreeing repeats (`--repeat` 2 or
  more) and the scenario text in the repository.
- The published vanilla baselines were re-recorded from the committed scenario file.
- Kill-animation scenarios (`kill_animations.toml`): each animal is released toward the
  hunter and checked to kill it with the right death, body clip and killer pose. Runs on
  Carnivores 2, Ice Age and three modded packs, and against the original engines
  (`run --engine original`), where Carnivores 2 matches clip for clip.
- A visual check that compared no pictures no longer counts as a pass. `--same-state` fails a
  comparison on differing checkpoint state, and `publish` won't overwrite a baseline without
  `--replace`.
- The rendered pass's checkpoint hashes must match the headless run's.

### Android

- The Android tab warns when two instances' phone names give the same package, and install
  refuses.
- The manifest version matches the workspace again (it had stayed at 0.4.3); a test fails if
  they drift.

### Mods

- A recipe or instance `skip` of 0 is rejected when read from a file, as it already was on the
  command line.

### Nintendo 3DS

- First Old 3DS / 2DS readings for ten packs: about 30 fps with animals in view on most, up to
  45–57 fps with none. JPHL's AREA4–5 and Genesis Redux don't start a hunt yet. Details in
  `3ds-ver/README.md`; the optimisation plan is in `docs/ROADMAP.md`.

## 0.5.1 — 2026-11-07

Two fixes: standalone instances could play Carnivores 2's animals, and observer mode
dropped the night vision and flashlight. Notes: `docs/releases/v0.5.1.md`.

### Launcher

- An instance on a standalone base (Carnivores Triassic, Triassic MEE, JPHL and the other
  non-Carnivores 2 bases) no longer takes the overlays set on the old Game tab. Those are
  Carnivores 2 releases such as the Modder's Edition; copied over a standalone, their
  `_RES.TXT` replaced the game's animals with Carnivores 2's. The hunt menu showed
  Carnivores 2 names over the standalone's pictures, Carnivores 2 animals spawned, and the
  editor's Advanced lists (predation, spawn rules, base game parts) and Balance page listed
  Carnivores 2's animals. Opening or saving such an instance in the editor, or using it
  from the recipe page, saved those overlays into it.
- Instances that already have the Game tab's overlays saved into them drop them when
  played, edited or built. An overlay given to the instance on purpose (one that differs
  from the Game tab's) is kept. Carnivores 2 instances that combine standalones still take
  the Game tab's overlays, as before.

### Hunt (PC, Android)

- Night vision and the flashlight work in observer mode. The menu left them out of every
  observer hunt, though the gear page let you pick them, so N, L, the pad's stick click and
  the phone's light buttons did nothing. Camouflage, scent and the tranquilizer are still
  left out of observer hunts.

## 0.5.0 — 2026-11-07

Regression scenarios for validating changes, instance rules, experimental predation, the first
Android build, a high-end view and hardcore survival. Notes: `docs/releases/v0.5.0.md`.

### Gameplay regression scenarios (PC; developer tool)

- `c2-regress` runs a versioned, deterministic scenario (`vanilla_map2_four_dinos`): area 2, a fixed
  seed and view, four pinned animals, every weapon fired once, binoculars, map, call switch and the
  PC's pickers. It records the manifest, metrics, events, tick and frame times, logs and checkpoint
  screenshots, compares runs with approved baselines (functional, visual, performance) and builds an
  HTML report and a side-by-side screenshot PDF. Carnivores 2 and Ice Age run on PC; Carnivores 1 is
  reported unsupported; the consoles are planned. See `docs/REGRESSION.md`.
- Documented how to use the scenarios to validate fixes and optimizations, AI-assisted ones included
  (`docs/REGRESSION.md`, "Validating a change"), with what a passing run does and does not prove.
  `CONTRIBUTING.md` and a new pull request template ask for the results.
- Published reference baselines from the vanilla Carnivores 2 and Ice Age data (scores only, verified
  against the base hash lists): compare a pull request's runs with `--baseline published`.
- The Android Dinosaur Hunter and Ice Age games (2.0.1) as scenario games (`c2-apk`, `ice-age-apk`), run
  on a folder rebuilt with `c2-pc mobile-base`. Their screenshots cover the phone HUD (compass, wind,
  binocular mask, scope, painted map). Published baselines for both, verified against hash lists of
  the rebuilt folders.
- Scenario version 2: Ice Age (and its APK build) runs on area 4, where all four animals are in view
  from the first landing; on area 2, terrain hid some of them. All four baselines re-published.
- The scenario runner loads Modder's Edition data as the PC hunt does; such games pick species by
  character index. `c2-regress species` lists a game's characters for picking them. Game sections
  can override the landing and view direction, and set-up refuses an animal whose model is a
  `blank.car` placeholder.
- `c2-pc recipe base` hashes the game's files as they are on disk, without the engine's compatibility
  layers. Ice Age data built from the APK got its rewritten `_RES.TXT` hashed instead, so the list
  never matched the folder. The existing PC lists are unchanged.

### Parity fixes from the original-engine comparison (PC, VR)

- The hunt view now shows the original's wind indicator (bottom left) and compass (bottom right) on
  every game not built from the APK, which keeps its own. Hidden with binoculars or scope up, in the
  trophy room, during the death camera, and while Caps Lock is toggled on, as in the original.
- Hunt-setup lists use the original's 16 px rows (was 19 px), so ten rows fit in each box.

### Instance rules (launcher Advanced tab; PC, Android, 3DS, Wii U)

- The Advanced tab (Settings, off by default) now gathers what a modpack maker fine-tunes. Nothing set
  there, the game plays as it was. Recipes keep it all (`[settings.rules]`, `docs/MODDING.md`).
  - **The base game's parts**: which of its areas, animals and weapons are kept (moved here from the
    Base game page).
  - **Times of day**: drop dawn, day or night from the hunt menu; one always stays.
  - **Locked settings**: dynamic time, advanced observer mode, multi ammo, mobile radar, teleport, all
    dinos huntable, secret dinos per map and the survival starting budget can be fixed at a value and
    removed from the in-game settings. The player's own values in the shared settings file are kept.
  - **Spawn rules**: at most so many picked animals per area; secret animals (which, how many, on which
    areas) that always spawn at random spots; and animals kept off the hunt menu that still roam as
    secrets, e.g. herbivores to pick and each area's carnivores to track down.
- **Ammo** per weapon on the Balance tab, for the base game's weapons and add-ons', with the game's value
  on hover.

### Predation (all platforms; experimental)

- **Carnivores hunt herbivores**, an advanced instance setting (recipe editor > Advanced, or
  `[settings.predation]` in a recipe), off by default. Carnivores chase and kill animals below them on
  the food chain (hunt-menu price, one apex, or a custom drag-and-drop order), and the prey runs from
  them. Options: hunter-first or prey-first, carnivore-on-carnivore fights (10% price margin, face-offs
  when closer), per-animal diet, scavenger and defensive tags. A predator's kill is no trophy.
  Experimental everywhere (marked so in the launcher): maps where many animals spawn close together
  can get chaotic. Least tested on 3DS, Wii U, Android and VR.
- A predator eating its kill stops when shot, alarmed or when it notices you; unpriced carnivores
  (e.g. roaming add-ons) no longer hunt each other; prey stops fleeing a predator that is eating,
  resting or chasing something else, and grazes unless it comes very close.
- An animal chasing or fleeing another no longer alarms its pack against you: pack members stay calm
  while one hunts, and a herd fleeing a predator runs from the predator, not from you. A far-off chase
  is no longer heard as if it were next to you.
- Observer hunts no longer apply your saved camouflage, scent or tranquilizer (the radar stays; the
  save keeps them for your next hunt).
- `c2-parity predation AREA.MAP [seconds] [prey]`: a headless run of an area's animals with predation
  on, reporting kills and step cost.

### Video (PC)

- **Foliage distance** slider (Video options), replacing the "Far sprites" switch: how far trees, bushes
  and rocks are drawn as models before their flat far sprite. Leftmost is the original's 24 cells;
  rightmost is models out to the render distance. A saved "Far sprites: Off" carries over as rightmost.
- **High-end view** (Video options, off by default; experimental on Android): render distance up to the
  whole map (1024 cells instead of 384), the far ground drawn in coarser levels so it stays affordable,
  and objects fading from model to sprite instead of popping. Animals behave as at the old maximum.
- `c2-pc area ... --high-end --foliage CELLS` previews both in the map viewer.

### Android (new; menus checked on an Android handheld)

- First Android build (`android-ver/`, `build-apk.sh`): the PC game in an APK. It needs a game built from
  your own mobile APK as its base, plus optional add-ons. No standalones; at most 8 animals and 8 weapons
  per hunt. Touch works in the menus.
- **Faster hunts on phones** (measured on one Android handheld): only what the camera can see is drawn,
  the ground past 128 cells uses coarser levels, textures get mip levels as in the original, and near
  objects draw first. About 47 -> 55 fps at the same settings; higher view settings are now playable.
- **Resolution** option (Video options, 50-100%): draws fewer pixels and lets the phone scale up.
- The frame rate is logged to `c2rs.log` every 5 seconds.
- Standalone games beside the mobile base: `c2-pc combine` now takes a base built from an APK (it has no
  `_MENU.TXT`; its `_RES.TXT` serves as the menu) and keeps its `from-apk.txt`, so the combined game plays
  on Android. Combined on the PC and copied as the app's `game/` (`android-ver/README.md`).
- Android hunts: the mobile game's touch buttons (move stick, look, fire, weapons, binoculars, call,
  map, menu), shown while touching and hidden while a controller is used. Controllers work, built-in
  ones included.
- **Map puts the gun away** (Options, on by default): pressing the map with a gun out puts it away and
  opens the map.
- `c2-pc play --max-picks N`: at most N animals and N weapons per hunt (Android uses 8).
- Launcher: **Build > Android** packages a saved instance (APK-built base) as `c2rs.apk` plus the
  app's `files/` folder, and can install both over adb. The game is swapped in only once fully
  pushed; saves are kept. The app reads an optional `files/balance.toml`.
- Launcher, Android: **Name on the phone** per instance. Empty, the instance installs as the shared
  `c2rs` app; named, it becomes an app of its own (`io.github.c2rs.<name>`) with its own files and saves,
  so a vanilla and a standalone instance can be installed on one device. Needs the Android SDK's build
  tools: the APK is relabelled with `aapt2` and signed with the debug key.
- Fixed: in a game built from an APK, add-on maps, animals and weapons showed only their names in the
  hunt setup's honeycomb. They now show their own menu picture, cut to the cell's shape (also on PC
  with `--mobile-menu`).

### Survival (PC)

- **Hardcore survival**, off by default (Game options > "Enable hardcore survival mode"):
  Modded's waves on Mount Ravan at a night that never ends, with the flashlight on, the
  radar and the shotgun. `c2-pc hunt ... --survival hardcore` starts one directly.

### Launcher

- `[recipe.skip]` leaves out the base game's own maps, animals and weapons beside add-ons
  and standalone games too, so modded content can replace them (instance editor, Base game
  page). Leaving out every area is refused.
- An experimental **Advanced** tab in the instance editor, hidden unless Settings >
  Experimental shows it (its contents: Instance rules and Predation, above).

## 0.4.3 — 2026-11-05

Overhauled Launcher: it is now built around instances, and an instance can no longer play
a game other than its own.

### Launcher

- **Instances first.** overhauled the launcher so that the main focus is the instances.
- Each box has **Play**, **Build** and **Options**. Build asks for the platform (Windows,
  3DS or Wii U) and shows only that builder, with **Cancel** to go back. Options (also on a
  right click): **Edit**, **Clone**, **Fix missing**, **Export…**, **Play in VR**(note VR is 
  in very early stages, only working on vanilla gameplay currently), **Delete**.
- **Edit** has Base game, Add-ons and Balance pages, with **Save**, **Save and exit**,
  **Discard changes** and **Cancel**. Any standalone game can be the base game; the page
  says which game the chosen folder is. Balance has **Refresh list**.
- **Fix missing** looks through the mod and add-on folders (Settings) for an instance's
  base game and downloads, and keeps what it finds.

### Fixes

- **Launch protection:** each instance keeps its own game folder. Play and the builds stop
  when that folder doesn't match the base game's hashes in the recipe; **Use this game
  anyway** lets another version of the same game through. Before, an instance could play
  whatever game was last set on the Game tab.
- Steam Deck game mode: the Auto interface size no longer flips every frame, and no longer
  deadlocks on the first frame.

## 0.4.0 — 2026-11-05

Gameplay options, a reworked survival mode and a much larger launcher. Versions 0.2 and 0.3
were skipped; nothing was released under them.

### Gameplay (core, PC, 3DS, Wii U)

- Opt-in day/night cycle with night vision and flashlight equipment (one at a time) and
  night scoring by the current time. This Feature is left out of the 3ds for the time being.
  PC also has a switchable narrow flashlight beam (K).
- Multi ammo: Half tranquilizer splits each weapon's rounds between lethal ones and darts,
  with a key or button to swap them. A darted animal sleeps and scores as the tranquilizer's.
  This is an optional in-game setting, to both preserve the vanilla gameplay and add a 
  feature I personally always wanted
- Advanced observer mode: instant teleports with no cooldown, flying, and a time stop.
  Hopefully now its easier to take screenshots. This is also an in-game option in the 
  settings.
- Survival rewritten on a points budget. Only Classic and Modded remain, both starting in
  the Mount Ravan ravine with the shotgun and a configurable starting budget. Mobile
  Survive files, dialect waves, Hardcore/Custom and the ring spawn are removed. Fixed
  survival spawns. The death message names the killer.
- PC map: zoom centred on the cursor, panning, and aligned radar and teleport selection.
  Scrollable modifier lists.
- Android-based games always use the phone's menus: round night vision and light buttons,
  touch input, and an animals page with twenty per page. Pad sticks and the D-pad scroll
  menu lists (Steam Deck). There was an issue where the launcher would allow for the PC
  menu to be selected when using ONLY the android apk and would result in an unusable 
  game menu.

### Launcher

- All on / All off for add-ons and standalones. **Pick parts** leaves out areas, animals
  and weapons of a standalone game (`c2-pc --skip`, recipe `skip`). This also works for an
  instance's base game when it is a standalone (PC, 3DS and Wii U).
- Editing an instance covers its balance: a new hunter's points and each area's, animal's
  and weapon's price and score.
- A Menu choice for Android and for each standalone's own menus. **Stop using** moved to the
  Instances page and "Use my APK as the game" moved into the Base game box. Instances are
  shown as wrapping cards with an always visible scroll bar.
- An Interface size setting, and full screen under Steam's game mode / Big Picture
  (Steam Deck). New `steamdeck/` install script.
- Fixed: a standalone that is on is no longer also loaded as an add-on. 3DS and Wii U
  builds no longer stay greyed out when the game was picked as a parent folder. Recipe
  artwork and instance saves are fixed too.
  
### Steamdeck Support

- see [install script](steamdeck/installscript.sh) for the install script, this will help
  make installation on a steamdeck easier and also for updating on the steamdeck! 

### 3DS cooker

- `c2-cook` writes a pack when only some areas are over the memory budget. It names those
  areas and fails only when none fits. Some modded maps might not load on the 3ds but will
  still let you build the 3ds files. Looking into the possibility of shrinking those areas
  down when writing 3ds packs.

### Hardening, CI and code health

- Bound `_RES.TXT` nesting, keep sub-blocks when writing, and reject invalid menu slot
  hints without dropping their files.
- Validate PKM payloads before allocating, bound mobile ELF scans, and reject cooked-pack
  payload ranges outside the file.
- Check texture tiling preconditions in every build. Synthetic tests cover legacy ZIP names,
  blood rendering and small-radius survival attacks.
- Split the shared hunt and survival logic into focused modules.
- Windows VM test notes. Experimental Wii U lighting and controls

See [release notes](docs/releases/v0.4.0.md) for platform limits and validation scope.

## 0.1.0 — 2026-11-02

First tagged source release of the Rust rewrite and console-port tools.

- PC gameplay, mod merging, saved recipes/instances, Ice Age and mobile conversion,
  plus 3DS cooking/build tools, a Wii U demo and experimental OpenXR support.
- Hardened archive extraction, bounded untrusted input and runtime writes, and
  staged generated-install publication. Automatic RAR/7z extraction is disabled.
- Extracted launcher workers into typed jobs with one shared handle; pending
  results cannot be overwritten. Split presentation into focused modules.
- Published a roadmap and documented the release process.

See [release notes](docs/releases/v0.1.0.md) for platform limits and validation scope.
