# Decisions and deviations

Newest first. Each entry: what, why, and what it affects.

## 2026-10-30 — Wii U: warmer dawn, the PC's map on R, map settings, button hints (Wii U and 3DS)

Dawn on the Wii U is tinted warmer than the original's slot-0 light map: the
world texels and fog are scaled by (282, 266, 210)/256 (`DAWN_TINT` in
`lib.rs`), as the owner found the dawn too pale on a TV. This is a Wii U-only
look; the PC and 3DS draw the original's dawn unchanged.

R now draws the PC's map over the TV view (`MAPFRAME.TGA` with the area's
overview at (11, 23), centred, at twice the original's size on 1080 lines)
instead of switching the TV to the GamePad's full-screen map. Settings gain
the PC and 3DS options: "Map look" (the original's green dotted range and red
hunter, or the mobile game's yellow frame, line and view cone; mobile is the
default as before) and "Teleport" (off by default). With teleport on, a tap
on the GamePad map starts the core's teleport (5 s, then a minute's
cooldown) straight away; the 3DS asks for a confirm tap because its map is
small, the GamePad's is not. A button on the GamePad map zooms out to the
whole area and back.

"Button hints" (off by default) shows the controls along the bottom of the
view as the PC's hint bar does: on the Wii U in the overlay, on the 3DS on
the top screen (two lines, following the crouch-button setting). They hide
over a notice, a question, the pause, the map and the binoculars' readout.

## 2026-10-30 — Wii U: trophies, kill notice, survival, trophy room, exit prompt

The Wii U had no way to leave a hunt but HOME, so nothing could be kept. Now
+ is the original's Esc (the exit prompt; the residency diagnostic it was
went, as the owner pressed it looking for a pause and got 7 fps) and D-pad
Right the pause, the owner's layout: A leaves, B stays, Y restarts unsaved.
Leaving writes `trophy00.sav` in the original format, with its character
numbers (the hunter first; this port keeps him last, so +1 on save and -1 on
load) and the original weapon numbers (the hunt keeps carried slots;
converted when the trophy is taken), so a save moves between PC, 3DS and Wii
U. Kills go to `trophy00.rec` (`c2_core::record`, now built without std, as
is `profile`) on leaving or dying, as the PC does. The kill notice and plaque
are the PC's (its layout and text, `TROPHY_G.TGA`), drawn by Rust into a
small texture as the menus are and alpha-blended by the GPU. Survival uses
the core's (`SurvivalOptions::pick`, at most 8 a wave); the trophy room the
core's `place_trophies` with every species already loaded.

## 2026-10-30 — Time of day: slot 0 is dawn, slot 1 is day (all platforms)

`TimeOfDay` numbered `Day = 0, Dusk = 1`, and the menus map Dawn to `Dusk`
(entry "Menu mapping" below), so Dawn loaded slot 1 and Day slot 0. The
original says slot 0 is dawn: `Game.cpp` gives `OptDayNight` 0 the low sun
(`Sun3dPos.y` 2048, shadow 0.7) and 1 the high one (4048, 0.5); the menu's
`DAY1.NFO` is "Hunting time: Dawn"; in all five areas light map 0 averages
~150 against ~225 for map 1, and sky 0 is the warm one; the mobile
`survivalDTM` and our dialect mappings (`hunt_view.rs`, `rules.rs`) already
number dusk 0, day 1. The owner noticed on the Wii U that Dawn looked like
the normal game. Now `Dusk = 0, Day = 1` (the name "dusk" kept; the menus say
Dawn), with `sun_pos` following. Every lookup goes through `time as usize`,
so PC, 3DS, VR and Wii U all change; the 3DS packs' `SKY0..2.TX` are by slot
and need no recook. Supersedes "day, dusk, night" in FORMATS.md.

## 2026-10-30 — Wii U: the view model gets its own depth slice

The gun was drawn with depth off and clip-space z = 0, so its triangles
painted over each other in file order: the double-barrel's shells showed
through the stock while firing (all guns but the crossbow) and the pistol's
arm showed through the hand. As on the 3DS (`C3D_DepthMap(-0.02, 0.98)`), the
world is now drawn into depth 0.02..1 and the gun into 0..0.02 (by viewport),
with its own depth test and writes; its vertices get real depth over
1..4096 camera units (`hunt_setup::GUN_NEAR`/`GUN_FAR`). A host test checks
every frame of every weapon animation fits (the farthest vertex is 589 units
ahead; parts behind the camera are clipped as before).

## 2026-10-30 — Wii U: HD menus drawn in Rust; every hunt option

The owner found the Wii U menu low quality: a 512x256 texture (the 3DS's
size), a 5x7 font and point sampling. Asked to choose, they kept the 3DS
design (hexagon grids, time icons, gear) over recreating the original
800x600 screens as the PC does, and asked for every option the core has.

- The menus moved from C to Rust (`wiiu-ver/perf-probe/src/menu.rs`, drawing
  in `src/ui.rs`), so the host tests can drive and render them. C passes
  buttons and touches and shows two textures: 1920x1080 (TV) and 854x480
  (GamePad), drawn from one 1280x720 layout, redrawn only on change. Scaled
  pictures are cached; the TV redraw is ~4 ms on the PC (from 22).
- Text uses the console's system font (`OSGetSharedData`, read at runtime,
  nothing bundled), rasterized with `ab_glyph`; without it (an emulator
  without system fonts), the 8x8 `font8x8` font, smoothed.
- New: dawn/day/night (night: the night sky and light map, and night vision
  on the world's atlases), camouflage, cover scent, double ammo,
  tranquilizer, observer mode (no weapons), run speed, ambient animals,
  metric/US descriptions, and Exit. The original descriptions
  (`MENU/TXT`) and EQUIP1-4 joined the install (143 files).
- The picks and settings persist in `settings.txt` beside the app.
- The two-second blue/magenta display check before the menu is gone (the
  hardware review called it temporary); the display re-enable calls stay.

## 2026-10-30 — Wii U: resident terrain and water; scenery updated per crossing

The resident-scenery capture (`wiiu-ver/perf-probe/hardware-logs/2026-10-30-resident-scenery-results.md`)
showed turning frames bound by CPU-projected terrain (~13 ms) and water
(~4 ms), and every cell crossing rebuilding ~190k scenery vertices (~106 ms
frames). With residency on (PLUS):

- Terrain and water are written in world space once into the existing
  148x148-cell buffers and projected by the shader, like scenery. Slots are
  toroidal (cell mod 148), so a crossing rewrites only the entering row or
  column; 4x4-cell blocks are culled with a sphere around everything they
  hold and drawn as merged ranges. That draws ~17% more terrain triangles
  than per-cell culling (all off-screen), and a host test checks every
  triangle the CPU path drew is still drawn, on all five maps.
- Fog mode -4 (world space, terrain fog start 11366.4) was added to
  `terrain.vert`; -3 (CPU-projected) remains for the PLUS bypass.
- A crossing now writes only the scenery objects that entered the window,
  into the gaps left by objects that went out of it (exact size first: the
  models repeat), and drops the ones that left. Scenery is no longer
  strictly in the original near-first order between rebuilds. Blending is
  off, so only equal-depth ties can differ; objects are kept sorted by
  buffer position so visible neighbours still merge into one draw.
- The scenery buffers grew from 524,288 to 1,048,576 vertices (48 MiB with
  fog, from 24). The second capture showed a full rebuild every ~13 s in
  Manya Jungle, and measuring every cell showed a third of AREA4's windows
  (up to 742,053 vertices) and parts of AREA1, 2 and 5 never fitted, so
  they used the old per-frame path. Now every window fits; a host test runs
  6,400 jungle crossings with one rebuild.

## 2026-10-29 — The README for GitHub; the roadmap kept locally

Before publishing (owner's request), `README.md` became the public page:
why the project exists, each platform's state, and how to install the PC,
3DS and Wii U versions from the command line or the launcher. The design
spec that was README §1-§6 and §8 moved, with its numbering, to
`docs/DEVELOPMENT.md`, so "§N" references elsewhere point there (older
entries below that say "README §N" mean the same sections). The roadmap,
milestone status and known issues (README §7 and Status) moved to
`TODO.md`, which is gitignored: it's the owner's working list.
`DEVELOPMENT.md` §7 keeps a table of milestone names, since code comments
cite them ("M13").

Also: CI installs the ALSA and udev headers (kira and gilrs need them on
Linux), and `c2-parity` gained the `teleport` hunt option it was missing.

## 2026-10-29 — PC VR: the hunt setup on the panel; the panel drawn on the CPU

HUNT on the VR main menu now opens the PC's hunt setup screen
(`MENU2.TGA`, its lit version, `M2_MAP.RAW`) with the game's own area and
animal names, prices, pictures (`PICS`) and texts (`TXT`: areas, animals,
`DAY1-3.NFO`, `OBSERVE.NFO`), laid out as `c2-pc menu.rs` lays them out.
Offered: the area, the animals (several), dawn / day / night, observer
mode, BACK and HUNT. Weapons and equipment (marked "Not in VR yet"), fog
and the tranquilizer are dimmed, not hidden, so the screen stays the
PC's. Hunts cost nothing: VR has no account yet. A hunt starts at one of
the area's landing spots; an observer's hunt places the original's random
mix (as `c2-pc launch_hunt`), otherwise the picked animals are the targets
and can kill you, which returns you to the camp menu after the original's
12 seconds.

This needs text, so the panel's picture is now drawn on the CPU (`canvas.rs`)
at twice the menu's 800 x 600, with the PC's proportional 8x8 font
(`font8x8`, as `c2-pc hud.rs`), and uploaded only when it changes; the
shader just shows it with the pointer dot. That replaces the main menu's
one-shader drawing (entry below); a canvas is simpler than a text pipeline
and costs nothing while the menu is still.

The pad moves a gold focus box: up and down a list go row by row (it
scrolls); otherwise the nearest thing in line with the focus that way
(sharing its rows, or its columns), else the nearest within 45 degrees;
nothing further off, so BACK's left is nothing rather than DAWN. A or +
takes, B goes back. Checked off the headset by drawing the canvas from the
real game data to pictures and stepping the focus through every item.

## 2026-10-29 — PC VR: animals, in an observer's hunt

HUNT in the headset now runs `c2_core::game::Hunt`, the PC's own
simulation, ticked every 10 ms as `c2-pc hunt_view.rs` does, with the
hunter put where the head is each tick (and `alpha` from the head's
heading; no controls, the sticks move the head). As a first step it is an
observer's hunt (`-observ`): animals are placed, roam and feed, but ignore
the hunter, and there are no weapons or calls, so the AI and drawing can be
judged before shooting comes in. With no hunt setup screen in VR yet,
every huntable species is a target (placed as the original places
targets), at the default density.

Characters are drawn by the PC's `model.wgsl` (`animals.rs`, sharing the
objects' pipelines and set): each frame every character in view range is
posed (`dino::pose`, as `c2-pc dinos.rs`), its faces expanded into one
per-frame vertex buffer, one instance each, a texture layer per `.CAR`.
Ticks and 120 Hz frames don't line up, so each character is drawn between
its place before and after the last tick (the PC does this for its camera
only); poses aren't blended. The trophy room is still empty (trophies need
the player's save), and the ship isn't drawn.

## 2026-10-29 — PC VR: trusting the floor less; a recenter button

On the first menu run Monado's PS VR2 put the standing owner's head 0.86 m
above its floor, which passed the old 0.6 m check, so the world was too
high ("like how it was before"). A floor under 1.2 m below the head is now
taken as wrong (the head as standing eye height); the game is played at
standing height anyway. The pad's right stick click recentres any time
(standing height from the head, and the menu in front again): `c2_pad`
gains a VR-only `Recenter` action. The panel was raised twice at the
owner's word: its middle is 0.25 m above the eyes (it was 0.15 m below).

## 2026-10-29 — PC VR: the main menu on a panel in the camp

The owner's design: you start in a corner of the first area, the abandoned
military camp, with the PC main menu standing where you face at launch;
point with a controller or use the D-pad, a yellow box on the pick, A to
take it. The camp is AREA1's man-made cluster around cell (645, 292): the
satellite dish (objects 22-24), the bunker and huts (25-31). The head is put
at (645.5, 302.7), looking north to them.

The panel draws the game's own screen (`MENUM.TGA`, `MENUM_ON.TGA`,
`MAIN_MAP.RAW`) in one shader, not by re-running `c2-pc`'s wgpu menu: the
pictures are an sRGB texture array (so no colour conversion), the hit map an
integer texture, and the pick (lit, gold box), the dimmed items and the
pointer are push constants. Only what works in VR is offered (Hunt, Trophy,
Quit); the rest are dimmed rather than hidden, so the screen is the PC's.
The name and account plaques are empty (no text drawing in VR yet).

It stands in the tracking space, not the world, and walking and turning are
off while it shows, so it can't be walked away from. It's drawn last with
no depth test, so camp objects never cut it. Controllers point with their
aim pose and select with the trigger; the gamepad's D-pad up / down (crouch
/ next call in the hunt) and the left stick step through the items, A or +
takes one.

Scenes can now hand over to another (`Scene::next`: stay, quit, load): the
frame loop waits for its frames, frees the old scene's targets and the
scene, and builds the new one on the same swapchain. HUNT stays in the camp
scene (the panel goes, the sticks walk); TROPHY loads `TROPHY.MAP` (start
cell (76, 70) as on PC), and leaving it loads the menu again. `-` quits only
in the menu (`Scene::handles_back`); elsewhere it and `+` go back to it.

## 2026-10-29 — PC gamepad: menu pointer, button hints, the teleport map
- Menus: a drawn pointer (`c2-pc pad_cursor.rs`; Wayland lets no program
  move the system one) fed to the menus as the mouse, so every screen
  takes it unchanged; A clicks (held: held), B is Escape, + is Enter; the
  mouse hides it. "A Select  B Back" shows bottom right meanwhile.
