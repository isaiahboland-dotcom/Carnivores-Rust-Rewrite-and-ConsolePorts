# PC VR gameplan

Research only, 2026-10-28. PC VR and PS VR2 are future work after the Wii U
port; this document does not start implementation. See
[the PS VR2 plan](../../../psvr2-ver/gameplan.md) for headset-specific testing and
the separate native PS5 question.

## Goal and existing code

Bring the shared Carnivores 2, Ice Age and MEE experience to PC headsets,
using the user's own game data and existing mod workflows. Preserve flat-screen
PC play. Test the compatibility that the existing engine actually implements,
including MEE spawns and variants, rather than assuming every mod works.

`pc-ver/crates/c2-core`, `c2-formats`, `c2-vfs` and `c2-mods` are the main
reuse candidates. `c2-pc` uses wgpu, winit and Kira; its GPU setup chooses a
desktop surface and its camera uses a single projection and mouse look. VR
therefore needs a new frame loop, per-eye views and input, and a renderer
integration; the desktop backend is not already a stereo renderer. The nominal
`c2-platform` crate currently has no renderer trait to implement.

The existing PC launcher GUI should let users choose **PC** or **VR** when
setting up and launching their game. Keep the chosen game and mod workflow
familiar in either mode and make the selection changeable later. Check for a
usable VR runtime when launching VR and show a helpful error if unavailable;
choosing PC must not require a headset. Decide whether the choice selects a
launch mode or separate executables after proving renderer integration. Define
and test save compatibility between modes before sharing saves.

## Platform and rendering approach

