# Wii U port gameplan

Research and repository assessment, 2026-10-28. This is a plan, not a working
port. The target is a native Wii U homebrew RPX, with Carnivores 2, Ice Age,
and Modder's Edition Engine (MEE) data supported to the extent the shared
engine supports them. Users supply their own game and mod files.

## Starting point

- `pc-ver/crates/c2-core`, `c2-formats`, and `c2-vfs` contain the reusable
  game logic, parsers, and data access. `c2-platform` currently contains no
  usable backend interface; do not assume a new renderer can simply implement
  existing traits. The PC and 3DS frontends each own substantial game glue.
- The 3DS cooker already handles MEE overlays, Ice Age menu art and snow, and
  other data options. Its `C2PK` container holds game files and `C2TX` textures;
  those textures are specifically tiled for the 3DS PICA200. A Wii U must not
  interpret them as GX2 textures. Reuse the container and common cooking
  logic only after verifying compatibility; add a distinct texture encoding
  or use decoded source textures initially.
- Shared code uses Rust `std`, including file I/O and mutexes in the cooked
  VFS. A bare-metal `core`/`alloc` build alone cannot run it. Proving a suitable
  Rust standard library and filesystem implementation is the first technical
  gate; otherwise evaluate a narrowly scoped `no_std`/platform abstraction or
  an alternative runtime before committing to the port architecture.
- The Wii U CPU is big-endian and 32-bit. Audit all binary parsing, save data,
  packed colors, PCM, GPU uploads, casts and shader inputs; explicit file
  byte order does not by itself guarantee correct GPU byte order. The `C2PK`
  index uses 32-bit offsets, so pack-size limits also need verification.
- The Rust/WUT boot probe has been built with the devkitPro Docker image and
  its message was confirmed in Cemu. Real hardware and HOME exit are still
  unverified. See [the Wii U gameplan](../wiiu-ver/gameplan.md) and
  [runtime investigation](../wiiu-ver/runtime-investigation.md).

## Toolchain and platform research

- [devkitPro WUT](https://github.com/devkitPro/wut) provides the Wii U
  homebrew SDK, system headers, libraries and RPX tooling. Its recommended
  installation uses devkitPro pacman and the `wiiu-dev` package. Use the
  official setup instructions for the host OS; record installed versions.
- [rust-wiiu/wut](https://github.com/rust-wiiu/wut) is an archived Rust bindings
  project with a custom PowerPC target; [rust-wiiu](https://github.com/rust-wiiu)
  describes its current work as experimental. Neither establishes that this game's
  `std` and dependencies work: check the project's documented toolchain,
  runtime, maintenance and license, pin a tested revision, and prove it in a
  small executable. Avoid assuming the 3DS nightly configuration transfers.
- [WUT's GX2 samples and libraries](https://github.com/devkitPro/wut) are the
  starting point for native graphics and application lifecycle. The
  [devkitPro SDL fork](https://github.com/devkitPro/SDL) is worth evaluating
  for input, audio and window setup, but its Wii U rendering capabilities and
  Rust integration need a prototype; SDL's 2D renderer is not a substitute for
  the game's 3D pipeline. Prefer GX2 for the initial 3D investigation.
- Test in an emulator where practical and on actual hardware; emulator success
  alone does not establish graphics, audio, lifecycle or performance parity.
  [Decaf](https://github.com/decaf-emu/decaf-emu) is one research emulator.

## Milestones and gates

1. **Toolchain spike.** Install or locate `wiiu-dev`, establish a reproducible
   pinned Rust/WUT build, and produce an RPX displaying a message and exiting
   cleanly. Verify startup, allocator, panic behavior, controller input and
   SD access on hardware. Document exact commands and versions. Gate: a
   repeatable build and boot, not just a host compilation.
2. **Shared-code feasibility.** Compile `c2-formats`, `c2-vfs`, and `c2-core`
   for the target. Exercise allocation, file reads/seeks and synchronization.
   Run synthetic little-endian format and pack fixtures on PowerPC, including
   representative signed values and floating-point fields. Resolve runtime and
   endian issues before importing the frontend. Gate: same parsed values and
   simulation checkpoints as host tests, within documented float tolerance.
3. **Data pipeline.** Open a user-supplied pack from SD, with useful errors and
   separate writable saves. Extract platform-neutral cooker work where it makes
   sense; define a Wii U profile and identify its texture format explicitly.
   Preserve MEE overlays, Ice Age data, recipes and pack selection. Validate
   pack bounds and use synthetic fixtures for automated checks. Gate: inspect
   equivalent asset inventories on host and Wii U for all three game types.
4. **Vertical rendering slice.** Bring up GX2 and present on TV and GamePad;
   implement a minimal terrain, texture, camera and depth path, then models,
   fog, sky, sprites and UI. Start with a synthetic scene and then a user's
   area. Investigate shader build tooling, alignment, cache coherency and
   resource lifetime using WUT examples. Gate: navigate an area with stable
   visuals and measured frame times on hardware.
5. **Playable hunt.** Connect the shared simulation and the needed frontend
   behavior: menu, selection, AI, weapons, HUD, sound, saves, pause and exit.
   Map movement and aim to the GamePad sticks and expose configurable controls;
   choose useful GamePad screen behavior after the basic mirrored display
   works. Handle foreground/background transitions and clean shutdown. Gate:
   complete a hunt and reload a save on hardware.
6. **Compatibility and release.** Exercise C2, Ice Age and MEE data and
   representative add-ons, including variants, spawn tables, audio and saves.
   Compare with PC behavior, profile memory and frame times on real hardware,
   and tune based on measurements. Add reproducible packaging, installation
   instructions and regression coverage; do not redistribute game assets.

Keep each milestone independently reviewable. If the Rust runtime gate fails,
record the failure and evaluate the cost of making shared crates portable
before investing in graphics or a full frontend. The exact SD layout, memory
budgets, frame-rate target, Pro Controller support and secondary-screen design
remain decisions to make after the first hardware measurements.
