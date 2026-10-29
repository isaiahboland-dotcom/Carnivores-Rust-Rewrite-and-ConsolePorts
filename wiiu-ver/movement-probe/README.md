# Wii U first-map movement simulation

Run `build/c2-wiiu-movement-probe.rpx` in Cemu. This is a text-only
interactive simulation: it loads the base Carnivores 2 first area's original
`AREA1.MAP` and `AREA1.RSC`, runs three scripted host-comparison checkpoints,
then lets the player move while coordinates are printed. It has no 3D scene,
weapons, dinosaurs, menus, sounds or map display.

This machine's game files have been copied from `source/c2-original/HUNTDAT/AREAS`
into Cemu's SD folder at
`~/.local/share/Cemu/sdcard/wiiu/apps/carnivores-probe/`. For another
installation, supply your own base game files named `AREA1.MAP` and `AREA1.RSC`
in `wiiu/apps/carnivores-probe/` on the emulated SD. Do not redistribute them.
The checkpoint coordinates are based on this particular original-data copy;
other revisions may legitimately produce different values.

After loading and preparation, expect `Landing checkpoint: PASS`, `Walk
checkpoint: PASS`, `Sprint checkpoint: PASS`, then `AREA1 simulation: READY`.
The scripted path uses a separate player, leaving the interactive one at the
first landing point. The host baseline is landing `(155008, 5248, 45696)`,
after 2 seconds walking `(155008, 5150.500, 45126.102)`, and after another
2 seconds sprinting `(155008, 5264.768, 43802.586)`. Position tolerance is 4
world units. This validates only that path, not general floating-point parity.

Configure Cemu's Wii U GamePad or Wii U Pro Controller controls, including
both sticks and the A, B, and D-pad up mappings. Left stick moves and strafes; right
stick turns and looks; A jumps; B toggles sprint; D-pad up toggles crouch.
Inputs are digital thresholds in this early test; it uses a fixed 16 ms tick
and is not frame-time calibrated. Coordinates and state print when changed,
at most once per roughly 63 frames. The screen reports when GamePad or Pro
Controller samples are detected, or when neither is detected after about three
seconds. Detection does not guarantee the buttons and sticks are mapped. `Yaw` and `speed` are multiplied by
1,000. Try walking, turning, sprinting, jumping and crouching, and report
whether the values respond. If load fails, the printed status identifies the
stage; -6 usually means the file is missing. The first preparation may take
some time. HOME exit is not required to validate this probe.

Build from the repository root:

```sh
cargo +nightly build --offline --release \
  --manifest-path wiiu-ver/movement-probe/Cargo.toml \
  --target wiiu-ver/movement-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/movement-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_movement_probe.a target/wiiu-movement.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'
```

Cemu result: the user reported AREA1.MAP and AREA1.RSC loaded, all three
landing/walk/sprint checkpoints passed, and the interactive simulation reached
READY with coordinates displayed. The initial Y was 5248. The first GamePad-only build did not respond with
the user’s Pro Controller profile. After adding Pro support, the user reported
A changed Y, B toggled sprint, D-pad up toggled crouch, all four left-stick
directions changed coordinates, and right-stick input changed yaw. Right-stick
pitch, simultaneous inputs, GamePad input, HOME exit and hardware remain
unverified.
