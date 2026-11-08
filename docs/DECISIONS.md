# Decisions and deviations

## 2026-11-08 — Kill-animation regression scenario
- **A second script, not a second runner.** `kill_animations.toml` sets
  `script = "kills"`; the same `Director`, `Hold`, headless runner, render
  pass, recordings and comparison run it. Kill steps and checks are in
  `c2_core::scenario::kills`.
- **One hunt, every kill.** Each animal is released toward the pinned
  hunter, and after its kill the hunter is revived (`Arrange::Revive`: body
  removed, death camera ended, health restored, killer calmed and pinned
  again). One run per game, with one baseline and one PDF per game.
  Reviving is test-only and never reachable from play.
- **Killers are released, not scripted.** The released animal is set
  hunting, as `c2-pc --die` does, and its own AI makes the kill. This
  covers each AI's real strike path (T-Rex grab, eat, gore, Modder's
  Edition kill types) at the cost of waiting up to 15 s for a T-Rex to
  sniff and roar first.
- **"Body with the killer"** allows a body that stays where it fell. Our
  goring killers (the Chasmosaurus AI, which vanilla Ice Age's Diatryma
  uses) strike and run on. The check fails only when the body moves away
  from both its killer and the spot where it fell.
- **Species by model** (`model:FILE`) for add-on and standalone packs,
  whose character indices change with their mod lists.
- **Weapons**: the kill script fires nothing, so metrics record
  `weapons_to_fire` (0) and the comparison uses it. Older recordings
  without the field fall back to the registry size.

## 2026-11-07 — Game tab overlays only for Carnivores 2 instances; v0.5.1

An instance with no overlays of its own took the old Game tab's overlays when
its game was the Game tab's game or it combined standalones
(`Launcher::recipe_game_with_overlays`). Those overlays are Carnivores 2
releases shipped as changed files (the Modder's Edition). Over a standalone
base, their `_RES.TXT` won and the game ran Carnivores 2's roster with the
standalone's menu pictures. The editor, the recipe page's "use" and Play all
resolve the game through that function, and the first two save the result
into the instance, so the overlays became part of it.

Now only recipes on the `c2-vanilla` base, or with no base named, take the
Game tab's overlays. On any other base, overlays identical to the Game tab's
are treated as copies saved by earlier versions and dropped; any other
overlay is kept as the instance's own. A later change to the Game tab's
overlays would leave an old copy in place; the editor's Base game page lists
it under "Unpinned overlays" with a remove button.

v0.5.1 is a patch release: one fix, no features.

## 2026-11-07 — Regression scenarios as the check for fixes and optimizations; v0.5.0

Changes to gameplay, AI, rendering, loading or performance are validated with
the regression scenario against a baseline from the base commit
(`docs/REGRESSION.md`, "Validating a change"). An optimization or refactor
must leave every checkpoint state hash unchanged; a fix that changes them
lists which and why. `c2-regress compare` does not compare those hashes (an
animal that moves but stays alive passes), so the check is a documented
manual `diff` of `metrics.json` checkpoints for now. The pull request template
asks for these results.

The README's Windows install, PC command line, 3DS settings and build, `.cia`
and Wii U build sections were dropped with the launcher rewrite while seven
links still pointed at them. They are restored, updated for instances and
**Build > Platform**.

The release after v0.4.3 is v0.5.0, not v0.4.4: it adds features (regression
scenarios, instance rules, predation, the Android build), not only fixes.

## 2026-11-03 — Split survival configuration, navigation and wave updates

The 996-line `game/survival.rs` combines mode configuration and species
selection, terrain navigation, wave progression and animal placement. Keep it
as a 29-line facade and extract four private modules under `game/survival/`:
`options` (rosters, mobile/MEE waves and species predicates), `navigation`
(spawn-box fences, flow fields and their existing synthetic tests), `runtime`
(survival state, setup and wave lifecycle), and `spawning` (placement and animal
initialization). Each implementation file stays below 400 lines.

Keep the public `game` re-exports stable. Navigation types and shared helpers
are visible only within survival; `survival_step` remains accessible within
`game` for tick scheduling. Preserve random draws, fallback order, rest timing,
health/ammo behavior, navigation thresholds and feature attributes. Source
comparison confirms all 36 original function bodies match after whitespace
normalization; no gameplay logic or new dependency is introduced.

Validation: workspace Clippy with all targets and warnings denied, formatting,
all-features core rustdoc, the minimal `portable-hunt` build, and all 14 survival
regressions under `no_std` (`portable-load,hunt-diagnostics`) pass. The full
all-features core suite also passes. Asset-dependent integration cases may
return early without local game paths; ignored timing/stress cases and console
hardware remain outside this validation. The existing `sound.rs` unused
`FloatMath` warning is still present in the no_std unit-test harness.

## 2026-11-03 — Split hunt orchestration by responsibility

The 4,596-line `c2-core/src/game.rs` contained weapon definitions and ammo,
frontend events, options/input, hunt state, tick scheduling, firing and bullets,
animal detection, death cameras, audio, view helpers, trophies and exit handling,
plus 1,458 lines of tests. It already delegated chambering, survival and teleporting
to `game/` modules; extend that layout rather than impose menu/entity systems
that this file does not contain.

Keep `game.rs` as a thin facade with private submodules and public re-exports.
Existing `c2_core::game::{Hunt, Weapons, GameEvent, ...}` paths, including
`game::weapon_anim`, remain available to PC, VR and console callers. Split
`impl Hunt` by concern; shared state stays in `state.rs`. Helpers and fields
needed by sibling modules use `pub(super)`, limiting access to `game` rather
than the whole crate. Dependencies are imported explicitly. Tick order,
random-number consumption, gameplay bodies and feature attributes are preserved;
no event-channel or trait-object layer is needed for this mechanical extraction.

| Modules | Responsibility |
|---|---|
| `state`, `options`, `events`, `session` | Hunt ownership/initialization, inputs/tuning, frontend events, pause/exit |
| `weapon_info`, `mee_weapon`, `weapons`, `weapon_actions` | Weapon data, dialect keys, ammo/poses, input and animations |
| `update`, `combat`, `wildlife` | Tick order, shooting/projectiles, hearing/detection and species identity |
| `death`, `audio`, `view`, `trophies` | Death camera, ambience/calls, view queries/map image, scoring/collection/room |
| `tests/*` | The existing hunt regressions by behavior, with shared synthetic fixtures |

New implementation and test files are below 400 lines; `game.rs` is 43 lines.
The pre-existing `survival.rs` implementation is left intact except for explicit
imports; splitting its navigation and wave logic is a separate follow-up.

Feature-matrix validation also exposed stale test fixtures: add the existing
`DinoInfo::runs_as` and overlay `View::masked` fields with their neutral values,
remove an unused integration-test import, and import `alloc` macros/string types
in area/record tests so the no_std library test harness compiles.

Validation: the default-feature workspace Clippy check with all targets and
warnings denied, workspace formatting, core rustdoc, and both core suites
(all features; no defaults with `portable-load,hunt-diagnostics`) pass. Each
suite reports 97 unit tests and 31 integration cases passing, with five ignored.
Source comparison checked all
117 original functions and 17 type/module declarations after normalizing
whitespace and internal visibility. The minimal `portable-hunt` build and
`portable-load,hunt-diagnostics` no_std build also compile. Asset-dependent
integration cases require local environment variables and may return early;
the five ignored tests are stress/timing checks. Console hardware was not re-tested.
All-features Clippy still encounters the existing `prof.rs` diagnostic callback
`type_complexity` warning; no_std unit tests expose an existing unused
`FloatMath` import in `sound.rs` when the std test harness supplies float methods.

## 2026-11-02 — Public roadmap and versioned releases

`docs/ROADMAP.md` is now the canonical public roadmap, linked from the README
and development guide. It supersedes the earlier decision to keep the roadmap
only in an ignored local TODO file. Local investigation notes remain local.

The first tagged source milestone is v0.1.0. Both Rust workspaces and the shared
path-package entries in all lockfiles use 0.1.0 without dependency upgrades.
Every release gets an annotated tag, published notes and a minimal changelog;
`docs/RELEASING.md` records the gate. Build/unit-test evidence is distinguished
from interactive gameplay and hardware testing, especially for Windows, consoles
and VR. No game or mod assets are included in the release.

Newest first. Each entry: what, why, and what it affects.

## 2026-11-02 — Windows instance packaging and optional console UI

The full launcher packages saved instances for Windows using the current host merger
and a supplied x64 PE executable, matching the console builders' prebuilt-binary
workflow. Windows CI builds/tests native MSVC binaries and uploads asset-free engine
artifacts, with a static C runtime. Native gameplay remains unverified until tested
on Windows; Linux checks are not a claim of Windows runtime support.

Exports copy merged files (including dereferencing file symlinks), runtime overrides
and menu art into a staged portable directory. Only marked generated packages can
be replaced, after copying succeeds. No-argument executable startup uses its adjacent
portable config; explicit CLI arguments preserve the caller's working directory.
Per-instance output preferences remain local, outside shareable recipes.

3DS and Wii U visibility settings independently gate tabs, instance build actions
and tool diagnostics in the full launcher. Dedicated builders retain their controls.

## 2026-11-02 — Runtime borrowing and hunter merge review

Weapon fire eligibility borrows dialect settings instead of cloning them on each
trigger-held update. PC HUD notifications borrow existing message text instead of
allocating a copy every frame. Ownership copies needed across mutable shot/hit
processing remain explicit; no shared ownership or new dependencies were added.

Hunter merging rolls back appended animations, sounds and sound mappings when
an add-on exceeds the animation limit. A rejected add-on must not consume slots
or alter subsequent merges. Rollback truncates append-only buffers and restores
the small mapping table rather than cloning the base model. Sound mapping lookup
is bounds checked. A synthetic regression checks byte-for-byte preservation and
successful merging of the next compatible add-on. These are structural allocation
improvements; no frame-time improvement has been measured.

## 2026-11-02 — Follow-up mixed-pack playtest

Resolved hunt tables now record menu identity for every character, including
unslotted roamers. An authored radar flag cannot make a different animal a
selected target. This distinguishes selected targets from blue roaming blips.
A missing `eatAnim` has no eating phase instead of aliasing the first logical
animation; imported Diatryma therefore moves while running. Survival navigation
waypoints cannot trigger predator attack jumps, just as they cannot kill the hunter.

Loose add-ons with incompatible hunter meshes keep a separate, namespaced model
and AI-0 hunter entry, linked with `hunterFile`, as standalone games already do.
This preserves Saurophaganax's supplied 1003-vertex body and local kill indices.
Map selection uses the actual map count instead of the old 32-item bitmask limit.

At the owner's request, crouching suppresses weapon sway in both shot direction
and rendering on all three platforms. The PC mobile-style map no longer draws a
touch teleport button; its keyboard instructions use a small ordinary HUD label.

## 2026-11-02 — Console rebuilds from saved instances

The PC launcher's Wii U tab is always visible. Instances expose explicit PC,
3DS and Wii U actions; console actions resolve the recipe's downloads before
opening the builder. Output folders, packaging and artwork are stored per recipe
path in local launcher settings, keeping machine-specific paths out of shared
recipes. Resolved base folders and overlays are remembered with the instance.

Both console builders use the same recipe-input preparation. Combined recipes
always run `c2-pc recipe build` before cooking instead of reusing a stale merged
folder or falling back to unrelated global add-ons. Wii U artwork preparation
runs after that merge. Builds use the currently installed cooker and console
binary; rebuilding the toolchains remains a separate development step.

Validation: launcher regression tests cover recipe arguments (including spaces,
overlays and explicit mod bindings), combined-game preparation and per-instance
settings persistence. Console hardware validation remains separate.

## 2026-11-02 — Mixed-pack spawning and APK play-test fixes

Global MEE spawn/pack tables now load before characters, regardless of section
order. Previously a trailing global table discarded inline groups and reused
their indices for unrelated populations (Manospondylus replacing selected
Indoraptor, among others). Each group's failed-placement budget is independent;
an impossible region cannot prevent later target groups from being placed.

Selected animals retain their authored selected spawns when active on this map.
Otherwise they join an explicit whole-map target group, sized like the default
MEE hunt (at least one place per selected animal), instead of inferring the
largest resident group. Passive roaming spawns remain. Selected animals always
receive radar visibility. This implements the owner's cross-map hunting rule;
it deliberately overrides source mods' map restrictions for selected targets.
PC hunt-start logs now include selected species, model, slot and placed count.

The permissive script reader recovers at recognized top-level section headers
when a preceding section lacks a closing brace, and emits a warning. The local
Dangergrounds script's unbalanced pack table had swallowed its weapons and
characters; recovery restores 58 animals, its hunter and nine weapons. Nested
conditional blocks still parse normally. Standalone imports also use the same
engine compatibility layers as individually launched games.

`hunterCarryAnim` is preserved at runtime, and synchronized carried bodies
switch to it when the killer leaves its eating phase, following MEE's
`AnimateHuntDead` carry behavior. This fixes the repeated kill clip while
Manospondylus walks away.

Survival's route field rejects locally steep terrain using movement's slope
probes, disallows diagonal corner cutting, and only skips waypoints across
traversable segments. Spawn retries may relax height preference, but never
reachability; a failed spawn box falls back to reachable ring placement instead
of an unchecked final position. These address confirmed routing weaknesses;
full live play remains necessary to assess all terrain/AI combinations.

APK conversion excludes the unused free-to-play shop sprite config: its stale
Ice Age GUI_03 extent incorrectly halved sprite scaling. Dino button sprites
are remapped from original character IDs into the converted roster's order.
The PC More screen now uses the displayed board for both click detection and
hover highlighting.

Nibelungen uses `use_second_lod` with an empty primary terrain grid. Conversion
now reads its 512-square tile/rotation grids, subdivides each tile into four PC
cell textures, and preserves the original atlas indices for water. Other maps
retain their primary grids. Synthetic tests cover all four rotations and
preservation of non-texture flags.

Validation includes focused regressions for the above, workspace tests, strict
Clippy, portable-core compilation, and PC release builds. Local placement
probes checked the reported targets across the combined pack's 36 maps (216
species/map combinations, none missing), plus 15 each in MEE-addons and
C2+APK-addons. These probes use real maps, scripts and model-file existence,
with synthetic animation durations; they do not claim animation/gameplay parity.
Offscreen images confirm Nibelungen's terrain, all Ice Age portraits/indicators,
and More -> Options/Credits. Game assets and probe outputs remain untracked.

## 2026-11-02 — Standalone engine layouts, death cameras and desktop APK HUD

The original Triassic release is an Ice Age-layout executable with a T-Rex
at AI 19, despite using dinosaur names and files. Its Megalosaurus (AI 18)
has the bear's eight clips; its AI 2 is a flyer, not the C2 ground ambient.
A compatibility layer recognizes the joint Lystrosaurus/Peteinosaurus/
Megalosaurus/Gojirasaurus roster and original AI numbers, and skips already
scripted MEE games. It reuses our Ice Age rules through AI 18, adds the
Gojirasaurus T-Rex mapping, hunter clip 12 and camera settings, and preserves
model files, prices and menu order. These layouts were checked against the
locally supplied models; the Gojirasaurus settings match the local MEE
edition. This is an explicit compatibility approximation of the patched
executable, not a claim of exact parity with that unavailable engine source.
Hash verification reads base files before compatibility layers so our own
rewrites do not make a correctly pinned game appear modified.

Dialect loading appends a `hunterinfo` body only when `characters` has no
AI 0, retaining all animal indices. JPHL previously fell back to character
zero (Parasaurolophus) when the hunter died. A death type without `sleepAnim`
now disables sleeping: phase zero cannot stand in for an absent clip. This
intentionally tolerates incomplete older scripts instead of reproducing the
zero-initialized reference engine's running/dead phase alias.

The death camera restores MEE's minimum height at the watched demo point.
Additional framing changes requested after gameplay testing: scripted camera
distance/height grow with animals larger than scale 1; the view yaw faces the
actual watched position while the orbit position eases into place; movement
speed accounts for the orbit radius, since a fixed step cannot catch a large
circle moving at radius/1224 units per millisecond. Regression tests cover
view direction, height, settled radius and scale. Local offscreen rendering
confirmed the JPHL hunter model/clip and Gojirasaurus clip 12. Triassic's
supplied hunter clip 12 itself stays near ground level throughout its 38
frames; synchronization cannot turn it into a carried-body animation.

The reported Dinosaur Hunter session used an unmatched base plus a desktop
MEE overlay (also evidenced by Ice Age save storage and snow in its log).
APK recipe builds now require the recorded APK identity. The recipe page can
find the matching converted base, includes the launcher's `apk-base` folder
in its search, and refuses readiness for a mismatched APK. APK recipes do
not inherit unrelated Game-tab overlays. Explicit recipe overlays remain
supported. A local Dinosaur Hunter APK conversion and recipe build passed;
an Ice Age APK base was rejected before materializing the wrong pack.

On PC, the APK hunt HUD retains a smaller upper-left map compass and a
numeric ammunition count, removing the persistent weapon, binocular and
call touch icons and their backgrounds. When an APK binocular mask is
available, it replaces the PC frame; missing masks retain the PC fallback.
Offscreen images verified the normal HUD and single binocular overlay.
JPHL's reported jitter on nonfatal shots still needs a live reproduction;
we have not changed its reference-engine damage reaction speculatively.

## 2026-11-02 — Reusable animation poses and direct upload staging

The shared core adds `dino::pose::pose_into` and `model::pose_car_into`.
Callers own the output vector and receive a slice borrowing it; subsequent
poses reuse its capacity. Invalid clips clear previous output and return
`None`. Existing APIs that return owned vectors remain available, including
for console backends and collision tracing. Character interpolation and
transforms come from one private iterator used by both output forms.

PC animal rendering keeps one position scratch vector for all successive
animal and pickup-ship poses, rather than one retained vector per character.
`GpuModel::set_pose` expands face vertices directly into
`Queue::write_buffer_with` staging. The write-only view is dropped at the end
of each upload, so the shared position buffer can immediately be reused.
The destination receives byte copies through wgpu's `WriteOnly` API; no
alignment assumption or unsafe cast is needed. This removes our temporary
expanded-vertex vector and its copy into staging. wgpu still allocates its
own staging memory; this is not an allocation-free renderer.

Before refactoring, synthetic known-position tests established interpolation,
phase mapping, morphing, scale and rotation behavior. A temporary differential
audit matched the committed implementation exactly across 756 combinations
of phase mappings, previous/current phases, times and morph durations. Retained
tests cover known positions, front/tail bends, invalid clips, buffer reuse,
frame wrapping and zero playback rate. Upload tests verify face order, UVs,
lights and missing-vertex defaults using an unaligned byte destination, and
an opt-in GPU test successfully reads back the expected bytes:

```sh
cd pc-ver
cargo test -p c2-pc staged_pose_reaches_gpu -- --ignored
cargo run --release -p c2-core --example pose_bench
```

The synthetic CPU benchmark alternates allocating/reused paths after warmup
and reports five-run medians for 5,000 poses. One local release run measured
7.26/7.16 ms at 64 vertices, 58.31/56.54 ms at 512, and 226.54/226.38 ms at
2,048. Buffer reuse alone is approximately equal to slightly faster in this
workload. These are CPU-only timings, not GPU upload timings or frame rates.
PC gameplay and console hardware still need separate validation. Drawing
batches and per-tick event vectors remain profiling candidates.

Validation: workspace tests, all-targets workspace Clippy with warnings denied,
formatting, the core's no_std `portable-load` check, and PC/launcher release
builds pass. The GPU test is ignored by default so CPU-only environments can
run the suite; it was also run explicitly and passed during this follow-up.

## 2026-11-02 — Allocation and ownership review

Reviewed the hunt tick, particle updates, PC animal rendering and menu setup,
and merged-pack roster binding. Keep the existing data layout and safe Rust;
these improvements need no new dependencies or shared mutable ownership.

- Particle landings borrow the ring and burst buffers separately, appending
  rings directly. This removes the temporary landing vector and the repeated
  height query. New rings still age in the same tick and preserve their order.
- Trophy-room collision obstacles and the renderer's CPU instance staging
  buffer retain their allocation between updates (`clear` plus `extend`).
  Contents are rebuilt each time, including after animal removal.
- Menu filtering copies the small, fixed-size `Bits` value before clearing
  hidden selections; it no longer allocates a vector to end an iterator borrow.
- Pack roster binding indexes normalized menu names once. The first duplicate
  still owns the slot, preserving the old linear search's behavior.
