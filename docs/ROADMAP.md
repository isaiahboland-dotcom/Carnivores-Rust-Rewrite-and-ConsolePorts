# Roadmap

This is the public roadmap. Release history lives in [CHANGELOG.md](../CHANGELOG.md);
build instructions and milestone definitions live in [DEVELOPMENT.md](DEVELOPMENT.md).
Implementation does not imply that every platform or combination of mods has been
validated. Game and mod assets must come from the player's own copies.

## Current milestone: v0.4.3

A launcher built around instances (home grid, per-platform builds, Edit pages, Fix
missing, import/export) with launch protection: an instance only plays or builds its
own base game. Gameplay is as in v0.4.0 (hunt options, points-based survival). Linux PC
play is the gameplay baseline. The 3DS, Wii U and Windows paths, and the new launcher on
the Steam Deck and Windows, still need fresh runs. See the
[v0.4.3 notes](releases/v0.4.3.md) and [v0.4.0 notes](releases/v0.4.0.md).

## Next priorities

| Area | Work remaining | Validation needed |
|---|---|---|
| PC and mod merging (M3–M5, M9, M16) | Fix mixed-pack roster, spawning, weapon and menu regressions as reported; expand reusable recipes with credits and hashes | Full hunts with representative combinations of owner-provided assets |
| Launcher (M10) | Preserve single-job coordination as features grow; improve import and packaging recovery | Headless regression tests plus interactive import/build/save testing on Linux and Windows |
| Mobile bases (M15) | Refine converted menus, visuals, calls and trophy placement | Rebuild from supported owned APKs and re-cook console packs after converter changes |
| Nintendo 3DS (M6–M8, M11) | Check far terrain seams, frustum/LOD behavior, memory and performance with large packs | Old and New 3DS hardware, including 56/76-cell view ranges and comfort settings |
| Wii U | Validate survival, trophies, saves, pause/exit and modded packaging; improve jungle performance and reproducible cross-builds | Aroma hardware runs and clean-toolchain builds; portable C tests cover only host-testable code |
| Windows | Follow up the owner VM run with native gameplay and portable distribution tests | CI builds/tests pass; VM log confirms software rendering, with input/performance comparisons pending. Fresh physical Windows machine, interactive launcher and complete hunt still needed |
| PC VR (M17) | Hands-on hunting with vanilla guns: floating guns in the hand (no arms), firing from the muzzle, two-handed support for long guns, sights that snap to the eye, a handheld map on the off hand, a head-aimed gamepad fallback; no recoil or reloading yet. Also: audit Vulkan/device/session lifetimes and tune performance | Monado's simulated controllers first, then headset hunts with tracked controllers; runtime diagnostics during scene replacement, session loss and shutdown |
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