- Start with [OpenXR](https://registry.khronos.org/OpenXR/specs/1.1/html/xrspec.html)
  and evaluate the [OpenXRS Rust bindings](https://github.com/Ralith/openxrs).
  Users need a compatible active runtime, headset and controllers. Windows is
  the initial validation target; investigate Linux runtimes and device support
  separately instead of promising every headset works on both systems.
- Prototype an OpenXR session, reference spaces, predicted display time,
  runtime-provided per-eye poses and asymmetric projections, swapchain images,
  frame pacing, focus and session loss. Keep one simulation update per frame or
  fixed step, independent of the two eye renders. Respect `shouldRender` and
  runtime lifecycle. See the [OpenXR frame specification](https://registry.khronos.org/OpenXR/specs/1.1-khr/html/xrspec.html).
- Investigate interoperability between the current wgpu version and OpenXR's
  graphics device and swapchain ownership. Verify runtime-selected GPU, image
  formats, synchronization, lifetime and backend support with a small Vulkan
  prototype. Alternatives include a native Vulkan renderer or an intermediate
  target and copy if supported; neither path is assumed to work or perform
  adequately until measured. Preserve the normal desktop renderer.
- Render world scale, depth, sky, terrain, animated models, effects and UI for
  both eyes. Avoid simply placing the existing first-person camera or weapon
  sprite in front of both eyes. Determine correct transforms and coordinate
  conventions, including headset height, clipping and recentering.

## Interaction and comfort

- Map OpenXR actions to tracked hands/controllers and allow rebinding. Plan
  controller-based weapon aiming, firing, binoculars, map, calls and inventory,
  with a usable alternative for players who prefer conventional controls.
  Validate exact gestures and mappings with users before committing to them.
- Offer seated and standing use, adjustable height, left/right handedness,
  snap and smooth turning, optional smooth locomotion and teleportation, and
  comfort settings such as a vignette. Avoid forced head movement, camera shake
  and motion-locked HUDs; account for climbing, swimming, death and other
  scripted camera behavior. Test comfort with real players.
- Adapt menus and maps into readable, stable VR panels; provide clear reticles
  or controller rays where needed, accessible text and input without a keyboard.
  Aim for spatial audio and verify positional cues without changing simulation.

## Future milestones

1. **Feasibility:** pin tested tooling; make a synthetic stereo scene with
   tracked head pose and controller actions, clean lifecycle and no game data.
   Demonstrate a compatible graphics bridge on real hardware.
2. **Vertical slice:** render one user-supplied area, navigate with comfort
   options, and display a model and usable in-world UI. Measure frame times,
   missed frames and latency against the active runtime's refresh interval.
3. **Playable hunt:** integrate the shared simulation, aiming, equipment,
   sound, saves, menus and hunt completion. Test tracking loss, recentering,
   paused/focused sessions and return to flat-screen mode.
4. **Compatibility and release:** add and test the PC/VR launcher choice;
   validate C2, Ice Age, MEE, representative
   mods and headsets; tune render scale and quality from measurements, retain
   desktop functionality, and document setup and accessibility. No game or
   mod assets are redistributed.

## Status

- 2026-10-29, first draft: `c2-vr` lives in `pc-ver/crates/c2-vr` (moved
  from `pcvr-ver/`). The launcher's Play tab has **PLAY IN VR**, which runs
  `c2-vr play` with the same arguments as `c2-pc play`. It checks the game
  folders and finds the active OpenXR runtime (`c2-vr check` on its own),
  then says VR play isn't built yet. No headset code, no new dependencies.
- Milestone 1 written; checked in Monado's simulated headset (below), not
  yet on a real one: `c2-vr scene` (and `play`
  for now) opens an OpenXR session with Vulkan (`openxr` + `ash`), one
  swapchain layer per eye, head and controller poses in the stage space,
  and a traced test scene: sky, a 1 m floor grid, eight boxes in a ring 4 m
  out (the gold one straight ahead), a ball per tracked controller (blue
  left, orange right). Quits on the runtime's quit or the left menu button
  (Index: B); prints frames, fps and skipped frames at the end.
- To check on the headset: the floor grid at your real floor, boxes about
  1 m tall that stay put as you move your head, both eyes agreeing (no
  double image), the balls following the controllers.
- 2026-10-29, Monado 25.1 (`monado-git`, AUR) with its keyboard-and-mouse
  "qwerty" headset, RTX 3080 Ti, Linux/Wayland: 896 x 1007 per eye, both
  controllers tracked, 60.0 fps over 40 s, clean exit; the window showed
  the scene as meant in both eyes. To repeat (SteamVR stays the default
  runtime):
  ```
  tail -f /dev/null | QWERTY_ENABLE=1 XRT_COMPOSITOR_FORCE_XCB=1 monado-service &
  XR_RUNTIME_JSON=/usr/share/openxr/1/openxr_monado.json c2-vr scene --seconds 30
  ```
  (`monado-service` needs a pipe on stdin; the qwerty controls are in
  Monado's docs.)
- SteamVR's null driver is no use on Linux: its compositor needs a direct
  mode display (`VRInitError_Compositor_CannotDRMLeaseDisplay`).
- PS VR2 on Linux, 2026-10-29: Sony's PC app is Windows only, but Monado's
  own `psvr2` driver runs it (PC adapter on DisplayPort + USB; Monado leases
  the headset's display from the Wayland compositor). Test scene: 2800 x
  2856 per eye at 119.8 fps; the owner looked around and it tracked. No
  Sense controllers under Monado (head only). Start Monado for it with
  `tail -f /dev/null | monado-service`.
- Milestone 2 (first part), 2026-10-29: `c2-vr area MAP` (and `play`, the
  game's AREA1) draws the area's ground and water with the PC's own
  `terrain.wgsl` (naga to SPIR-V), fog in the sky colour, from the landing
  spot at the hunter's eye height; the ground follows the head; left stick
  walks, right stick snap-turns 30 degrees (Index / Touch / Vive / WMR).
  Colours match `c2-pc area` (checked side by side). PS VR2: 119.9 fps over
  90 s, slowest frame 2.8 ms on the CPU.
- Head height: Monado's PS VR2 driver reports a stage space whose floor is
  wrong and moves between runs (the head started 0.37 m, then -0.37 m above
  it). Where there's no stage space, or the head starts under 0.6 m above
  its floor, the first head pose is taken as standing eye height (1.7 m =
  the hunter's 220 units). The owner: "the new head height is awesome".
- Objects, 2026-10-29: the area's trees, plants and rocks with the PC's
  `model.wgsl` (naga), lit as on PC (sun per vertex, ground-lit ones from
  the light map), far ones as sprites past 24 cells; ground, objects, water
  in the PC's order. PS VR2: 119.9 fps over 120 s, slowest frame 2.5 ms.
  Animated objects stand still.
- Sky, 2026-10-29: the PC's `sky.wgsl` (naga): the area's scrolling cloud
  plane, drawn first with no depth test, and the sun (`SUN2.3DF`; the moon
  at night) placed in each eye's view space, so it sits at infinity for
  both eyes. Matches `c2-pc area` in Monado's window.
- Gamepads (`c2-pad`), 2026-10-29: left stick walks (3 m/s, "solid"),
  right stick snap-turns 30 degrees, - / Select quits; checked on the PS
  VR2 with a PowerA Nano. Far trees are whole models now (the owner: the
  sprites "move oddly" in stereo; "way better").
- Main menu, 2026-10-29: `c2-vr play` opens on the PC's main screen
  (`MENUM.TGA`, its lit version and hit map) on a 2 x 1.5 m panel 2.5 m in
  front of wherever you face, standing in AREA1's abandoned military camp
  (south of the bunker, the satellite dish behind). A controller's ray (a
  gold dot where it meets the panel) or the D-pad / left stick picks; the
  pick is lit in a gold box; trigger, A or + takes it. HUNT walks the area
  from the camp (no animals yet), TROPHY loads the trophy room (empty),
  QUIT quits; Options, Credits and Statistics are dimmed. Away from the
  menu, + or - returns to it. Scenes swap in the session (`Scene::next`).
  Checked in Monado's qwerty window (menu, trophy room, back again) and
  119.6 fps on the PS VR2; not yet tried with the pad in the headset.
- Height, 2026-10-29: Monado's PS VR2 floor once put a standing head 0.86
  m up, so a floor under 1.2 m below the head is distrusted now; the pad's
  right stick click recentres. The panel's middle is 0.25 m above the eyes
  (the owner: "solid").
- Animals, 2026-10-29: HUNT runs the PC's `Hunt` as an observer's hunt
  (every huntable species placed; they roam and ignore you), drawn with
  `model.wgsl`, each drawn between its last two ticks. Checked in Monado's
  qwerty window (23 animals placed, one walking past the camp, 60 fps).
- Next: the weapon and shooting, calls, the HUD; the hunt setup screen on
  the panel; trophies in the trophy room.