- The PC menu's spawn-location helper now uses the common `menu_slot` resolver
  instead of `callNo`, so location hints follow combined-pack selection identity.

A release-mode host microbenchmark of a set-bit-only iterator showed a large
gain for sparse selections but a regression with all 512 slots selected. Keep
the current iterator until representative workload profiling justifies a
tradeoff. No frame-rate improvement is claimed from these local changes.

Further performance work should measure per-animal pose generation and upload
(`dino::pose::pose`, `GpuModel::set_pose`) and the temporary drawing batches in
`Dinos::draw`. These still allocate during rendering. Reusing pose storage
would affect shared APIs and needs animation equivalence tests and PC/console
measurements. Likewise, the hunt tick's temporary event vectors are candidates
only if allocation profiles show material cost; persistent scratch buffers
would add state and borrowing complexity. This review does not certify all
platform backends or establish that the engine has no remaining bottlenecks.

Validation: workspace tests pass with one existing ignored test; all-targets
workspace Clippy with warnings denied, formatting, the core's no_std
`portable-load` check, and PC/launcher release builds pass. Synthetic regressions
cover same-tick ring aging and menu-slot-based spawn hints. GPU rendering and
console hardware have not been exercised in this review.

## 2026-11-02 — Mixed-pack selection slots, survival waypoints and synchronized kills

**A menu slot belongs to a menu entry, independently of character order or AI.**
Pack builds preserve a separate `_MENU.TXT` even when the base used `_RES.TXT`
for its menu. Replacements update the entry in its existing slot; roaming
add-ons do not consume numbered menu pictures or prices. Combined characters
record `menuSlot` (our key), with -1 for an unslotted loose add-on. A source
game's `callNo` can name somebody else's menu slot in the combined pack, so it
cannot establish that add-on's selection identity. Selected roamers receive
their temporary menu slot in the resolved hunt tables. Original-engine
characters in a mixed pack record `nativeAI = TRUE` and keep their fixed
animation layout and original AI; adding Android animals must not reinterpret
the base animals as MEE animations. Mobile imports drop these inherited keys.

Numbered call files can establish an add-on's donor slot when it supplies no
menu picture. Art references and nested call numbers follow a moved slot;
file renames are applied together so one destination cannot be renamed again.
PC menus suppress repeated animal names without compacting slot numbers, clear
hidden selections when the huntable filter changes, and render missing animal
pictures from the resolved model. Existing materialized packs need rebuilding
for the new selection metadata and preserved menu list.

**Survival pathfinding waypoints cannot kill.** While a wave animal's AI is
steered toward a waypoint in place of the player, its contact attack cannot
start an eating animation or emit a hunter kill. This applies to stock and
MEE AIs. The normal attack resumes when pathfinding returns to the player.

**`hunterSync` is implemented.** The reference MEE
`Game/CharacterAnimation/AnimateHuntDead.cpp` synchronizes the body to the
killer during the configured eating phase (or while carrying a corpse),
including position, heading, pitch, roll, scale and animation time. This is
active even though the corresponding block in `AddDeadBody` is commented out.
Ignoring it left the Diabloceratops hunter clip displaced beside the animal.
The death state now retains the killer index, corrected when characters are
removed, and applies that synchronization after animation each tick.

## 2026-11-02 — Recipes edited in the launcher; a loose add-on's `_RES` entry over its menu lines; animals without models left out

**Edit and Clone (owner: "an edit option in the gui and also a clone option").** The Instances list has
Edit and Clone... beside Play/Open. The editor changes what a recipe's own fields hold (name, id,
version, base profile, each entry's credit and download page, how standalones' ambients are combined),
removes entries (with their load-order line and the resolutions they win), moves add-ons in the load
order (the `mods` list kept in the same order), and adds an entry from a folder, pinned off the UI
thread as "Save as instance" pins (`pin_game`, or `load_mod` + `new_recipe` against the recipe's game).
Clone is a save-as: id and name from the new file name, the folders given for the original remembered
for the copy, then opened in the editor. A recipe with patches must stay in its folder (they are found
beside it). The file is written by `Recipe::to_toml`, so only the comment block that opens the file is
kept (that is where every repository recipe has its instructions); a test checks every repository recipe
writes out unchanged. `[settings]` are not edited here: "Save as instance" takes them from the other tabs.

**A `_RES` entry wins over a `_MENU.TXT` one of the same name.** ARK Yutyrannus, Giganotosaurus 1.0.4
and Carcharodontosaurus V1 ship their `_MENU.TXT` lines (`MENU.txt`, `MENU_ ENTRY.txt`, `MENURES.txt`)
beside the `_RES` entry; the first text read gave the entry, which then had no model and took the
T-Rex's animation keys. Of two blocks with one name, the one with `file` is taken.

**An animal whose model is nowhere is left out.** Tany is made for Carnivores Cordelia: its `_RES.TXT`
is that game's whole file. TROPHY-MOSA's readme entries name another setup's models. Such an entry
(its `file`, or one an `overwrite` gives it, in neither the game nor the add-on, after the near-name
match) would be an animal with no model; it is left out, with a note. The hunter's entry is exempt (its
model is merged, not placed).

**Blank descriptions.** Genesis Redux ships `DINOn.TXM/TXU` with blank lines only. The menus make a
missing or blank description from the `_MENU.TXT` entry: name, then length (m / ft) and weight
(t / lbs) when the entry gives `length` and `mass`.

## 2026-11-02 — Death camera distance; one hunter model; one menu slot per animal

**Death camera.** The original circles every killer at 824 (1424 over a
T-Rex carrying the body). The Modder's Edition replaces this with values
from the killer's entry: `CamBaseLand`/`CamBaseWater` and
`CamYLand`/`CamYWater`, defaulting to 824 and 256, with the water pair used
when the killer stood deeper than its `waterLevel`. We now read and use
them. For killers that give none, we go further than both engines: the
camera stands back by the model's reach times its scale ×1.4, and lifts by
0.6 of it. The factor keeps the original's animals at the original's numbers
(the C2 T-Rex reaches about 1000, so 1401 is under 1424), so only bigger
add-ons move the camera. Without this, a big add-on fills the screen and
hides the body.

**Hunter entries in add-ons.** Some add-ons' `_RES` text includes their own
Hunter entry, naming the hunter model they ship
(`hunter1_gigano_mee.car`), as their readme says to "replace the hunter".
We merge that model's kill animations into the base hunter model rather than
copying it, so the entry keeps the base's `file`. Otherwise the last such
add-on names a model that isn't there, and the hunter's body is never drawn.

**Menu slots.** A menu slot is given only to an entry the menu lists
(`ai >= 10`). A second add-on of the same animal takes the first one's slot:
the merge keeps one entry, the later, in the first's place. Before this, both
cases took a number, and every later animal showed the next one's picture and
text. An add-on with a copy per game (`CARNIVORES (MEE 1.1.1)`,
`ICE AGE (MEE 1.1.1)`) has only the base game's copy read.

## 2026-11-02 — A folder checked for a recipe entry is also read from above its wrapper folders

Recipes pinned from an unpacked download list a standalone's files with the
download's folders in front (`bilewaters here/HUNTDAT/...`,
`Carnivores Enchanted (0.1.3)/Game/HUNTDAT/...`), but the launcher remembers
the game folder itself, so checking that folder found nothing. `check_folder`
now also reads the folder from k levels up, where k is the most of its last
folder names that any listed path starts with (a download can hold several
games, so not every path has to share the prefix). The entry's root is still
the folder as found. The search (`locate`) already had this view, through
the download folder.

APK imports (`c2-pc mobile`) are pinned by hash like any mod, so a recipe
pinned before an importer change no longer matches a fresh import. Such
recipes are re-pinned; the committed ones already match today's importer.

The same view applies to `--mods ID=FOLDER`, which is how the launcher passes
remembered folders to `play` and the builds. A recipe whose base the recipe
page found away from the Game tab (its own `game`) used to lose the Game
tab's overlays. When it combines standalones and lists no `[[overlays]]`, it
now keeps them, since combining needs the Modder's Edition and the page only
looks for the base game. Recipes with a standalone game as their base (no
standalones to combine) still play that game alone.

## 2026-11-01 — Fixes from the owner's play test: phone ground, phone calls, points, memory, forests, survival ways

- **Phone areas' ground tiles are transposed**: the APKs' terrain atlas keeps
  each tile with x and y swapped from the PC's. Checked on every patterned tile
  of both APKs' first areas against the PC games' own (the maps are the same
  cell for cell, rotations too): 62 of 62 (Ice Age) and 57 of 57 (Dinosaur
  Hunter) match only transposed. Untransposed, the edge tiles between snow and
  ice pointed the wrong way: white squares along the edges (3DS, Wii U, PC).
  `mobile_area::tiles` transposes. Games built from an APK need rebuilding.
- **Ice Age APK calls**: its call sets go by character index too, but its
  huntables are #116-#130 (the Dinosaur Hunter's start at #16): `call1`-`call9`
  are the PC Ice Age's own (same lengths, within hundredths of a second) for
  #116-#124 in its AI order. Every Ice Age animal now has a call (and so a call
  button on the consoles).
- **A new hunter's points**: a save under a new hunter's points is lifted to
  them (was: only under the cheapest hunt, which is 0 where some area, animal and
  weapon cost nothing, as in the all-in-one: a hunter at 0 stayed at 0). Points
  are needed, not spent, and kills only add, so a lower score is a save made
  under a lower start. PC and 3DS. The all-in-one recipe starts at 200
  (`[settings.balance] start`). 3DS Settings "Test": unlimited points for
  testing, the save's score untouched.
- **Long ambient loops** (3DS): an area's ambient loop is cut to 45 s by the
  cooker, its first half second cross-faded from what followed the cut. Fallen
  Kings loops ~5 minutes of music (13 MiB) on four areas, so no hunt there fit
  the 22 MiB of video memory ("Too much for this console"); now 8-16 MiB.
  Deviation: the loop is shorter than the game's.
- **Forests denser than the original's** (3DS packs): `c2_core::area::thin_objects`.
  No 48 x 48 cell window of the original's areas holds objects of more than
  36 888 faces; some standalones' reach 3-6 times that (Anachronous 142k,
  Mandibles 223k). Where the square the 3DS draws objects in (twice
  `object_view`) holds more than the original's budget (40 000 faces per 48
  cells, scaled), each tree or plant is kept with the chance budget / density,
  decided by a hash of its cell: the same for every player with that pack, and a
  removed tree is gone (collision too), as the owner asked ("as if half don't
  exist", not invisible). Landing spots and objects without a far sprite
  (buildings, bridges) stay. The original's areas are never thinned. Deviation
  from those games' scenery, on the 3DS only.
- **Ambient cap for dialect games** (3DS): the setting's cap (now 5, owner) also
  applies to a dialect game's spawn tables (`Herd::cap_placed`; packs are never
  split). Anachronous placed ~30 animals besides the picks.
- **"Trees as sprites"** (3DS hunt option, off by default): every object with a
  far sprite is drawn as it, near ones too. "Random ambients" is offered only
  for games with add-ons (the pack's `addons` flag from `c2-cook`).
- **Survival ways**: the straight line is taken only when it is a way (no cell
  on it farther from the hunter than where the animal stands); else the flow
  field's way, now from 3 cells out (was 12, where waves went straight up ravine
  sides). An animal on ground with no way (a slope too steep) is led down to the
  nearest that has one. Ways keep to 80% of the steepest slope an animal can
  climb. A lure is never nearer the hunter than 3 cells (the AIs measure their
  kill distance to it).
- **Wii U art**: the launcher's Wii U tab takes the user's own icon and TV and
  GamePad start screens per install (`c2-wiiu-builder --icon --tv-image
  --drc-image`); those not picked are cut from the menu picture as before.

## 2026-11-01 — The Modder's Edition's weapons: chambered rounds, reloads and racking, harpoons, retrieving, rounds and flashes drawn

A weapon with the dialect's keys now follows the Modder's Edition's ammo model
(`Hunt.cpp`: `ProcessReload`, `ProcessPump`, `ProcessFireMode`, the states in
`DrawPostObjects`; `refillWeapons`), in `c2_core::game::arms`. Weapons without
those keys keep the original's model.

- **Ammo**: rounds are fired from the chamber (`chambered`). After a shot, a
  magazine weapon (`reload = 0`) chambers the next round from the reserve
  unless `mustRack`. A weapon that loads one by one (`reload = N`) holds N and
  is reloaded by hand: a part reload (`rldAnimPart`) when some are left or
  fewer than N remain, else the full one. A magazine weapon's reload swaps in
  the spare magazine (double ammo). Racking (`rckAnim`) throws out what is
  chambered and chambers the next round. `autoReload` and `autoRack` do them on
  their own. `semiAuto` + `fullAuto` + `modAnim` switch the firing mode, and
  full auto starts on. An empty trigger clicks (`CLICKn.WAV`). The
  "no weapon" test counts chambered, reserve and spare. Unlimited survival ammo
  stops taking rounds from the reserve, as the dialect's `SurvivalMode` does.
- **Animations**: ten slots (`weapon_anim`): draw, fire, hide, the full and part
  reloads, rack, mode, and the empty poses (`emptyAnim`, `getEmpAnim`,
  `putEmpAnim`). Draw, fire and hide keep the original order when not given; the
  others are none, as the dialect's -1 (`rldAnimFull` too: ten of the games'
  weapons, all crossbows, bows and one shotgun, leave it out). A dialect weapon plays each animation's
  own sound (`Anifx`), and under water its `*AqSnd` or nothing. So
  `GameEvent::WeaponSound` now carries the model's sound number, which all
  three platforms already played it as. Under water every weapon animation
  runs at half speed. A load-one-by-one weapon with no reload animation at all
  (the dialect would read slot -1) takes its rounds at once.
- **Under water**: a `harpoon` stays out and fires there, with `aqua_prec`
  and `aqua_veloc` (not given: the land number, where the dialect would read
  -1). A round under the surface does `aqua_power` and falls by `aqua_fall`,
  leaves bubbles, and passes through the surface. Crossing it, a round takes the slower speed less its
  fall so far (`ldif`, `fallTotal`). No noise is made under water.
- **Retrieving**: a `retrieve` round that stops in the ground or an object
  lies there and is picked up within 300 units when there is room for it
  (`COLLECTn.WAV`). It goes into the chamber of an empty weapon that chambers by hand, else the reserve.
- **Drawn (PC)**: the round's model in flight and lying there (`bModel`, its
  animation looping, light 210), pointed along its way with its file's +z end in front. The dialect's
  own placement (a pitch in the morph and another in camera space) lines up
  only when the camera looks along the round. The muzzle flash is `MUZZ4.CAR`
  for its animation's length, turned by a random roll, with the view model. The HUD
  shows the chamber picture (`picc`), the chambered rounds, the reserve after
  room for a full load, the spare magazine below, and the round flash
  (`FLASH1..4.TGA`, a step each). The rounds' slide during a reload is not
  drawn.
- **Keys**: the dialect gives reload, rack and firing mode to the original's
  Look up, Strafe and Turn right bindings, and they still do their old jobs too. Here they get their own:
  R, F and V on PC (free in the default map), and D-pad Left, D-pad Right and
  the left stick click on a pad. The consoles have no buttons left (the
  owner's layout), so there `HuntOptions::auto_chamber` reloads and racks on
  their own. Their counters show all the rounds a weapon has. The muzzle
  flash, round models, chamber picture and click/collect sounds are PC only
  for now.
- Checked: `tests/dialect_weapons.rs` (`C2_GAMES_DIR`) loads all 70 weapons of
  the owner's extracted games from their own files, and fires each until it
  is empty, by hand and with `auto_chamber` (140 runs). Plus unit tests for part
  and full reloads, racking and the mode switch, the spare magazine with
  `autoReload`, harpoons under water and picking a bolt up.

## 2026-11-01 — Combined standalones: a hunter of another body is kept as the game's own

When a standalone's hunter model has a different vertex count from the base's,
its kill animations can't be merged into the shared hunter. Before, it was left out, and
its animals' kills played the shared hunter's animations at the standalone's
numbers (JPHL: 924 vertices against 475). Now the combined game keeps it as a second AI 0
character, "Hunter (<game>)", with its files under `MODS/<game>/`. The game's animals carry
`hunterFile = '<that model>'` (our own key), and their kill types keep its own animation
numbers (no remap). `Hunt::kill_hunter` lays the body on the hunter its killer
names (matched by model file, so platform rosters that are subsets still find
it), else on the first. A kill type naming an animation past the hunter's own
falls back to the first death (an original-engine game's kill types are borrowed
from the base's same-AI animals). Checked: JPHL combines with its own 8-animation hunter (its
animals' borrowed numbers 0 and 3 are `Hr_dead1` and `Hr_Kill` on it).
The consoles cook each standalone alone, so only a combined pack carries the extra
model.

## 2026-11-01 — M2 polish: see-through faces at 0x70, object turns checked, the water's shimmer

From the original's source, settling the M2 "known approximations":

- **`sfTransparent` faces** blend at the Direct3D renderer's vertex alpha
  0x70 (112/255) times the texel's alpha, SRCALPHA / INVSRCALPHA
  (`renderd3d.cpp`, `almask`). Its 3dfx path used a constant 0x80; we follow
  Direct3D, which is the renderer the game runs with today. Applies to PC and VR (`model.wgsl`) and the
  3DS (`TRANSPARENT_ALPHA`). The Wii U doesn't blend model faces yet
  (opaque or cut out), and stays that way for now.
- **Object quarter turns**: the original turns an object by
  `fi = CameraAlpha + FI * pi/2` with `x' = x cos + z sin, z' = z cos - x sin`
  (`RenderModelClip`), models z-flipped at load (`*= -2`). It turns collision boxes the same way
  (`Game.cpp`). Ours on PC, VR, the 3DS (`from_rotation_y`), the Wii U
  (`(x, z) -> (z, -x)` for one turn) and `collide::rotated` is the same
  formula, so this is confirmed, with no change.
- **Water light** (`Hunt2.cpp`): each map vertex's light is
  `168 - (int)(24 * sin(-pi/2 + RandomMap / 128 + RealTime / 200))`, with the
  `/ 128` an integer division (9 phases), and it modulates the water texture. So
  the water is darker than our flat 1.0 was (about 0.57 to 0.75) and pulses with a period of about 1.26 s, out of
  step from cell to cell. `c2_core::area::water_light` / `Area::water_phase`. On PC and VR the
  phase rides in the terrain cells' top byte. The 3DS keeps
  `127 sin(theta)` and `127 cos(theta)` in the water vertices' light bytes, and the draw's light
  selector carries `cos t` and `sin t` (the sum of angles), so its static
  buffers shimmer at no cost per frame. On the Wii U, the water vertices mark their
  light `-(phase + 1)` and `terrain.vert` works it out with `uZoom.y` (the time,
  `c2_water_time`). The original's water alpha (by depth and view angle, plus
  `2 * wdelta`) is not taken: ours keeps the water type's transparency.

## 2026-11-01 — Wii U: Ice Age and APK games get installs of their own

The launcher named the Wii U install of the base game `carnivores2` /
"Carnivores 2" whatever the base was, so an Ice Age or APK game would have
landed over a Carnivores 2 install. The base now names it
(`config::base_install`): `carnivores-ice-age` "Carnivores: Ice Age",
`carnivores-ice-age-mobile` "Carnivores: Ice Age (phone)",
`carnivores-dinosaur-hunter` "Carnivores: Dinosaur Hunter", else
`carnivores2`. The add-ons install takes the same name with `-addons` /
" Add-ons". Nothing changes for Carnivores 2 installs that already exist. The Wii U app needs no
change: `c2-cook --target wiiu` writes `_RES.TXT` from the layered files,
so Ice Age's rules reach it already turned into the shared form, as on the 3DS. Checked
headless: the Ice Age APK cooks to a 199 MiB install (5 areas, the trophy room, survival).
Not yet checked in Cemu or on the console.

## 2026-11-01 — Ice Age's APK as a game: its own animals on Ice Age's AIs, the rest from Ice Age donors

A game built from the Ice Age APK had only its 6 animals that aren't in Ice
Age, each run as a Carnivores 2 kind. Its 10 Ice Age animals were left out,
because no PC kind fits them: Brontotherium, boar, wolf, woolly rhino,
Diatryma, giant deer, Smilodon, mammoth, bear and yeti. So classic survival
on its 5th area had none of its stand-ins.

**The APK's Ice Age animals.** The APK's models have the PC Ice Age models'
clips in the same order. That was checked by clip count for all 12 (pig and
archaeopteryx included) and by the PC files' clip names. The hunter's 13
kills match too. So `mobile_base` writes them as Ice Age's own `_RES.TXT`
does, with Ice Age AI numbers 10–19 in Ice Age's menu order. The existing
`ice_age::layer` then runs them on the Modder's Edition's Ice Age AIs,
exactly as with the PC Ice Age.

**The APK's own animals.** On the Ice Age APK, Indricotherium, Hyaenodon,
Doedicurus, Andrewsarchus and Titanis take an Ice Age donor: the second
name in `DONORS` (Brontotherium, Wolf, Brontotherium, Bear, Diatryma). They
keep their clips as they are, with roles marked in the block (`iaDonor`,
`roles`). The layer builds each one's entry from the donor's
(`mobile::make_entry`): the donor's speeds and kill (the hunter's matching
death), with its own animations, numbers and menu slot. Their AI numbers
start at 20, after the Yeti.

