# Roadmap

This is the public roadmap. Release history lives in [CHANGELOG.md](../CHANGELOG.md);
build instructions and milestone definitions live in [DEVELOPMENT.md](DEVELOPMENT.md).
Implementation does not imply that every platform or combination of mods has been
validated. Game and mod assets must come from the player's own copies.

## Current milestone: v0.7.1

v0.7.1 fixes the Wii U hunt setup's shared structure and the 3DS title page
layout, and brings CI back to green on the current stable toolchain
([v0.7.1 notes](releases/v0.7.1.md)). v0.7.0 added the survival switch,
poisonous fog, scuba gear, advanced binoculars and the DHC OS field manual,
island rules and CRT pass ([v0.7.0 notes](releases/v0.7.0.md)). v0.6.0 brought
spawn ecosystem and awareness rules, bounded fleeing, the DHC OS terminal and
launcher look, memory-planned 3DS hunts and Play Vanilla
([v0.6.0 notes](releases/v0.6.0.md)). Hardware checks for these releases remain
pending on 3DS and Wii U.

v0.5.3 prepared Windows/Linux desktop bundles and exact-tag release automation,
adds PC map object side files, and extends MEE regression coverage
([v0.5.3 notes](releases/v0.5.3.md)). The Windows test launcher opens in a VM;
bundle hunt acceptance, clean-machine checks and physical Windows performance
remain pending. Console payloads and third-party tools remain separate.


v0.5.2 makes launch protection fail closed, fixes predation leaving carnivores stuck or
emptying the map, adds kill-animation regression scenarios, and records the first Old
3DS readings for ten packs ([v0.5.2 notes](releases/v0.5.2.md)). v0.5.1 fixed standalone
instances playing Carnivores 2's animals and restored night vision and the flashlight in
observer mode ([v0.5.1 notes](releases/v0.5.1.md)).
v0.5.0 brought:

Gameplay regression scenarios (`c2-regress`) with published vanilla baselines, used to
validate fixes and optimizations before they are merged ([REGRESSION.md](REGRESSION.md)).
Instance rules and experimental predation in the launcher's Advanced tab, the first
Android build, a high-end view and hardcore survival on PC. Linux PC play is the
gameplay baseline. The 3DS, Wii U, Android and Windows paths still need fresh runs for
the new features. See the [v0.5.0 notes](releases/v0.5.0.md).

## Next priorities

