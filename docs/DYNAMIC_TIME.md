# Dynamic time

On PC, open **Options → Game** and enable **Dynamic time**.
Use the mouse wheel over the game options to reach the lower rows, or click
its up/down arrows. This works in the classic and mobile-style menus.
The setting is saved in `c2rs.cfg` and defaults to off.

Choose Dawn, Day or Night when starting the hunt. With dynamic time enabled,
that selection becomes the starting point of a repeating one-hour cycle:

| Elapsed from a Day start | Appearance |
|---|---|
| 0 minutes | Day |
| 15 minutes | Dusk (uses the authored Dawn assets) |
| 30 minutes | Night |
| 45 minutes | Dawn |
| 60 minutes | Day again |

A Dawn start moves toward Day; a Night start moves toward Dawn. Transitions
blend gradually. The clock follows played simulation time, stops when the
simulation is paused, and resets to the selected time when restarting.
Trophy rooms remain fixed. Turning the option off preserves fixed-time hunts.

On PC, the sky, fog, light maps, object lighting and sun/moon transition together.
Dynamic nights darken naturally; night vision is optional equipment. The original data supplies only three
lighting states, so dusk reuses dawn rather than introducing new assets.
The starting choice still determines spawn tables; animals are not respawned as
time passes. Dynamic hunts award **+20%** for kills between dusk and dawn
(minutes 15–45 relative to a Day start), based on the time of the kill.
This replaces the classic Night hunt's +10%; the bonuses do not stack.

## Night equipment

With dynamic time enabled, Equipment offers **Night vision (150 points)** and
**Flashlight (15 points)**. These prices use the existing hunt loadout budget,
not a permanent equipment inventory. Select what to carry before starting.
Both devices start switched off and reset to off when restarting a hunt.

On PC, press **N** to toggle equipped night vision or **L** to toggle the
flashlight; on a gamepad, click the **right stick** to step through off,
flashlight and night vision. Only one is on at a time: turning one on turns
the other off. The HUD shows their state; a game built from an APK shows the
phone-style round **nv** and **light** buttons at the right edge, which a tap
or a click also switches (the other greys while one is on). Night vision provides the familiar
green view; the flashlight illuminates a soft forward cone with distance
falloff, reaching up to 80 metres. It does not add a shadow-map pass. On PC,
**K** switches it to a narrow beam (less width, twice the reach) and back; the
choice is remembered for the next hunt.

Fixed-time hunts keep automatic night vision and their existing scoring.
Turning dynamic time off hides the extra equipment and clears its PC selection.

## Wii U: experimental

Enable **Settings → Dynamic time (experimental)**. Timing and starting points
match PC, and the option is saved with the other Wii U settings.

The Wii U blends the sky and fog, dawn warmth, manual night vision and average
map brightness. Click the **right stick** for equipped night vision or the
**left stick** for the flashlight. These clicks replace the performance-debug
toggles only during dynamic-time hunts. It retains the starting light map's shadow pattern in cached
geometry; it does not perform the PC's per-cell lighting interpolation.
The same +20% dusk-to-dawn scoring rule applies.
Only the small sky texture is refreshed, once per played second, after the
GPU finishes its previous frame. Terrain and model atlases stay resident.

The option needs console testing for appearance and frame pacing. It is not
exposed on 3DS or PC VR.

## Validation

Synthetic tests cover the cycle boundaries, light/fog interpolation, modifier
viewport bounds and settings persistence. GPU tests validate the pipelines and
read back sky pixels at dawn, day, night and an intermediate blend. Additional
GPU checks verify flashlight cone falloff and manual night-vision toggles.
Scoring tests verify the current-time 20% bonus replaces the classic 10%. Run the
optional GPU tests serially on a machine with an available adapter:

```sh
cargo test --manifest-path pc-ver/Cargo.toml -p c2-pc render::time_tests -- --ignored --test-threads=1
```

Local validation also passed workspace Clippy/formatting, the PC release
build, the shared core's 3DS cross-check, Wii U Rust archive/shader builds and
portable C tests. The final Wii U executable has not been linked or tested
on hardware for this change: the local devkitPro Docker daemon was unavailable.