**Result.** `c2-cook --check` of the Old 3DS pack passes: 15 menu animals
that place and move on all five areas, and classic survival on area 5 sends
its waves (rhinos, Smilodons, wolves). The Dinosaur Hunter APK's game is
unchanged (its `_RES.TXT` is byte for byte the same). Games built from the
Ice Age APK before this need rebuilding.

## 2026-11-01 — The phone's pictures and rules on any game: `--phone-pictures`, PC or phone rules

The owner wants the consoles' hunt screen layout to stay as it is: the
3DS's picker and its ammo counter, nothing over the top screen or the TV.
Only the pictures change, to the phone's.

**c2-cook `--phone-game DIR --phone-pictures weapons,animals,scope`.**
`DIR` is a game built from the APK. Its pictures replace the game's in the
merged game, so the 3DS pack and the Wii U install both get them:
- `MENU/PICS/WEAPONn.TGA` and `DINOn.TGA`, wherever both games have the
  model. They're matched by model file, because the names differ
  ("Moshops" vs "Moschops").
- `CALLn.TGA`, made from the middle square of the animal's picture, since
  the phone has no call icons.
- The scope mask, `HUNTDAT/MOBILE/SCOPE.RGBA`: the phone's `optic_sight`
  (the left half, which is drawn mirrored), at 180x240.

`VIEWFINDER.PNG` is the photo mode's frame, not the scope, and the white
"placeholder" in the old TODO note was a misreading.

How each console draws the scope:
- **3DS:** the mask is cooked as `C2RS/SCOPE.TX`, with a one-bit alpha cut
  at 32, so the faint cross-hairs (alpha about 70) survive. It's drawn as
  tall as the top screen, mirrored, with black beyond.
- **Wii U:** four new overlay quads: the mask's two halves, and black sides.
  `OVERLAY_QUADS` goes from 9 to 13.
- **Both:** the weapon model isn't drawn under the mask, as on the PC's
  phone layout, and the PC-style cross-hairs give way to the mask's own.

**The launcher.**
- **Picture choices** are on the 3DS and Wii U tabs: weapons, animals and
  scope, each Auto (phone), Phone or PC. Auto takes the phone's whenever a
  game built from an APK exists. That game is the recipe's or the Game
  tab's own if it was built from an APK, else one built before: Ice Age's
  for an Ice Age game, otherwise Dinosaur Hunter's.
- **Bullets:** the consoles keep their ammo counter, so `bullets` isn't
  offered there.
- **"PC or phone rules"** is on the Balance tab: shotguns, scores and
  prices, and animals' health. Auto is the PC's. On a game built from the
  APK, the rules are already the phone's. Otherwise a choice of Phone takes
  the APK import's values (`Balance::phone_tuning`) under the player's own
  edits (`Balance::with`), written to `balance-tuned.toml` and passed as
  `--balance`. That covers PC play, the 3DS and the Wii U.
- **Not yet possible:** "PC" on a game built from the APK alone, because
  there are no PC values to take. The phone's shotgun is its values only:
  how its pellets behave isn't modelled.

## 2026-11-01 — Recipes are whole instances: `[settings]`, the launcher's Instances tab

The owner wants the launcher to work like a Minecraft instance manager such
as Prism: each recipe is a game you can rebuild after an update without
redoing the setup. A recipe can now carry `[settings]`:
- `menu`, `survival`, `three_ds_target`, `ice_age_title`;
- `balance` (prices and scores, as the balance editor's file);
- `[settings.pictures]` (`scope`, `weapons`, `bullets`, `animals`) and
  `[settings.tuning]` (`shotgun`, `scoring`, `animals`), each `auto`, `pc`
  or `mobile`.

Unset values leave the launcher's choice alone, and a recipe with no
choices writes no `[settings]`, so the existing recipes are unchanged.

The owner's rule for `auto` tuning (`Recipe::mobile_rules`): the phone's
shotgun, scoring and animal values are used only on an APK base with no PC
game beside it, meaning no overlay (the Modder's Edition) and no
standalone. Everywhere else the PC's are used. Add-ons over an APK base
don't change this, since they bring no PC rules of their own. For `auto`
pictures, the game uses the phone's where the game has them.

The pictures and tuning choices are stored and kept through saves, but
nothing reads them yet. They take effect with the phone's hunt pictures on
the 3DS and Wii U, and with the phone's rules.

Instances are every recipe under `recipes/`, any folder deep
(`c2_mods::recipe::find_recipes`, `.toml` files with a `[recipe]` table, so
`bases/` is passed over). The owner's own go in
`recipes/instances/testing/` (gitignored), and the rest are the shared ones.
A recipe in a subfolder finds `recipes/bases/` by looking upward.

The launcher:
- opens on the Instances tab.
- Play looks for the downloads: first the folders the recipe was last given
  (`remembered` in `launcher.toml`), then the search folders (Settings;
  default `source/`, `source/mods/`, `addons/` and `mods/` beside
  `recipes/`). It plays straight away if everything is found. Otherwise the
  recipe page stays up with a "Missing" list: each download's id, author and
  download page as a link, plus "Copy the links".
- "Save as instance" saves to `recipes/` by default. It saves the recipe in
  use with the current choices, or the setup from the other tabs (base
  matched to a `bases/` profile, add-ons, overlays and standalones pinned
  from their hashes).

## 2026-11-01 — Wii U: ground reads stay inside their atlas tile at every mip level

The owner saw a faint grid over part of the ground on the Wii U, like
cracks between the textures. The terrain atlas packs its 128-texel tiles
edge to edge, and a cell's UVs sit 0.9465 texels inside its tile. That
covers the bilinear half-texel at level 0, but the inset halves at each mip
level (2026-10-30), so from level 2 the four reads at a cell's edge take
up to ~40% of the neighbouring tile (another texture, or an empty one).
Mid-distance and far ground showed every cell edge.

`terrain_opaque.frag` (terrain and water, the only users of the terrain
atlas) now picks the level itself by the GL rule (log2 of rho, nearest of
the 5 levels), clamps the read to half a texel inside the tile at that
level, and samples it with `textureLod` at the same level. Near ground is
unchanged: the clamp only takes effect where the GPU already read a smaller
level. It costs a few ALU instructions a pixel. The pixels are still tested
before shading (early Z), so hidden ground costs nothing. Scenery and
animals (256-texel cells, the cut-out shader) are unchanged.

## 2026-11-01 — 3DS: the phone's 76-cell range unless "3DS friendly"; frustum-culled, coarse far ground

**Gameplay range (owner's call).** With "3DS friendly" off (the default),
a 3DS hunt's `view_r` is the phone's fixed 76 cells (`MOBILE_VIEW_R`, from
`Game::Process`), so animals notice, chase and give up as on the phone. On,
it stays as before: 42 cells (the PC's lowest view-range option) and 60%
noticing, since a far animal on the 400x240 screen is a few pixels to aim
at. The setup carries `friendly` instead of `notice_k`. The hunt's
`render_r` is now the pack's drawn `view`, so shots carry and animals out
of sight are re-placed past what is drawn. The map's view circle and dots
follow `view_r`, as `DrawHMap` follows `ctViewR`.

**Drawing farther.** The chunk test was a radius plus "not behind the
camera", which kept over half the disc. Each chunk now has a box (ground and
water heights, its objects' tops and reach, worked out at load) tested
against the frustum's four sides, which keeps about a quarter. Past a
pack's new `detail` radius, chunks are built a quad per 2x2 cells, as the
original's far ground (`RenderGround` / `ProcessMap2` past `ctViewR1`: the
cell's texture and direction over the quad, never the reversed diagonal),
with skirts down to the lowest full-detail corner along each side to cover
the cracks. A chunk not built yet at its detail is drawn at the other one if
cached. The phone builds its quadtree on a worker thread; here chunks are
still built on the main thread, a coarse one costing a quarter of the
per-frame budget. **Deviation:** `ProcessMap2` uses the far texture layer
(`TMap2`); the 3DS frees that layer to save memory (`hunt::trim`), so the
near one is stretched instead.

Cooker profiles: New 3DS `view` 76, `detail` 42; Old 3DS `view` 56,
`detail` 34 (the full-detail radii are the old view distances, so up close
nothing changes; the Old 3DS's 56 is a guess until measured). The memory
estimate for chunk buffers is now the disc the renderer keeps (chunks whose
middle is within `view` + 16) with coarse chunks at 3/8, not the full
square. Packs from before have no `detail` and draw exactly as before.
Copies of the culling run on the PC count ground vertices a frame: 46k at 42
cells before, ~39k at 76 now looking level, 6k looking down. Not measured:
sprites (objects past `SPRITE_CELLS` go out to the drawn view, now farther)
and the frame rate on hardware.

## 2026-11-01 — APK games' home-menu art; survival waves never give up (the mobile Survive mode)

**Home-menu art for a game built from the player's APK.** Such a game has
no `MENU/MENUM.TGA`, so `cia::game_art` / `wiiu_art` (3DS icon and banner,
Wii U icon and splash screens) fall back to the phone game's own: the icon
is its launcher icon (the largest `res/*/ic_launcher.png`, kept by
`mobile_base` as `MOBILE/ICON.PNG`), the banner and splash screens its main
menu (`menu_main_background`) with its logo (`menu_main_logo`) over the top
left, as on the phone. All made on the player's machine from their APK.

**Survival waves never give up the chase.** Read from the user's
`libdinHunter.so`: `Menu::OnArcadeButtonAction` calls
`StartPlayingTheGame(false, true)`, which stores the second argument in a
global flag. Every carnivore's AI (`AI_Velociraptor`, `AI_Allosaurus`, ...)
and `AI_Chasmosaurus` read it. With the flag set, a hunting animal's target
is the hunter whatever the distance; without it, the PC's give-up rule with
a fixed range (12 160 units, `ctViewR` 76 x 160 for the raptor). Ours scaled
the give-up distance by `view_r`; the 3DS's is 42 cells, so a raptor gave
up past ~26 cells and an Allosaurus past ~21, while a wave spawns ~29 cells
out (`spawn_wave_animal`): it turned away at once, survival re-aimed it,
and so on. The classic carnivore AI (`dino/carnivore.rs`) now skips the
give-up for a wave animal (`c.wave > 0`), as the modded AI and the
herbivores already did. Hunts are unchanged.

**The phone's view range** is fixed at 76 cells (`Game::Process` passes
19 456 units to `Camera::CalculateFrustum`; fog per time of day is a
fraction of it, `day_fog_start` etc. in the `.rst`). It draws that far with
a terrain quadtree (`Terrain::CalculateQuadTree`, a bounding sphere and
lowest height per block, culled against the frustum) prepared on a worker
thread. `CharactersManager::Navigations_Process` is empty: the phone has
no navigation mesh; pathing is the original's `LookForAWay` /
`CheckPossiblePath`. Its noticing (`AI_CheckAfraid`) also uses a fixed
distance (9 760 units a step) rather than `ctViewR`.

## 2026-11-01 — Recipes: `--mods` repeatable and `ID=FOLDER`; APK and standalone bases; the launcher's recipe page

`--mods` (`c2-pc recipe verify|build`, `play --recipe`, `c2-cook`) now takes
several folders, and a value `ID=FOLDER` (only when no such path exists)
is the folder of entry `ID` alone (`recipe::mods_arg`): the launcher gives
each entry the folder the player picked, so nothing else is searched or
hashed. A recipe with nothing to find needs no `--mods`.

A base profile (`recipes/bases/*.toml`) may name its game's `credit` and
`url` (a standalone game as the base: `anachronous`, `jphl`,
`carnivores-triassic`, `triassic-mee`, each recipe of its own), or an `apk`
hash: the game `mobile_base::build` makes records its APK's
`sha256:` in `from-apk.txt`, and verify compares that (an APK base has no
file list: the folder is ours, rebuilt per release). `c2-cook --recipe`
refuses a recipe with standalones (build it with `c2-pc recipe build`
first) and puts the recipe's own `[[overlays]]` over the game.

The launcher's recipe page (Add-ons tab; drop a `.toml` or open one) lists
the entries with credit, download link and a folder each. A folder is
checked against the entry's hashes in a thread (`recipe::check_folder`);
dropping folders, or "Look in a downloads folder", runs the usual matcher
for the entries still to find. Only the listed files are taken. The game
is the Game tab's (with its overlays) or the recipe's own (a standalone
game, or built from the player's APK on the page); a base mismatch is a
warning as in `verify`. In use, play, the 3DS build (`<id>.c2pk` in place
of the add-ons and standalone packs) and the Wii U build go by it.

The 3DS batch of 2026-11-01 uses P.Rex's "my last carnivores update"
(ModDB) for Carnivores Triassic and Mandibles; Triassic MEE moved to an
install of its own; c2-android and ice-age-android became `c2-apk` and
`ice-age-apk` (the APK as the base game).

## 2026-11-01 — APK base: the phone's trophy room gets stands of its own (`TROPHY.SPOT`)

The PC places trophies on a fixed ring (`Area::set_trophy_mode`, cells
66..87) and starts the hunter at its door (76, 70). The phone's room is
elsewhere: a walled pit (its `.rst` bounds, cells 131..178) with a raised
block in the middle, so our trophies stood outside it on the plain. Where
the phone stands its trophies is in its code, not its data; until the
owner's screenshot of it, ours: the conversion writes `TROPHY.SPOT` beside
the map (our text: `start X Z`, then 24 `stand X Z` in the ring's order,
six a side four cells in from the walls, facing in as the ring does; the
hunter on the south walk facing the middle). `c2_core::load` reads it in
trophy mode (`Area::set_trophy_spots`, `Area::trophy_start`), so the PC, the
3DS (the cooker packs the file; `c2-cook --check`: 24 of 24 stands in
cells 135..174) and the Wii U all use it; a dialect game's own start still
comes first. Without the file nothing changes. `c2-pc hunt --fill` fills
the stands in memory for screenshots. The hunter starts at the south end
of the west walk, looking north along the west row (he starts facing
north). The PC's area loader reads its files itself, so it reads `.SPOT`
(and `.SNOW`) beside the map as `c2_core::load` does.

**Snow (the Ice Age APK).** Each area's `snow_density` (0.04 to 1 in its
`.rst`) becomes `AREAn.SNOW` (`density D`); the engine snows as Ice Age's
(the Modder's Edition's flakes) at D times its density, on every platform
(the 3DS and Wii U at their own flake caps). The Ice Age APK's stereo
sounds (IMA ADPCM and PCM) are now decoded and mixed to mono: the
conversion had stopped on one.

## 2026-11-01 — APK base: the launcher builds the game from the player's APK

The game tab has "Use my APK as the game..." (and an APK dropped on the
window with no game set, or on the game tab, does the same): the launcher
runs `c2_mods::mobile_base::build` into the data folder
(`apk-base/<apk name>/`, rebuilt clean each time) and makes it the base
game, which the PC game, the 3DS builder and the Wii U installer then use
as any other. The game tab calls it "the mobile game, built from your APK"
(a `HUNTDAT/MOBILE` folder). The conversion also writes the APK's Survive mode
(`mobile-survival.txt`) beside `HUNTDAT`; the game offers it (SURVIVE on
the main menu) when no `--mobile-survival` is given. Its area number is the
APK's own, which an APK-built game shares (Mount Ravan, area 5).

## 2026-11-01 — APK base: the options as the phone's list, in three tabs

From the owner's options screenshots (a game built from the APK, PC): the
phone's single list (labels at the left, values centred at the right,
sliders as dots between brackets with a red-framed knob, BACK, "[RESET TO
DEFAULTS]") on the options picture. Ours has some 30 settings to the
phone's 11, so they keep the PC's grouping as tabs (GAME, CONTROLS, VIDEO);
CONTROLS' 20 rows (mouse and keys) in two columns. RESET TO DEFAULTS puts
back a new hunter's values for the tab shown only (its keys too), saved
with the rest on BACK. The PC original's options are unchanged.

## 2026-11-01 — APK base: the hunt screen as the owner's phone screenshots; pick keys tap or hold

From the owner's in-hunt screenshots, for a game built from the APK (PC):

- **Controls.** The call and the weapon at the top right, the compass
  ("AREA MAP") and the binoculars at the top left, each side on the kit's
  translucent back (`game_buttons_back`); a "menu" tab at the top (Esc).
  The phone's compass is a 3D model (`compas.3dn`); ours is its face
  (`compas.crt`, the black around it cleared from the edges by the
  conversion) seen flat, a drawn needle where the hunter looks.
- **Binoculars.** The sight is the goggles' ends: set at the screen's sides,
  its inner edge stretched to the middle, as the phone's.
- **Map.** The phone's painted maps (`textures/maps/areaN_{dawn,day,night}_map`,
  grid and letters painted in) are kept as `MOBILE/MAPS/AREAn_TIME.PNG`; the
  map sits on a dark panel titled "AREA MAP" and the area's name, a pale
  circle round the hunter, the hunted animals yellow, RELOCATE under it
  when teleporting is on (Enter, as before: the mouse only moves the
  marker).
- **Esc menu.** The phone's hexagons along the top: RESTART, RESUME (in the
  phone's OPTIONS place: no options mid-hunt yet) and ESCAPE!, each with its
  key; HELP left out.
- **Pickers.** Weapons and calls in the phone's hexagons near the top, the
  one taken ringed yellow; a changed call shows its animal's hexagon.
- **Tap or hold (owner, all PC games).** The mouse looks, so the pick keys
  do both: a tap of the weapon key takes the next carried weapon (its
  hexagon flashes in the phone's look), a tap of the call key changes the
  call as the original's; holding either for 0.35 s pauses the hunt on its
  picker (wheel, arrows or 1-9 move) and letting go takes the one ringed.
  The 3DS and Wii U keep the original's switching on their second screens
  (owner).

## 2026-11-01 — APK base: the menus laid out as the owner's phone screenshots

The owner sent four screenshots of the phone game (main menu, maps,
weapons, dinosaurs). A game built from the APK now follows them:

- **Wide screens.** The phone's pictures fill the window (cropped, centred);
  the 800 x 600 board stays in the middle, darkening around it when a
  question is open. The PC's own menus keep their 4:3 frame.
- **Main menu.** The phone's logo and four buttons (HUNT, SURVIVE when a
  survival is offered, TROPHY, MORE). The PC's other buttons (statistics,
  options, credits, quit) and the hunter's name and points move to MORE;
  Esc goes back.
- **Hunt setup.** The honeycomb at the left (corner-up cells, the longer
  rows last: 2-2-3 maps and weapons, 4-4-5-5 animals; more rows when a game
  has more), the empty cells all over the background; at the right the
  item's name in the red FieryTurk face, its cost, and the files' own
  lines in Officina (whose small size has the ratings' pips); two rows of
  toggles with lamp and picture; BACK, "Your Account", NEXT/HUNT along the
  bottom. NEXT stays dim until an animal is picked, as the phone's. No
  prices on the cells (the phone shows the cost by the name).
- **Area texts.** The Dinosaur Hunter APK carries Ice Age's strings too;
  the conversion read them over the game's own, so the areas had Ice Age's
  names. Ice Age's strings are read only for Ice Age.
- **Kill notice and plaque** are on the phone's panels (`game_stats_window`,
  `trophy_info_window`).

## 2026-10-31 — APK base: each new animal its own slot; the hunt's missing pieces

**Slots.** The original's rules hunt, call and place by AI number, and the
menu's slot is a huntable's place in `_RES.TXT` (`MenuData::from_res`); the
two agree while the k-th huntable has AI 10 + k. The APK's nine new animals
shared their kind's AI, so on every platform a pick of one hunted its donor
(the 3DS loaded them together, `c2-cook --check` placed none). They now take
the next AI numbers after the PC's nine, 19 to 27, in `_RES.TXT` order (the
PC's nine first), as a patched exe's extra slots (Triassic's Gojirasaurus is
AI 19), so slot, call and placement are theirs. What they run is given by a
key of ours, `runsAs = <PC AI>` (kept in `CharacterDef::extra`, read into
`DinoInfo::runs_as`), which `stock_ai` takes before inferring one from the
clips: inferred, three of the nine stood still and the raptors ran as the
Allosaurus. The PC's original placement cycles targets over AIs 10..30
(was 10..20). Original data is unchanged: its huntables are AI 10..18 in
order. This supersedes "new animals take their donor's AI" (animals entry
below) for the AI number; the clips are still arranged in the donor's order.

**Missing pieces.** The conversion now writes the hunter's death cries
(`HUM_DIE1-4`), both footstep sets (`STEPS/HWALKn`, `FOOTWn`), the menu
sounds (`MENUGO`, `MENUMOV`; the phone's menu music as `MENUAMB`), a
`DOUBLE.NFO` of our words, and the hunter's own sound (its model is a
character's but its sound is in `sounds/hunter`). The phone game has no
water-ring model: `WCIRCLE2.CAR` is ours, procedural (a 16-segment band
growing from 24 to 128 units over two seconds, as the original's ring; our
texture). No sun or moon model either: none drawn, as on the phone. The kit
hunt screen shows the rounds left beside the weapon button and, without call
pictures, the picked call's animal on the notification bar; the loading
screen is the phone's picture and logo. `c2-pc play --screen loading` shows it.

## 2026-10-31 — APK base: the hunt screen from the phone's kit

An APK-built game has none of the PC's hunt pictures (`MAPFRAME.TGA`,
`BINOCUL.3DF`, `PAUSE.TGA`, `EXIT.TGA`), and its sniper has no scope model
(its texture is a white placeholder). With `MOBILE/` present, c2-pc draws
the phone's instead (`hunt_kit.rs`): the binocular and scope masks (the kit
keeps half of each; drawn mirrored, screen-tall, their outer edge stretched
to the sides of a wide screen; the weapon model is not drawn while scoped,
the mask's cross being the aim), the map large (three quarters of the
screen's height) in the kit's borders with its player marker and the mobile
radar look, the round weapon / call / binocular buttons down the right as
indicators (the mouse is captured, so they are not clicked), messages on the
notification bar, and the pause and leave questions on the pop-up panel
(also a quarter, mirrored four ways) in the game's lettering. Positions are
by eye until the owner's phone screenshots. A PC game folder has no
`MOBILE/` and is drawn as before. `c2-pc hunt` gained `--pause`, `--exit`
and `--message TEXT` for screenshots.

## 2026-10-31 — APK base: the PC menus drawn from the phone's interface kit

When the game folder has `MOBILE/` (an APK-built game) and no PC menu art
(`MENUM.TGA` missing, `MENU_ART` unset), c2-pc composes its menu boards from
the kit's sprites instead: the hunt background, wooden planks for buttons,
the hexagon cells, selector ring and lamps of the hunt setup, its sliders.
Each board keeps the PC's regions (the original's hit-map rectangles, so
keys, clicks and the menu code stay unchanged); a lit button is the same
plank drawn brighter. Words are drawn with the phone's own fonts (BMFont
`.fnt`/`.fnthd`: Lithos Pro for headings, the body fonts for text,
`c2_formats::bmfont`), exported by the conversion with their atlases. The
waiver and credits pages are our own text: the mobile game has no
equivalent. Positions follow the PC layout until the owner's phone
screenshots let us match the mobile screens.

