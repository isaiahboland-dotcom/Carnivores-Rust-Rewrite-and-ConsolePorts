# PS VR2 gameplan

Research only, 2026-10-28. This is a final-stage objective after Wii U and
the general [PC VR work](../pc-ver/crates/c2-vr/GAMEPLAN.md). There are two separate
targets: PS VR2 attached to a Windows PC, and a native PS5 PS VR2 application.
The first is a PC VR compatibility target; it does not create a PS5 build.

## PS VR2 on PC

Sony's [PC setup guide](https://www.playstation.com/en-us/support/hardware/psvr2/)
describes its adapter, Windows PC requirements, DisplayPort connection,
Bluetooth, SteamVR and the PlayStation VR2 app. Check the current guide and
device support when this milestone starts; headset requirements do not imply
that the game will meet a particular performance target. Set up an OpenXR
runtime compatible with that SteamVR configuration.

Use the PC OpenXR implementation and add PS VR2 and Sense controllers to its
hardware test matrix, rather than building a second PC renderer. Verify the
runtime's controller interaction profile, action bindings, pose tracking,
recenter, rumble, headset removal and session recovery on actual hardware.
Make no assumption that optional runtime extensions or controller features
exist: probe them and provide fallbacks.

Sony states that on PC, HDR, eye tracking, headset feedback, adaptive triggers
and advanced Sense haptics are unavailable; basic controller vibration is
supported. See [Sony's PC and PS5 feature comparison](https://www.playstation.com/en-us/support/hardware/ps-vr2-console-pc-difference/).
Do not depend on those unavailable features for gameplay. The headset's
[published specifications](https://www.playstation.com/en-us/ps-vr2/ps-vr2-tech-specs/)
list 2000 × 2040 pixels per eye and 90/120 Hz panels; use runtime-recommended
render sizes and measured frame budgets rather than hardcoding panel values.

Gate: complete hunts with C2, Ice Age and MEE data on PS VR2 through PC,
including controls, menus, comfort settings, sound and saves, with frame-time
and tracking behavior recorded on real hardware.

## Native PS5 / PS VR2 homebrew feasibility

The intended console route is homebrew. This is a research problem, not yet an
established supported target: public PS5 homebrew toolchains exist, but this
research did not find a documented end-to-end PS VR2 homebrew example. In
particular, a PS5 homebrew app displaying on a TV does not prove that it can
start a VR session, obtain tracked poses, render to the headset or use Sense
controllers. Console model, firmware, homebrew environment and headset
firmware compatibility must be checked together on the actual test setup.

If feasible, provide both flat-screen TV play and PS VR2 play in the PS5
version, with a choice in the game's settings. Flat-screen play must work
without a headset; VR selection should report when a headset or the necessary
VR services are unavailable. Determine through the platform prototype whether
switching can safely happen during a session or requires returning to the main
menu or restarting. Keep saves compatible across modes where practical and
verify behavior before sharing them. Controls and menus must work in each mode.

- [PS5Dev/PS5SDK](https://github.com/PS5Dev/PS5SDK) documents a toolchain for
  payloads and ELF files. [ps5link SDK](https://github.com/Rufidj/ps5link-sdk)
  describes native homebrew titles and system-library linking. These are
  starting points to evaluate, not proof of PS VR2 support; compare maturity,
  licensing, graphics access and supported firmware before choosing one.
- First identify publicly documented headset initialization, compositor and
  stereo submission, display timing, head and controller tracking, input,
  audio, and lifecycle APIs available to homebrew. Verify them with a minimal
  synthetic stereo test on compatible hardware before attempting a game port.
  If those APIs or permissions are missing, investigate clean-room community
  work and record the blocker; do not assume an OpenXR runtime exists on PS5.
- Independently test Rust cross-compilation and C ABI, allocation, file I/O,
  threads and shared-crate dependencies in the selected homebrew environment.
  A flat-screen native boot and then a small stereo/tracking demo are separate
  gates. Preserve C2, Ice Age, MEE and user-provided data as port goals.
- Consider a community Linux-on-PS5 route only as a separate experiment: it
  would need compatible hardware, a working GPU stack, a headset driver and
  OpenXR runtime on that setup. Do not assume the PC VR build or PS VR2 PC
  adapter works there without evidence.

An official native console release is a distinct alternative, not a prerequisite
for investigating homebrew. For reference, Sony says approved

[PlayStation Partners](https://partners.playstation.net/) gain access to its
development tools and documentation after registration and agreement; see its
[developer onboarding description](https://sonyinteractive.com/en/news/blog/showing-your-game-to-playstation/).
The publicly available sources do not establish the project's eligibility,
SDK details, exact Rust support, hardware access or certification requirements.
Confirm those through Sony before choosing a build architecture or making a
release commitment. Sony's [2022 hardware program announcement](https://sonyinteractive.com/en/news/blog/complimentary-development-hardware/)
describes conditional dev/test kit loans; verify current eligibility and terms.

The existing user-supplied game-data and mod workflow needs a suitable console
storage path, transfer method and save layout. Continue to exclude game and mod
assets from the repository and distributed builds.

For any official release, separately assess rights, data provisioning and
platform requirements using authorized documentation.

## Future sequence

1. Finish the general PC VR vertical slice and test PS VR2 on PC.
2. Survey available PS5 homebrew runtimes and VR interfaces, then select a
   compatible console and toolchain for a flat-screen Rust boot test.
3. Prove headset presentation, tracking and Sense input in a synthetic demo;
   if feasible, port the shared simulation and frontend.
4. Implement and test the flat-screen/VR setting and its unavailable-headset
   behavior; validate C2, Ice Age, MEE, saves, accessibility and performance
   in both modes on hardware.

No PS VR2 or PS5 implementation or hardware test has been performed yet.