- Hints follow the device used last (pad or keys/mouse). The exit prompt
  (A leave / Y restart / B resume; keys Y / R / N), pause and the teleport
  map always show theirs; Options > Controls "Button hints" (off by
  default, the original's look) adds map, binoculars, switch gun, jump,
  crouch, from the player's bindings for the keys.
- Teleport map: D-pad 64 cells/s (the owner found 40 slightly slow), right
  stick up to 160 (curved), ZR marks, A goes; meanwhile the D-pad and A
  don't crouch, change call or jump.

## 2026-10-29 — Gamepads: a `c2-pad` crate (gilrs) for PC and VR
The owner asked for native gamepad support on PC and in VR. `gilrs` is the
README's named choice; a small shared crate (`c2-pad`, a layout change)
reads every pad as the game's actions so `c2-pc` and `c2-vr` agree.
- Layout: the Wii U plan's, so the console-style frontends match. Face
  buttons by position, Nintendo style (A right = jump, B bottom = run, X
  top = call, Y left = next weapon).
- PC hunt: the left stick walks past a third of the way (the hunt's
  movement is key-like; speed is the run toggle), the right stick adds to
  the mouse look (900 px/s full over, squared for fine aim, scaled by the
  mouse sensitivity), D-pad up toggles crouch; with the exit prompt up, A
  is yes and B no. Menus and the weapon pick screen still need the mouse:
  they point and click, and Wayland lets no program move the pointer (a
  drawn pad cursor would be next).
- VR: the left stick walks, the right stick snap-turns, − quits.
- Pads `gilrs`'s database lacks get their raw codes guessed; `c2-pad`
  carries SDL-format lines for such pads (the owner's PowerA Nano: the
  database's PowerA Core line under the Nano's id, checked against its
  evdev events) and gilrs reads `SDL_GAMECONTROLLERCONFIG` for more.
  `cargo run -p c2-pad --example pad-events` / `pad-state` show what a pad
  sends and what the game reads.
- `c2-pc` sets `HuntOptions::distant_ai` off (the Wii U's staggered far AI;
  the field came with the Wii U work and left the PC build broken).

## 2026-10-29 — PC VR terrain: the PC's WGSL through naga; scale; gamma
- `c2-vr`'s area scene compiles `c2-pc/src/shaders/common.wgsl` +
  `terrain.wgsl` (read with `include_str!` across the crates) to SPIR-V with
  `naga` 30 (the version wgpu already uses; `wgsl-in`, `spv-out`), at start,
  so the terrain is drawn by one shader on PC and in VR. Descriptor sets
  follow the WGSL groups. Its own fragment entry (`shaders/terrain_vr.wgsl`)
  is `fs` plus an sRGB decode: the PC renders in gamma space into a
  non-sRGB surface, the headset's images are sRGB.
- Scale: `UNITS_PER_M` = `HEAD_STAND` (220) / 1.7 m, so a standing player
  sees from the hunter's eye height; the tracking floor sits on the ground
  under the head (eased).
- Each scene implements `c2_vr::scene::Scene` (render pass, per-eye targets,
  recording); `xr` keeps the session and frame loop.

## 2026-10-29 — PC VR: OpenXR with raw Vulkan (`openxr`, `ash`), unsafe in one module
The owner approved the `openxr` crate. `c2-vr` draws with Vulkan through
`ash` 0.38 (already in the lock through wgpu), not wgpu: OpenXR's
`XR_KHR_vulkan_enable2` has the runtime create the Vulkan instance and
device, which wgpu 30 can't adopt without its `hal` layer; for a first
scene raw Vulkan is less code and less risk. Revisit when the game's
renderer comes to VR (share `c2-pc`'s wgpu renderer via `wgpu-hal`
interop, or port it).
- `openxr` with its default `loaded` feature: the loader is opened at run
  time, so `c2-vr` starts without one installed and says so.
- `unsafe_code` is denied workspace-wide; `c2_vr::xr` allows it (FFI handles
  between OpenXR and Vulkan). Nothing else in the crate uses it.
- Each eye is drawn into its own swapchain layer (two render passes) rather
  than multiview, so no multiview feature is needed; the test scene is one
  fragment shader tracing each pixel's ray (no vertex buffers, no depth),
  its eye pose and tangents in push constants (96 bytes).
- Shaders are GLSL in `crates/c2-vr/shaders/`, compiled with `glslc` and the
  `.spv` committed (our own code), so building needs no shader compiler.
  Recompile both after editing: `glslc fullscreen.vert -o fullscreen.vert.spv`,
  `glslc -O test_scene.frag -o test_scene.frag.spv`.
- Tracking space: stage (floor at 0) where the runtime has it, else local
  with the floor 1.6 m below the start. Controllers: grip poses and a quit
  button suggested for the simple, Index (B), Touch, Vive and WMR profiles.

## 2026-10-29 — PC VR first draft: `c2-vr` in pc-ver, PLAY IN VR in the launcher
The owner chose to keep PC VR inside `pc-ver/` (`crates/c2-vr`, its plan
moved from `pcvr-ver/gameplan.md` to `crates/c2-vr/GAMEPLAN.md`) rather than
a workspace of its own. It is a separate binary from `c2-pc` so the flat
game never links OpenXR or needs a headset. The launcher's Play tab has
PLAY IN VR beside PLAY; it runs `c2-vr play` with exactly `c2-pc play`'s
arguments (unknown options ignored), so the game and mod workflow stay the
same in either mode.
- No new dependencies yet (README §8 asks first): the active runtime is
  found as the OpenXR loader finds it (`XR_RUNTIME_JSON`, XDG config dirs,
  `/etc`; the registry on Windows via `reg`), reading the manifest's name and
  library. The loader stays the final word once `openxr` is added.
- Until the renderer exists `c2-vr play` stops after its checks with a
  plain message, and the launcher runs it as a waited-for step (not
  launched) so its findings land in the log.

## 2026-10-29 — Launcher: a Wii U builder (base game only, the rest placeholders)
`c2-launcher --bin c2-wiiu-builder` (and a "Wii U" tab in the full launcher,
off until Settings > "Show the Wii U section") sets up a Wii U install the
way the 3DS builder does: same game folder, overlays, add-ons and standalones
tabs. It lives in `c2-launcher` beside the 3DS builder rather than under
`wiiu-ver/`, since it is the same app (one `Mode` more); the Wii U app itself
stays in `wiiu-ver/`.
- What it builds today is what the Wii U app reads: the original game's
  files as they are (no cooked pack, no balance), flattened into the fixed SD
  folder `wiiu/apps/carnivores-probe/` (`main.c`). The launcher can output
  `carnivores-probe.rpx` for Homebrew Launcher or package that RPX as
  `carnivores-probe.wuhb` for Aroma using devkitPro's `wuhbtool` from
  `wut-tools`. The game files remain beside the executable because the
  current runtime reads them from that SD folder. The file list
  (`c2_launcher::wiiu::assets`, 143
  files) mirrors `wiiu-ver/perf-probe/stage_assets.py`; change both together.
  Files are found in any case. A base that isn't plain Carnivores 2 (Ice Age,
  a Modder's Edition style game) is refused for now.
- Add-ons, overlays, standalones, Ice Age, APK imports and balance show as
  greyed "not on the Wii U yet" lines; the lists the user sets up are
  kept, so the build can use them once the port reads them. The install name
  is fixed until the app reads its folder from where it was started.
- The Wii U builder has no Balance tab (nothing would apply it).

## 2026-10-24 — PC: one kill notice
After a trophy kill the PC showed the original's plaque (`TROPHY_G.TGA`)
and the kill box added in M10 (after the 3DS's). The box now shows only
when there is no plaque (tranquilized, or no trophy for that animal).

## 2026-10-24 — The mobile game's radar and teleporting (options)
- **Mobile radar** (PC Options > Video, 3DS Settings "Mobile map"): the map
  gets a yellow frame, its view range circle in yellow, and inside it a
  yellow slice for what the hunter sees (the camera's width: 77 degrees on
  PC, 90 on the 3DS's top screen), its edge rounded along the circle; no
  heading tick.
- **Teleport** (PC Options > Video, 3DS Settings): pick a spot on the map,
  shown as a circle, then confirm, as the mobile game asks (PC: with the map
  open the mouse moves a marker, a click puts the circle, Enter confirms;
  3DS: tap the map, then Confirm; its zoom moves to a corner button). After
  5 s (a countdown on
  screen) the hunter stands on the nearest dry, reachable cell; then a
  minute's cooldown. Not in a survival (its waves come one way) or the
  trophy room. Core: `game/teleport.rs`.

## 2026-10-24 — Owner's requests: survival, profiles, map edges, PC map heading
- **Survival**: unlimited ammunition in every kind (was custom and modded
  only); wave animals start at 3 health at most (`WAVE_HEALTH`): the
  original's shotgun is 6 pellets of 1, so a solid blast or two glancing
  ones kill; a rifle still kills at once.
- **3DS menu**: a survival's top screen says "Survival" and lists no gear;
  "Hunter" is "Profile". The new-profile keyboard's OK never lit: ctru-rs's
  `set_max_text_len` also sets fixed-length validation, so a name had to be
  exactly 20 letters; now any non-blank name up to 20.
- **Map edges**: the hunter is held to cells 36..980 (`CheckCollision`);
  animals walked, spawned and aimed out to cells 4..1018, out of his reach
  behind that invisible wall. They now keep to the same box
  (`collide::WALK_CELLS`), a deviation from the original.
- **Survival fence** (classic and modded, the ones with a spawn box): wave
  animals keep to the ground between the box and the hunter, the cells
  reachable from him without climbing more than 4 height steps above the
  highest point of the lowest way to the box (minimax over the map's
  heights, 40 cells round both). On Mount Ravan that is the ravine floor:
  they can't run up its walls, the way on is straight at the hunter. One
  stepping off is put back and pointed on; the way-finding and spawn spots
  keep to it. "Ours" (spawning round the hunter) has none.
- **PC map heading**: Options > Game "Map: where you look" (off by default):
  the 3DS map's tick on the PC map.

## 2026-10-24 — Menu animals marked by `overwrite charN`
One game marks its menu animals not with `callNo` (it writes `callNo = -5 1`,
which the engine reads as -5: answers no call) but by an `overwrite charN`
block that gives the picked animal its radar mark and target spawns; its
models are set per map (`overwrite areaN { file }`, `blank.car` elsewhere).
Combined, those blocks are tied to the game's own maps, so the animal picked
on another game's map didn't come and the radar showed nothing (PC and 3DS).
- `c2_formats::mee::menu_slot`: `callNo` when 0 or more, else (huntable
  AIs only) the slot of its `overwrite charN`. The 3DS's roles, the cooker's
  figures, the roaming add-on lists and the picked-anywhere rule use it.
- `c2_core::mee::read_picked_anywhere`: a picked animal joining another map's
  targets gets `radar = TRUE` too, and a placeholder model (`blank.car`)
  becomes the first of its maps' models.
- The 3DS roaming add-on list in the pack (`roam.N`) is the cooker's, so
  the menu opens without resolving every map.

## 2026-10-23 — The owner's 3DS hardware test: fixes; one pack for a combined game
- **Gyro aim** (stealth crouch): 1:1 with the console was too twitchy. Now a
  2.5 °/s deadzone (hand tremor) and a setting, *Gyro aim* 25/40/60/100 %
  (default 40 %).
- **A dialect ambient's `callNo`** is 0 when not given, as the engine reads
  it, so every ambient counted as the first menu animal (green on the radar
  whenever that one was hunted). `Hunt::call_bit` is `None` for a dialect
  animal outside the huntable AIs (10-29); `radar = TRUE` still marks one.
- **Dialect scopes**: `optic` is the zoom (`atof`, `CameraW *= Optic`), not
  on/off with the original's 3x; `cross` (default off) draws the cross-hairs,
  also without a scope; `unzoom` drops zoom and cross while drawing, hiding,
  reloading. Original data keeps 3x and its cross.
- **Survival spawn spots**: a box made for the mobile maps takes in the PC
  maps' mountain sides. Spots with a walkable way to the hunter (the waves'
  flow field, where it reaches) within 1536 units of his height are tried
  first; then the old rules.
- **Every 3DS survival goes through the hunt options** (run speed, ambients),
  as the owner asked; classic and modded used to start from the title.
- **A whole combined game as one 3DS pack** (the owner's "all of them"):
  an ambient whose tables spawn it only on other maps (a combined game ties
  each game's ambients to its own maps) isn't loaded, and the cooker's per-map
  figures count only that map's (from about 46 to 23 MiB heap a map); menu
  picks, targets and carried weapons are `Bits` (the menu stopped at 64
  animals, targets at slot 117, weapons at 32); roamers take slots after the
  menu's own; the grids page (< > on the bar, D-Pad) and pictures load for the
  page shown (21 MiB of them in all).
- **PC roaming add-ons are pickable**: a dialect game's roamers are listed
  after its menu animals and hunt by those slots (`c2_core::mee::roamers`,
  `pick_roamers`), as on the 3DS.
- **Combining**: map files in a folder under `AREAS/` (a maker's spare copies)
  are not the game's maps; one standalone's backup replaced its real map.
- Menu: the settings file and save are written only when changed.

## 2026-10-23 — Fixes from the first 3DS baseline run
- **Doubled slashes in paths** (`ANIMALS/X//Y/m.car`, written so by one
  standalone): Windows reads them as one, so the VFS key drops empty and
  `.` parts (`c2_vfs::key`). Those models were left out of 3DS packs (the
  pack check reported them on every hunt).
- **A dialect game without spawn tables, played alone** (one standalone
  uses the dialect's animation keys but its own engine's placement): no
  animal came. `c2_mods::vanilla` adds a layer (`standalone::spawn_layer`)
  with the Modder's Edition's three default spawn groups (ambients, random
  roster, picked), each ambient in the first, each huntable in the second
  and, picked, the third (`overwrite charN`, its menu slot as `callNo`).
  Entries with their own spawn blocks are left as they are.

## 2026-10-23 — `.cia` art from the game's own menu picture
- Owner's request: when no icon or banner is picked, they are made from a
  game's main menu picture (`MENU/MENUM.TGA`) on the player's machine, at
  build time; nothing is kept in the project. The banner (256x128) is its
  top band at 2:1 below the name / account bar (the logo and the menu's
  dinosaur); the icon (48x48) a square round the dinosaur (centre at 70%
  across, 45% down; every menu checked has it there). Box-filtered down.
- Which game: the 3DS tab's "from" choice: the base game (default), any
  standalone in the list, or plain placeholders. A picked file still wins.
- Tried and dropped: the last animal's menu picture as the icon (one game's
  is a camouflage pattern) and the middle band as the banner (cut the logo).

## 2026-10-23 — M16: standalones in the launcher
- **PC**: the Add-ons tab lists standalone games (a folder each, as
  extracted; on / off); Play passes the enabled ones as `--standalone`, so
  they are combined beside the base (the overlays and add-ons as before).
- **3DS** (owner: one pack per standalone): the builder cooks
  `<game>.c2pk` from each enabled standalone's game folder ("A pack per
  standalone game" in the 3DS tab, on by default), beside the original and
  add-ons packs; the app's pack chooser picks one at start, each with its
  own save. Checked: an original-engine standalone cooks and passes
  `c2-cook --check`.

## 2026-10-23 — M16: standalone recipes
- A recipe lists `[[overlays]]` (releases copied over the base game, the
  Modder's Edition) and `[[standalones]]` like mods: id, credit, URL, folder
  hint and the hash of every data file, by path in the game's folder (the
  one holding `HUNTDAT`). `[combine] ambients` says how they combine;
  `[[mods]]` are add-ons over the combined game (read against it, with the
  recipe's resolutions and patches).
- Games are found under `--mods` by folder name or id, else by matching
  hashes, their game folder up to five folders down (downloads nest it).
  A recipe listing overlays uses them instead of `--overlay`; that also
  answers add-on recipes needing the Modder's Edition.
- Only the pinned files are checked; the combining reads the game folder
  as it is (a standalone's files are its whole game).
- Checked: a recipe of the Modder's Edition, three standalones (one for the
  original engine, one five folders down) and two add-ons, verified and
  built from a folder of downloads under other names.

## 2026-10-23 — M16: standalone games combined (`c2_mods::standalone`)
- **Owner's choice**: the combining is public, generic code (users drop
  their own standalones in); no standalone is named in public files. The
  3DS keeps one pack per standalone; the PC combines them all.
- **How** (`build`): the base is the game with the Modder's Edition over it
  (its `_MENU.TXT` is required). Each standalone gets the next area numbers,
  huntable and weapon slots (menu pictures, texts, calls renumbered;
  `_MENU.TXT` extended), its files under `HUNTDAT/MODS/<game>/` (weapons as
  `WEAPONS/<game>_<file>`), its characters appended with `areaN` / `charN`
  conditions, `callNo` / `fearCall`, spawn and pack table numbers and
  `hunterAnim` (merged hunter) moved. **Tied**: a game's animals spawn on its
  own maps only (the base's on the base's); `--ambients random` lets
  ambients roam every map. Same-name animals get " (<game>)"; a weapon the
  game already has (same name and model) is shared.
- **Fixes on real data**: models a condition switches to per area
  (`overwrite areaN { file = ... }`) are moved too (they pointed nowhere);
  a menu listing more areas than the game has maps keeps those it has.
- **Original-engine standalones** (`to_dialect`), and dialect-looking ones
  without spawn tables: that engine fixes each AI's animation order, gait
  and placement, so each animal takes the base's same-AI animal's animation
  and movement keys (unless it has its own) and its spawn blocks, its
  "picked" condition moved to the animal's own slot (only the spawn part: the
  donor's score or radar there are its own); the base's spawn and pack
  tables come along. The original's brachiosaur (AI 6; the dialect's 6 is
  the pig) takes the base's brachiosaur. An AI the base doesn't have (a
  patched engine's extra slot, 19) takes a hunter's kind if `danger`ous,
  else a grazer's, with roles from its model's animation names
  (`run`, `wlk`, `die`, `slp`, `idle`, `jmp`, `swim`, `eat`, `roar`...);
  its own health, mass, score and senses stay.
- `c2-pc combine GAME... --base --overlay [--mod] [--ambients] --out`;
  `c2-pc play BASE --overlay ME --standalone DIR...` builds it in the data
  dir (`packs/combined-...`); add-ons over it are kept to the base's maps,
  then the balance file.
- Checked: the owner's 13 standalones (6 dialect, 7 original-engine or
  similar) combine in under a second into 86 areas, 130 huntables and 101
  weapons; `c2-parity mee-sim` places and runs animals on each game's areas.
  Open: a hunter model with another body (one game's) keeps its kills
  unmerged.
- **Past 118 huntables**: picked slots (`mee::Hunt::picked`) and hunt
  targets (`HuntOptions::targets`, the no-call bonus, placement) are a
  512-bit set (`c2_formats::bits::Bits`) instead of a `u128` (slot + 10
  left 118); `MAX_CALL` follows. The 3DS keeps its 64-animal menu and
  converts at the boundary; weapons stay a `u128` (128). Stray entries
  without a name in an original-engine game are left out.

## 2026-10-23 — M9: the wary flyer; the AI families done
- **Survey**: every dialect `_RES.TXT` here (game folders, add-ons, the
  APK imports) uses AI 0-6, 10-27 and 30-35, all ported already. None uses
  28 (Titan), 29 (Micro), 36 (wary flyer), 8 (poacher) or 9 (dog).
- **Wary flyer** (`AnimateWaryDimor`, 36; manual 1.2: "Wary Flyer AI",
  needs `hearK` / `lookK` / `smellK`): the flyer of `mee_flyer` plus
  alarm. Alarmed by `CheckAfraid` (a sniffer: state 2), its pack, or the
  hunter within 812 (5-10 s); it flies straight away weaving
  (`tGAIncrement` 2), scared 15-20 s while he is within 2048, calm past
  4096 unless its pack is alarmed (then a target 4048 off). Its
  `alarmCall` sound plays once on alarm, then instead of its usual calls
  while alarmed. Placed like the other flyers (at `minalt`).
- **Not ported**: Titan is in the upstream source (`AnimateTitan`) but
  marked unfinished and not in the manual; Micro's AI is commented out
  upstream (its animals stand still there). Both keep the interim
  original AI (Ceratosaurus, Velociraptor) so a mod using them still plays.
- Checked: a synthetic wary flyer (spooked, calls, flies off, calms);
  `c2-parity mee-sim` over the Modder's Edition and the five other dialect
  game folders we have: no failures, slowest step 0.71 ms.

## 2026-10-23 — Survival modes: classic, modded; ours benched
- **Owner's choice**: the survivals offered are the mobile game's classic
  (from the Carnivores 2 APK; on Ice Age with its stand-ins) and a dialect
  game's own ("modded"). Ours (and its hardcore / custom kinds) is benched:
  the code stays, nothing offers it unless `--survival-modes` names `ours`.
  A game with neither (plain C2, no APK import) has no survival button.
- **One button**: SURVIVAL on the PC main menu and the 3DS title; with more
  than one mode, a chooser (PC over the menu, 3DS on the bottom screen).
  `--survival-modes classic,modded` (`c2-pc`, `c2-cook`'s `INFO.TXT`); the
  launcher's two checkboxes (older saved choices still read: `Ours` meant
  no classic).
- **Modded** (the Modder's Edition's `-survival`; the modernized fork's
  `ScriptParser` for `common {}`): `survivalArea` (1-based), `survivalWeapon`
  (one-based there; an index here), `survivalDTM` (0 dusk, 1 day, 2 night);
  the area's `areatable` entry gives `survivalPlayerX/Y/A` and the spawn box
  `survivalDinoXMin/XMax/YMin/YMax` (cells). The roster is the characters
  with `survivalIndex` in its order; a later entry with the same index takes
  it over (`SurvivalIndex[n] = TotalC`). Deviation: the engine counts every
  `survivalIndex` line (`SurvivalIndexCh++`), so a repeated index made it
  reach past the roster into type 0 (the hunter); we count the roster.
- **Waves** (`PlaceCharactersSurvival`): wave `n` has `2^n`; the `k`-th
  costs `3^k`; the dearest that fits first, as often as it fits. Worked out
  to the engine's limit of 26 (then the 26th again), capped at our wave size
  (the dearest kept). From the box (`spawn_in_box`), the hunter at the spot
  facing `survivalPlayerA`, unlimited ammunition, the next wave 2 s after
  one falls (`WaveNoteTime`). 3DS: only roster animals the pack loaded.
- Checked: a dialect game's pack (`c2-cook --check`): a roster of five
  (an Allosaurus first, a T-Rex last); five waves growing as the rule says,
  each coming to the hunter.

## 2026-10-23 — M9: the dialect's weapons
- **Scope**: the keys the dialect games here use (counted over every
  `_RES.TXT` we have): `land_veloc`, `land_fall`, `recoil`, `shake`,
  `fullAuto`, `canRun`, `cannotMortal`, `radar`/`radR/G/B`/`radTime`, the
  animation slots (`getAnim`, `shtAnim`, `putAnim`, `rldAnimFull`).
  `game::MeeWeapon`, present when a weapon has any of them; plain C2
  weapons are unchanged.
- **Bullets** (`AddBullet`, `AnimateBullets`, `AnimateBullet`): a round
  flies `64 * veloc` units per 17 ms, its fall speed growing by
  `land_fall` a step; each tick traces the stretch it covers; gone on a hit
  or past the view range. Scaled by the tick (the engine steps per frame).
  A weapon without `land_veloc` keeps the original's instant trace (the
  engine's would never move). Fix: `land_fall` was read as the original's
  whole-number crossbow flag (0.6 -> 0), so the dialect crossbow didn't drop.
- **Recoil**: the view kicks `-recoil/100` rad up and up to half that
  sideways, moving each 17 ms and easing 0.005 rad a step (the dialect's
  camera code). **Shake**: the sway starts at `shake * 4` when drawn.
- **Firing**: a dialect weapon fires once per pull unless `fullAuto`
  (`alreadyFired` / `FiringMode`); the original's keep firing while held.
- **Running** (`Hunt2.cpp`: `RunMode && Weapon.state == 0`): only with the
  weapon away, as the original; we let the hunter run with it drawn before
  (undocumented). The dialect's `canRun` weapons allow it.
- **Trackers**: an animal hit by a `radar` weapon shows on the map (PC and
  3DS), radar or not, in the weapon's colour, for `radTime` ms (0: for good).
- **Animation slots**: the model animation for draw / fire / hide / reload;
  the sound stays the slot's own (the dialect plays the animation's
  `Anifx`; every dialect weapon here numbers them 0-3, so the same).
- Not yet: muzzle flash, bullets drawn in flight (`bModel`), chambered
  rounds and part reloads (`rldAnimPart`, `picc`; one game's double shotgun),
  underwater stats, harpoons, retrieving arrows, held breath.
- **Weather** in the dialect is the area table's snow, done with M14.

## 2026-10-23 — M2 polish: far sprites, `RandomMap`, ground light
- **Far sprites** (`_RenderObject`, `RenderBMPModel`, `LoadBMPModel`): past
  `ctViewRM` = 24 cells (3D distance) an object but an `ofNOBMP` one is its
  RSC 128x128 sprite on an upright quad over the model's x / y extent,
  turned to the camera, light `mlight - 16` (ground-lit: 112). The
  original's top-edge start (`mxy` from vertex 0's x) is kept.
  `c2_core::model::sprite_mesh` makes it a plain mesh (texel 0 see-through),
  so the PC draws it as a model. PC: on by default, option "Far sprites"
  (`far_sprites` in `c2rs.cfg`), `c2-pc area --models` to compare. 3DS: the
  cooker packs each sprite at the model textures' sharpness (`SPRn.TX`,
  64x64 n3ds, 32x32 o3ds); the app draws them out to the ground's view
  (objects used to stop at 28 / 24 cells), batched per object kind on the
  CPU; packs without sprites draw as before.
- **`RandomMap`**: `rand() * 1024 / RAND_MAX` per run in the original; ours
  is the MS generator from seed 1, fixed. Objects lit by the light map lose
  `RandomMap >> 5` (0..31).
- **Ground light** (`CalcModelGroundLight`, `GetLandLt2`): the PC object
  shader reads the light map (a storage buffer) under each vertex, bilinear
  between every other cell. The 3DS (no texture reads in the vertex shader;
  per-placement buffers would cost Old 3DS video memory, for at most ~600
  objects an area) takes `GetLandLt2` under the object for the whole model.
- Checked: PC shots of AREA1 with sprites and with `--models` match in
  place and size; AREA3 (607 ground-lit objects) renders; the 3DS hunt runs
  at 60 fps in Azahar on an o3ds pack.
- Seen, not changed: the original's water light shimmers per cell
  (`168 - wdelta * 24`, `wdelta` from `RandomMap` and time); ours is flat.

## 2026-10-23 — Classic survival on Ice Age, in place of its Arcade
- **Owner's choice**: Ice Age's own mobile Arcade isn't ported. The Ice Age
  game has our survival, and the Dinosaur Hunter's classic survival when
  that APK has been imported (`mobile-survival.txt`), "for now".
- **Stand-ins** (`mobile_arcade::STAND_INS`, `Survive::for_game`): an
  animal of the mode the game lacks becomes an Ice Age one alike in role:
  Allosaurus the Smilodon, Velociraptor the Wolf, Spinosaurus the Bear,
  Ceratosaurus the Diatryma, Chasmosaurus the Wooly rhinoceros. A game with
  the originals (C2, the combined game) is unchanged.
- **Where**: the 5th area, as the mobile mode uses Mount Ravan's slot. The
  box and the hunter's spot are Mount Ravan's, so with stand-ins the hunter
  keeps the normal start and the waves come round him as in our survival;
  the waves, their sizes and the timing stay the mobile game's.
- `c2-cook --check` now plays the first five classic waves of a pack that
  has the mode (hunter kept alive; each wave felled once it has come).

## 2026-10-23 — M15: numbered mobile animation sets by their clips
- **Bugs found**: numbered sets were given roles by count. 11 clips took the
  Dinosaur Hunter's order (death 6th), and other counts played as the
  donor's when the donor had as many. Both wrong in shipped imports: Ice
  Age's Andrewsarchus, Doedicurus, Hyaenodon and Titanis (death 5th) growled
  with their death and died into the sleep pose; the Pachycephalosaurus on
  a plain C2 donor idled with its death. Old imports must be made again.
- **Two layouts**: the Dinosaur Hunter's 11, and the PC one, which Ice
  Age's mobile animals keep (its Bear, Wolf, Mammoth... match the PC models
  clip for clip, frame counts and speeds): walk, run, idles, death, sleep,
  extras (jump, trophy poses).
- **Roles from motion** (`mobile::roles`): the death is the one clip whose
  vertices end below 80% of their starting mean height (exactly one per
  animal in both APKs); 11 clips with it 6th keep the Dinosaur Hunter's
  order; else walk / run are the first two (the slower cycle walks), the
  clips before the death idle, the one after it sleeps if it lies as the
  death ends, and the extra lifting the body highest (over 1.2x) jumps
  (Wolf's and Smilodon's known jumps are picked). No falling clip, or
  several: not imported. Named sets and the tables are checked the same
  way (their death must fall).
- **Known PC layouts** (`PC_SETS`): the Diatryma is Ice Age's `Diatr.car`
  and the Pachycephalosaurus the Modder's Edition's `PACH.CAR`, clip for
  clip; their slide and trophy pose get no role. Only role orders are kept,
  read from the PC files' animation names.
- **Donors on an Ice Age base**: after each Carnivores 2 kind, an Ice Age
  one (raptors the Wolf, Carnotaurus the Smilodon, Gigantoraptor the
  Diatryma, Pachycephalosaurus the Wooly rhinoceros...); they all became the
  Brontotherium before. The Diatryma takes the Velociraptor's kind on C2.
- **The PC games' own animals** in the APKs (C2's `allo`, `tirex`...; Ice
  Age's `bear`, `wolf`...) aren't imported: they come from the PC files
  (with roles found, Ice Age bases got `Rapt73` and `Tirex1e`). The Diatryma
  is the exception, for players without the Ice Age import; a game with one
  skips it by name.
- Checked: both APKs on C2 + ME and on Ice Age; a pack with all 18 imports
  hunts each on every area (`c2-cook --check`); the Indricothere and
  Diatryma render in the viewer.

## 2026-10-23 — Recipes for loose add-ons
- A loose add-on in a recipe is marked `loose = true`; its `files` are paths
  from the add-on's folder (not from a `HUNTDAT` parent) and cover what the
  loose reader used: placed files, the hunter model, and only the text files
  an entry was read from (a readme without one changes nothing, and pinning
  it would make an edited readme a mismatch).
- Building, the add-on is read by the same loose reader, limited to the
  pinned files (`loose::read_only`), so a stray text with another entry in
  someone's download can't add an animal. Placement depends on the base
  too; a Modder's Edition recipe needs the same `--overlay`.
- `locate` matches loose recipe mods only against loose folders, and
  `HUNTDAT` ones only against `HUNTDAT` folders.
- Checked: the owner's 14 add-ons (on the Modder's Edition) as a recipe
  build the same game as `mod build`, every file identical.