The conversion's TGA reader now keeps full 8-bit alpha (types 2 and 10, 24
and 32 bpp; a picture whose alpha is all zero counts as opaque); the kit's
atlases lost their soft edges with the top-bit-only alpha. Some sprite
configs give half pixels (`tc_x = 238.5`), so sprite rectangles are floats;
an atlas's scale is the largest power of two fitting both its width and its
height (width alone picked 4x on the sparse `gui_04`).

## 2026-10-31 — APK base: menu files, and the phone game's interface kept

Owner: a game built only from an APK should look as close to the mobile game
as possible. So the conversion keeps the phone interface's art for the
frontends to draw their menus with, instead of us drawing PC-style frames:
`HUNTDAT/MOBILE/*.PNG` (the atlases, alpha kept) and `MOBILE/SPRITES.CFG`
(every `ui/sprites_*.cfg` sprite, the other game's menu left out, rectangles
scaled to the atlases' own pixels: the APK's atlases are a power of two
larger than the configs' layout; `c2_formats::mobile_ui`).
- Also written, for every frontend: `MENU/TXT` (areas, animals, weapons,
  times, observer, accessories from `localizations/strings.csv`, `%d` filled
  from the balance table), `MENU/PICS` (animals from `Help/*.png` by name,
  else drawn from the model; areas and weapons from the menu's hexagon and
  weapon sprites), `SOUNDFX/CALLS` and the underwater sound.
- The APK's `callN` sets go by character index (`call1` is index 16's):
  checked on the four whose calls are named. Weapon sprites 1-6 are the
  PC's six in order, 7 the photo camera, 8 the revolver.
- Positions of the phone layouts live in the game's code; matched by eye
  (owner's screenshots asked for).

## 2026-10-31 — APK base: animals, weapons and `_RES.TXT` from the game's code

The mobile games keep no `_RES.TXT`: `CharactersManager::CharacterInfo_Init`
calls `CharacterInfo_SetDefaultParams_DH` (`_IA` in Ice Age) and
`CharacterInfo_SetDefaultWeaponsParams` per character index, each a `switch`
filling a 5768-byte `CharacterInfo` with constants, a localization key
(`STR_T_REX`) and balance-table values (`DINO_T_REX_HEALTH`, found by
`strcmp`). `c2_formats::mobile_chars` runs those functions on a small AArch64
interpreter (calls stood in for by their PLT names) and reads the stores;
`Game::LoadCharacters` gives each index's files.
- **Field layout**, matched against the PC's values for the shared animals:
  0x1630 health, 0x1634 mass, 0x1638 length, 0x163c radius, 0x1640 score,
  0x1644 smell, 0x1648 hear, 0x164c look, 0x1650 shipdelta, 0x1654/0x1658
  scale0/scaleA over 1000; weapons 0x1664 power, 0x1668 prec, 0x166c trace,
  0x1670 loud, 0x1674 rate, 0x1678 shots, 0x167c reload (magazine), 0x1680
  aim field of view (the sniper's 30: `optic`).
- Index i runs the PC's AI i - 6 (7 Moschops: 1 ... 24 T-Rex: 18); those
  animals' mobile models are the PC's clip for clip.
- **A plain `_RES.TXT`, not a Modder's Edition one**: each new animal runs as
  its kind's PC AI (the add-on import's donors: Troodon as the Velociraptor)
  with its clips reordered into that AI's numbering; one whose death slot
  wouldn't hold a falling clip is left out. Names from
  `localizations/strings.csv`; prices, health and score from the balance
  table (mobile values, not the PC's).
- The mobile sniper's texture is an 8x8 white placeholder (the phone draws
  its scope in the interface): the PC scope picture is still to make.

## 2026-10-31 — APKs as base games: rebuilt as a PC game folder

The user's own APK (Dinosaur Hunter or Ice Age for Android) becomes a base
game by rebuilding its pieces as a PC `HUNTDAT` folder
(`c2_mods::mobile_base`, `c2-pc mobile-base APK --out DIR`), locally, from
the user's file. Every frontend (PC, the 3DS cook, the Wii U installer)
then reads it as it reads a PC install, so nothing downstream forks. First
step: the areas.
- **`.man`** (from the game's loader, `Terrain::Load` and
  `FilesManagerAndroid::ReadCompressed`): raw 1024x1024 height, three
  light maps, flags' low byte; a raw 512x512 mobile-only grid; then
  run-length grids (`u32` bytes, `(value, run)` pairs): texture, a
  mobile-only 512x512, objects, object heights, water, fog, ambient zones.
  Dinosaur Hunter's areas 1-3 are the PC's layer for layer (object heights
  93%). No second texture or flags' high byte: the PC leaves both unused.
- **`.rst`** (text) into the `.RSC`: the terrain atlas's tiles (64 px)
  scaled to the PC's 128; objects from `models/objects/*.3dn` (the PC's
  model plus a bone count and a sprite name; the model's own name is its
  texture's, as objects share them), bounds and shadow lines halved (the PC
  stores half), collision and lighting keys as the PC's flags; sprites
  from the 128-px atlas cells; the sky plane blended over the sky colour;
  fog colours as the PC's `0x00BBGGRR`; volumes from 0-1 to 0-255.
- The mobile games blend by alpha where the PC keys colour 0 on flagged
  faces: faces over see-through texels get the PC's see-through flags, and
  32-bit TGAs keep alpha (unless it is all clear) — this also reaches the
  mobile animals import.
- The mobile trophy room is its own map (an open field), not the PC's room;
  Ice Age's area 2 reuses Dinosaur Hunter's area 6 terrain with its own
  look. Both kept as the APK has them.

## 2026-10-31 — Trophy room start; Triassic's "-ERROR-" area kept

- **Trophy room start.** Both Modder's Edition engines start the hunter at
  the original's door (76, 70): the modernized fork hard-codes it, and ME
  1.2 reads `trophyPlayerX` / `trophyPlayerY` from `common {}` (every
  shipped `_RES.TXT` says 76 / 70). So our start was already the ME's. We
  now read those two keys too (`mee::Tables::trophy_start`, PC / 3DS /
  Wii U), so a game that moves them is followed; without them, 76, 70.
- **Triassic's 8th area is called "-ERROR-" on purpose.** Its
  `AREA8.TXT` reads "-ERROR- / Error 4052: This data file is missing or
  corrupted.", with a blank picture and, in the MEE version, a "Rank:" line
  that decodes (letters as numbers) to "FLEE WHILE YOU CAN"; the readme
  calls it the mystery 8th map. It's the author's, so the menus show it as
  written.

## 2026-10-31 — "All dinos huntable"; Wii U map dots and teleport button

Owner's requests after the console test.
- **"All dinos huntable"** (PC Options > Game, 3DS Settings, Wii U
  Settings), **off by default**. Off: a modded game's roaming add-ons
  (huntable AI, no menu slot: Suchomimus, Manospondylus) aren't listed, so
  they are met where their makers put them; on: listed after the menu's
  animals and hunted by their own slots, as before. This changes the
  default: until now roamers were always listed.
- **Consoles: roamers not picked now come on their own maps.** The 3DS and
  Wii U loaded a roamer only when picked (memory), so off would have hidden
  them entirely. A hunt now also loads the unpicked roamers whose tables
  spawn them on its map: on the 3DS as far as the pack's memory figures
  allow after the picks (`Setup::native_roamers`; the menu's memory check
  leaves them out, so they never block a hunt), on the Wii U before the
  ambient animals while model slots last. Not in survivals or the trophy
  room. The PC loads every character, so it already did this.
- **Wii U menu order**: roamers come after the menu's animals (by price
  within each), as on PC, so the list shrinks from the end when the setting
  is off. A saved pick by position may move once.
- **Wii U GamePad map dots**: on a whole-area view (zoomed out, or the TV
  map alone) an animal is a disc 9 texels across on an 11-texel dark rim
  (about 4 and 5 GamePad pixels; it was 3 texels, about 1.5 pixels, on
  green ground). Zoomed in they stay 3x3 texels (about 6 pixels).
- **Wii U teleport confirm**, as the 3DS: a tap on the map marks the spot
  with a yellow ring and shows a TELEPORT button under the map's middle;
  tapping it starts the 5 s countdown, a tap elsewhere on the map moves the
  spot. Switching the GamePad to the game view or restarting drops it.

## 2026-10-31 — Owner's console test: Gojirasaurus, the ME trophy room, the 3DS trophy room, a Mandibles stall

- **An AI the original engine lacks runs as the one its clips fit.**
  Carnivores Triassic's Gojirasaurus is AI 19, a slot its patched exe added;
  we ran unknown AIs as nothing, so it stood still, never fell when shot,
  and its body still killed the hunter. Its model has the T-Rex's ten clips
  in the T-Rex's order (run, walk, roar, swim, two looks, two smells, death,
  kill), so the patched exe's AI must have been a T-Rex one. Now an AI
  number outside 0-6 / 10-18 in original rules (`Species::stock_ai`) runs as
  the original huntable AI whose layout its model has: as many clips or
  more, a clip named as a death at that AI's death index, a hunter's first
  if `danger`, an exact clip count first; none fits: it stands, as before.
  `info.ai` stays its identity (menu slot, call, radar, placement): the
  character's `ai` is what it runs as, and calls and the target bit read the
  species' number. Dialect species (`mee::tune`) are unaffected: the
  dialect has AIs past 18 of its own. Played alone on PC, 3DS and Wii U;
  the combined game already converted it (`standalone::to_dialect`).
- **Empty menu pictures are drawn from the model.** Triassic's `DINO10.TGA`
  is an empty frame (the original shows a blank too). A picture whose middle
  half is one flat colour (`portrait::blank_picture`; 655 animal pictures in
  the owner's mods: real ones vary there by 12 levels or more, empty ones by
  1) is replaced by the model's portrait, as roamers' are: the Wii U menu
  and the 3DS cook (`C2RS/MENU/DINOn.TX`). Deviation from the original, for
  a picture its author never made.
- **The Modder's Edition trophy room uses the map's landing cells.** The
  dialect's `tropPos` indexes the map's own `OMap == 254` cells: the engine
  dropped the original's fixed 24-stand ring in trophy mode
  (`Resources.cpp`, the ring commented out). We loaded the ring for every
  trophy room, so `tropPos` picked ring cells: trophies stood in the
  original room's 66-87 square, a corner of the ME's bigger room (20 cells,
  52-102 x 64-114), on its paths. `Area::marked_landings` keeps the map's
  cells; dialect spots stand on them (the ring if a room has none). Original
  games keep the ring.
- **3DS trophy room fits the memory.** It loaded a model for every species
  on display; with add-ons that is up to 24 big animals, past what the menu
  allows any hunt, and it crashed on every modded pack. Species now load by
  stand while the pack's `cost.` / `heap.` figures fit (as the menu checks a
  hunt); a trophy whose species doesn't fit isn't shown (kept in the save).
  `place_trophies` skips species without a model, as `spawn` does.
  `c2-cook --check` now opens the trophy room too (every stand filled).
  Not yet tested on hardware.
