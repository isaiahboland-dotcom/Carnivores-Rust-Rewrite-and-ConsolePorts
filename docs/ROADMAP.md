# Roadmap

This is the public roadmap. Release history lives in [CHANGELOG.md](../CHANGELOG.md);
build instructions and milestone definitions live in [DEVELOPMENT.md](DEVELOPMENT.md).
Implementation does not imply that every platform or combination of mods has been
validated. Game and mod assets must come from the player's own copies.

## Current milestone: v0.5.0

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
| PC VR (M17) | Hands-on hunting with vanilla guns: floating guns in the hand (no arms), firing from the muzzle, two-handed support for long guns, sights that snap to the eye, a handheld map on the off hand, a head-aimed gamepad fallback; no recoil or reloading yet. Also: audit Vulkan/device/session lifetimes and tune performance | Monado's simulated controllers first, then headset hunts with tracked controllers; runtime diagnostics during scene replacement, session loss and shutdown |
| Android (M19) | First APK builds (`android-ver/`): the PC frontend on a phone, with an APK-built base game only, add-ons, standalones combined beside it on the PC, and at most 8 animals and 8 weapons. Next: the mobile game's hunt touch controls (stick, fire, weapon buttons), and an on-screen message when the game folder is missing | First run on a phone, for each mobile base: vanilla and with add-ons; pause and resume; saves |
| Regression scenarios | `vanilla_map2_four_dinos` runs on PC for Carnivores 2, Ice Age and both APK-built bases, headless and rendered ([REGRESSION.md](REGRESSION.md)); the runner also loads Modder's Edition data. Next: a Fallen Kings baseline, then the 3DS, Wii U and Android | Approved baselines per machine; emulator automation to be investigated before any console run |
| PS5 / PS VR2 (M18) | Research a dedicated port | No port implementation or hardware compatibility claim yet |

## Implemented foundations

- M0–M5: workspace and CI, file readers, PC viewers/gameplay, layered mods and recipes.
- M6–M8: cooker, 3DS gameplay and modded packs; earlier real-hardware testing exists,
  but new changes still require hardware checks.
- M9–M16: Modder's Edition data, launchers, hunt options, profiles/trophies,
  survival, Ice Age, mobile conversion and standalone combinations are implemented
  with compatibility and platform testing still ongoing.
- M17: OpenXR menus, movement and animals exist; weapons are not implemented (design in
  `pc-ver/crates/c2-vr/GAMEPLAN.md`, "Hands-on hunting"). Later: gesture reloading, a
  held scope and binoculars, and per-gun VR configs so modded guns can join VR hunts.
- M18: research only.

## Release maintenance

Keep this file current when priorities or platform evidence change. Every release
gets a tag, release notes and a short changelog entry. Security work and remaining
limits are documented in [SECURITY-HARDENING.md](SECURITY-HARDENING.md). Track
reproducible bugs with platform, revision, steps and logs without distributing game
assets or private machine identifiers.