## 2026-10-23 — All-in test; engine options file; Ice Age + Android on Ice Age
- **All in** (C2 + ME + 14 add-ons + both APK imports + the Ice Age import):
  builds with 3 conflicts, all understood: the Dilophosaurus add-on and the
  mobile import's (now: ids that clash are told apart instead of refused,
  and mobile imports load first so the PC add-on wins — PC is the default
  where both exist); `Edmonto.car` from Anatotitan and SkinPack (the skin
  pack's, as meant). The 3DS pack hunts every animal on all 10 areas;
  Majungasaurus still shares the Ankylosaurus's slot (its own `callNo`).
- **3DS menu: up to 64 animals** (was 32; the all-in game has 34): animal
  masks are 64-bit; past 40 a 10-column grid of smaller hexagons.
- **Engine options** (launcher Settings: Export / Import options): one TOML
  with the balance file (its prices, scores and the PC-or-mobile choices,
  which are balance edits), the survival choice, the menu style, the PC
  game's `c2rs.cfg`, the 3DS target and title art; no paths, no game data;
  what a file leaves out stays.
- **Add-ons on a dialect base without `_MENU.TXT`** find "slot n" by the
  menu's order (the n-th animal with AI 10+), not AI 9 + n (the original
  game's rule, kept for it): Ice Age rewritten in the dialect numbers its
  AIs otherwise, so the APK's animals on an Ice Age base became roamers.
  On Ice Age they take fitting donors (Andrewsarchus the Bear's, Doedicurus
  the Brontotherium's, Hyaenodon the Wolf's, Titanis the Diatryma's).

## 2026-10-23 — M14: the combined game (Ice Age beside Carnivores 2)
- **An import, like the APK one** (`c2_mods::ice_age::import`, `c2-pc iceage`,
  launcher "Import Ice Age..."): from the player's Ice Age folder, add-ons
  for their Carnivores 2 + Modder's Edition game (the entries are dialect;
  a plain C2 base is refused as for mobile animals):
  - one per animal: model, entry, menu picture and texts, call icon and
    sounds, and Ice Age's `HUNTER1.CAR` as `hunter_ice_age.car`. The hunter
    merge (`crate::hunter`) appends its kill animations to the game's hunter
    (same 475-vertex body) and moves each entry's `hunterAnim` (the wolf's 5
    becomes `Hr_wolf` wherever it lands). Each entry is presented as made for
    one of the base's menu slots (Ice Age's slot modulo the base count) so it
    is added in a slot of its own (`SlotAdd`) rather than left in a slot
    past the base's that another takes (the Yeti, Ice Age's tenth, collided
    with the Bear); its own `price` keeps Ice Age's.
  - "Ice Age areas": its maps as `AREAn` after the base's last (6–10 on C2)
    with `.c2map` descriptors (name, price, picture, text) — numbered, not
    named, so the 3DS cook (which packs `AREAn` only) takes them too.