- **Wii U: a scenery window too big for the buffer shrinks.** In a dense
  Mandibles jungle the 149x149-cell window passed the 1,048,576-vertex
  buffer; the app then streamed every visible object each frame (~120 ms a
  frame, ~7 s while crossing it; the owner's "lag spike"). Now the window
  keeps the nearest whole rings that fit 7/8 of the buffer (room for the
  crossings' new objects) and grows back once the whole window fits; a
  window that fits is taken whole, as before. The far rings it drops are
  in the fog.
- Both Wii U golden-hash tests (`wiiu_static_scene`, the resident scenery's
  visible streams) fail on the committed code as well: stale goldens since
  a lighting change, not this work; left for a separate update.

## 2026-10-30 — Wii U: world atlases back to RGBA8; roamers' menu pictures from their models

- **RGBA8 again.** The owner's first console run of the installs (red-high
  R5G5B5A1, `fb289f7`): the world "hella pixelated", animals' skin with
  pixel-sized holes, the gun not drawn. That is what the console reading
  another bit order does: the low colour bits land in the high ones (texel
  noise), and alpha comes from a colour bit, so the cut-out shader drops
  dark texels (most of a dino's skin, all of the dark-grey gun). The low-red
  order before it was also wrong on the console ("as if inverted"), and
  Cemu showed both differently from the console, so the right order is not
  known without a hardware test pattern. The atlases go back to RGBA8 with
  their mip levels, as in `carnivores-mipmaps-v1`, which the owner saw
  right on the console at near 60 fps. The 16-bit packer and its test are
  removed; 720p (L3) and the opaque terrain shader stay.
- **Roamers' pictures.** A roaming add-on (Manospondylus, Suchomimus,
  Anatotitan, Agathaumas in `c2-mee-addons`) has no menu slot, so no
  `DINOn.TGA`, and the Wii U menu showed none. The app now draws one from
  the model when the install opens (`c2_formats::portrait`, the 3DS cook's
  renderer; 357x208 like the originals, on the menu's panel colour), so
  installs stay the original files.

## 2026-10-30 — Wii U: any game as an install; one Wii U Menu entry per game

The Wii U read a fixed base-game roster (15 species, 6 weapons, five areas)
from flat files in `wiiu/apps/carnivores-probe/`. It now reads a game the way
the PC does, so add-ons, the Modder's Edition and standalones work:

- **Installs.** `c2-cook --target wiiu` writes the merged game (base,
  overlays, add-ons, recipe, balance) in the original formats, nothing
  converted: the `HUNTDAT` tree with upper-case names, `FILES.TXT` (the
  app's index; the SD card is read through `c2-vfs` by it) and `INFO.TXT`
  (game name, areas, snowy areas, roaming add-ons, survivals: what the 3DS
  pack's info says). The app (`src/install.rs`) builds its menus from the
  rules the 3DS port uses (Rules / Role / Roam / Pick: vanilla `_RES.TXT`
  and the Modder's Edition dialect alike), so rosters of any size: up to 24
  animals, 12 areas, 8 weapons (a 2x4 GamePad grid), paged calls. c2-core's
  `load` and `mee` modules build `no_std` (feature `portable-load`) for it.
- **One entry per game.** Each install is `wiiu/apps/<name>/` with its own
  `<name>.wuhb`, whose `/vol/content/install.txt` names the folder; the art
  (icon, TV and GamePad boot images) is cut from the game's menu picture as
  the 3DS `.cia` does. Saves and logs stay in each install's folder. An
  `.rpx` can't carry `install.txt`, so it reads the default
  `carnivores-probe`. Built with `c2-wiiu-builder --install NAME --title T
  --sd DIR -- <c2-cook game args>` or the launcher's Wii U tab.
  `stage_assets.py` (the old flat layout) is gone.
- **`c2-formats` at opt-level 1 on the Wii U.** At opt-level 2 or 3 the
  Modder's Edition rules spun forever in Cemu, inside `mee::trophy_spot`
  (the reverse key lookups of a `tropinfo` block; the emulated CPU thread
  at 100%, no panic, no allocation failure). Statically the generated code
  looked right and the host runs it fine, so it is either an LLVM PowerPC
  miscompile or a Cemu recompiler bug; not known which, so the workaround
  goes to hardware too. The crate is load-time parsing; the frame's hot
  code (c2-core, the app) stays at opt-level 3. Revisit with a newer
  nightly or a hardware test of an opt-level 3 build.

## 2026-10-30 — Wii U: the scope zooms; leaving from the menu with HOME

- **Scope.** The original multiplies `CameraW` / `CameraH` by 3 in optic mode
  (`Hunt2.cpp`), and `RenderNearModel` draws the gun with the same camera,
  so the world *and* the sniper's scope frame are magnified. The Wii U drew
  both at 1x: the frame showed as a small square over an unzoomed view. Now
  `Hunt::zoom()` scales the world in the vertex shader (`uZoom`, fog modes
  below 0 only, after the fog distance, so fog and the CPU culling are as
  before; the culling stays wider than the zoomed view, never narrower) and
  the view model on the CPU, widened from 4:3 to 16:9 as the PC does. The
  cross-hairs (`DrawOpticCross`, at the gun's last vertex) are two overlay
  quads, shown by the PC's rule (near the middle of the view).
- **HOME from the menu.** The owner's hang (2026-10-30 log: foreground
  release, EXITING, then nothing): after the loop, the menu waited for the
  GPU with `GX2DrawDone` although WHB's release callback had waited already
  and the system owned the GPU; that wait never returns. The menu now skips
  it when exiting, as the hunt's cleanup already did, and logs its cleanup.

## 2026-10-30 — Wii U: 720p TV world, opaque terrain shader, 16-bit atlases, 32-byte vertices

Four GPU savings for thick jungle, after the mip levels:

- **720p TV world.** The TV's world (sky to gun) is drawn at 1280x720, 2.25x
  fewer pixels, then scaled up (bilinear) onto WHB's 1080p TV buffer; the HUD,
  the overlay and the performance text go on top at 1080p, so text stays
  sharp. The 720p colour and depth buffers live in the memory of WHB's 1080p
  TV depth buffer (fast MEM1, 8.3 MiB, unused once the world is done: the 2D
  layers draw without depth), so no allocation of our own has to follow WHB
  across a foreground release. Only when the TV buffer is taller than 720
  (a 720p or 480p TV draws as before). L3 draws at the TV's own resolution
  for an A/B (overlay line 6, 5th letter S / N; capture `tv_world`).
- **Opaque shader for terrain and water.** `discard` in a pixel shader makes
  the GPU shade before testing depth. Terrain and water never cut out (the
  terrain texels are all opaque; water's flag is 0), so they draw with
  `terrain_opaque.frag`, the same shader without the discard; its
  `DB_SHADER_CONTROL` is 0x10 (early Z) where the cut-out one is 0x50 (kill).
  Same pixels, same draw order.
- **R5G5B5A1 atlases** (reverted the same day, see above). The three tiled world atlases are 16-bit. The
  original's textures are RGB555 widened to 8 bits (`rgb555_to_rgba8`), so
  level 0 comes back exactly; alpha keeps the 0.5 cut. The mip levels are
  still averaged in RGBA8 and then rounded to 5 bits, and so are the Wii U's
  dawn warmth and night vision (the original's `BrightenTexture` also works
  on 16-bit texels). Sky, map, HUD and menus stay RGBA8 (small or CPU-drawn).
- **32-byte vertices.** The fog (colour, and the mode in w) was a third
  16-byte stream with the same value in every vertex of a draw; it is now a
  vertex-shader uniform set per draw, and the seven fog buffers are gone.

Capture schema 11 (`tv_world`, `tv_target=` in the header). The first build
packed red in the low bits and alpha on top (`FMT_1_5_5_5`); on the console
the colours came out scrambled ("as if inverted", owner, 2026-10-30).
`GX2_SURFACE_FORMAT_UNORM_R5_G5_B5_A1` is OpenGL's `5_5_5_1` (red in the top
five bits, alpha the lowest; Cemu maps it so), the low-red order being
`A1_B5_G5_R5`: the texels are now packed red high, alpha low.

## 2026-10-30 — Wii U: mip levels for the world's atlases

The Wii U is GPU-bound in thick jungle (43-50 ms frames, ~29 ms of it waiting
for the GPU, at 1920x1080). Its three world atlases (terrain, scenery,
animals and guns) were 2048-wide RGBA8 with one level, so distant trees and
ground read scattered texels from main memory for every pixel. They now carry
five levels (to 128 wide), built on the CPU at load and checked by reading
every level back (on a mismatch, the linear atlas is drawn as before). The
sampler reads bilinear from the nearest level, the same four reads per pixel.

This follows the original: `MIPMAP = TRUE` by default (`Game.cpp`), and the
D3D renderer swaps to half-size textures beyond 10 cells for the ground and
12 for models (`renderd3d.cpp`). The GPU picks the level per pixel instead
of per triangle at a fixed distance. Cut-out texels (leaves) average their
colour over opaque texels only and keep the 0.5 alpha cut, so the distance
shows no black fringes; very distant foliage may thin a little.

The linear atlases (40 MiB) are freed once the tiled ones pass. R3 turns the
levels off in a hunt for a same-session comparison (overlay M / F; capture
`tiled_atlases` 2 / 1, schema 10).

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
  (Superseded 2026-10-30: installs cooked by `c2-cook --target wiiu`.)
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

**Known approximations** (to check against the retail game): (`sfTransparent`
and object turns: settled 2026-11-01); sun is not
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


### 2026-11-02 — PC secret population and mobile trophy room

PC Options adds “Total number of secret dinos on map” (0–255, default 10),
stored as `secret_dinos` in `c2rs.cfg`. Normal hunts cap unselected huntable
animals after placement, including dialect packs; retained packs get valid
leader/member indices. Zero removes all unselected animals, including ambient
animals, so only selected species remain. Survival waves keep their own roster.

The APK room's generated stands now follow the six-cell floor markings rather
than interpolation between arbitrary margins; their two-cell tiles are centred
on cell boundaries. Old generated `.SPOT` files migrate on load. Mobile trophy
movement wraps between the first and last stand of the closed room. The PC
mobile kill notice and post-kill plaque use smaller sizing; trophy-room plaque
sizing is retained. Visual confirmation in gameplay is still required.

### 2026-11-02 — Bound untrusted archive output and preserve generated installs

The security pass prompted by the local QC review implements ZIP decoded-byte
limits, extraction budgets, path validation in both ZIP VFS and pack output,
and metadata-name validation in mobile/Ice Age import. External RAR/7z extraction
is disabled because its actual expanded output could not be bounded. Generated
launcher installs use staged publication, restore the previous install when
publication fails, and retain/report recovery paths when restoration or cleanup
fails. A busy guard covers jobs and pending completion; PC game launch retains
its job until exit to protect its input folders. Dependency traversal is iterative
with stable ordering, and area-name parsing rejects invalid/overflowed numbers.

VR scope reviewed here: `gpu.rs::Buffer::write`, its mapped size field and raw
buffer/image destruction signatures; `animals.rs` capacity/destruction callers.
Out-of-range writes no longer perform pointer arithmetic. Capacity is read-only,
and destruction APIs explicitly require unsafe caller obligations. A synthetic
allocation test checks bounds without a Vulkan device. This is not a completed
VR ownership audit: constructor-failure cleanup and device/session/resource
lifetimes across scene replacement, session loss and exit still need review and
runtime diagnostics. No headset validation is claimed.

Policy, recovery instructions, validation commands and remaining limits are in
[SECURITY-HARDENING.md](SECURITY-HARDENING.md). Changes preserve gameplay rules;
cosmetic refactors and performance experiments from the review remain separate.

## 2026-11-02 — One launcher job owner

The launcher owns configuration, presentation data, and a single `jobs::Handle`.
Mobile/Ice Age importing, APK-base rebuilding, archive extraction, instance
pinning, recipe verification, recipe-editor pinning, and balance reads run through
that handle. Each worker extraction was validated separately before proceeding.
Tab rendering and build-step construction live in focused `app/` modules;
`app.rs` coordinates navigation and dispatches typed completion results.

A handle stays busy while its worker runs **and until its result is consumed**.
All entry points refuse to replace it, including dropped files. Recipe checks
are batched into one job instead of spawning independent row/base/scan workers.
Completion errors and worker panics release the handle on polling and clear
presentation loading states, allowing another operation. Configuration controls
are disabled during work; tab navigation and the log remain available. Running
the PC game continues to hold the handle until exit, protecting its inputs.

Regression tests exercise overlap rejection, results consumed once, failure and
panic recovery, staged rebuild preservation, recipe verification, editor pinning,
and balance reads. A headless egui test draws every tab without game data. These
checks do not establish native Windows, headset, or console hardware behavior.

## 2026-11-03 — Audit fixes follow parser and address-space boundaries

Reject excessive `_RES.TXT` nesting at parsing, so downstream recursive walkers
only receive bounded trees from files. Do not silently prune or clone a deep
subtree at a recursion limit: cloning would recurse again, and pruning loses
mod data. Serialisation instead uses an explicit stack, preserves out-of-range
sub-block positions at the end, and avoids cloning section bodies.

Bound mobile ELF instruction scans by the file-backed bytes at their virtual
address. Clamping virtual addresses to the file length can silently omit valid
code when section addresses differ from offsets. Validate PKM padding and the
whole encoded payload before output allocation, rather than just estimating
pixel counts from available blocks.

The pinned ZIP dependency already implements CP437 fallback; retain it and test
its behavior instead of maintaining another mapping table. Check tiling's
programmer preconditions in every build, so release cooking has the same clear
failure mode as debug cooking. No gameplay rules change. See
[the audit follow-up](SECURITY-HARDENING.md#2026-11-03-audit-follow-up) for scope,
CI coverage and the distinction between synthetic tests and owner-data parity.

## 2026-11-03 — Select the 3DS CI toolchain explicitly

The formats portability job requires nightly Cargo for its build-std invocation.
Set toolchain: nightly explicitly when using the shared pinned rust-toolchain
action revision. Dependabot can replace a nightly-branch action pin with a
stable-default revision; a trailing nightly comment does not select a channel.
Keep rust-src installed and retain the real 3DS target build as the CI gate.

## 2026-11-03 — Opt-in dynamic time on PC and experimental Wii U

The owner requested a 30-minute day-to-night transition and a 30-minute return.
Use a deterministic 60-minute visual cycle, starting at the selected time;
Dawn is the rising transition, and the same authored assets supply dusk.
Smooth weights interpolate the three authored states. The clock advances by
simulation time, preserving pause behavior, and restarts with the hunt.
Trophy rooms and existing settings default to fixed time. No 3DS or VR option
is added. Spawn conditions retain the selected starting time.

PC keeps the three sky/light maps on the GPU, blends the night-vision tint,
and updates object lighting as the clock moves. Extract the celestial
renderer into `render/sky.rs` rather than expanding the main renderer further.
PC game options now have a bounded scroll viewport shared by drawing and hit
testing, with fractional wheel accumulation and clickable arrows. Reset in
the mobile-style menu includes hidden options.

Wii U keeps cached terrain/scenery geometry: shader grading blends warmth,
night vision and average authored map brightness, while the shadow pattern
stays at the starting time. Refresh only the 256x256 sky texture once per
played second, after the existing GPU synchronization. This deliberately
approximate path is labelled experimental; hardware performance is unverified.
See [Dynamic time](DYNAMIC_TIME.md) for timing and user-facing limitations.


## 2026-11-03 — Dynamic hunts use optional night equipment and current-time scoring

The owner requested a darker, optional long-hunt mode. In dynamic-time hunts,
night vision (150 points) and a flashlight (15 points) are selected through the
existing equipment/loadout budget. They start off and toggle independently:
N/L on PC, right/left stick clicks on Wii U. Fixed-time hunts retain automatic
night vision; 3DS exposes none of the new options. The Wii U stick-click debug
controls remain available in fixed-time hunts.

Dynamic hunts award 20% extra between the dusk and dawn clock stops; this
replaces, rather than stacks with, the fixed Night selection's 10%. Include
the awarded percentage in the score event so kill notices cannot change as
the clock moves. Spawn tables remain selected at hunt start. The flashlight
is a soft camera-facing cone with distance falloff; it does not render shadows.

Shorten the PC option label to "Dynamic time" to avoid its On/Off value.
PC map navigation uses a bounded viewport shared by the map image, markers,
and teleport picking. Wheel zoom anchors at the pointer, right-drag pans and
Home restores the full map. Keep the original map frame fixed and crop marker
geometry/UVs at the viewport edges. Rendering is extracted into hunt_view/map.rs.

PC map zoom and panning are opt-in through "Map zoom / pan" in Video options.
Persist the setting in c2rs.cfg and default missing entries to off. Disabling
it keeps the full map and hides navigation hints/pointer, except the pointer
needed for independently enabled teleport selection. Both PC menu styles share
the setting; console controls are unchanged.

### Extended Night Hunts (optional)

Dynamic time of day, toggleable night vision with point costs, and flashlight gear with a 20% score bonus for night hunts. Off by default; the game plays as the original when disabled.

PC dynamic-time hunts add procedural stars to the existing sky pass. Use
camera-relative direction with a seamless upper-hemisphere projection, so
stars stay fixed while walking rather than scrolling with the cloud texture.
Fade with the night blend and toward the horizon; stars emit light independently
of ground ambient darkening, while retaining night-vision grading. No game
assets or extra textures are needed. Classic skies and console renderers remain
unchanged. GPU pixel tests cover night visibility, daytime/classic absence,
twilight fading and stability under camera translation.

Dynamic-time night scoring checks active night vision at the scoring tick: it
cancels the +20% night bonus while switched on. Carrying inactive goggles or
using only the flashlight keeps eligibility. PC and Wii U supply the active
state to shared scoring before each simulation tick; the scored event retains
the actual awarded percentage. Classic night scoring remains +10%.


### Survival population and spawn boundaries (2026-11-03)

Survival clears all ordinary map wildlife, including herbivores and ambients,
and disables ordinary kill replacements. Only the wave roster populates a
survival hunt. Classic/mobile and MEE waves with a designated spawn box must
stay inside that box. The navigation field covers the fenced corridor including
the box, rather than rejecting spawns beyond the former 96-cell search radius.
After random placement attempts, check cell centres in
it instead of falling back to a ring around the hunter. If no animal can be
placed, report the blocked spawn and retry the same wave after its rest.
Existing modes without a designated box retain their wave placement rules.
Player movement remains enabled, including classic survival, by owner request:
it allows sidestepping carnivores while hill pathfinding remains imperfect.


### Recipe collections and instance hunters (2026-11-03)

A pinned loose add-on collection is loaded as separate child add-ons before
assigning menu slots. Each child retains only recipe-pinned files; recipe load
order and winner declarations still address the collection. This prevents
several Android imports' donor-slot pictures and calls overwriting each other
or the base animals.

PC recipe launches keep hunters and records under `c2rs/instances/<encoded recipe ID>`.
Portable Windows instance builds preserve that ID through `--save-instance`.
Instance hunters start empty instead of importing source-folder trophies.
Direct launches retain the existing shared saves (including Ice Age's separate
shared profile). Existing saves are neither moved nor deleted; cloning an
instance must give it a distinct recipe ID to keep progress separate.

### Experimental PC flashlight beam

(Superseded 2026-11-05: the beam is now switched in the hunt with K, see
"Multi ammo, advanced observer, in-game beam" below.) PC Game options could
select a narrower flashlight cone with twice the reach (40 cells instead of 20). The existing broad beam remains the default. This
changes illumination only: equipment cost, the L toggle, fog and render-distance
limits, and night scoring remain unchanged. It uses the existing surface-light
shader without an additional rendering pass; consoles retain their current beam.

### Points-based survival rewrite (2026-11-03)

The menus now offer Classic and Modded, independently of APK survival files or
MEE `survivalIndex`/arena declarations. Classic uses stock C2 or Ice Age animal
identities (exact name and root model path); it is hidden when fewer than six
stock identities remain, so standalone packs and namespaced imported animals
are not mistaken for the base game. Replacements retaining a stock name and
path remain that stock identity; this is identity detection, not an asset hash
or authenticity check. Modded includes every eligible loaded species.

An animal costs its configured hunt-menu price, including numbered `prices.dino`
overrides from `_MENU.TXT` (or `_RES.TXT` when there is no menu file). Roaming
add-ons inherit the same-AI donor price used by the hunt menu (zero when an
explicit roaming add-on has no donor). Negative or missing prices do not qualify; a configured zero price stays free and is bounded by the
wave cap. Kill scores do not set wave prices.

By default, starting budget B is max(10, 2 × cheapest eligible animal's price),
computed separately for the selected mode's loaded roster. Round n receives
floor(B × 1.1^(n−1)); rounding happens after compounding. Players can override B
with **Survival starting budget** in PC Game options (Automatic or 10-point
steps through 2550), or `survival_budget` in `c2rs.cfg` for other integer values.
Packs can set `survival_start` in the menu script's `prices` section on all
platforms; zero/absent means automatic. A nonzero PC setting overrides the pack
value. Nonzero values below 10 clamp to 10.

Random affordable choices repeat until nothing fits or the platform wave cap
is reached; unused points do not carry over. This budget never reads or spends
the hunter's saved points. For example, prices 50 and 100 give B=100 and round 7
has 177 points. Very late budgets saturate at u32::MAX. Existing platform
population and model-memory limits still apply;
the 3DS loads as many candidate species as its memory check permits.

Missing models, non-attacking AIs, dialect attacks with zero kill distance or
zero running speed, and invalid costs do not qualify. A hunt with no affordable
first-wave animal reports an error rather than starting an empty survival.
Movement, health/ammo replenishment, capped wave health, no hunting trophies or
score rewards, and removal of ordinary wildlife remain unchanged. Legacy wave
readers remain internally for compatibility tests; Hardcore/Custom/APK-specific
choices are no longer exposed by the game menus.

### Survival arena correction (2026-11-03)

The points rewrite initially used random maps and circular spawns. That was an
implementation error: Classic/Modded select rosters within the mobile game's
survival mode, not a different arena mode. Both now launch directly from the
mode chooser into Mount Ravan (AREA5), daylight, with only the shotgun and no
equipment. The APK is not required: placement uses the known PC-coordinate
hunter spot and approach box. Budget waves use the existing fenced ravine
navigation and boxed spawn retry, never the circular fallback. The hunter faces
the approach; movement stays enabled. No animal/map/weapon setup pages intervene.
Packs must provide the survival map and shotgun; missing assets produce an error.
Testing an arbitrary first wave on another map did not validate this behavior.

### Survival: only Classic and Modded; stock entries in dialect packs (2026-11-05)

Owner playtest: an Ultimate modpack instance (Modder's Edition over the base,
plus standalones) could not start Classic. Cause: the Reborn standalone's stock
replacements (`allo.car`, `velo2.car`, ...) are original-engine entries
(`runspeed`, `kradius`, no animation keys). With the Modder's Edition as a base
layer the merger treated the base as dialect and did not mark them `nativeAI`,
so they loaded as dialect animals with no gait, speed or kill distance: survival
rejected them all (and they could not move in ordinary hunts either). Fixes:
the merger now marks any entry with an original AI (1-18) and no animation
layout `nativeAI` whenever the merged rules are dialect, and the loader applies
the same rule to packs already built. The Modder's Edition's own entries always
carry animations (only its Hunter, AI 0, has none), so they are unaffected.

Every survival variant not based on the mobile game's is removed: the game
offers Classic and Modded only (mobile Survive files, the dialect's own
`survivalIndex` waves, Hardcore/Custom and the ring spawn are gone, in the
shared code, PC, 3DS, the cook's check and the launcher). Both launch straight
from the chooser into the Mount Ravan ravine with the shotgun. `--survival-modes`
and `--mobile-survival` are still read by `c2-pc` and `c2-cook` and ignored, so
older launch lines keep working. The launcher's survival checkboxes are gone.

### Android (APK) mode: phone menus only, one light at a time, paged animals (2026-11-05)

Owner playtest (PC and Steam Deck, an instance built from the Dinosaur Hunter
APK):

- **Menus.** A game built from an APK has no PC menu frames. Choosing the
  "Original" menu passed the APK folder itself as `--menu-art`, the menu then
  skipped the phone layout and drew empty PC boards (the black screen). `c2-pc`
  now uses the phone's menus whenever the `--menu-art` folder has no frames, and
  the launcher plays such a game (on its own, no standalones) with the Android
  menus only, without changing the Menu choice other instances use. The
  "Experimental mobile layout" choice is gone from the launcher: without the
  APK's pictures (calls, binoculars, map, menu icons) it was not the phone game;
  "Android" is offered only when the base is built from an APK. `--mobile-menu`
  still works from the command line.
- **Night vision / flashlight.** Bought on the gear page (dynamic time), they
  could only be switched with N / L, so a pad or a touch screen could not use
  them. Now: the pad's right stick click (VR's recenter, unused in PC hunts)
  steps off → flashlight → night vision, skipping what was not bought; the
  Android hunt screen shows two round yellow buttons, "nv" and "light", that a
  touch (new: `WindowEvent::Touch`, since the hunt's captured pointer gives
  clicks no place) or a free pointer's click switches. Only one light is on at
  a time, as the owner asked: switching one on turns the other off and greys
  its button. A tap on a button does not also fire. The Wii U keeps its own
  stick clicks.
- **Animals page.** With more than 20 animals (modded rosters) the honeycomb
  used to add rows and shrink every picture. It now keeps the phone's size, 20
  a page (four rows of five), with back / on arrows beside the account and the
  page number under it (also Page Up / Page Down).
- **Scrolling.** The Steam Deck needed its back buttons mapped to the mouse
  wheel for the options list. The pad's right stick and D-pad up / down now
  scroll lists in every menu, and dragging the Android options list scrolls it.
- **Dawn at night** (same report) did not reproduce: the APK's areas carry the
  PC's three light maps in the PC's order (identical averages to the original
  `AREA1.MAP`/`AREA2.MAP`), and Dawn picked in the Android menu renders as dawn
  with dynamic time on (the owner's setting), from the command line and through
  the menu. Left open for the owner to retest.

### Multi ammo, advanced observer, in-game beam (2026-11-05)

Owner requests; none of these is in the original, and all are off by default.

- **Flashlight beam.** The "Narrow flashlight" Game option is gone; K switches
  wide / narrow in a dynamic-time hunt with the flashlight bought, and the
  last choice is kept in `c2rs.cfg` (`narrow_flashlight`, the key the option
  used). Consoles keep their one beam.
- **Multi ammo** (a setting on PC, 3DS and Wii U). With it on, picking the
  tranquilizer asks Half or Full. Full is the original's tranquilizer (the
  hunt option). Half is per round: each carried weapon's rounds (spare
  magazines counted) are split, the odd one lethal, and the hunt starts with
  lethal rounds loaded. The other kind waits aside (`Weapons::stash`); the
  swap control exchanges them whole (chambered rounds too, and a dialect
  weapon chambers its new reserve at once, so the swap is seamless). A shot
  using up the loaded kind loads the other. Each hit marks the animal with
  the round's kind (`Character::darted`, the last hit counts); a darted kill
  sleeps and scores as the tranquilizer's (+25%, no trophy, no ship). Extra
  ammo refills give both kinds half again. Survival drops the split. Controls:
  PC T; 3DS a Switch button under the map with "lethal" / "tranq" under its
  label; Wii U D-pad Left (the vsync probe toggle when multi ammo is not in
  use). HUD: PC two rows, "lethal:" and "tranq:", the loaded one bright; 3DS
  weapon cards `L7 T7`; Wii U the GamePad's ammo line (`+LETHAL:7 TRANQ:7`;
  its HUD texture shows two text rows, the second is the kills).
- **Advanced observer mode** (a setting; with Observer picked). Teleports by
  the map whatever the Teleport setting says, arriving at once with no
  cooldown. Flying moves where the camera looks, jump rises and crouch sinks,
  over the ground and under 8192 units above it, no collision. Time stop runs
  only the camera, map, binoculars, teleports and leaving: no animals,
  effects, clock or day cycle (`real_time` is held, which freezes the dynamic
  cycle), unlike Pause which also freezes the camera. Controls: PC F / T;
  3DS Fly and Stop buttons in the (empty) weapon column; Wii U D-pad Left /
  Y. HuntChoice (Wii U, shared with C) grows to 112 bytes.

### Launcher: pick parts, more menus, Instances layout, Steam Deck fit (2026-11-05)

Owner requests from the launcher backlog.

- **Pick parts** (per standalone, Add-ons tab). `c2_mods::standalone::Skip`
  lists a game's areas, huntables and weapons to leave out (numbers from 1
  in that game's menu order); `Options::skip` holds one per game. The rest
  closes up: area, slot and weapon numbers (and the renumbered menu, call
  and area files) are given in order to what is kept, so the combined menu
  has no holes. Left-out huntables' characters (matched by their menu
  name) are not added. In the kept characters and the spawn / pack tables,
  `areaN` / `charN` words naming something left out are removed from
  condition headers, and a condition left naming none of a kind it named
  is dropped (it could no longer hold); `callNo` / `fearCall` / `dontFear`
  entries pointing at a left-out slot go. Files of left-out parts that
  carry no number (models) are still copied under `MODS/<game>/`, unused.
  Passed as `c2-pc --skip N=area2,dino1,weapon3`, saved as the
  standalone's `skip` in a recipe (and by "Save as instance"). PC only: the
  3DS and Wii U take each standalone whole, as its own pack. Checked on
  Carnivores Triassic beside the Modder's Edition: leaving out one area,
  two huntables and one weapon gives 12 areas, 17 huntables, 13 weapons
  and two fewer characters.
- **Menu choice** (Play tab). New `MenuStyle::Game` with
  `Config::menu_game`: a standalone's own menu frames (`--menu-art` its
  game root); in a recipe `menu = "game:<standalone id>"`. With an APK base
  beside standalones, "Android" is offered (`--mobile-menu`) in place of
  Original / Mod: the APK folder has no PC frames and the combined game
  takes no standalone's `MENUM.TGA`, so those two drew the phone's menus
  anyway. An APK base on its own still plays with the phone's menus only.
  The Windows build uses the same arguments as Play.
- **Stop using** is on the Instances page (a line at the top naming the
  recipe in use, and on its row), so a test recipe is unloaded in one
  click; the Add-ons tab keeps the on/off box and Edit.
- **Steam Deck.** The owner's report: as a non-Steam game the launcher was
  zoomed in and the instances' Play buttons could not be reached, also
  with `WINIT_X11_SCALE_FACTOR=1.0` and the game resolution set. The
  instances grid was wider than the 900-point window (one row: name, path,
  base, status, then four action buttons), with no horizontal scroll. Now
  each instance is a card whose actions wrap below its name, Play first;
  the tabs row wraps too; scroll bars are solid, 12 points wide and always
  shown. A Settings "Interface size" (default Auto) shrinks the UI (down
  to 50%, never above the system's) until an 880x600-point page fits the
  window; fixed sizes 60-150% are offered. Under Steam's game mode or Big
  Picture (`SteamGamepadUI`, `SteamTenfoot`, `GAMESCOPE_WAYLAND_DISPLAY`,
  or `SteamDeck=1` with `SteamAppId`) the window opens full screen;
  `C2_LAUNCHER_FULLSCREEN=1` asks for that anywhere. Needs the owner's
  retest on the Deck.

### Launcher: Auto interface size jitter on the Steam Deck (2026-11-05)

Owner retest (fresh install, no launch option): in game mode the launcher
jittered between the zoomed-in menu and the fitted, full-screen one, so it
could not be used; Desktop Mode was fine. Cause: Auto turned the window's
size in points into pixels with this pass's `pixels_per_point`, but egui
measures that size before the pass at the old zoom and applies a new zoom
only at the start of the next pass. One pass after each change the two
disagree, so the fit came out as the square of the right zoom, then right
again, forever. Desktop Mode never shrinks (the 900x640 window fits), so it
never started. Now the size is converted with the zoom it was measured at
(the last pass's, kept in egui's memory). Needs the owner's retest.

Follow-up the same day: that build never showed its window, on the Deck
(Steam, both modes) or anywhere. It read `ctx.zoom_factor()` inside
`ctx.data_mut`, which holds egui's context lock, so the first frame
deadlocked. Auto now measures with egui's `viewport_rect` instead, which is
in the current pass's points (egui rescales it on the pass a new zoom takes
effect), times the current `pixels_per_point`: no stored state, no lock.
`inner_rect` also turned out to be missing under X11 here, so the old Auto
did nothing there. Checked on the desktop: windowed, frames draw and a
636x345 window settles at 0.575 in one step; full screen at 2560x1440 stays
at 1.0.

### Instance editor: balance, standalone parts, the base game's parts (2026-11-05)

Owner request: editing an instance should set its starting points and
each map's, animal's and weapon's price and score, and leave out maps,
animals and weapons of its standalone games, as the Balance and Add-ons
tabs do.

- The balance is the recipe's `[settings.balance]`, edited with the
  Balance tab's table, over values read from the instance's own game
  (its standalones combined with their parts left out, its add-ons over
  that). Entries whose folder the launcher hasn't been given yet are left
  out of that reading.
- A standalone entry's parts are its `skip` (as before).
- An instance whose **base** is a standalone game (Anachronous, JPHL...)
  had nothing to pick: `skip` only applied to games combined beside a
  base. New: `[recipe.skip]`, applied as a layer of rewritten files over
  the built game (`c2_mods::standalone::leave_out`), like a balance
  file. The left-out parts' blocks and `prices` lines go from `_RES.TXT`
  and `_MENU.TXT`, what is kept is renumbered (conditions, `callNo`,
  map, menu and call files) as the combiner does, and conditions naming
  only left-out parts go. Used by `c2-pc` recipe builds and `c2-cook`
  (so the 3DS and Wii U packs too). Ignored when standalones are combined
  beside the base: their numbering would no longer be the base's alone.
  Left-out parts' own higher-numbered files stay in the pack, unread.
  (Superseded 2026-11-06: "Leaving out the base game's parts", below.)

### A standalone that is on is not also an add-on (2026-11-05)

Owner's test: Genesis Redux as the base, JPHL beside it with a few
animals and weapons picked. With one menu choice the hunt showed JPHL's
menu screens and all of its animals; with another it was right. The menu
choice was not the cause: it only swaps the frame pictures, and the
command line gave the same, correct lists with "Mod", "Original" and
JPHL's own menu. The launcher's build folders showed the failing run had
built `combined-jphl-special-edition+jphl-special-edition`: the same JPHL
folder was also an enabled add-on, which merges all of JPHL (its menu
pictures too) over the combined game. A folder of add-ons holding it
(`addons-extracted/`) does the same once expanded. Now
`Config::enabled_addons` leaves out any add-on whose game is an enabled
standalone (compared by game root, resolved on disk), and the Add-ons
list marks such rows. Not changed: the owner's idea of blocking the
"Mod" menu for Modder's Edition bases (it shows the base's own screens
correctly); "Android" was already offered only on a base built from an
APK.

### An instance's game is its game folder; why BUILD is greyed out (2026-11-05)

Owner's report: the 3DS and Wii U builds stayed greyed out for some
instances (jphl, and others) with every option picked, and said nothing
("set the game folder first" on the Wii U page, with the Game tab set).
Cause: an instance's game was kept as given, the folder picked on its
recipe page or the search folder its base was found under (`source/`,
`addons-extracted/jphl-se`). Play and `recipe verify` search a few
folders down for the game and worked; the build pages looked for
`HUNTDAT` in that folder itself. Now the recipe page keeps the game
folder found in what is picked or found (`config::game_root`, as
`c2-pc` finds it), and on start the launcher corrects the instances it
remembers (once; stored). The BUILD buttons on the 3DS and Wii U pages
list what keeps them greyed out (no game, no SD card, nothing ticked).

Found while checking: the launcher still could not make JPHL Special
Edition's pack. `c2-cook` refused the whole pack when any area's base
(the area, the hunter and every ambient animal placed there) was over
the budget: JPHL's AREA4-6 are (22.3-25.7 MiB of 22 linear; AREA1-3 and
7 fit). It had been built by hand with `--force` (its recipe said so)
and plays on the owner's Old 3DS, because the console checks each hunt
as picked (the ambient animals it actually places, the picks) and
refuses one that doesn't fit ("Too much for this console") instead of
loading it. The cook's figure is the worst case. Now `c2-cook` writes
the pack and names the areas that may be refused; it fails only when
no area fits (`--force` still overrides that). The shrinking it does
was all there and runs as before (Old 3DS textures, thinned animations,
thinned trees and plants).

## 2026-11-05 — Second tagged release is v0.4.0

The owner chose v0.4.0 for the release after v0.1.0, skipping 0.2 and 0.3. Nothing
was published under those numbers. The changelog and notes say so, so the gap does
not look like missing history. Both workspaces and the shared path-package entries
in all three lockfiles move to 0.4.0 with no dependency upgrades, following
`docs/RELEASING.md`.

## 2026-11-05 — Launcher: instances first, and launch protection

Owner's report (PC and Steam Deck): an instance made for a standalone game
(Anachronous) played the Android game. The instance had switched off, the
base game on the Game tab was changed to the APK game, and the instance
kept no game of its own: `RecipeUse::game` was `None` ("the Game tab's"),
so it followed the new game. A base that didn't match the recipe's hashes
was only a warning for desktop bases, so it played anyway. The editor
showed the right base id from the recipe, the Game tab the wrong folder.

Fixes:

- The recipe page keeps the game it checked as the instance's own when the
  instance is used (`recipe_page::Page::game`), so later changes elsewhere
  never move it.
- Launch protection: Play and the builds go on only when the game matches
  the base the recipe names (its hash list), for desktop bases as well as
  APK ones. **Use this game anyway** lets a different version of the same
  game through on purpose (never for an APK base). This differs from
  `c2-pc recipe verify`, which still only warns.
- The editor checks the chosen folder against the base profiles
  (`instances::match_base`, off the UI thread) and says which game it is.

The owner also asked for the launcher to be reorganised around instances
(no global game and add-on setup to get out of step with them):

- Home: Instances (a box each: Play, Build, Options) and Settings only.
  Options (also on right click): Edit, Clone, Fix missing, Delete; Play in
  VR moved here from the removed Play tab.
- Build asks for the platform, then shows only its page with Cancel. The
  "Show the 3DS/Wii U section" settings and the first-launch question are
  gone (`show_3ds` and `show_wiiu` stay in the config file, unused, so old
  configs load).
- Edit: Base game, Add-ons and Balance pages over one instance, with Save,
  Save and exit, Discard changes and Cancel. The instance edited is the one
  in use while editing, so the menu and rule choices shown are its own.
- Fix missing checks the folders the instance was given, then searches the
  mod and add-on folders (the old "search folders" setting) for its base
  game and anything still missing.
- New instance and Clone ask for a name in the window (no file dialog,
  which is awkward in Steam's game mode). The old Game and Add-ons setup
  can still be turned into an instance once ("Instance from the old
  setup").
- Instances from before that kept no game open with none chosen in the
  editor, rather than the launcher's current game: that was the bug.
- Archives can't be dropped on the editor (pins need folders); unpack them
  first. The search for downloads still reads `.zip` files.

Follow-up, same day (owner's report: Play, Build and Options did nothing,
even after a rebuild; boxes cut off):

- Each box laid a click area over its own contents after drawing them, and
  egui gives a click to the widget added last, so the box took every click.
  The boxes are now cells of a grid sized to the window (as many columns as
  fit, all the same size); each cell's area is laid down first and its
  buttons over it. A headless test clicks across a box (Play, Build, right
  click) and fails with the old order.
- Settings no longer exports or imports engine options (the `options`
  module is removed; the 2026-10-23 "engine options file" entry above is
  superseded). Sharing is per instance: **Export…** in Options writes the
  recipe with its patches and its base game's hash list (`bases/`), and
  **Import recipe…** copies one in under a free name. Both copy only
  credits, links and hashes.
- Each box shows its base game, add-on count, a yellow count of what is
  missing (no folder, or the folder is gone; not hash-checked, Play does
  that) and the time played from the launcher (`playtime` in the launcher
  config, per recipe path; measured while `c2-pc` runs), or "Unplayed".

Second follow-up, same day (owner's report: every instance played as
JPHL; the editor showed the right game and Anachronous's animals):

- The instance's game was right. The **Original** menu style passed
  `--menu-art` the old Game tab's folder (`Config::base`), which the owner
  had last set to JPHL, so every instance showed JPHL's menu frames, hit
  maps and slider. Original now takes the retail game from an instance
  whose recipe base is `c2-vanilla` (the edited/in-use one first); with
  none, the game's own menus are shown.
- No other path lends an instance the old Game tab's game any more: an
  instance without a game of its own has none (`recipe_game`), Play
  refuses it, the 3DS/Wii U pages show it as a blocker, and the recipe
  page looks for its base in the candidates (standalones, the mod and
  add-on folders, built APK games) instead of checking the old game.
  `Config::base`, `overlays`, `addons` and `standalones` remain only for
  "Instance from the old setup".


## 2026-11-05 — Third tagged release is v0.4.3

The owner chose v0.4.3 for the launcher release after v0.4.0, skipping 0.4.1 and
0.4.2; nothing was published under those numbers, and the changelog and notes say so.
Both workspaces and our own packages in all three lockfiles move to 0.4.3 (outside
crates that happen to be at 0.4.0 are untouched), with no dependency upgrades,
following `docs/RELEASING.md`.


## 2026-11-06 — PC VR: guns in the hands, vanilla only

The owner's design for VR hunts (full list in `pc-ver/crates/c2-vr/GAMEPLAN.md`,
"Hands-on hunting"): guns float in the tracked hand with no arms, the right
grip held equips, the right trigger fires, the off-hand trigger opens a map
held in that hand, long guns need the off hand on the barrel to fire, and the
sights snap to the eye. No recoil, sway or reloading in VR for now; a
head-aimed gun when there are no controllers.

- **Arms are skipped, not edited.** Each vanilla weapon `.CAR` keeps its
  hands and sleeves as separate mesh pieces with their own bones, so VR
  leaves those pieces out at load and uses the old hand's position as the
  grip point. The game's files are untouched.
- **Vanilla guns only.** Two-handed support, sight lines and grip points need
  per-gun knowledge that modded models don't carry and can't be guessed
  safely. Modded guns stay out of VR hunts until a per-gun VR config can be
  dropped in beside a mod.
- **The flat game is unchanged.** The VR shot comes from the muzzle through an
  optional aim given to `c2-core`; with none, the eye ray and the original's
  recoil apply as before.
- **Which button is the support hand's:** the off hand's trigger is the map,
  so supporting a long gun uses the off hand's grip (the controller's other
  trigger).

## 2026-11-06 — Android app: the PC frontend, APK bases only, 8 picks

M19's first build (`android-ver/`). The owner's rules: the Android version
needs a game built from the player's own mobile APK (Dinosaur Hunter or Ice
Age) as its base, so it can use that game's touch menus and buttons; modded
means that base plus add-ons (no standalones); at most 8 animals and 8
weapons per hunt, because more is too hectic on a phone for now.

- **Same frontend, not a new port.** `c2-pc` is now a library plus its
  binary (`cli.rs` holds the command line), and `android-ver/c2-android` is a
  `cdylib` with `android_main` that runs `c2-pc play`. wgpu, winit and kira
  (AAudio) all target Android, so nothing is duplicated. It is a separate
  workspace like `3ds-ver/`, so PC builds never compile it.
- **No game data in the APK** (§2). The app reads `game/` and `addons/` from
  its folder on the phone; `game/` must carry `from-apk.txt`, or the app
  stops with a message in `c2rs.log`. Saves go under that folder too
  (`XDG_DATA_HOME` is set there, since Android has no home).
- **The cap is a general flag**, `--max-picks N` on `c2-pc play`; Android
  passes 8. Unset on PC, so PC behaviour is unchanged. It counts animals and
  weapons separately and refuses only new picks.
- **Android lifecycle:** the window surface is dropped when the app is paused
  and remade from the kept adapter on resume (`Gpu::window_target`), in the
  colour format the pipelines were built for. If the GPU refuses wgpu's
  default limits, the device is requested with the adapter's own.
- **Touch, for now:** the first finger acts as the mouse. That works for the
  menus, but the hunt still needs the mobile game's own controls (stick,
  fire, weapon buttons). Back maps to Escape.