| Area | Work remaining | Validation needed |
|---|---|---|
| PC and mod merging (M3–M5, M9, M16) | Fix mixed-pack roster, spawning, weapon and menu regressions as reported; expand reusable recipes with credits and hashes | Full hunts with representative combinations of player-supplied game and mod files |
| Launcher (M10) | Preserve single-job coordination as features grow; improve import and packaging recovery | Headless regression tests plus interactive import/build/save testing on Linux and Windows |
| Mobile bases (M15) | Refine converted menus, visuals, calls and trophy placement | Rebuild from supported owned APKs and re-cook console packs after converter changes |
| Nintendo 3DS (M6–M8, M11) | Check far terrain seams, frustum/LOD behavior, memory and performance with large packs | Old and New 3DS hardware, including 56/76-cell view ranges and comfort settings |
| Wii U | Validate survival, trophies, saves, pause/exit and modded packaging; improve jungle performance and reproducible cross-builds | Aroma hardware runs and clean-toolchain builds; portable C tests cover only host-testable code |
| Windows | Follow up the Windows VM run with native gameplay and portable distribution tests | CI builds/tests pass; VM log confirms software rendering, with input/performance comparisons pending. Fresh physical Windows machine, interactive launcher and complete hunt still needed |
| PC VR | Hands-on hunting with vanilla guns: floating guns in the hand (no arms), firing from the muzzle, two-handed support for long guns, sights that snap to the eye, a handheld map on the off hand, a head-aimed gamepad fallback; no recoil or reloading yet. Also: audit Vulkan/device/session lifetimes and tune performance | Monado's simulated controllers first, then headset hunts with tracked controllers; runtime diagnostics during scene replacement, session loss and shutdown |
| Android | First APK builds (`android-ver/`): the PC frontend on a phone, with an APK-built base game only, add-ons, standalones combined beside it on the PC, and at most 8 animals and 8 weapons. Touch controls and missing-game messages are implemented; next validate modded picker/map artwork, texture holes and device input | Dinosaur Hunter menus/registration reached a Retroid; full hunts for both mobile bases, add-ons, pause/resume and saves still need acceptance |
| Regression scenarios | `vanilla_map2_four_dinos` runs on PC for Carnivores 2, Ice Age and both APK-built bases, headless and rendered ([REGRESSION.md](REGRESSION.md)); the runner also loads Modder's Edition data. `standalone_map2_four_dinos` covers Genesis Redux locally, on our engine and on its own executable under Wine. Next: behaviour scenarios on it (running, dying, eating, spawning, radar, hunter walk and run speed) against its own executable, a Fallen Kings baseline, then the 3DS, Wii U and Android | Approved baselines per machine; emulator automation to be investigated before any console run |
| Performance figures (3DS, Wii U) | First Old 3DS / 2DS spot readings for ten packs are in [`3ds-ver/README.md`](../3ds-ver/README.md#hardware-results-2026-11-08): about 30 fps with animals in view on most packs. Next: averages for the 3DS and Wii U, for vanilla hunts and representative modded setups. 3DS performance work targets the Old 3DS first, so the game stays playable on every 3DS model | Repeatable measured runs per platform and scenario, labelled by console model rather than individual hardware |
| Limits past the original | Character entries, random sounds and terrain textures are already unbounded (textures to the GPU's array limit). More than 254 kinds of object per area is in on PC (`AREAn.OBX`/`.RSX` side files and `c2-pc area-layers`; 3DS and Wii U packs still use the `.MAP`'s). Planned: a per-animation sound table past 64 animations in a `.CAR`, a compact skin list per animal, random spawn groups by opt-in, and event reactions (design in [EXPANSION.md](EXPANSION.md)) | Round trips of the original formats unchanged; a stress pack past each limit; `c2-parity spawns` for skin rarities |
| PS5 / PS VR2 | Research a dedicated port | No port implementation or hardware compatibility claim yet |

## 3DS performance plan

The Old 3DS readings show that animals in view, not the models themselves, set the frame
rate. The dinosaur models are already much simpler than on PC, so the next gains should
come from doing less work rather than cutting their detail further. In priority order:

1. **A cap on active animals per area**, so simulation and memory stay predictable.
2. **Ambient animal limits that adjust in play** to what the frame can afford.
3. **AI updates by distance**: far animals think less often.
4. **Visibility and occlusion culling**: skip what the hunter can't see.
5. **View distance that adjusts** in demanding areas.
6. **Fewer animation updates** for distant animals.
7. **Trees as sprites by default** in an Old 3DS preset (helped in every comparison so far).
8. **Memory-aware mod loading**: refuse or trim a hunt before it fails to start, as
   JPHL's AREA4–5 do now.

Also planned: per-pack compatibility profiles (tested areas, recommended settings, known
limits), a setting to turn off Ice Age snow, and repeatable 3DS runs (fixed spawn, view
direction, animal count and settings) that record average fps, 1% lows, peak memory and
frame timings, so each change can be checked against these first readings.

## Implemented foundations

- M0–M5: workspace and CI, file readers, PC viewers/gameplay, layered mods and recipes.
- M6–M8: cooker, 3DS gameplay and modded packs; earlier real-hardware testing exists,
  but new changes still require hardware checks.
- M9–M16: Modder's Edition data, launchers, hunt options, profiles/trophies,
  survival, Ice Age, mobile conversion and standalone combinations are implemented
  with compatibility and platform testing still ongoing.
- PC VR: OpenXR menus, movement and animals exist; weapons are not implemented (design in
  `pc-ver/crates/c2-vr/GAMEPLAN.md`, "Hands-on hunting"). Later: gesture reloading, a
  held scope and binoculars, and per-gun VR configs so modded guns can join VR hunts.
- PS5 / PS VR2 dedicated port: research only.

## Further milestones and distribution

- **Add-on variants:** flag duplicate dinosaur names and allow intentional shared
  menu slots, with map-specific eligibility and weighted spawn rates. Reconcile this
  with compact skins and opt-in spawn groups in [EXPANSION.md](EXPANSION.md).
- **3DS compatibility:** user-overridable low-memory presets, ambient species from
  combined loaded rosters, and content-identified pack restrictions/compatibility
  profiles accompany the performance plan above.
- **VR after basic hunting:** calls, HUD, accounts, options/credits, and displayed
  trophies; later gesture reloads, held optics, flashlight and per-mod gun configs.
  Validate each stage with simulated controllers, then tracked-controller hardware.
- **Expanded formats on consoles:** consider OBX/RSX only after cooker and console
  memory-budget checks; preserve original formats and existing PC support.

## Expanding upon the Advanced setting in the launcher
- **All these should be optional and opt-in to avoid messing with the vanilla or intended mod experience** The recipe.toml will be expanded upon to support the configuration of advanced settings, allowing users to share their advanced configs for modpacks or expand upon their original mod. If no advanced features are turned on, these will be ignored, with a popup that notifies the player that a recipe they imported contains advanced settings and which advanced settings, and a quick way to disable if the user doesnt want to play with those advanced features. Allowing players who prefer the classic game, to avoid the advanced settings. Advanced settings are also hidden in the launcher by default. Go to settings in the launcher and enable advanced settings if desired. Also for performance, some of these settings may not be available on the wii u and 3ds due to hardware constraints with the 3ds being the most worrisome. the main focus will be to fully support these on android and PC while leaving the wii u and 3ds versions either dumbed down or disabled entirely. 
- **Advanced Dinosaur Behavior and AI** (the launcher setting; predation is its implemented first part) - configurable species archetypes built on the classic and Ice Age behaviors (fleeing, cautious investigation, territorial defense, pursuit, scavenging, stalking, ambushes, sound investigation, lost-target searches, hesitation, retreat), per-species senses, hunger/thirst-driven travel, migration and habitat preferences, herds with juveniles and circle defense, friend/foe/neutral lists across modded species, an optional auto mode, and an opt-in hunter relationship (animals that like the hunter's company and follow him, or come to trust him through sustained crouched coexistence) with per-species responses to stance, weapons, sound, scent and history. Design and phases: [BEHAVIOR.md](BEHAVIOR.md).
- **Lock a dinosaur to specific maps** the dinosaur selection can dynamically change in the menu depending on what map is chosen, allowing players to make certain maps a requirement to hunt certain dinosaurs. this can also only apply to only certain dinosaurs/maps as well.
- **"secret" dinosaurs can be further configured** make dinosaurs that are not in the hunt menu but found randomly on a map or certain maps, unlockable in the hunt menu after being identified in a hunt (kill/binoculars/death), extend what maps they can be found on, and decide if there is a permanent point cost to unlock the dinosaur license in the menu. the permanent license option is **optional** they can remain harder to find if desired.
- **lock weapons and equipment to certain maps** Allow the weapon selection list to be dynamically shown based on what map is chosen. this an also be only certain maps/equipment/weapons as well
- **Custom menu for compatibility with more of the advanced settings** an advanced menu to support the advanced settings options. for example, when viewing a dinosaur, you can look at an advanced tab, that gives a 3d model of the dinosaur to spin around or shrink/zoom and see what maps it can be hunted on (for the advanced option of locking dinosaurs to specific maps), see what dinosaurs it prefers to hunt or herd-wtih (for advanced behaviors) and dinosaurs they are scared of (if any). Also in the menu, selecting a map will show what dinosaurs are available to hunt (if the advanced setting for locking dinosaurs to maps is enabled) and a total number of hidden "secret" dinosaurs that havent been found just yet - if any are discovered they will be added to the list but their name will glow a different color until the permanent license is unlocked.