- **Snow there**: the dialect gives snow by area number (its `areatable`),
  which an add-on can't extend in place, so the `.c2map` carries
  `snow = TRUE` (ours; the Modder's Edition ignores unknown keys). PC reads
  it for the map; `c2-cook` lists such areas in `INFO.TXT` (`snow = ...`)
  for the 3DS.
- Ice Age's ambient animals (pig, archaeopteryx) are imported too, spawning
  only on Ice Age's areas (`addition area6 area7 … { spawninfo }`; C2's maps
  keep their own ambients).
- Conditions on picked slots (`overwrite charN`) were checked for slots
  0–31 only: in an all-in game (35+ menu animals) a picked animal past
  slot 31 didn't get its "picked" spawn group or radar. Now 0–117.
- Checked: the combined pack (C2 + ME + the import) hunts all 19 menu
  animals on all 10 areas (`c2-cook --check`, Old 3DS budget).

## 2026-10-23 — M14: Carnivores: Ice Age as a base game
- **How**: Ice Age's engine is C2's with its own animals' AIs (its `_RES.TXT`
  `ai` 10–19 are Brontotherium … Yeti, 1 the pig, 2 the archaeopteryx). We
  have no Ice Age source; the Modder's Edition re-implemented those AIs
  (its AI numbers 20–27, ported here in M9), and ships `_RES ICE AGE.TXT`
  to run Ice Age in its engine. So an Ice Age folder plays as a dialect
  game: `c2_mods::ice_age` rewrites its `_RES.TXT` (a layer over the
  player's files, in `c2_mods::vanilla`, so PC, cook and launcher alike):
  per Ice Age AI the dialect AI, animation indices into Ice Age's own
  models, speeds, idles, deaths, kills (Ice Age's `HUNTER1.CAR` has the kill
  animations), spawn groups, and snow in every area. The per-AI values are
  written out as ours after the Modder's Edition's file (credited there);
  the animals' own numbers (health, mass, scores, senses), the weapons and
  the prices stay the player's file's.
- **Differences from that file**: Ice Age's own models (its Diatryma has no
  swim animation, the ME's `diatr_s.car` adds one; its weapons are Ice
  Age's, not the ME's re-made ones); no trophy-room spots (Ice Age's room
  places trophies as the original); no halved score for non-targets (as
  every dialect game here, owner's rule); the Yeti is not hidden (`hide`).
  Diatryma runs as the dialect's spinosaur and the Yeti as its bear, as in
  the ME (Ice Age's own AIs for them are unknown).
- **Snow** (the ME's `areatable snow{}`, `AnimateElements` / `RespawnSnow`):
  up to `dens` flakes in a 12-cell box round the hunter, falling at `vSpd`,
  drifting with the wind and a sway, lying ~2 s while fading; PC draws them
  as the ME does (soft dots), the 3DS at most 300 as small depth-tested
  diamonds.
- **Saves**: Ice Age's hunters are its own (PC: `ice-age/` in the data dir;
  the 3DS keeps a save per pack already).
- PC: `c2-pc play ICE_AGE_DIR` builds a combined game folder (named after
  it) even without add-ons; the launcher names the base "Carnivores: Ice
  Age".

## 2026-10-23 — M13: survival as the owner wants it
- **Ours**: the only screen is the gun pick (PC: the setup screen with only
  the weapons list, the kind switch in the bottom bar; 3DS: the gear screen,
  the kind in place of Tranq / Observe). The area and time of day are random;
  the waves are random **dangerous** animals: the original's hunters and its
  Chasmosaurus, or in a dialect game a carnivore AI or any huntable with an
  attack (`aggress` > 0); the T-Rex only as custom's. Wave animals never back
  off (`Character::wave` stops the dialect's flee rule and makes the
  Chasmosaurus charge). PC: every dangerous animal of the game can come; 3DS:
  up to three at random that fit in memory (plus hardcore's / custom's).
- **Classic** (the mobile one): no screen; straight in on Mount Ravan with
  the shotgun (the weapon named "Shotgun", else the first with "shotgun").
- Entry: PC main menu SURVIVAL / CLASSIC SURVIVAL (our text under the
  original's buttons); 3DS title SURVIVAL / CLASSIC (one button if only one
  is offered). The launcher's choice says which.

## 2026-10-23 — M13: the mobile game's Survive mode, from the player's APK
- **What it is** (read from `libdinHunter.so`, `c2_formats::mobile_arcade`):
  `Game::ArcadeGameGenerateWave` is a switch over 19 waves (then one for
  every later wave) of 2–8 animals of types 19–23, which
  `CharacterInfo_SetDefaultParams_DH` names Allosaurus, Chasmosaurus,
  Velociraptor, Spinosaurus, Ceratosaurus; they appear at random good spots
  in a ±2000×±3000 box; the next wave comes 2 s after the last one falls (the
  first after 0.5 s); `Menu::StartPlayingTheGame` loads `area5`.
- **No separate map**: the mobile `area5.man`'s heights are byte for byte the
  PC `AREA5.MAP`'s (Mount Ravan), so the mode is played on the player's own
  PC area. The mobile map is the PC one transposed (PC x = mobile −z, PC z =
  mobile x; from `Player::placeHunter`'s height lookup, checked in game).
  The APK is needed only for these rules; the import writes them to
  `mobile-survival.txt` beside the add-ons (the player's machine only, as
  the tuning values) for the Carnivores 2 APK. Ice Age's APK has its own
  Arcade on an Ice Age map: with M14.
- **Ours on top**: health and ammunition come back between waves as in our
  survival (not checked in the mobile code); waves are capped as ours (12 PC,
  6 3DS; the mobile game sends up to 8).
- **Wave Chasmosaurs come all the way** (our Chasmosaurus AI only charges
  within 16 cells and otherwise backs off; wave 1 is two of them).
- **Ways to the hunter** (both survivals): the AIs only steer round what is
  just ahead, so the mobile box's animals stuck on the ridge above the
  hunter's gully. Survival keeps a breadth-first map of walkable cells (no
  water, no steeper than `CheckPlaceCollision`) over 192×192 cells round the
  hunter, renewed when he moves 3 cells; a wave animal more than 12 cells off
  is told he is 8 cells along its way (`Character::lure`). Closer, as before.
- **Choice** (launcher, PC and 3DS alike): Ours / Mobile / Both. Both: the
  PC setup switch and the 3DS options page include "Survival: mobile"; Mobile
  only: it's the only survival. A mobile survival goes to Mount Ravan
  whatever area is picked; on the 3DS its five animals load as extras (they
  fit an Old 3DS with a weapon). `c2-pc --mobile-survival FILE
  --survival-modes ..`; `c2-cook` the same (pack `C2RS/SURVIVE.TXT`,
  `INFO.TXT survival =`).

## 2026-10-23 — M13: survival
- **Waves** (`c2_core::game::Survival`): the map's own hunting animals go
  (plant-eaters stay as scenery); after a 3 s start, each wave is 2 + n
  hunting animals (at most 12 on PC, 6 on the 3DS), placed on land in a ring
  of about 20–40 cells around the hunter, already after him. A wave animal
  that calms down takes up the hunt again; the dead go when the next wave
  comes, after a 4 s rest that brings health and ammunition back. It ends
  when the hunter dies (or leaves).
- **Who comes**: the hunting animals picked (PC: none picked means every
  hunting animal but the T-Rex). Survival is free (no points spent) and
  gives no trophies, score or ship; kills and the best wave go in the
  hunter's record (also on death, unlike a hunt's trophies).
- **Hardcore** adds 1–3 mods' animals (by name: not one of the original's
  hunters; the game's own if there are none) to every wave. On the 3DS
  they're rolled on the options page and listed on the top screen before
  starting (they need loading, so they must fit in memory); on PC they come
  from every hunting animal the game has. **Custom**: unlimited ammunition,
  and the T-Rex joins every wave from 15. "Choose any weapon" is how
  survival is anyway (free).
- PC: the switch is our text in the setup screen's empty bottom bar
  (Hunt / Survival / Hardcore / Custom), `--survival` for `c2-pc hunt`. 3DS:
  a SURVIVAL title button; the same steps, the kind on the options page.

## 2026-10-23 — M12: records, trophy room, hunters
- **The record** (`c2_core::record`) is a text file beside each save
  (`trophy0N.rec`): one line per kill (mode, animal, score, range, weapon,
  area, date) and the best survival wave. The original's save has no room;
  statistics (longest shot, best kill, kills per mode) are worked out from
  it. Kills go in when a hunt is left alive (as its trophies are saved).
- **Trophies' area and lock** use the save's `reserved` ints (the original
  never reads them), so saves stay the original's format.
- **Trophy room**: move = the next free stand that way (`[` `]` on PC, L/R
  on the 3DS; in a dialect game, its spots); remove and lock are asked first
  (Y/N on PC, A/B on the 3DS); a locked trophy can't be removed. "Remove
  unlocked trophies" (PC options, 3DS settings) takes a second click / tap.
- **3DS hunters**: `trophy0N[-pack].sav`, N 0..=5 as the PC numbers them; the
  last one played is in `settings.txt`; a new one's name comes from the
  system keyboard (`swkbd`).

## 2026-10-22 — M9: the Modder's Edition trophy room
The dialect's trophy room (`CharacterSpawn.cpp`, `Ship.cpp`): every
`tropinfo` (in a `trophytable` group, or in a character, in file order) is
a spot: a landing point of the trophy room's map (`tropPos`), offsets plus
offsets per `scaleA` the trophy is bigger than `scale0`, facing (degrees),
a pose (`tropAnim`, a model animation: Majungasaurus' 9 and 10 are past its
logical phases) and which characters it takes (its own; `tropgroup = n`
joins group n's). Trophy slot n stands on spot n; a kill takes the first
free spot made for its animal, none: no trophy (as the engine). We had
placed them on the original's fixed stands, onto the room's paths.
- Our save keeps 24 trophies (the original's format); the engine keeps 128
  (`TrophyRoom2`). Spots past 24 aren't used yet.
- Trophies saved before (by the original's stands) stand on the first free
  spot made for them, the save unchanged (removing still works by slot).
- **Deviation**: an animal no spot is made for (add-ons written without
  `tropinfo`, the mobile imports) takes any free spot instead of getting no
  trophy, posed as it stands (another animal's `tropAnim` would be wrong).

## 2026-10-22 — Menu styles and the experimental mobile layout
- **PC menu styles** (owner): Original, Mod, Ice Age. `c2-pc play
  --menu-art GAME_DIR` takes the menu frames, their hit maps, slider and
  loading picture from another game's `HUNTDAT/MENU` (Ice Age's boards
  have the same names and button regions); the pictures and texts stay
  the game's. Mod is the game's own (no flag).
- **3DS**: Carnivores 2 or Ice Age title art (`c2-cook --menu-art`); the
  rest of the 3DS menu is our own.
- **Experimental mobile layout** (`--mobile-menu`, launcher "Experimental
  mobile layout"): the hunt setup as three pages of hexagons (maps,
  animals, weapons; cheapest first) like the 3DS menu, with gritty green
  buttons and a red lamp for dawn / day / night / observer and tranquilizer
  / radar / cover scent / camouflage. All art is generated (no mobile
  artwork): pictures under a hexagon-shaped frame of the background colour
  and a tinted rim, so cells must not overlap (columns a cell apart, every
  other one half a row down). A quick first version.
- Menu test scripts work before the first frame (the canvas at 1:1).

## 2026-10-22 — Owner's mobile + Ice Age test: fixes
- **Mobile animals racing across the map**: an entry without `jumpAnim`
  gets jump = animation phase 0 (the run), so the jumping AIs (raptors,
  Allosaurus: Titanis, Andrewsarchus, Dilophosaurus...) ran without stop.
  The import now uses the animal's attack lunge as its jump; headless,
  they move like the native animals (a Dilophosaurus ~2,500 units a minute,
  was ~95,000).
- **Both APKs have the revolver**: an import skips what an import beside it
  already made (two add-ons named the same can't load together).
- **3DS calls without a sound** (add-ons and imports with no call): no
  button, ZR doesn't call them, and the hunt starts on a call that sounds.
- **Crouch button** (3DS setting; the Old 3DS has no ZL): ZL, Hold L (a tap
  jumps), or D-Pad Up cycling walk / run / crouch; the default follows the
  console (`APT_CheckNew3DS`). Stealth crouch toggles with each.

## 2026-10-22 — M11: hunt options, 3DS friendly, stealth crouch, random ambients
- **Hunt options** is a 3DS menu step after Gear (HUNT moves there): run
  speed and ambient animals move from Settings, with the new options;
  stored in `settings.txt` as before.
- **3DS friendly** (off by default): `HuntOptions::notice_k` = 0.6 divides
  into `CheckAfraid`'s distance term, so sight and smell notice the hunter
  at 60% of the distance, for every animal (vanilla, the dialect's AIs,
  add-ons, mobile imports). Gunshots are unchanged (they already scale with
  the view distance).
- **Stealth crouch** (off by default): ZL toggles crouch instead of being
  held; while crouched the gyroscope adds to the look (raw x the system's
  degrees/s factor, as look units of 0.0025 rad, the core's mouse step),
  `k_look` doubles (half as visible; smell still finds you: the original
  game has no footstep noise, so "half as loud" is the shot's reach,
  halved). Gyro direction is a guess until tried on hardware.
- **Random ambients** (modded games): 4 ambient species picked from the
  hunt's seed are loaded; the rest aren't, and the spawn tables skip
  animals without a model (M8).

## 2026-10-22 — M15: mobile animals, values and the PC-or-mobile choice
- **Formats**: the mobile games split a `.CAR` into `.cat` (text), `.3dn`
  (mesh: PC vertices; faces with packed 16-bit texel UVs), `.ani` (the
  `.CAR` animation block), textures (`.pkm` ETC1, `.crt` raw RGBA, 32-bit
  `.tga`; up to 512 wide, UVs stay in 256 units) and IMA ADPCM WAVs. Shared
  animals are byte-identical meshes to the PC ones.
- **Tuning values are compiled code**, not data: `BalanceManager_
  SetDefaultValues*` stores `{key[32], double}` entries. We follow its
  AArch64 (adrp/add for the key, movz/movk/movn/orr/fmov + str at 0x20 for
  the value) in the user's own APK at import time; nothing extracted is
  kept in the repo. Only the free-to-play set has values (Classic/IA empty);
  its prices/scores are on a bigger scale, converted by the median PC/mobile
  ratio over the animals both games have.
- **Import = loose add-ons** (one folder per new animal or weapon, as
  community add-ons ship): an entry from a same-kind PC donor (AI, senses,
  kill setup, its slot block moved to the new slot) with the animal's own
  animation roles (named, or the shared 11-animation order), mobile health
  and score; its own `price` key now sets a new slot's price for any loose
  add-on. Menu art from `Help/` or drawn from the model; calls from the
  `callN` set containing its call/growl, else its own call sound.
- **Not imported**: animals the game has (by model file or by name), people
  (poacher), PC-style numbered animation sets whose donor differs in count.
- **See-through textures**: faces mostly on alpha-0 texels (the mobile
  engine adds a muzzle flash's colour) become colour-keyed and
  double-sided; dim alpha-0 texels black.
- **PC or mobile** (owner's rule): PC by default. The import writes
  `mobile-values.toml` (mobile values for the game's own entries, PC
  scale); the launcher's Balance tab lists each entry that differs with
  PC / Mobile, and all PC / all mobile; mobile choices are balance edits.
- `png` (already in the build via eframe) decodes the Help pictures.

## 2026-10-22 — Fixes from the owner's mobile + add-ons test
- **3DS trophy room**: loads only the species with a trophy on display
  (and the hunter); it loaded every species, out of memory with add-ons.
- **Call icons**: an add-on slot with call sounds but no `CALLn.TGA`
  gets the middle of its menu picture as the icon (`c2-cook`).
- **3DS hunt menu**: animals sorted by price (cheapest first; ties keep
  the game's order), not game, then mobile, then add-ons.
- **Mobile import** keeps the donor's `overwrite charN` block when the
  animal gets a slot: building moves it to the new slot, so it has the
  radar flag and the picked spawn group like the PC animals.
- **Combined game folders** with a long add-on list are named by its
  start, the count and a hash (Linux limits names to 255 bytes).
- The 3DS run-speed setting already applies to every AI, modded too.

## 2026-10-22 — M10: `.cia` builds
- **Tools are the user's downloads**: makerom (Project_CTR) and
  bannertool, found like our own tools (tools folder, beside the launcher,
  the source tree's top folder); never committed (`.gitignore`). The GUI and
  both READMEs (this repo's and `3ds-ver/`, which becomes the 3DS repo)
  link their GitHub release pages.
- **One title per install name**: title ID `00040000` + FNV-1a of the
  name into `F0000`-`FEFFF` + `00`, so several installs sit side by side.
  The app finds its SD folder from `romfs:/name.txt` (written into the
  title's RomFS by the builder) when it has no start path, as an installed
  title has none.
- **RSF**: the common homebrew application spec (SD access only, the usual
  services and calls); `SystemModeExt` 124MB for New 3DS builds, Legacy for
  Old. `makerom -exefslogo` for older firmware.
- **Art is the owner's** (icon 48x48, banner 256x128 PNG, banner sound);
  plain olive placeholders, made by the launcher, until set.

## 2026-10-22 — M10: balance files and the balance editor
- **A balance file** (`c2_mods::balance`, TOML) lists prices and scores by
  entry name (a repeated name is "Name #2"), so it survives add-ons being
  reordered and is skipped where a name is missing. Only changed values
  are kept.
- **Applied as a layer of rewritten text files**: `_MENU.TXT` (or
  `_RES.TXT`) for prices, where the menus read them (a `prices` line wins
  over the entry's `price`, as `MenuData` reads them; `.c2map` areas in
  their file); `_RES.TXT` `basescore` for scores, in the entry and in any
  `overwrite`/`addition` block that sets one. Every reader (PC menus and
  hunt, `c2-cook`, the 3DS) sees ordinary data. `c2-pc play --balance` builds a
  combined game even with no add-ons (`packs/base`).
- **Roaming add-ons** (huntable AI, no menu entry or `callNo`) are listed
  with a score only; their 3DS price stays the donor's.
- **The launcher reads the game's values itself** (`c2-mods` linked into
  `c2-launcher`, off the UI thread), so the 3DS-only builder needs no
  `c2-pc`. Edits go to `balance.toml` in the data folder and are passed on
  to play and to the 3DS add-ons pack (the original-game pack when there
  are no add-ons; otherwise it stays stock).

## 2026-10-22 — No borrowed calls for roamers; the dialect never halves a score
Owner's calls after playing the roamers on hardware:
- **Roamers have no call**, as in the Modder's Edition: it loads calls
  only per menu slot (`CharacterLoader.cpp`: `call%d.tga`,
  `call%d_a.wav` for slots 1-10), and these add-ons set no `callNo` and
  ship no call files. The borrowed donor call (entry below) is gone; the
  roamer keeps its slot (target, loading) but the slot can't be called
  (`Setup::callable`). The donor now only sets the price.
- **+10% score** for such a kill (`HuntOptions::no_call_bonus`), as for
  night. Menu: a blue rim in the grid; on the top screen the name in blue
  and, under the time of day, "No calls supplied, proceed with caution"
  (+10% score) while one is picked (the bottom bar's middle shows the points
  left). Hunt map: blue with a gold rim.
- **Non-target kills keep their full score in dialect games** (PC and
  3DS). Both the upstream source (`Game.cpp`) and the modernized fork
  (`EngineInit.cpp: SubmitDinoScore`) comment out the original's halving.
  Original-data hunts still halve, as retail does.

## 2026-10-22 — Roaming add-ons are hunt targets on the 3DS
A picked roaming add-on (huntable AI, no menu slot) spawned but scored half,
had no call and no menu picture. Its `callNo` also defaulted to 0, so it
answered the first menu animal's call.
- **Its own call slot**: the menu numbers roamers from slot 64
  (`rules::ROAM_SLOT0`, in `_RES.TXT` order); picked, the slot's target bit
  is set and `Rules::hunt_with` writes it as the roamer's `callNo`. From
  there it is an ordinary menu animal: a target (full score, green on the
  radar), answers that call, loads and spawns as picked.
- **Donor**: the first menu animal with the same `ai`. The roamer borrows
  its call sound and icon (loaded at hunt start, `Setup::call_of`; nothing
  extra in the pack) and its price, now that it scores in full (it was
  free while it scored half). With no donor it has no call button.
- **Menu picture**: `c2-cook` draws one from the model (rest pose, side
  view turned 30 degrees, textured and shaded, software rasterizer in
  `portrait.rs`) as `C2RS/MENU/R/<model>.TX`. Older packs still work, with
  the name in place of a picture.
- `c2-cook --check` now reports a picked animal that isn't a hunt target.

## 2026-10-22 — Original-game add-ons move like the animal they replace
A raptor add-on written for the original game (speeds as `runspeed` etc.,
another engine's keys and units) stood still in a Modder's Edition game:
that engine reads `runspd` / `wlkspd` / `swmspd` / `jmpspd` only. Besides
the animation keys (entry below), such an entry now drops those foreign
speed keys and takes the donor's movement keys it lacks (speeds, kill
distance, swimming, water level, weave, camera), as the original engine
gives every animal of an AI the same gait. `c2-cook --check` reports a
dialect animal that can't move.

The 3DS offers roaming add-ons (huntable AIs 10-29 with no menu slot) in
its picker; brachiosaur-like and fish AIs (30 and up) without a slot are
ambient wanderers: not listed and not loaded there (`Role::Off`, a couple
of MiB each).

## 2026-10-22 — M10: the launchers
- **egui / eframe** (owner's pick) with `rfd` for file pickers, in a new crate
  `c2-launcher` with two binaries: `c2-launcher` (PC play; on first start it
  asks whether to show the 3DS section) and `c2-3ds-builder` (the 3DS side
  only), as the owner asked. Themed with the game menus' colours (dark
  olive panels, pale green text, gold, sand); no original artwork.
- **They run the existing programs** (`c2-pc play ...`, `c2-cook ...`) rather
  than linking the game: the launcher stays small, and what it does is what
  the documented command lines do. Tools are looked up in a chosen folder,
  beside the launcher, then in this tree's build folders.
- Settings in `launcher.toml` in the game's data folder. Add-on archives are
  unpacked there (`addons/<name>/`): `.zip` by the launcher (names leaving
  the folder are skipped), `.rar` / `.7z` with `7z`, `unrar` or `bsdtar`.
- **3DS installs by name**: the builder writes `3ds/<name>/<name>.3dsx` with
  `c2rs.c2pk` and/or `modded.c2pk`, and the app now keeps packs, saves and
  settings in the folder it was started from (`sdmc:/3ds/c2rs` if it can't
  tell), so differently named installs don't share anything.

## 2026-10-22 — Picked animals come on any map
Owner's rule: only ambient animals keep to their maps; anything picked can
be hunted anywhere. The dialect spawns an animal only where its tables say
(combined games: its own game's maps; roaming add-ons: theirs), so
`c2_core::mee::spawn_picked_anywhere` adds each picked character (by menu
slot; on the 3DS also picked roamers) to the map's target group, the spawn
group that map's own menu animals use, unless it is already in it; its own
spawns stay. Ambient animals (AI under 10) follow their tables as before,
and the modpack's random-ambients setting still applies. This replaces the
"not on this map" dimming (PC) and the roamer lock (3DS) of the entry below;
the menus still say where each animal is native to.

## 2026-10-22 — Owner's first add-on pack test: fixes
- **Original-game add-ons in a Modder's Edition game had no animations.**
  That engine numbers animations only from an entry's `runAnim`,
  `deathtype` and so on; the original fixes each AI's order instead, so an
  entry written for it (a raptor add-on) stood frozen and didn't die
  properly. When such an entry joins a dialect game, `c2-mods` now copies
  the animation keys and blocks of the slot it was made for, or of an animal
  with its AI (same order). `c2-cook --check` reports a dialect animal
  without them.
- **CheckAfraid on the 3DS every other tick** (`HuntOptions::afraid_every`,
  25 times a second, as the original did once a frame at 25 fps): with a
  pack of alarmed raptors its look traces took 14-40 ms a tick on the Old
  3DS. The PC keeps every tick.
- **Where animals live.** In a combined game an animal from one game
  spawns on that game's maps only, so picking it for another map spawned
  none of it (and a "mystery" add-on that joins the base game's spawns
  took every place: what looked like it replacing other animals). The PC
  menu now says where each lives and dims the ones not on the chosen map;
  the 3DS menu lists each roaming add-on's areas and won't pick one for a
  map it doesn't roam.
- **3DS map**: animals not picked but huntable are blue (picked: green),
  as on PC.
- **Night +10% score** (owner's request; PC and 3DS), with the
  tranquilizer's +25%; the PC kill notice says so.
- 3DS gear screen: the message line sits above the Tranq / Observe buttons.

## 2026-10-19 — M8: add-ons on the 3DS (a second pack)
Owner's plan: the 3DS gets the vanilla pack plus one pack of vanilla and
add-ons, as the PC plays them (over the Modder's Edition).
- **Packs**: the app lists every `.c2pk` in `sdmc:/3ds/c2rs/` and asks
  which one when there are several. `c2rs.c2pk` keeps `trophy00.sav`;
  another pack keeps `trophy00-<name>.sav`, since its trophies are other
  animals. Pack layout 4 (cook both again).
- **Cooking**: `c2-cook --overlay DIR` (as `c2-pc play --overlay`). Models
  are cooked by file (`C2RS/M/<FILE>.TX`, sounds `C2RS/SND/M/<FILE>/k`),
  not by character number, because the Modder's Edition can give a
  character another model per area. Calls and their icons go by menu slot
  (`CALLn`, call id n + 9; in the original the slot is AI - 10, so vanilla
  is unchanged). `_MENU.TXT` is packed.
- **Per hunt on the 3DS** (`c2-3ds/src/rules.rs`): the rules are resolved
  for the area, time and picks as on PC (`c2_core::mee::read`, `tune`,
  `place_mee`). Each character has a role: the hunter; ambient animals
  (AI under 10), loaded unless turned off; menu animals (with a `callNo`),
  loaded when picked; and the rest ("roamers": add-ons with no menu slot,
  brachiosaurs, fish), loaded only when picked. Roamers are listed after the
  menu animals with a blue rim, cost nothing, count toward the three
  animals, and spawn where their tables put them but aren't targets. Not
  picked, nothing of theirs loads and they don't spawn: memory.
- **Spawning with animals missing**: dialect spawn groups now skip packs
  whose leader has no model, so their share goes to the loaded ones (before,
  those rolls were lost, and a picked roamer sharing a group with unloaded
  animals rarely came). Everything loaded, as on PC, nothing changes;
  data with broken models (placeholder `blank.car`) now spawns its other
  animals in their place.
- **Memory**: `c2-cook` records each model's and weapon's main memory
  (`heap.*`, about its cooked file) besides video memory (`cost.*`), and
  each area's loading peak with the hunter and ambient animals. The 3DS menu
  adds up the real picks against both budgets. The cooker fails only if the
  base hunt can't fit, and says when the three costliest picks wouldn't.
- **Thinned animations**: on Old 3DS a character's animations are kept
  under 2 MiB (6 MiB New 3DS) by dropping every other frame, length kept;
  the engine blends between frames. Several add-ons needed it (one was
  12 MiB of animation).
- `c2-cook --check PACK` loads a pack with the 3DS's own `rules.rs` and plays
  30 s with each animal in each area, reporting missing files and how many
  spawned.

## 2026-10-19 — 3DS: at most 3 animals and 3 weapons; no Double Ammo
Owner's call, for Old 3DS memory: the 3DS hunt menu takes at most three
animals and three weapons (`MAX_PICKS`); a fourth tap says so. The
original's Double Ammo accessory is gone from the 3DS gear; the 3DS's
extra-ammo refills take its slot and its picture (`EQUIP4`). The PC is
unchanged.

## 2026-10-19 — Blood and death-animation settings (PC and 3DS)
Both on by default, as in the original (owner's request).
- **Blood** off: a hit on an animal throws no blood spray, and wounded
  animals leave no trail (`HuntOptions::blood`).
- **Death animation** off: once the hunter is killed the screen is black
  and the leave / restart prompt shows at once instead of after the 6 s
  death camera (`HuntOptions::death_anim`, `Hunt::show_exit`). Sounds still
  play; the hunt still ends on its own 12 s into the death camera's
  circling, as with it on.
- PC: Options → game panel, kept in `c2rs.cfg` (`blood`,
  `death_animation`). 3DS: Settings screen, kept in `settings.txt`.

## 2026-10-19 — 3DS: 20 ms ticks, full LTO, restart
- Owner's Old 3DS readout with ambients on: 41 fps, sim 11.3 ms, of which
  the animals about 2.8 ms a tick. On PC each animal costs about 1.5 µs a
  tick, ambients as much as huntables; an Old 3DS (268 MHz ARM11, no L2) is
  60-90x slower at this, so nothing is broken, it's just 100 updates a
  second of ~25 animals.
- The 3DS steps the game every 20 ms instead of 10. The original runs one
  update per frame with any `TimeDt` and the port keeps that scaling, so
  play is the same (checked with `tick-bench` at 20 ms: same hits, kill and
  pickup), at half the cost. Still a fixed step, so runs stay reproducible.
  The PC keeps 10 ms.
- 3DS release builds use full LTO with one codegen unit.
- Y or X at the leave prompt, or once dead, restarts the hunt with the
  same setup; like dying, the hunt isn't saved (the PC's `R`).

## 2026-10-19 — 3DS slow ticks, round two: flush-to-zero
- The report's "sim 93 ms at 40 fps" is consistent: the pause screen freezes
  the ms averages but keeps measuring fps, so the hunt really ran near
  10 fps just before pausing, with ticks of about 9 ms.
- `tick-bench` now draws the first weapon, wounds and kills the
  Stegosaurus with real shots (hit reactions, blood trail), and counts
  subnormal floats in the state. On PC: no tick above 0.6 ms, no
  subnormals.
- The 3DS start-up code never sets FPSCR, and on the ARM11 every operation
  on a denormal goes to slow support code (a PC pays almost nothing). The
  app now sets flush-to-zero and default-NaN ("RunFast") at start, leaving
  rounding alone. The pause screen's tick line ends `fp boot/now`
  (FPSCR >> 22): 3 or f in the second means flush-to-zero is on; the first
  shows whether the system already had it on.

## 2026-10-19 — Kill notice, two-colour radar, weapon-pick screen (PC)
- **Kill notice** (PC, not in the original): for 6 s after a kill a box in
  the bottom right shows the animal, the points scored, the range (metres or
  feet, as the options say) and whether it was tranquilized.
  `GameEvent::Scored` now carries `range` and `tranq`.
- **Radar colours**: animals picked for the hunt are green, as before; other
  huntable animals that spawn anyway (spawn tables, add-ons) are blue.
  Ambients, flyers and fish stay hidden. `Hunt::is_huntable`: AI 10 and up,
  or MEE AI 10–29.
- **Weapons past slot 32 had no ammo**: `Weapons::new`/`resupply` still
  stopped at 32 from the old 32-bit mask, so in big packs those weapons
  started empty and wouldn't draw. Now 128, like the mask.
- **Weapon-pick screen** (PC only): `Q` (or `G` when `Q` is bound) pauses
  the hunt and lists the carried weapons with their ammo; arrows/`W`/`S`/wheel
  or `1`–`9`, `Enter` takes, `Esc` or the key again closes. The number keys
  only reach the first six weapons, which isn't enough with add-ons. Picking
  the weapon already in hand keeps it out.

## 2026-10-19 — PC render distance and fog
- **Render distance** (Options → Video, PC only, kept in `c2rs.cfg` in the
  data folder since the original save has no room): from the original
  view range (default, slider at 0) up to 384 cells. Past the original
  range, fog only softens the last 15% instead of the original's 40%.
- **Fog** (the setup screen's empty box beside the times of day, any time
  of day): hunt at the original view range and fog whatever the render
  distance says.
- Gameplay keeps the original view range for detection, aggression and
  noise. Two things follow what is drawn (`HuntOptions::render_r`): where
  an animal out of sight is re-placed (just beyond the drawn distance, so
  it isn't seen appearing) and how far a shot carries (what you see you
  can hit). With fog on, or on the 3DS, both equal the original.

## 2026-10-19 — Radar by slot; points in the menu
- The map's radar blips (PC and 3DS) use `Hunt::is_target`, which now
  knows the dialect: an animal marked `radar` when picked, else picked by
  its `callNo` slot. The old test (AI bit) missed most Modder's Edition and
  combined-game animals.
- The hunt menu's info panel adds "Score when bagged: N (tranquilized: M,
  no trophy)" for each huntable: its `_RES.TXT` `basescore` as when picked
  (the games' own descriptions differ). The tranquilizer already scores
  x1.25 (`Game.cpp`, as the original), with no trophy.
- Base scores are decimals for dialect species (`basescore = 7.5`, the
  engine's `atof`); the original's stay whole (`atoi`).

## 2026-10-19 — Calls by menu slot; up to 118 huntable slots
- **Calls** are numbered `10 + menu slot` (the original's AI number, so the
  original game is unchanged) and play `CALL{slot+1}_A/B/C.WAV`. Dialect
  species follow the engine's `MakeCall`: an animal answers the call of
  its `callNo` slot (0 when not given, as the engine), hears it within
  `view * 400 * 2 * hearK`, and flees calls in its `fearCall` list (not
  flyers or the old brachiosaur). The original game's species still answer
  by AI. An add-on given a slot of its own gets `callNo` for it.
- Call sounds and icons load for the picked slots only (as the engine:
  with add-ons there can be a hundred).
- **Areas past 10**: a map's area is the whole number in its name
  (`AREA23` is 23; the engine reads one digit, any `area1x` being the
  tenth) and `areaN` conditions match whole numbers (`area0` still means
  10), so large combined games keep their areas apart.
- **128-bit target and weapon masks** (`HuntOptions::targets`,
  `mee::Hunt::picked`, `Weapons::new`'s carried set, the PC menu's picks):
  up to 118 huntable slots and 128 weapons, for large combined games. The
  3DS still picks from 32 bits.

## 2026-10-19 — M4: loose add-ons, hunter merging, base overlays
Real add-ons turned out to be loose files plus instructions, not
`HUNTDAT`-shaped folders; most are for the Modder's Edition (MEE).
- **Loose add-ons** (`c2_mods::loose`) are placed by rules standing in for
  the instructions (docs/MODDING.md): files by name and kind, entries
  pulled from text files, a slot takeover inferred from `DINOn` / `WEAPONn`
  menu files, variant folders chosen by the base. Fix-ups a person would
  make are made and reported (a misnamed model, a plain entry keeping the
  replaced slot's MEE spawn tables). A no-entry add-on (skin pack) only
  adds files, so it can't silently replace call sounds.
- **Hunter models are merged** (`c2_mods::hunter`): every add-on hunter
  checked so far is the base hunter (same body, same first animations)
  plus its kill animations. Extras are appended once (identical ones
  shared) and each add-on's `hunterAnim` remapped, instead of the last
  add-on's hunter winning and breaking the others' kills. Texture: the
  base's (some add-on hunters are reskins). A different body is left out
  with a warning.
- **Base overlays** (`--overlay DIR`, repeatable): the MEE release ships
  only changed files; it is layered over the game as part of the *base*
  (its `_RES.TXT` replaces the original's), and add-ons diff against that.
- Recipes don't cover loose add-ons yet (no hash pinning for them).
- A loose add-on's model always goes to `HUNTDAT/<name>`, where its entry
  looks; matching a game file by name is for menu pictures and sounds, and
  never a file another game keeps in its own folder (`HUNTDAT/MODS/...`).
- **Add, don't replace** (owner's call): an add-on made for a menu slot
  gets a new slot alongside the original (handed out at build time in
  load order; menu files renumbered, `overwrite charN` conditions and
  `callNo` / `pic` moved, `_MENU.TXT` extended, price copied). Animals on
  the original game's data still take the slot over: its engine places by
  AI number. Slot conditions past `char9` match by whole number (the
  engine's substring test would let `char1` match `char10`); the PC hunt
  passes up to 22 picked slots (it masked them to 10), and the menu lists
  scroll.

## 2026-10-19 — 3DS: slow-tick report; catch-up capped, tick timings
Owner's Old 3DS report: after a Stegosaurus came into view (and was
killed) the frame rate fell to 40 with "sim" at 93 ms. That is about 12
fixed 10 ms ticks per frame at about 8 ms each, against well under 1 ms
normally: one tick got slower, and the catch-up loop (up to 25 ticks per
frame) multiplied it. The same scene on PC (`c2-parity tick-bench`: hunter
beside a Stegosaurus, the kill, the ship's pickup) never exceeds 0.12 ms
a tick, so the cause is not found yet.
- The 3DS runs at most 4 ticks a frame (`MAX_TICKS`); below 25 fps the
  game slows down instead of spiralling. Deviation from the PC build,
  which keeps catching up.
- `Hunt::prof` times the tick's parts when given a clock; the 3DS pause
  screen shows ticks last frame and per part mean / worst ms (`p` player,
  `w` weapon, `d` dinos, `e` events, `f` effects, `a` CheckAfraid, `s`
  ship), to find the slow part on hardware.

## 2026-10-19 — Jumping needs a fresh press
The original jumps whenever the jump key is down and the hunter is not
rising (`Hunt2.cpp: ProcessPlayerMovement`, `YSpeed == 0`), and walking
onto higher ground zeroes `YSpeed` as a landing. Holding jump therefore
relaunches the hunter every frame on a slope: on PC and 3DS you could
climb mountains in the air (owner's report). Deviation, at the owner's
request: a jump needs the key released and pressed again
(`Player::jump_held`). Holding it still keeps a swimmer afloat.

## 2026-10-19 — M9: the dialect's shared AI (`AnimateHuntable`)
- **Source**: ported from the upstream Modder's Edition release the mods
  target (`Characters.cpp: AnimateHuntable`, `AnimateDeadCommon`,
  `SetNewTargetPlaceRegion`, `ReplaceCharacterForward`, `MakeNoise`,
  `CheckAfraid`; `Resources.cpp: ReadCharacterLine`, the `AIInfo` table;
  `Game.cpp: registerDamage`), not the modernized fork's redesigned
  awareness. Read and cited only (the upstream code has no licence).
- **Who runs it**: dialect species with AI 10-17 and 19-27 (the original's
  herbivores and hunters, Pachycephalosaurus, the Ice Age animals). The
  others still run their interim family (previous entry). Original data
  never reaches this code: the original's AIs are unchanged.
- **Logical phases**: a dialect species' phase is a position in its
  `animIndex` table (`AddAnim`), mapped to a model animation for timing,
  sounds and posing (`Species::anims`, `pose::pose(c, car, anims)`). A
  missing key leaves its phase at 0, as in the original. We number from the
  *resolved* block, so an `overwrite` replaces an entry in place instead of
  appending a new phase; only a species missing a key *and* overwriting the
  first listed one could differ.
- **Awareness**: `awareHunter`, `heardShot`, `damaged`, packs (shared alarm
  and attack, leader following, a dead leader replaced), kill types (eat
  animation, body offset, `dontloop`, silent kills), death types, idle
  groups, weaving, the region-aware target picking and the forward respawn
  only for `moveForward` groups (resetting the animal, as the original).
  For dialect species `MakeNoise` reaches `2 * hearK` times as far,
  `CheckAfraid` asks the AI table which animals look (and keeps renewing
  alarmed ones), and neither restarts an alarm already running.
- **T-Rex** (`AnimateTRex`, dialect AI 18): never flees; once it notices
  the hunter (2: seen or heard, 3: smelt) it turns, plays a `lookAnim` /
  `smellAnim` (else its roar), roars, then chases; obstacles only steer it
  while calm unless `TRexObjCollide`. Unlike the original's, it does not
  carry the hunter's body (the dialect disabled that), and `MakeNoise` only
  wakes a calm one. Roars are picked per animal at reset.
- **Ambients** (`AnimateClassicAmbient`, dialect AI 1, 2, 5, 6): bolt
  from a hunter within 812 units, zig-zag away, settle beyond 4096; idle
  groups, packs, swimming, `dash`. A killed animal of a `moveForward` spawn
  group is replaced in its group's regions (`Characters_AddSecondaryOne`;
  any species, as the dialect does). Dialect animals always leave blood
  trails (the original exempts its AI 6, the brachiosaur; the dialect's 6
  is the pig).
- **Flyers** (`AnimateDimor`, dialect AI 3, 4): flap up toward
  `1.5 * maxalt`, glide down from `maxalt`, flap again below `minalt`,
  follow packs, call now and then; a forward respawn starts at `minalt`.
  Shot, they fall (`AnimateIcthDead`: the death type's `fallAnim`, a
  splash on water, then `dieAnim` or a water death).
- **Brachiosaurs** (`AnimateBrahiOld` 30, `AnimateBrahi` 34 / 35): wade
  (or, 35, walk) between spots 200-400 deep (`SetNewTargetPlace_Brahi`),
  idle in groups, follow packs, and move without obstacle checks (the
  original's call is commented out). 34 / 35 with `aggress` charge an
  aware hunter in water no deeper than 550, kill within `killdist`,
  otherwise flee; deeper than 550 they head back. Kept: the dangerous
  wader's path test also runs the common land check 64 further on (a
  missing `else` upstream), and turning while alarmed scales by
  `runrotatespeed`, which the manual makes required (0 freezes the turn).
- **Fish and Mosasaurus** (`AnimateFish`, `AnimateDeadFish`, 32 / 33):
  swim at a depth between `spcDepth / 2` from surface and bottom, in water
  `mindepth`-`maxdepth` deep, pitching toward a target depth; `DangerFish`
  hunt a hunter standing in water within `view * aggress` (jumpers leap
  at one near the surface), others swim off; idles (`lookAnim` /
  `fishIdleAnim`) near the surface; dead ones sink. Kept: the target
  range is 5024 for both (a `switch` without `break`); the depth retry
  compares an `int` pitch difference; `spcdm` is an `int`, so 0 while
  idling. Not done: leap spray particles (`partCnt`), and the vertical
  bend `bdepth` (computed upstream, never drawn). Shots are not heard by
  them (`Aquatic`). Their target picking can retry 3024 times: watch on
  Old 3DS.
- **Ichthyornis** (`AnimateIcth`, 31): walks the shore and swims (land
  and water idle groups, shaking itself dry every eighth walk cycle),
  takes off when the hunter comes within 1050 (or after a minute on the
  ground), flies up, glides down and lands. Kept: `waterNear` ignores its
  position and tests map cell (0, 0); `swimmingAnim` is true whenever the
  species lists a `shakeWAnim` (an index tested as a flag). Deviation:
  `ProcessPrevPhase` runs once per step, not once per `goto TBEGIN`.
- **Not ported** (no game folder we have uses them): Titan (28) and Micro
  (29) still run as the interim Ceratosaurus / Velociraptor; the wary
  Dimorphodon (36) is not placed; poacher (8) and hunting dog (9) are
  multiplayer-only upstream.
- **Kept quirks**: clearing the water flag clears all of `StateF`; the
  herbivore turn tests `State` (not `StateF`) for water; `morphTime`
  defaults to 1 (the manual; the released code leaves it 0, which would
  never finish a blend).
- **Deviations**: `goto TBEGIN` repeats at most 16 times per step (the
  original could loop forever on a bad target); a pack forms only when its
  leader was actually placed; the evacuation ship's `DShipPickup` and
  survival mode are not modelled yet; roar and water-death animations are
  not rolled at reset (their AIs are not ported).
- **Checked**: synthetic tests (fleeing, a wolf's kill type, defensive
  animals, idle groups, packs, death types, phase numbering), and
  `c2-parity mee-sim` over every dialect game folder we have: no
  non-finite positions, slowest step under 0.5 ms with 147 animals.

## 2026-10-19 — M9: dialect hunts on PC (menus, spawning; interim AIs)
- **Menu**: `_MENU.TXT` / `.c2map` lists (`c2_formats::menu_txt`); a
  huntable's menu slot, not its AI, is what a hunt passes on.
- **Resolution**: `c2_formats::mee::resolve` / `Tables::read` apply a
  hunt's `overwrite` / `addition` blocks with the engine's substring rules
  (so `area10` also matches area 1, and in the trophy room every block
  applies, as in `ReadCharacters`). The area number comes from the map name
  the way the engine reads its launch path (`area_of`).
- **Placement**: `Herd::place_mee` follows the modernized fork's
  `PlaceCharacters` / `spawnMapAmbient` (pack types, ratio picking in turn
  or at random, regions, avoid areas, the 40-cell distance, water depths,
  the density option), capped at the engine's 256 characters. Deviation:
  the engine halts past 256; we stop placing.
- **Targets**: `Hunt::is_target` uses a species' `radar` flag when the data
  sets it, else the chosen-AI bits (scoring halves non-targets).
- **Interim AIs** (until step 4 implements the families): the dialect's
  AIs run as the closest original one (`c2_core::mee::family`: Ice Age
  grazers as Parasaurolophus/Stegosaurus/Ankylosaurus, wolf and Micro as
  Velociraptor, Smilodon and bear as Allosaurus, Titan as Ceratosaurus,
  Ichthyornis as Pteranodon, the brachiosaur variants as the brachiosaur);
  fish, Mosasaurus, poacher and dog are not placed. Per-species animation
  slots and speeds (`runAnim`, `runspd`, `deathtype`, the first
  `idlegroup`) override the family's (`dino::Tuning`).
- **Weapons**: the renamed keys (`land_power`, `land_prec`, `land_fall`,
  `pic1`) are read as the original ones; the dialect's extra weapon
  features are not implemented yet.

## 2026-10-19 — Modder's Edition Engine data in scope (M9)
Owner's decision. Many full-game mods are made for the community Modder's
Edition Engine (MEE) rather than the original executable; they need its
`_RES.TXT` dialect and features. They are now supported in the public
crates as a dialect of the game data, reimplemented in Rust:
- Reference only, never committed, under gitignored `source/`: the
  upstream engine (`Carnivores-CPE`, branch `Map-Amb-Demo-2`, no licence
  file, so read and cite only, like the original source) with its
  *Modders Edition 1.2 Manual*, and the MIT "Modernized" fork. Code
  comments cite their file / function as they do for the original.
- No game or mod data in fixtures; tests stay synthetic.
- The vanilla path stays as the original: the dialect only applies to
  data that uses it.

## 2026-10-18 — Reading more modded game folders
Found by scanning a batch of mods (all game folders now parse):
- `_RES.TXT`: a `{ ... }` block outside any section is kept as an unnamed
  section and ignored by `to_data`, instead of failing. The original's
  `LoadResourcesScript` only looks for lines containing `weapons` /
  `characters` and skips everything else, so this matches it. Modded
  engines put their own tables there.
- `.CAR`: bytes after the `Anifx` table (some modding tools leave a few)
  are kept in `Car::trailing`, so rewrites are byte-identical.
- PC file lookup: a `_RES.TXT` file name may be a relative path with
  either slash (`MODELS\x.car`); each part is matched ignoring case, as
  on Windows.
- Many full-game mods target community-modified engines whose `_RES.TXT`
  is a different dialect (spawn / area / trophy tables, per-area and
  per-time `overwrite` sections, new AI kinds, new weapon keys). They
  load and render; their menus, spawning and AIs need that dialect
  implemented, which is not done yet.

## 2026-10-18 — M7: 3DS settings: animal run speed, ambient animals
Owner's request, because the game handles awkwardly with 3DS controls
and 10+ ambient animals nearby caused lag spikes on Old 3DS:
- `HuntOptions::dino_run_k` / `Ctx::run_k` multiplies the running gait's
  speed of carnivores, herbivores, small animals and the T-Rex (not walking,
  swimming or flying), so species keep their relative speeds. The 3DS
  setting is 50–100% in 5% steps; the PC passes 1.
- `PlaceOptions::max_ambient` caps the small ambient animals (`None` on PC
  = the original count). The 3DS uses 4, or 0 with ambient animals off,
  which also leaves their models and sounds unloaded (brachiosaurs and
  flyers included).
- Settings live in `settings.txt`, not the save, which keeps the
  original's format.

## 2026-10-18 — M7: kill notice, radar on the 3DS map
- `GameEvent::Scored { ctype, score }` when a kill adds to the account
  (with the tranquilizer too, which keeps no trophy). The 3DS shows the
  species and points for 4 s in the top screen's bottom right (owner's
  idea; the original shows nothing until the trophy plaque). The PC
  ignores the event.
- Radar: the 3DS map shows the hunted species everywhere, as the
  original's radar blips do; without it, only animals in view range (the
  owner's always-on map).

## 2026-10-18 — M7: title screen, extra ammo, longer Old 3DS view
- **Title screen** (owner): Play / Trophies / Quit over the original's
  `MENUM.TGA` (its painted buttons covered by the hunter's record). The
  trophy room left the area grid for its own button.
- **Extra ammo gear** (owner's idea, not in the original): x1 10 points,
  1 refill, kills score x0.95; x2 50, 2 refills, x0.95; x3 150, 3 refills,
  no penalty. A refill is `Weapons::resupply` (every carried weapon back to
  its starting ammo); the penalty is `HuntOptions::score_factor` (1.0 on
  PC, so nothing changes there).
- **Old 3DS view distance 26 -> 34 cells, objects 18 -> 24**: the owner's
  Old 3DS ran 60 fps with ~8 ms of CPU work per frame (sim 1.5, world 2.3,
  2D 4.4, waiting 11.4). More if the next measurement allows.
- Screenshots (L+SELECT, test script) now save both screens
  (`<name>.tga`, `<name>_b.tga`).

## 2026-10-18 — M7: one pack for the whole game, and the 3DS menu
- **Packs hold the whole game** (pack version 3): every area (plus the
  trophy room), animal and weapon. The menu picks, and a hunt loads only its
  area, the hunter, the ambient animals (always placed, as in the
  original), the targets and the carried weapons
  (`load::species_with` / `weapons_with`). Vanilla o3ds: 136 MiB on the SD
  card.
- **Night vision on the GPU** (a texture-combiner stage) instead of in
  the cooker, so one set of textures serves every time of day.
- **Linear memory 20 → 24 MiB.** With every ambient animal's sounds, a
  hunt needs 12–17.5 MiB; the heap still fits the 23 MiB loading peak.
  The cooker writes each piece's cost and the menu refuses a hunt that
  doesn't fit ("Too much for this console") instead of risking a crash.
- **Menu (owner's design, after the mobile game)**: touch steps on the
  bottom screen (area hexagons + sun/sunset/moon; animal hexagons; weapon
  and gear hexagons with Tranquilizer / Observer), a summary on the top.
  Prices, points and what is saved follow the PC menu (`Menu.cpp`). One
  hunter for now (`trophy00.sav`, named "Hunter"); name entry and more
  hunters come later.
- Dying goes back to the menu without saving, as from the PC menu.

## 2026-10-18 — M7: 3DS buttons
Owner's request, for the Old 3DS / 2DS, which have no C-Stick: the face
buttons look (X/B up/down, Y/A turn) and the D-Pad does what they did:
down next weapon, left binoculars, right show/hide weapon. Then (owner):
L jumps and D-Pad up toggles running. A/B still answer the leave prompt.

## 2026-10-18 — M7: 3DS sound
- `c2_core::sound` now holds what used to be the PC's `play_sounds` and
  `lr_gains`: which `SoundId` an event plays (`commands`) and
  `CalcLRVolumes`. Both front ends map ids to their own samples.
- Packs carry sounds as raw 16-bit mono at 22050 Hz (`C2RS/SND/*.PCM`,
  names in `c2-cook`'s `Cooker::car` / `wav` calls). They take the samples
  out of the slimmed `.CAR`/`.RSC`. They load into linear memory, which
  went from 16 to 20 MiB. The o3ds heap budget went from 40 to 38 MiB, the
  linear budget from 12 to 16 MiB. The vanilla test pack needs 7 MiB for
  sounds.
- No DSP firmware means no sound and a message, not an error.

## 2026-10-18 — M7: 3DS bottom screen
The owner's design: a **touch weapon picker on the left, the map in the
middle, a call picker on the right**, replacing the PC's number keys,
change-call key and Tab map. The map is always on, which the original
isn't. It shows **the animals in view range** (huntable ones bright),
whatever the radar option says. That's a deviation the owner asked for. In
the original, blips need the radar. Tapping the selected call calls; a
touch that moves more than 6 px is a look drag, so the Old 3DS can still
look by touch. The font is font8x8 (as on PC), cooked into the pack.
Pictures are the game's own `MENU/PICS/WEAPONn` / `CALLn`, resized by the
cooker.

## 2026-10-18 — M6: Old 3DS memory (first hardware test)
The first build crashed with an out-of-memory fatal error on the owner's Old
3DS and Old 2DS while loading. The emulator didn't catch it, since it gives
3DSX apps 96 MiB on either model. On hardware an Old 3DS app has 64 MiB,
libctru reserved 32 MiB of it as linear (GPU) memory, and loading the area
needed over 30 MiB of heap.
- **Linear heap 16 MiB** (`__ctru_linear_heap_size` in `c2-3ds`). The GPU
  data is 5–7 MiB, so the Old 3DS heap goes from ~30 to ~47 MiB.
- **Packs carry slimmed `.RSC`/`.CAR` files** (pack version 2): no terrain or
  model textures, which are in the `.TX` files, and no sound samples. Sounds
  stay as empty entries so indices hold; M7 will cook sound separately.
  The `.MAP` stays whole (fixed layout), but the 3DS parses it with
  `map::parse_lean`, keeping one light map and no `texture2`. The area loads
  first, while the heap is still empty. Packs went from 31 to 17–18 MiB.
- **The cooker measures instead of estimating**: it repeats the 3DS load
  steps under a counting allocator and checks the peak against the target
  (o3ds 40 MiB, n3ds 96 MiB), and checks the GPU total against 12 MiB.
  Vanilla AREA1 + 2 animals + 2 weapons peaks at 23 MiB.
- Running out of memory now prints a message and waits for START (nightly
  `alloc_error_hook`) instead of the system crash screen and reboot.
- Side effect, unchanged from before: the effects code's terrain colours
  (`texture_mid_color`) see no textures on the 3DS, which already happened
  after `trim()`.

## 2026-10-18 — M6: cooker and first 3DS build
- **Layout**: `3ds-ver/` is one nightly workspace holding `c2-cook` (host
  tool) and `c2-3ds`, as the README tree says; `default-members` is the cooker.
  rust3ds crates come from git (not on crates.io), pinned by `Cargo.lock`.
- **Pack = original files + GPU textures.** The 3DS runs the same `c2-core`,
  which needs the `.MAP`/`.RSC`/`.CAR` data anyway, so the pack carries them
  unchanged plus textures pre-tiled for the PICA200 (RGB565 terrain atlas,
  RGBA5551 models, scaled per profile). After loading, the 3DS frees what
  only the PC uses (≈ heap 16.6 MiB in use for the default pack). (Since
  the hardware test above, the pack's `.RSC`/`.CAR` are slimmed.)
- **Partial species sets**: the pack keeps the full `_RES.TXT` (character and
  weapon indices drive placement and AI) but only the chosen `.CAR`s.
  `Herd::spawn` now skips a species with no animations instead of placing an
  invisible animal; with every `.CAR` present nothing changes.
- **Gameplay view range on 3DS** is 42 cells (`ctViewR` at the original's
  lowest view option); only drawing distance depends on the profile.
- **Fog in the vertex shader**: the GPU's fog table is indexed by depth-buffer
  values, which at ~10 000 units of view put the whole fog ramp into one
  table entry. The shader computes distance fog like the PC, and a second
  combiner stage mixes in the fog colour.
- **Texture orientation** (found in the emulator): with our tiling (bottom
  image row first), the hardware's `t` equals the image's `v` directly.
- `unsafe` is allowed in `c2-3ds` only (citro3d is a C API).
- The test-script hook (`autotest.txt`) and L+SELECT screenshots exist so
  builds can be checked in an emulator and bug reports can carry pictures.

## 2026-10-18 — Zip mods
Owner approved the `zip` crate (9.0.5, deflate only via pure-Rust zlib-rs;
no bzip2/lzma/zstd/AES, which old Carnivores mods don't use). It sits behind
`c2-vfs`'s `zip` feature, so the 3DS build doesn't carry it. zip 9 needs Rust
1.88, so the workspace MSRV went 1.85 → 1.88. A zip's `HUNTDAT` may be at the
top or one folder down, like a mod folder; a zip's `mod.toml` is read from
beside `HUNTDAT`. Files from a zip are copied (not linked) into a built pack.
`recipe verify` looks at `.zip` files in `--mods` as well as folders, and a
zip's name without `.zip` counts as its folder name.

## 2026-10-18 — M5: recipes

**A recipe pins exactly the files it uses** (`sha256:` per file) and the
build takes only those from each mod, so the same recipe produces the same
game for everyone, whatever else their download contains. `recipe new`
pins a mod's changed files plus its `_RES.TXT` when that changes anything.

**Finding mods:** `folder` hint, then a folder whose name slugs to the id,
then the folder with the most hash matches. Download folder names vary;
hashes don't.

**Base check only warns.** `recipes/bases/c2-vanilla.toml` holds the SHA-256
of the owner's 192 vanilla `HUNTDAT` files (hashes, no content). Other
releases of the game will differ in places; mods still apply.

**Schema additions** (README §6 updated): optional `folder` per mod, and
`[[resolve]] entry = "section:name"` for `_RES.TXT` conflicts (the README
only had file paths). Patches are unified diffs, matched without `\r` and
allowed to drift from their header line; they must live inside the recipe
folder. Our own small applier, not a crate (≈100 lines, tested).

**Dependency:** `sha2` (RustCrypto) for SHA-256.

## 2026-10-18 — M4: mod system

**Combined games are materialised, not read through the VFS at run time.**
`c2-mods` layers vanilla and the mods in a `c2-vfs` VFS (provenance, merge
report), then writes the result as a folder of symlinks (hard links or copies
where symlinks fail) plus generated files (the merged `_RES.TXT`) in the data
dir. The engine keeps loading from a plain game folder, the folder is easy to
inspect, and the 3DS cooker (M6) can start from the same VFS. A folder is only
ever replaced if it carries our `.c2rs-pack` marker.

**Only `HUNTDAT/` counts as mod data**, minus logs/backups and map-editor
side files. Files byte-identical to vanilla are dropped, which turns
"full copy of the game" mods into their real changes.

**`_RES.TXT` is merged by entry**: blocks matched by `name`; a renamed block
in a slot vanilla no longer has by name replaces that slot (slots matter to
saves and the menu); otherwise appended. `prices` by key + occurrence.
Conflicting changes: later mod wins, reported. Identical changes from two mods
are not conflicts.

**Manifest extensions:** `[res]` gains `adds_weapons` / `modifies_weapons`
(README §5 updated). `[files]` empty means detect.

**Dependencies:** `serde` + `toml` for `mod.toml` (the format the spec
names). Zip support is not in yet: it needs the `zip` crate (with its
deflate dependencies), which per §8 waits for the owner's OK.

## 2026-10-18 — M3: menus and the trophy room

**The menus are reconstructed.** The released source is only the game; the
original menu was a separate front end that started it with command-line
arguments (`prj= din= wep= dtm= -double -observ`, see `Game.cpp`). We rebuild
the screens from the shipped art: each picture, its `*_ON.TGA` lit version and
its `*_MAP.RAW` hit map (`docs/FORMATS.md`). Player registration, the waiver,
deleting a player, main menu, statistics, hunt setup, options, credits and the
quit prompt all follow the pictures. List layout, fonts (the 8x8 font, scaled)
and wording of the statistics are our own.

**Prices are a budget, not a payment.** `_RES.TXT` `prices` gives `start`
(a new hunter's score) and a price per area, dinosaur, weapon and accessory.
The account shows the score and what is left after the current choices; a
choice that would go below zero is refused. Nothing is deducted: hunting a
5-point Parasaurolophus with a 10-point licence could never pay off otherwise,
and the shipped saves (scores 104/114) fit this reading. Observer mode is free
and needs no targets or weapons. No rank locks.

**Menu mapping.** Dawn/Day/Night = `TimeOfDay::Dusk/Day/Night` (the code
calls the first time "dusk", the menu "dawn"); dinosaur *n* = AI `9 + n`;
equipment = Camouflage, Radar, Cover Scent, Double Ammo (`EQUIP1-4`, the
`.NFO` texts). Camouflage, radar, scent and tranquilizer are kept in the save
as the game does (`CamoMode` etc.); other choices start fresh each visit.
Info texts are re-flowed to the panel width.

**Players.** Each registered hunter is a `trophy0N.sav` (N = 0..9) in our
data dir; on first run every `trophy0N.sav` in the game folder is copied
there (the game folder is never written). Key bindings, mouse sensitivity,
reverse mouse, units, aggressiveness, density, sensitivity and view range come
from the save and can be changed in Options; `ctViewR = 42 + OptViewR/8*2`.
Options the port doesn't use (resolution, textures, fog, shadows,
renderer, sound device) are kept in the file but not shown.

**Trophy room** is the original's `TrophyMode` (`TROPHY.MAP`): trophies stand
on the landing ring in their first animation frame, the hunter collides with
them, a plaque shows when 2.5 cells in front of one, `R` removes it, `Esc`
saves and leaves at once; no weapons, calls, map or AI. **Deviation:** the
original's exit path adds `Last` to `Total` again when leaving the room
(stats inflate on every visit); we leave the stats untouched. After a kill
in a hunt the plaque (`TROPHY_G.TGA`) shows for 20 s, as in the original.

**Menu logic lives in `c2-pc` for now** (README §4 puts the menus' state
machine in `c2-core`). It is tied to the PC art and mouse; the parts the 3DS
needs (budget, choices, player list) move to `c2-core` in M6/M7, when a
second front end shows what is actually shared.

**Brightness** is `BrightenTexture` done in the shader: each 5-bit channel
times `(OptBrightness + 128) / 256`, floored and capped, plus the same factor
on the sky/fog colour. **Pause** is `VK_PAUSE`, and `P` unless a binding uses
it (many keyboards have no Pause key). **Health recovery** (`MyHealth +=
TimeDt * 4` while alive and not paused) was missing until now; without it
drowning was 1.5x too fast.

**Death returns to the menu** (discarding the hunt's trophies, as the
original's `LoadTrophy` on exit does) when started from the menu; `c2-pc hunt`
still restarts.

## 2026-10-18 — Field of view, binoculars, scope, HUD text

**FOV matches the original:** `CameraW = VideoCX * 1.25` with its 4:3
correction gives a vertical half-angle tangent of 0.6 (61.9 degrees); the
binoculars (1.5x-3x) and scope (3x) divide it. On screens wider than 4:3 the
binocular and scope frames are stretched horizontally to cover the view (they
were drawn for 4:3); the zoom itself is unchanged.

**HUD text** uses the public-domain 8x8 font from the `font8x8` crate (MIT);
the original used Windows GDI fonts, which the game data doesn't include.
Units follow the save's `OptSys` (imperial when set).

## 2026-10-18 — Audio

**kira 0.12** (owner's choice), default features off except `cpal` (we only
play raw PCM; no file decoders needed). We don't use kira's spatial model:
the original's mixer (`Audio.cpp`) gives each voice left/right gains from
`CalcLRVolumes` (falloff `1224/(1224+d)` past 512 units, fading out between
6000 and 10000, its own pan law) and positioned voices stay where they were
emitted. We compute those gains and map them exactly onto kira's volume and
constant-power panning, re-panning every frame. 16-voice limit (extra sounds
dropped), ambience crossfades over ~0.74 s, as the original's 16 x 1024-sample
ramp. The jump and water in/out sounds are never loaded in the released
source, so they stay silent. Ambience zones and random sounds are game logic
in `c2-core`.

## 2026-10-18 — M3: weapons, detection, death

`c2-core::game::Hunt` holds a whole hunt (player, herd, weapons, wind,
health) so every frontend runs identical logic. Ported: `ProcessShoot`,
`MakeShot` (incl. the crossbow's three falling segments), `TraceShot`
(ground, water, objects, posed characters; `sfMortal` faces kill outright),
`MakeNoise`, `CheckAfraid` (sight with `TraceLook` and cover, smell with
wind), the weapon state machine from `DrawPostObjects`, `AddDeadBody`,
`AnimateHuntDead` and `ProcessDemoMovement` (death camera).

Quirks kept: `TraceModel` rotates objects by eighth turns (`(FMap>>2)&7`)
though they render in quarter turns; `ObjectsOnLook` carries over between
animals as the original's global does.

Deviations: weapon animation advances in the simulation tick (the original
advances it while drawing). Key bindings default to the owner's saved
`KeyMap` (W/S/A/D, LMB fire, E weapon, Shift run toggle, X crouch, Space
jump). The loadout menu doesn't exist yet, so every weapon is carried.

## 2026-10-18 — M3: dinosaur AI

All 15 `Animate*` routines from `Characters.cpp` are ported, plus
`PlaceCharacters`, `ReplaceCharacterForward`, `CreateChMorphedModel` and the
death animations. Where routines are copies with different constants they share
one parameterized function (`herbivore`: Par/Anky/Steg/Chasmo, `carnivore`:
Allo/Velo/Spino/Cerato, `small`: Mosh/Dimet/Gall); the parameter tables list
the original's constants and phase numbers per species.

**`MoveCharacter` runs its body.** In the released source its first line is
`return;`, so no dinosaur would ever move — clearly a debugging leftover, as
every dinosaur walks in the retail game. We port the body as written below it.

**Kept original quirks** (they affect behaviour subtly but faithfully):
- `StateF &= (!csONWATER)` (logical not) clears the whole field, not one bit.
- Herbivores test `State & csONWATER` (never set) instead of `StateF`.
- `PlaceCharacters` lifts Dimetrodon (not Dimorphodon) by 2048 at spawn.
- Raw phase-index comparisons (`_Phase <= 2`) differ in meaning per species.
- T-Rex and Ceratosaurus only spawn when chosen as hunt targets.

`AnimatePac` (Pachycephalosaurus) is not dispatched by `AnimateCharacters`;
it's a Carnivores 1 leftover and isn't ported.

The AI uses an exact port of MSVC's `rand()` (same LCG), so random decisions
come from the same kind of sequence as the original's.

`c2-parity dinos <AREA.MAP> [seconds]` places and runs the herd headlessly.
`c2-pc hunt ... --sim MS,AI,DIST,AFTER --shot x.png` renders a chosen animal.

## 2026-10-18 — M3: hunter movement

**Fixed 10 ms simulation tick** instead of the original's variable `TimeDt`.
The movement code is a direct port (it already scales by `TimeDt` and
sub-steps collision every 16 ms), so behaviour is the same at any frame rate,
and a fixed step makes runs reproducible for the planned logic-parity harness.
Rendering interpolates the eye position between ticks.

**Functions that read the global `PlayerY` take it as a parameter**
(`land_qh`, `land_ceil_h`, `check_collision`) so they can serve dinosaurs too.

`c2-parity walk <AREA.MAP> [seconds]` runs the hunter headlessly over real
terrain and prints the path: a quick regression check for movement/collision.

## 2026-10-18 — M2 viewer

**Stack: `winit` 0.30 + `wgpu` 30** (owner's choice), plus `glam` (maths),
`bytemuck`, `pollster`. Versions picked at least two weeks old. No image crate:
screenshots use a ~60-line stored-deflate PNG writer.

**Colour maths stay in the original's space.** The original lights
(`D3DTBLEND_MODULATE` with a grey vertex colour) and fogs raw 16-bit colour
values. We use non-sRGB textures and surfaces so `texel * light` gives the same
numbers. The owner's saves use `OptBrightness = 128`, which makes
`BrightenTexture` a no-op, so it's skipped for now.

**Terrain is generated in the vertex shader** from a per-cell storage buffer
(height, light, water, texture, flags) instead of building meshes on the CPU.
Each cell is 6 vertices; triangulation follows `fmReverse`, and the 4
`TDirection` UV tables in `DrawTPlane` reduce to rotating the corner, which
is how the shader does it. The PC draws a square window around the camera;
the 3DS will need cooked meshes instead (M6).

**Load-time fixups live in `c2-core::area`** (`CreateTMap` hardware path,
`GetLandH`, `GetObjectH`, landing list, trophy ring), so the cooker can reuse them.

**Sky, sun and lighting** follow `RenderSkyPlane`, `RenderSun`, `CalcLights`
and the cloud-shadow code in `Hunt2.cpp`. Night is rendered as the original
does it: `BrightenTexture` maps every texel to `(G/8, G, G/8)`, i.e. night
vision. The sun/moon is drawn additively (the original's black quad corners
are invisible in-game).

**Known approximations** (to check against the retail game): `sfTransparent`
faces blend at 50%; object quarter-turn direction is unverified; sun is not
occluded by terrain (`SkyTraceK` = 1). (`RandomMap`, per-vertex ground light
and distant sprites: done, 2026-10-23.)

## 2026-10-17 — TGA/WAV: read the header instead of assuming it

The original TGA loader assumes 16-bit uncompressed bottom-up and ignores the
header; the WAV loader assumes 16-bit mono 22050 Hz and ignores `fmt `. For
every vanilla file our output is identical to the original's (checked by
`c2-parity scan`). For files in other encodings (seen in some mods: 24/32-bit,
RLE and ID-field TGAs, a stereo 16 kHz WAV) we decode correctly instead of
reproducing the original's garbage, and report the format so the cooker can
convert it. This only changes behaviour for files the original misread.

## 2026-10-17 — Add-on scope; MIT license

**The public build targets add-on mods**: small, data-only mods that layer
on vanilla and merge cleanly. Whole-game total conversions are out of scope.

**License: MIT** (`LICENSE`, `license = "MIT"` in the workspace). Covers our
code only; game and mod data are excluded.

## 2026-10-17 — M0/M1 kickoff

**Repository layout differs from README §3 (owner's request).**
- `pc-ver/` is the Cargo workspace for all shared crates plus the PC backend.
- `3ds-ver/` will hold the 3DS backend as its own workspace (nightly toolchain,
  `armv6k-nintendo-3ds` target) that depends on `../pc-ver/crates/*` by path.
  A separate workspace keeps the nightly pin and devkitPro requirements from
  leaking into the stable PC build.
- `source/` (gitignored) replaces `reference/`:
  - `source/c2-original/` — the user's Carnivores 2 **install** (README called
    this `reference/game/`).
  - `source/c2-source/` — original C++ source (README's
    `reference/c2-original/`), cloned from
    `github.com/videogamepreservation/carnivores2` (mirror of the 2013 Assembla release).
  - `source/mods/` — mod installs.

**ReShade is out of scope.** The user's install includes ReShade
(`reshade-shaders/`, `dxgi.dll/.ini`, `d3d9.dll/.ini`, `opengl32.dll/.ini`).
These are third-party post-processing wrappers, not game data; they are
ignored by every tool and will not be ported to PC or 3DS.

**The released C++ source is not the retail 1.0.4 build.** It has no menu
code (`Interface.cpp` is a fragment) and never reads the `prices` section that
every `_RES.TXT` contains. Where the source is missing, behaviour will be
matched against the retail `carn2.exe` by observation, and noted here.

**`_RES.TXT` keys are matched exactly, not by substring.** The original uses
`strstr(line, "ai")` etc., so any line containing the substring sets the field
— e.g. a character named `'Maiasaura'` would have `ai` reset to 0. We match
whole keys, plus the aliases the substring rule actually made work in shipped
data (`hearK`, `lookK`). Verified: `c2-parity` carries a faithful `strstr`
copy of the original reader and diffs every typed field; no `_RES.TXT` we
have produces any difference, and none would trigger the original's halt. Unknown keys are preserved (`extra`) so mod data survives merge/write.

**Parsers return on-disk values; transforms live in the engine.** Vertex
`*2`/z-flip, `BrightenTexture`, face re-sorting, `TraceC = trace - 1`, and
`_RES.TXT` defaults are applied by `c2-core`, not `c2-formats`. This is what
makes byte-exact round-tripping possible, which the cooker and mod merger need.

**Counts are bounded by input size, not engine limits.** The original has
fixed arrays (1024 vertices/faces, 256 objects, 64 animations…). We reject a
count only if the remaining bytes can't possibly hold it, so mods that exceed
the old limits still load and no malformed file can trigger a huge allocation.

**Synthetic fixtures are generated inside the tests** (`c2-formats/tests/`)
rather than by a separate tool in `tools/`. Same guarantee (no real game data
in the repo), less machinery. A standalone generator can be added if the
parity shim needs files on disk.

**Format parity is checked by byte-exact round-trip instead of a C++ JSON
shim (for now).** Building the original requires MSVC 6-era DirectX/Glide
SDKs. Parse-then-write reproducing every real file byte-for-byte, with the
parser consuming the whole file, is a strong check of the layouts. A shim that
compiles just the loader functions with modern GCC remains an option if a
field's *meaning* (rather than layout) needs confirming.

**License:** superseded — MIT, see the entry above.