- **One run per process** (found on the first device run, 2026-11-06): Android
  can start the activity again in a process that already ran it; winit allows
  one event loop per process and `tracing` one global logger. The app logs
  once per process and exits the process when the game ends. Launch problems
  (no `game/`, not APK-built) are shown on screen (`c2_pc::notice`), and the
  soft keyboard opens while a scene takes text (`Scene::wants_text`: the
  hunter registration).
- **Android's own event loop** (2026-11-06, after the owner's first hunts on
  a Retroid Pocket 5): winit's Android layer gives pad buttons as unnamed
  keys and turns analog sticks into touches (the left stick only), and
  `gilrs` has no Android backend. The app now runs its loop on
  `android-activity` itself (`c2-pc/src/app_android.rs`; the PC keeps winit
  in `app_desktop.rs`) and feeds pads into `c2-pad`, which has its own
  positional buttons now (gilrs only off Android). Android's A is the bottom
  button.
- **The phone's hunt buttons, only while touching.** An APK game's hunt shows
  the mobile game's own buttons (its `GUI` sprites) while the last input was
  a touch, and hides them when a pad or a key is used, as controller-friendly
  phone games do. The PC never shows them unless a touchscreen is used. The
  buttons act as a virtual pad (`Scene::before_update`), so the hunt reads
  them as pad actions and taps no longer fire. The layout follows the owner's
  screenshots of the mobile game: compass (map) top left, binoculars under it
  replaced by fire while a weapon is in hand, stick bottom left, call top
  right and the weapon in use under it; a tap calls or draws / holsters,
  holding opens the hexagon picker (slide, let go; off the hexagons cancels).
  The "menu" tab opens the hunt menu, whose hexagons take taps; the phone's
  in-hunt Options are left out (options stay in the main menu, owner). Ours:
  the stick pushed all the way runs; no jump or crouch buttons yet.
- **The light buttons are touch buttons too** (owner, 2026-11-06): the nv /
  light discs show only while touching, like the rest, placed under the
  weapon button. With a pad or the keys they hide (N, L and the pad's right
  stick click switch the lights). Before, they showed whatever the input
  and jumped to a new place when a touch began, so a tap there hit the call.
- **Packaging without Gradle:** `build-apk.sh` uses only the NDK and SDK
  build-tools (aapt2, zipalign, apksigner), with native libraries stored
  uncompressed and 16 KiB aligned.

### Leaving out the base game's parts, beside add-ons and standalones (2026-11-06)

Request: choose which of the base game's maps, animals and weapons an
instance keeps, so modded content can supplement or replace them; in the
recipe and on the instance editor's Base game page.

