# Changelog

## Unreleased

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

### Android (new; menus checked on a Retroid Pocket 5)

- First Android build (`android-ver/`, `build-apk.sh`): the PC game in an APK. It needs a game built from
  your own mobile APK as its base, plus optional add-ons. No standalones; at most 8 animals and 8 weapons
  per hunt. Touch works in the menus.
- **Faster hunts on phones** (measured on a Retroid Pocket 5): only what the camera can see is drawn,
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
  Experimental shows it. Empty for now.

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
