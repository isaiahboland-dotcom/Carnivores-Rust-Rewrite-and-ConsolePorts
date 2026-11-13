# Diatryma movement investigation — 2026-11-11

The owner confirms the fast-legs/slow-travel symptom on PC and reports a
possible 3DS occurrence via their son. The affected base/pack and model source
are still unknown. Earlier walking stalls have not yet been reproduced.

## Local asset comparison

Read the PC Ice Age model, its exported loose add-on, the Ice Age 2.0.1 APK
loose add-on, and the prepared APK base. Both PC copies have SHA-256
`49ffe4b7757781d65bc98312b7029aaff5abb1fa6f1e606ebcd90ff3302ed6c0`;
both APK copies have SHA-256
`2bdb87b77f9d10280d2a3d9aa2d95d41bac3a3b90592298eafb8d7fa816718c3`.
The latter matches the Diatryma asset pinned by the c2-everything and
c2-all-in-one recipes; this does not identify the owner's affected pack.

Both models have nine clips in the same order. Walk is CAR clip 0, 15 frames
at 14 keys/s; run is clip 1, 15 frames at 20 keys/s. PC names are `Dm_wlk`
and `Dm_run`; APK names are numbered. Neither has a swim clip. The importer's
known PC layout assigns these walk/run roles correctly.

| Import path | AI | Run / walk speed (units/ms) | Swimming | Radius |
| --- | --- | --- | --- | --- |
| PC Ice Age loose import | 16 (Spinosaurus) | 1.28 / 0.448 | Disabled | 220 |
| APK loose import on C2 | 15 (Velociraptor donor) | 1.2 / 0.428 | Enabled, speed 0.4, no swimAnim | 256 |
| PC / APK Ice Age base | Native AI 14; Ice Age dialect layer maps to 16 | Layer supplies 1.28 / 0.448 | Disabled by layer | 220 |

These are configured speeds, before scale, turning, acceleration and collision.
The native base's raw AI 14 must not be interpreted as C2's AI 14 without its
Ice Age layer. APK bases and APK loose add-ons take different import paths.

## Reproduced shared-engine defect

An omitted `swimAnim` defaults to logical phase 0. In the APK loose entry,
`runAnim = 1` is registered first and becomes logical phase 0. The movement
code selected run speed, then overwrote it with swim speed solely because the
phase matched. It did this on dry ground too. The run clip continued normally.

`shared_swim_clip_keeps_land_speed` parses this configuration shape and measures
actual displacement on flat, empty terrain at scale 1 after acceleration.
Before the fix it fails with **400 units/s**, expecting **1,200 units/s**, while
remaining in the run phase. The fix requires the on-water flag before applying
swim speed. It changes no authored speed, animation timing or collision tuning.

The test covers run/swim and walk/swim sharing on land and in deep water:
1,200 units/s running, 428 walking, and 400 swimming (10 units/s tolerance).
Its controlled navigation target keeps pursuit/turning out of the measurement;
this is a synthetic simulation reproduction, not a captured hunt in the
owner's unidentified pack.

Collision remains a separate question. Both dialect imports use the shared
`move_character_avoid` path. Its water-collision flag is `!can_swim`, so these
imports differ in water avoidance. The empty-ground reproduction has no
scenery or avoidance regions and isolates speed selection from blocked travel.
No evidence yet establishes what caused the earlier walking stalls.

Validation: all 19 `dino::huntable::tests` passed on the host, including the
previous missing-eating-clip regression. `cargo check --locked -p c2-core
--no-default-features --features portable-load` passed. The corresponding
portable unit-test build is blocked by the existing missing `alloc::vec`
macro import in `game/weapon_info.rs:100`; no console runtime claim is made.

Pending acceptance is tracked under DIATRYMA in `testing-needed.md`: identify
the pack/model, retest PC and physical 3DS, and capture walking stalls with
terrain/obstacle context. Keep the TODO open until that evidence exists.