- `[recipe.skip]` (above, 2026-11-05) now applies whatever is combined:
  the `leave_out` layer goes over the finished game in `c2-pc recipe
  build` (so Play), after the standalones are combined and the add-ons
  merged. The base's parts come first in a combined game (`standalone::
  build` appends each game's areas, slots and weapons after the base's),
  so the base's numbers 1..N are the same there as in the base alone, and
  everything after moves up by as many as were left out. Checked on
  Carnivores 2 + the Modder's Edition, Carnivores Triassic beside it and
  one add-on: leaving out area 1, the second animal and the first weapon
  gives 12 of 13 areas, 18 of 19 huntables, 13 of 14 weapons; Triassic's
  maps and the add-on's `area`/`char` conditions moved down by one.
- Left-out animals are matched by name. The combiner renames a
  standalone's or add-on's animal that shares a base animal's name
  (`Name (Game)`, `Name (add-on)`), so only the base's own goes.
- The picker reads the base as it is built: the game with its overlays
  (`standalone::base_parts`, over `vanilla(bases)`), so a Modder's Edition
  base lists the Modder's Edition's roster, not the stock `_RES.TXT`.
  Before, it read only the first folder.
- Leaving out every area of the whole game is refused at build time (the
  hunt menu would have nothing to pick). Leaving out all of the base's
  animals or weapons is allowed, with a note in the editor, since add-ons
  or standalones may bring their own.
- Consoles: `c2-cook --recipe` already applied `[recipe.skip]` to the
  base + add-ons pack. A recipe with standalones is built with `c2-pc
  recipe build` first, so its output carries the leave-out too.
- Not done: putting a modded animal into the left-out one's slot (it is
  added after the kept ones). Menu order is the only visible difference.

### Experimental Advanced tab in the instance editor (2026-11-06)

A place for instance options still being tried out, kept out of the
regular pages. It is a fourth editor tab after Balance, shown only when
`Config::show_advanced` is on (Settings > Experimental; off by default,
saved in the launcher's config, not in recipes). Opening a hidden tab
falls back to Base game. It holds no options yet.

### Hardcore survival (2026-11-06)

Request: a hidden survival mode, like Modded but locked behind an in-game
setting ("Enable hardcore survival mode", off by default since it is not
in the original), played at endless dynamic-cycle night with the radar
and the flashlight.

- `SurvivalKind::Hardcore`: Modded's roster and budget (`roster` already
  takes everything but Classic's stock filter). `SurvivalKind::ALL` stays
  Classic and Modded, the always-offered pair, so the Wii U's index
  mapping and `c2-cook`'s pack checks are unchanged.
- The darkness is the dynamic cycle's night (ambient light at 12%, stars,
  the night fade colours) whatever the "Dynamic time" setting says:
  `DayCycle::endless_night()` starts at the night stop and is held, so
  time played never moves it. Restart keeps it held and the flashlight on.
- The kit: the flashlight is carried and starts on (L toggles it, K the
  beam, as in any dynamic-time hunt); the radar is on whatever the save's
  gear; the shotgun, as in every survival. The rifle was tried first and
  dropped the same day: its power (4) is over the wave health cap (3), so
  every hit is a kill, which made the mode too easy. Unlimited ammunition
  as in every survival.
- PC only for now (`PcSettings::hardcore_survival`, `c2rs.cfg`). The 3DS
  and Wii U menus don't offer it; the 3DS menu's description match takes
  it with Modded so it compiles.
- HUD: the flashlight's "L: Flashlight On/Off" label (an early testing
  aid) is gone; the beam shows whether it is on. The survival's wave box
  sits under the weapon's rounds (top left) instead of over them, at the
  lowest the rounds have reached since that weapon was drawn, so it
  doesn't move up as shells are fired (`hunt_view/survival_hud.rs`).
- Checked with `c2-pc hunt AREA5.MAP --survival hardcore --sim` screenshots
  on the stock game: night held, flashlight cone lit, wave 1 bought; an
  idle hunter was killed by a Velociraptor within 30 s.

## 2026-11-06 — "Map puts the gun away" (setting, on by default on PC)

The original opens the map only with the weapon put away (`MapMode` is
refused while a weapon is out), so a hunter with a gun drawn has to put it
away first. The owner asked for the map button to do that itself, on PC as
on the phone: with **Map puts the gun away** on (`map_holsters` in
`c2rs.cfg`; `HuntOptions::map_holsters` in `c2-core`), the map pressed with a
weapon out puts it away and the map opens once it is. Pressing the map again
while it waits, or taking a weapon out, cancels it. Map to gun needed no
change: taking a weapon out already closes the map. Off in `c2-core` by
default, so the 3DS, the Wii U and the parity harness keep the original's
rule until they offer the setting.

## 2026-11-06 — PC "Foliage distance" slider replaces "Far sprites"

The original switches an object from its model to its far sprite at
`ctViewRM` cells (`Game.cpp`: 24). A debug key in the original moves it
from 4 to 60 cells (`Hunt2.cpp: ChangeViewR`). The PC had only an on/off
switch: sprites at 24 cells, or models all the way. Video options now have a
**Foliage distance** slider (`model_distance` in `c2rs.cfg`, 0..=255). Every
map object counts as foliage: trees, bushes and rocks are all RSC objects, and
the game has no separate class for plants.
- 0 is the original's 24 cells (the default).
- In between, the slider runs linearly from 24 cells to the hunt's render
  distance (`PcSettings::model_cells`).
- 255 means models all the way, no sprites.

An old `far_sprites = 0` reads as 255, so a saved "off" keeps its look. The
slider has no lower setting than the original's; the PC has no need to cut
below it. The 3DS's separate `object_view` cull (objects past it as sprites
or not drawn) stays 3DS-only; the PC never used it.

## 2026-11-06 — PC "High-end view" (off by default; experimental on Android)

A Video option for strong hardware, `high_end_view` in `c2rs.cfg`. Off, the
hunt draws as before (screenshots of the default and the old maximum match
the previous build's, apart from the animated clouds). On:
- **Render distance** reaches 1024 cells (`HIGH_END_RENDER_CELLS`), the
  whole map from anywhere on it, instead of 384.
- **The ground in detail levels** (`render/terrain.rs: window`,
  `shaders/terrain.wgsl: lod_cell`). Within 128 cells (`LOD_FINE_CELLS`)
  every cell is drawn, as the original does. Each level out is a square
  ring twice as wide, made of cells twice as large: at 1024 cells, 4 levels,
  about 1.6 M vertices against 3.5 M for the old 384-cell window at full
  detail. The levels are centred on the coarsest grid (8 cells), so the
  full-detail square moves in 8-cell steps. Where two levels meet, the finer
  level's in-between edge vertices take the mean of their neighbours along
  the edge, so there are no gaps. A larger cell stretches its top-left
  cell's texture over itself. Water uses the same levels.
- **Objects fade from model to sprite** over 4 cells (`FADE_CELLS`) instead
  of switching outright, with a 4x4 ordered dither (`Instance::dither`), so
  no sorting is needed. The original's own crossfade is commented out in
  `Render3DFX.cpp: _RenderObject` (its `GlassL` band at `ctViewRM`), so this
  is not original behaviour and stays behind the option.
- **The animals keep the 384-cell rules.** `ReplaceCharacterForward` places
  an animal past `render_r` and `small.rs`/`huntable.rs` move it there once it
  is 20 cells farther. With `render_r` at 1024, every spot would be off the
  map, so animals would never be moved ahead of the hunter. The hunt's
  `render_r` is capped at `MAX_RENDER_CELLS`; only the drawing goes
  farther.
- The depth buffer is 32-bit float with standard Z and a near plane of
  16: at 1024 cells the depth step at the far edge is about two cells, so
  far shorelines may flicker. Reverse Z would fix this if it shows.
- Android shows it as "High-end view (experimental)": the same code, but
  phones vary far more than PCs.

Viewer: `c2-pc area MAP --view 1024 --high-end --foliage CELLS`.


## 2026-11-06 — Observer mode places the picked animals and keeps the radar

Observer mode used to drop the picked animals and the accessories at launch
(PC `menu.rs: launch_hunt` and `save_equipment`, 3DS `menu.rs: setup` and
`store_equipment`, `hunt.rs: load`, VR `menus.rs`). The hunt then placed the
original's random mix of the first huntables, the radar was off, and nothing
the player picked was placed or shown on the map. The menus still let the
player pick animals and accessories for free in observer mode, so the picks
looked like they were being used.

The original game takes `din=` (`TargetDino`) and `-observ` separately
(`Game.cpp`), so an observer's hunt with picked animals places those animals
and colours them on the radar like any other hunt. Observer mode now keeps the
picked animals as targets and saves the accessories as picked; it still carries
no weapons. With no animals picked, the hunt still places the random mix. The
Wii U setup already worked this way. The 3DS summary lists the animals under
"Observer: just watch". This replaces the observer line in "PC VR: the hunt
setup on the panel" (2026-10-29).


## 2026-11-06 — Predation: carnivores hunt herbivores (an opt-in setting)

Not in the original game, where every animal reacts only to the hunter. An
advanced instance setting, off unless a recipe's `[settings.predation]`
turns it on (`docs/MODDING.md`). The launcher writes it into the built game
as `HUNTDAT/_PREDATION.TXT`, so PC, Android, VR, 3DS and Wii U read the same
file; survival and the trophy room leave it off. PC is the tested platform;
the launcher labels the others experimental.

How it works (`c2-core/src/dino/predation*`):
- **No new AI per family.** Each step, an animal with a goal is shown a
  different "hunter" (as survival's `lure` does): its prey, a point far
  beyond what it flees, or, for AIs that run from the hunter, the target
  mirrored through the animal so it runs toward it. The original's and the
  Modder's Edition's AIs then chase, flee and kill as they do the hunter.
  A `KillHunter` from a steered animal is taken back and becomes the kill of
  its target, which plays its own death; the predator then eats for 12 s
  and rests 30 s. (Replaced by an explicit target: "Predation phase 2:
  explicit AI targets" above.)
- **Kill clips are for the hunter only.** The step that "kills the hunter"
  switches the AI to its hunter-kill clip; for a kill of another animal the
  pose from before that step is put back and the clip picked separately:
  - Carnivores eat with the ground-feeding clip the Velociraptor and
    Allosaurus kill the hunter with: the original AIs' `eat` clip, or a
    dialect kill type with `hunterAnim = 0` (HUNT_EAT) that is not
    `hunterSync` or `carryCorpse`. Kills staged around the hunter's body
    (the T-Rex lift; add-ons such as Suchomimus, Sigilmassosaurus,
    Carcharodontosaurus, Manospondylus) are skipped; a species with only
    those sniffs at the body (T-Rex `SMEL`, the dialect's first
    `smellAnim`).
  - Herbivores gore and run on, as the original's Chasmosaurus does with
    the hunter: no kill clip and no eating, even where a dialect herbivore
    has a kill type.
- **Chases stay in range.** Raptor-family AIs give up on a hunter past
  `ctViewR * 128`, and animals sense each other farther than that, so a
  chasing AI is shown a point at most 10 cells along the way.
- **Triceratops** (the original's defensive AI charges within
  `256 * 16 + OptAgres / 8` and runs beyond) is steered by distance: shown
  the target when near, the mirrored point when far.
- **Senses** follow `CheckAfraid` without its look traces (too costly for
  every pair, especially on the 3DS); the distance is doubled instead, for
  the cover they would find. A running animal can also be heard. Goals are
  re-picked every 300 ms. Measured on stock Area 1 (25 animals): 0.04 ms per
  step on PC.
- **Hunter first** (the default): a carnivore that has noticed the hunter
  drops its prey. `CheckAfraid` now also checks a stock carnivore chasing
  prey (it skipped any animal already alarmed), only marking it aware; and
  `MakeNoise` marks stock animals aware too (read by nothing else).
- **Roles** come from the AI unless tagged: the original's raptor, Allosaurus,
  Spinosaurus, Ceratosaurus and T-Rex AIs and the dialect's carnivore AIs are
  carnivores, other huntables herbivores; flyers, fish, ambients and
  brachiosaurs take no part.
- **No trophy**: kills are recorded only from the hunter's shots, so a body
  left by a predator scores nothing. The face-off between carnivores within
  10% of each other has the cheaper one run off; no roar clip is played yet,
  since the original's carnivores other than T-Rex have none.


## 2026-11-06 — Predation review fixes (phase 1) and observer accessories

Four isolated fixes from the predation code review, each with a regression
test that fails without it (`dino/predation/tests/regressions.rs`,
`game/tests/session.rs`):

- **Feeding ends when alarmed.** A kill leaves the predator calm (state 0,
  unaware), so `predation::step::feeding` now stops the meal when the state
  is set or the hunter is noticed: a shot (`Hunt::damage`), a heard shot
  (`make_noise`) or `CheckAfraid`. Before, the 12 s meal skipped the AI
  entirely and a feeding animal was a free kill.
- **Equal prices never fight.** Carnivore fights and `Rules::outranks` used
  `cb <= ca * 0.9`, true for 0 vs 0, so unpriced carnivores (roaming
  add-ons, ambient carnivores) hunted each other and each outranked the
  other. The comparison now also requires `cb < ca`, matching the doc and
  the launcher tooltip.
- **Prey stops fleeing a predator that has lost interest.** Prey kept
  `Goal::Flee` while the predator lived within 8192 units, and steering
  renewed its fear each step. How close a predator must be to scare
  (`menace` in `predation/run.rs`) now depends on what it is doing: any
  distance when after this animal, 4096 when ready to hunt, 1024 when
  eating, resting or chasing another animal; fleeing stops at twice that
  (at most 8192). A herd can graze near a predator that is eating.
- **Observer hunts drop the saved accessories.** Since observer mode keeps
  the saved accessories (entry above: the radar), the hunt applied camo,
  scent and the tranquilizer to an observer too. `HuntOptions::for_hunt`,
  called by `Hunt::new`, clears those three (and half tranq) in observer
  mode on every platform; the radar stays and the save keeps the loadout.

`c2-parity predation`, 600 s, same seed: Area 1 10 -> 11 kills, Area 3
4 -> 4; step cost unchanged (0.035-0.042 ms average).


## 2026-11-06 — Predation phase 2: explicit AI targets

Predation used to steer an AI by overwriting `ctx.hunter` with the prey,
then undoing part of it afterwards: removing the `KillHunter` event,
restoring the pose, and swapping `aware_hunter`/`heard_shot` back. The rest
leaked. Pack alarm and attack flags raised by a member chasing prey made its
packmates charge the real hunter. Sounds were filtered by distance to the
prey rather than to the player. A pack member fleeing a predator alarmed
the others, who then fled from the hunter's position.

Now (`c2-core/src/dino/mod.rs: Target`, `herd.rs: target_of`):
- `Ctx::hunter` is always the player: sounds and re-placement use it.
  `Ctx::target` is what the AI chases or flees, of kind `Hunter`,
  `Waypoint` (survival's `lure`), `Animal(n)` (a predation hunt or charge)
  or `Place` (a body to scavenge, or a point beyond what it flees). It
  carries what the animal knows of it (`aware`, `heard_shot`); for the
  player these are `aware_hunter`/`heard_shot`, so the AIs read them from
  the target.
- Every AI's kill goes through `Ctx::strike`: a `KillHunter` event for the
  player, `Ctx::struck` for an animal target, nothing for a waypoint or a
  place (replacing the `lure.is_none()` checks). The hunter-kill clip (and
  its slowdown) is played only on the player, so no pose restore is needed.
  The herd hands `struck` to `Predators::finish`, which counts it only on
  the animal the AI was steered at.
- Pack flags are written only when the target is the player or a waypoint
  (`Target::alarms_pack`); otherwise the AI gets a copy of the pack, so it
  still reads the pack's state but its writes are dropped. A pack member
  fleeing a predator passes its `Flee` goal to packmates with no goal of
  their own (`Predators::flee_as_packs`), so the pack runs from the
  predator.
- Still in place: `CheckAfraid`'s case for stock carnivores chasing prey.
  Steering still alarms an animal (`state`, `afraid_time`), which would
  otherwise stop `CheckAfraid` from looking at it for the player.

Tests (`dino/predation/tests/targets.rs`), stock and dialect AIs: a far
chase makes no sound for the player, a dialect kill strikes the prey with no
hunter kill or kill clip, a hunting pack member raises no pack flag, and a
dialect herd flees the predator, not the hunter. Each one fails with its
part of the change reverted. `c2-parity predation`, 600 s: Area 1 10 kills,
Area 3 5 (phase 1: 11 and 4); no hunter kills; step cost unchanged.


## 2026-11-06 — Standalones beside the mobile base on Android

Request: try a standalone modded game in the Android app, with the game
built from the Dinosaur Hunter APK as its base. Until now Android took the
APK base plus add-ons only.

- **The combiner takes an APK-built base.** `standalone::build` needed a
  `_MENU.TXT` (the Modder's Edition's), which an APK-built base has not.
  Such a base lists its areas, prices and roster in `_RES.TXT`, which the
  game already reads as its menu when there is no `_MENU.TXT`
  (`menu_txt`). The combiner now does the same: with no `_MENU.TXT` and
  areas in `_RES.TXT`, that serves as the base menu and the combined menu is
  written as a new `_MENU.TXT`. A base with neither still asks for the
  Modder's Edition.
- **Combined on the PC, not on the phone.** `play --standalone` rebuilds
  the combined game at each start; that is fine on PC but not for a
  500 MB game on a phone. So the app keeps one `game/` folder, and
  `c2-pc combine` makes it. The combined game keeps the base's
  `from-apk.txt` and `mobile-survival.txt` (the app requires the first;
  the 3DS cook takes the second), so no change to the app's launch rules.
- **Checked** with Carnivores Anachronous beside Dinosaur Hunter: 12 areas
  (the base's 7, then Anachronous's 5), 28 huntables, 15 weapons. On PC
  with the app's arguments (`--mobile-menu --max-picks 8`) the three hunt
  setup pages list all of them (animals paged, 1 / 2), and a simulated
  hunt on area 8 renders with the phone's buttons. On the Retroid Pocket 5
  the app starts on the combined game and lists the Anachronous animals.
- **Known gap:** the standalone's areas, animals and weapons show as names
  in the hexagons; the phone menu draws only pictures of the mobile game's
  kind. Long names are cut off.

## 2026-11-06 — Android rendering performance (GPU-bound on a Retroid Pocket 5)

Hunts on the Retroid Pocket 5 (Adreno 650, 1920x1080) stayed playable only
with the view sliders at their lowest. Measured on the device with the new
frame log (`c2rs.log`, every 5 s), the limit was the GPU: the game thread
is about a quarter busy. Changes, with their effect on the Retroid:

- **Frustum culling** (`render/frustum.rs`), all platforms. The ground and
  water are drawn as 16x16-cell chunks, instanced, and only the chunks the
  camera can see are sent; water only where a chunk has water. Object
  chunks and animals outside the view are skipped too (animals are not
  posed either: the pose only feeds the GPU). The picture is unchanged
  (PC screenshots byte-identical before and after). Area view at 200
  cells: 20 -> 45 fps.
- **Ground levels past 128 cells always on Android**, independent of
  High-end view (`area_view.rs`, `GROUND_LEVELS_ALWAYS`). Full detail out
  there cost the most and shows little; within 128 cells, which includes
  the original's whole view range (42-104), the ground is as before. The
  PC keeps this behind High-end view. Area view at 300 cells: 43 -> 60 fps.
- **Mip levels for the ground's and models' textures, Android only**
  (`mips.rs`, `gpu::WORLD_MIPS`). The original builds them too
  (`MIPMAP = TRUE`; `CreateMipMapMT` halves the textures and the D3D
  renderer switches to the half-size one past 10 cells for the ground and
  12 for models). The levels use the original's rule: a block's coverage
  is its top-left texel's, its colour the average with clear texels
  counted as the top-left one, so far foliage stays as dense. The GPU
  picks a level per pixel (nearest level) rather than per object by
  distance. Not on the PC yet, to keep its look unchanged; offering it
  there would follow the original. Hunt at the owner's settings: about
  47 -> 53 fps.
- **Object chunks drawn nearest first**, all platforms. Sprites and leaves
  discard, so the GPU can't reject hidden pixels before shading them
  unless something nearer already wrote depth. Only the draw order
  changes; on PC 7 pixels of 921,600 differed across 7 views (equal-depth
  overlaps). Hunt at render distance 255, sprites from 24 cells: 42 -> 60
  fps on the run measured.
- **Less CPU per frame.** Dynamic time lit every object each frame with
  map lookups; the light is now split into the parts that never change
  (`ObjectLight`, stored per object at load) and one blend per frame, the
  same formula and results. Instances go straight into the upload buffer
  (one copy instead of two), and the per-model lists are reused between
  frames. On this driver the frame waits in `present` for the GPU, so CPU
  and GPU time add up and CPU savings count.
- **Resolution option, Android only** (Video options, 50-100% per side,
  `render_scale` in `c2rs.cfg`). The swapchain is made smaller and the
  phone scales it up; touches are scaled to match. Not in the original.
  It helps less than expected (a hunt gains 5-8 fps at 50%): pixel fill is
  not the main cost.

Overall: a hunt at the owner's settings went from about 47 to 54-58 fps,
at the highest render distance from about 40 to 45-55 depending on the
landing spot; the map viewer holds 60 fps at every render distance on the
five stock areas.

Tried and not kept: Mailbox present mode (slightly slower) and a swapchain
latency of 1 or 3 (no repeatable change).

Still open: the ground's vertex work is the remaining cost (on a heavy
landing, skipping the ground alone reaches 60 fps). Each cell computes six
unshared vertices; options are fewer full-detail cells on Android or
sharing vertices, which needs the per-cell texture and rotation moved to
the fragment shader.

Measuring: `files/args.txt` in the app's folder (one argument per line)
replaces the launch arguments, e.g. `area`, a map path, `--view`, `384`;
`c2-pc hunt --seed N` gives the same landing and animals each run (random
hunts vary by about 15%). The APK is `profileable` from the shell, so
`simpleperf record --app io.github.c2rs` samples it.

## 2026-11-06 — Android: one app per named instance

Android keys an app by its package name, and the app's files folder
(`Android/data/<package>/files/`) follows it, so a second instance used to
replace the first. The launcher's Android tab now takes a name per instance
(`InstanceBuilds::android_name`). Empty keeps the shared `io.github.c2rs`;
a name gives `io.github.c2rs.<segment>`, the segment being the name
lower-cased, letters and digits kept, every other run turned into one `_`,
leading digits dropped (a package segment starts with a letter).

How the APK is renamed: the manifest (`android-ver/AndroidManifest.xml`,
included in the launcher) has its `package` and `android:label` replaced,
`aapt2 link` builds a fresh APK from it, the engine library is copied in
from `c2rs.apk` as a raw zip entry (still stored, so Android maps it from
the APK), then `zipalign -P 16 4` and `apksigner` with the debug key, the
same steps as `build-apk.sh`. Patching the binary manifest inside the APK
was the alternative; it still needs re-signing, so the SDK is required
either way, and re-linking from the source manifest is simpler to keep
right. The native code takes its files folder from the activity, so
nothing in the engine knows the package.

The same debug key signs every variant, so updates install over each
other. Renaming an instance later makes another app; the old one stays
until uninstalled.


## 2026-11-07 — Predation phase 3: one scan per animal

`Predators::think` (every 300 ms) walked the herd up to five times per
animal: the first animal charging it, then `nearest()` for predator, rival,
prey and body, each pair with a square root in `senses()`. Now
`predation/scan.rs` (`Scan::new`) makes one pass per animal. It notes the
first charger and checks each pair's relations once. Only the kinds the
animal's goal can turn to are considered (`Wants`: charging only for
defensive herbivores, prey only for carnivores not busy with the player,
bodies only for scavengers). The nearest per kind is chosen by squared
distance, and an animal clearly beyond the senses' reach (`kr > 3`, with a
1% margin on the squared distance) is dropped before the square root.
`senses()` itself is unchanged and still decides at the boundary. Ties keep
the earlier animal, as `min_by` did, so goals are bit-identical.
`c2-parity predation` kill logs and end-state digests match the previous
build on Areas 1–5, both priorities, at the stock population and at 20, 40
and 60 animals.

The cost is measured by `predation/bench.rs` (`fill`, `gather`, `measure`),
shared by `c2-parity predation --count`, `android-ver/bench-predation.sh`
and the 3DS (SELECT then X in a hunt, or the `animals` / `prof` / `bench`
script commands, written to `bench.txt`). On a desktop PC, a 60-animal
`think()` went from 27 µs mean / 48 µs worst to 15 / 26, about 10% of a herd
step. Console figures are still to be measured; see
`docs/PREDATION-PERF.md`.

Still O(n²): a spatial grid would cut it further, but at 60 animals the
pass is a small part of the step, so it waits for console numbers that
call for it.


## 2026-11-07 — Instance rules: `_RULES.TXT`

The Advanced tab's times of day, locked settings and spawn rules are one
recipe section, `[settings.rules]` (`c2_mods::rules`), written into the
built game as `HUNTDAT/_RULES.TXT` (`c2_formats::rules`), as predation is.
Every platform reads the same file, so a rule is never implemented per
backend twice.

- Areas are keyed by map name (`area3`, a `.c2map`'s project) in the file
  and by area title in the recipe. The build resolves titles against the
  built game (`balance::area_maps`), after the base game's parts are left
  out and add-ons are numbered, so renumbering never points a rule at the
  wrong area. Animals are matched by name; `X #2` is the second one so
  named, as the balance editor lists them.
- Spawn rules run once placement is done (`Hunt::apply_spawn_rules`). The
  picked animals are capped per area (`cap_population`, whole packs kept).
  Then each secret animal is placed as the original's main population is
  (`PlaceCharacters`: anywhere within ±500 cells of the centre, at least 40
  from the hunter, `check_place_collision_p`). On PC this happens after the
  "secret dinos" cap, so a rule's animals always come.
- `hide NAME` was added beyond the request's list. Leaving a huntable out
  of the base game removes its character, so it could no longer spawn as a
  secret. The "herbivores to pick, carnivores to track" instance needs
  animals that are in the game but not in the menu.
- Locks are applied when settings load and undone when they are stored.
  `c2rs.cfg` (PC) and the 3DS settings file are shared by every game, so a
  locked value must not replace the player's own. The Wii U's settings
  file belongs to its app (one pack), so its locks are applied to the
  choice directly.
- The settings rows a lock hides are matched by label (PC), button (3DS)
  or row id (Wii U). Only the eight listed settings can be locked.
- The base game's parts picker moved from the Base game page to Advanced,
  as asked: the Advanced tab gathers what a modpack maker configures, and
  stays hidden by default. Existing `[recipe.skip]` lists still apply with
  the tab hidden.
- Ammo is the weapon's `shots` (`WeapInfo.Shots`, `Resources.cpp`), already
  kept by the balance file. The Balance tab now shows it in the weapons'
  third column, which was empty.


## 2026-11-07 — Gameplay regression scenarios (`c2-regress`)

- The script lives in `c2_core::scenario` (no I/O, `portable-hunt`), so the
  3DS and Wii U apps can later run the same stages. The CLI, recordings and
  reports are a new crate, `c2-regress`, rather than more subcommands in
  `c2-parity`, whose `main.rs` is near the file-size limit.
- Rendering reuses `HuntView` through `c2-pc regress` (a child module,
  `hunt_view/regress.rs`) instead of a second renderer. It swaps in the
  scenario's hunt, built from the same species and weapons, and drives
  `HuntView::step` one tick per frame. Headless and rendered checkpoint
  hashes match.
- Test mode pins positions between ticks (`Hold`) instead of adding a
  gameplay option. Normal play is unchanged, and AI, animation and sound
  logic still run. Shots go straight ahead with every animal at least 4°
  off that line (checked at set-up), so nothing is hurt.
- Carnivores 1 is reported unsupported: it is a non-goal (§1) with no
  loader. Carnivores 2 has no Triceratops, so the scenario uses
  Chasmosaurus with T-Rex, Allosaurus and Stegosaurus.
- The original has no change-gun or change-call screen. The original's
  call switch shows an icon for 2048 ms, and that icon is the core's
  "call icon" stage. The pickers are PC additions and are checked in the
  render pass only; headless reports them unsupported.
- The PDF writer is ours (~200 lines): images go in as their PNG zlib
  stream with `/Predictor 15`, so it needs no PDF or compression crate.
  `serde_json` was added (with `itoa` and `zmij`): five JSON outputs and
  reading them back for comparisons.
- No video recording: it would need an encoder dependency.
- Fixed while verifying: a test in `render/frustum.rs` used glam's
  deprecated `Mat4::look_to_rh`, which failed `clippy -D warnings`; it now
  calls `glam::camera::rh::view::look_to_mat4`.
- Reference baselines are published from the vanilla games: this engine on
  the retail Carnivores 2 and Ice Age data (the original executables cannot
  be scripted). Only scores go in the repository (manifest, metrics,
  events). Screenshots are renders of the game's art and stay local, as do
  logs and machine names. `publish` checks every file the run read against
  `recipes/bases/*.toml`.
- Performance in a published baseline is reference only: contributors'
  machines differ, so it never warns. State hashes are compared but can
  differ across OS or CPU architecture from floating-point maths alone.
- `recipes/bases/ice-age.toml`: the `_RES.TXT` hash was updated to the
  installed file (dated 2000-10-19), confirmed as the vanilla one; the other
  242 files already matched.
- Original-engine baselines go through the shipped executables under Wine
  with an injected hook (`c2-regress/original/`), not a rebuild of the
  released source: Ice Age's source was never released, and the shipped
  C2 is a later build than that source. The hook is C (a 32-bit Windows
  DLL and launcher, built with clang/lld against Wine's headers), because
  the Rust toolchain here has no 32-bit Windows target installed.
- The original's hunt is started directly (`v_soft.ren` with the menu's
  command line), not through its menus: the menus only write the choices
  and start that program. The software renderer is used because it does
  not depend on the GPU or driver. Checks come from screenshots (ammo
  display, open/closed frames) until the shipped builds' memory layout is
  worked out.

## 2026-11-07 — Wind indicator, compass, setup-list rows

Found by comparing the original engine's regression screenshots with ours.

- **Wind and compass** (`c2-pc/src/hunt_view/gauges.rs`), from `Hunt2.cpp`
  after the binoculars: `WIND.CAR` posed at `Wind.speed * 50` ms of its
  first animation and turned by `CameraAlpha - Wind.alpha`, at
  `(-10, -37, -96)`; `COMPAS.3DF` turned by `CameraAlpha`, at
  `(8, -38, -96)`; light 192. The original moves the projection centre to
  `(W/5, H - 10H/23)` and `(W - W/5, same)` while keeping the camera scale;
  we keep our projection and shift each vertex in camera space so it lands
  at the same screen point at the same depth. Skipped with binoculars or
  scope (`BINMODE || OPTICMODE`), in the trophy room, during the death
  camera (`DemoPoint.DemoTime`), and while Caps Lock is toggled on
  (`KeyboardState[VK_CAPITAL] & 1`); our Caps Lock press toggles a flag,
  since the window system does not report the lock state. Games built from
  the APK keep the phone kit's compass and wind (see the APK hunt HUD
  entry) and do not get these.
- **Hunt-setup rows**: 16 px (`MenuHunt[..].Rect.top + 16 * i`, clicks
  `(y - top) / 16`, Modder's Edition `Menu.cpp`), was 19 px. Text still
  starts at y 382; the click test now measures from that row top. The VR
  menu, which copies these boxes, uses the same rows (10 instead of 8).

## 2026-11-07 — APK-built bases in the regression scenario

- The Android games run as the scenario's `c2-apk` and `ice-age-apk` on
  the folder `c2-pc mobile-base` builds. Species and the weapon count were
  checked against the rebuilt `_RES.TXT`: the four animals of each game
  keep their PC AI numbers, and both tables add a seventh weapon
  (Revolver). Scenario `version` stays 1: the C2 and Ice Age runs are
  unchanged, and the comparison keys on the version, not the file's hash.
- Hash lists for the rebuilt folders are kept apart from the launcher's
  (`pc-ver/crates/c2-regress/hash-lists/`, not `recipes/bases/`). The
  launcher's APK bases record only the APK hash (`BaseInfo::apk`, `files`
  empty), and `check_base` would flag every `mobile-base` change as a
  modified game if they listed files. The rebuild was checked to be
  byte-identical across two builds, which is what makes a file list of it
  meaningful. The lists also carry the APK's hash.
- `Game::load` adds every `HUNTDAT/MOBILE/` file to the run's content
  hashes: the render pass draws them, and only APK-built folders have
  that directory, so the PC content hashes are unchanged.
- `c2-pc recipe base` now hashes `pack::vanilla_files`, not
  `pack::vanilla`. The latter adds compatibility layers (Ice Age's
  rewritten rules), and `check_base` already compared against
  `vanilla_files`. Regenerating `c2-vanilla.toml` and `ice-age.toml` with
  the fix gives the same hashes. The APK-built Ice Age folder triggers the
  rewrite, so its `_RES.TXT` was the one file affected.
- `c2-regress publish` names the hash list by its path from the
  repository root (was: its last three path components), and refuses a
  list outside the repository, because the published-baselines test has to
  find it.

## 2026-11-07 — Regression scenario version 2: Ice Age on area 4

- Ice Age and `ice-age-apk` run on `AREA4`: from its first landing, at
  yaw 0 and 20 cells, all four animals stand on open ground in view. On
  `AREA2` terrain hid some of them. Carnivores 2 and `c2-apk` stay on
  `AREA2`.
- A changed area changes what a run does, so `version` is now 2 and all
  four published baselines were re-run (x5, identical) and re-published.
  The scenario id stays `vanilla_map2_four_dinos`, so recording and
  baseline paths do not move.
- The original Ice Age under Wine also passes every stage on area 4 (the
  runner takes the area from the scenario).

## 2026-11-07 — Modder's Edition data in the regression runner

- The runner resolves a Modder's Edition `_RES.TXT` the way the PC hunt
  does (`HuntView::load`): `c2_core::mee::read_picked_anywhere` for the
  scenario's area, by day, with no menu slots picked, then
  `c2_core::mee::tune` on the species. The render pass already loaded the
  dialect through `HuntView`, so both passes see the same characters and
  their checkpoint hashes match. Stock data is read as before.
- Such games repeat names and AI numbers, so their species are locked by
  character index. `c2-regress species --game DIR --area AREA` lists the
  characters as the runner loads them (index, AI after tuning, model,
  name).
- Set-up refuses an animal whose model is `blank.car`. Dialect games give
  an animal that placeholder on maps it does not live on; placed, it
  would pass every check while invisible.
- Game sections may override `[view] landing` and `yaw_deg`, like
  `anchor_distance`, for areas whose first landing hides the animals.
- The scenario stays at version 2: the new keys are optional, and the
  Carnivores 2, Ice Age and both APK runs are unchanged (checkpoint state
  hashes identical to the published baselines).
- Fallen Kings stays `planned` in the public scenario until it has a
  reference baseline; its runs are kept local.

## 2026-11-07 — A predation meal ends on the walk clip

- In the original engines a carnivore's eating clip only follows killing
  the hunter, and the hunt ends there, so the stock carnivore AI
  (`carnivore.rs`) and the dialect AI (`huntable.rs`) never leave it: both
  skip their chase/wander body and phase selection while it plays.
- Predation reuses that clip for a meal. When the meal finished, or a shot
  or noise cut it short, the animal was left on it and stood eating for
  the rest of the hunt (confirmed in play with an Allosaurus).
- `predation::step::feeding` now ends every meal by switching to the
  species' walk clip (`run::resume_phase`), only if the eating clip is
  still the current one. The Rex is left alone: it eats with its sniff
  clip, which its AI leaves by itself.
- Regression test: `a_finished_meal_hands_the_carnivore_back_to_its_ai`
  (Velociraptor, Allosaurus, Rex).
- Fed carnivores now hunt again after their 30 s rest, so predation can
  empty a map of herbivores faster than before; see the open population
  and corpse policy item from the same audit.

## 2026-11-07 — Regression baselines are tied to the scenario text and to agreeing repeats

- The four published baselines recorded a scenario file hash that matched no
  committed text of `vanilla_map2_four_dinos.toml` (same id, version 2):
  they were recorded from an uncommitted draft. The comparison checked id and
  version only, so nothing noticed.
- The functional comparison now also requires the same `scenario.file_sha256`.
  Any edit to a scenario file, comments included, needs a version bump and new
  baselines, as `docs/REGRESSION.md` already asked.
- Repeats that reach a checkpoint with different state now fail the run, on
  both engines. A single run still passes with determinism "unavailable", so
  quick local runs keep working.
- `publish` refuses a run unless `determinism.identical` is true (so at least
  two repeats) and its scenario hash matches the file in this checkout.
- The baselines were re-recorded from the committed text with `--repeat 3`.
  Their checkpoint state hashes are identical to the old ones, so the draft
  differed only in text that does not change the run. The scenario stays at
  version 2.

## 2026-11-07 — A dead animal settles no predation goal

- Predation re-picks goals every 300 ms (`THINK_MS`) but settles them on
  every AI step. An animal shot dead in between kept its hunt or scavenge
  goal and, within reach of its target, still killed it or ate from it.
- `Predators::settle` now drops the goal of a dead actor before anything
  else. Picking goals less often stays as it is (it keeps the scans cheap).

## 2026-11-07 — Launch protection fails closed; "Use this game anyway" is kept

Source audit findings L1 and L2 (v0.5.0).

- A recipe that names a base whose hash list is missing, malformed or lists
  nothing used to show "not checked" or "ok (0 files)" and play. It now
  blocks Play and the builds, with no override: the fix is to restore
  `recipes/bases/<id>.toml`, or to remove `base` from the recipe, which means
  any game. `c2_mods::recipe::read_base` is the strict loader; `load_base`
  stays lenient for callers that only read a base's details (APK, file count).
  A list with neither files nor `apk` is a parse error, since `Base` is
  `#[serde(default)]` and a misspelt `[files]` table parsed as empty.
- `c2-pc play --recipe`, `recipe build` and `recipe verify` now fail on a
  base mismatch instead of printing and going on. `--accept-base` lets a
  differing (non-APK) game through; it never covers a missing or broken list
  or a game not built from the recipe's APK. This replaces the 2026-11-05 note
  that `recipe verify` only warns.
- The launcher runs c2-pc for Play, Android and combined console builds, so
  it passes `--accept-base` when the player ticked **Use this game anyway**.
  The choice is saved per instance (`Config::accepted_bases`: instance ->
  game folder) and applies only while the instance uses that folder; a
  different folder is checked afresh, and a game that matches clears it. It
  lives apart from `RecipeUse` because several paths rebuild that from
  scratch (the editor, Fix missing, cloning).
- c2-cook (3DS, Wii U) takes the game as given and does not check base
  hashes; the launcher's recipe page is the check before those builds.

## 2026-11-07 — Predation: an animal's kill comes back out of sight

Source audit finding P3 (v0.5.0).

- With predation on, the population only went down: nothing respawns, in
  the original or here. Once P1 was fixed (fed carnivores hunt again),
  `c2-parity predation` on stock areas, 1800 s, hunter away: Area 1 17 of
  25 animals dead, Area 3 9 of 21, most later kills carnivore on carnivore.
- Options considered: a prey-density brake (carnivores stop hunting below a
  share of the placed population), sparing the hunt's targets, and
  respawning. Chosen: respawning, so predation keeps going for the whole
  hunt and the hunter keeps finding animals.
- A body killed by an animal lies 60 s (`run::BODY_MS`) for scavengers.
  Then, once it is beyond the hunter's render distance, the character is
  reset in its own slot as a new animal of the same species
  (`predation/respawn.rs`, `Herd::respawn_kills`): same index, spawn group
  and pack, so no goal, pack leader or ship task needs renumbering, and the
  population can't grow. The spot is out of render distance and found as a
  hunt places animals: the original's main-population rule (within 500
  cells of the map centre, at least 40 from the hunter), or the dialect's
  spawn-group regions (`mee_spot`). No spot found: retried a second later.
- Carnivore victims come back too (carnivore fights stay on when enabled).
- Unchanged corpse policy: a predator's kill fires no `GameEvent::Killed`,
  calls no ship and scores nothing (2026-11-06 "No trophy"). The hunter's
  own kills never come back.
- Tests: `dino/predation/tests/respawn.rs` (comes back out of sight after
  the body time; stays while the hunter can see it; hunter kills stay); the
  first fails without the change. `c2-parity predation` and
  `bench::measure` now count each death of a slot.
- After: 1800 s, same seed: Area 1 38 kills, 25 of 25 alive; Area 3 16
  kills, 21 of 21 alive. Step cost 0.05 ms average.
