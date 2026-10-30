# Carnivores Classic, rebuilt in Rust for PC, 3DS and beyond

A fan-made, ground-up Rust rewrite of **Carnivores 2** (Action Forms, 1999), with a mod system that can combine community mods automatically, and ports to the **Nintendo 3DS**, the **Wii U**(work in progress) and **PC VR** (work in progress).

You need your own copy of Carnivores 2: this project contains **no game files and no mod files**, only
code, documentation and recipes (lists of credits, download links and file hashes).

---

## Why this exists

This is a passion project. Carnivores Classic has been one of my favourite games for a long time. It's a game I grew up with and the overall tense feeling of hunting while potentially being hunted always stuck with me. This project is my way of showing how much I admire the game, and the community that has kept it alive for over twenty-five years!

The goal is to make the game **more accessible while staying faithful to it**. The original game's
behaviour is the reference: where this rewrite differs, that's a bug or a documented choice
(`docs/DECISIONS.md`), never an "improvement" slipped in quietly.

The modding community has been absolutely amazing. There are new animals, weapons, maps, whole
standalone games, and the Modder's Edition Engine. It can also feel a bit daunting. Different mods are
built for different engines and executables, each install is its own copy of the game, and getting
several mods running together can mean a lot of manual file shuffling. This project aims to:

- **Bridge the gap between the engines.** It plays the original game's data, the Modder's Edition
  Engine's `_RES.TXT` dialect, and standalone games made from Carnivores 2, all from one engine. It can
  also combine add-ons and whole games into one install.
- **Bring your favourite mods, or your favourite version of the game, to other devices.** You can play
  the same game, with the same add-ons, on a PC, a 3DS, a Wii U or in a VR headset.
- **Respect the people who made it all.** Recipes share a mod setup by credit, link and file hash only,
  so everyone downloads mods from their authors' own pages.

I know it isn't feasible to get the game running on every console or device ever made, but that won't
stop me from trying. This is also an ongoing exploration of what AI can do when adapting code for very
different systems: much of the work is done together with Claude and ChatGPT, from reading the original C++ to
the 3DS's PICA200 GPU and the Wii U's PowerPC CPU.

---

## Status

| Platform | State | Notes |
|---|---|---|
| **PC** (Linux; Windows untested) | Playable, feature-rich | The full original game, plus add-ons, Modder's Edition games, standalone games, Carnivores: Ice Age, animals from the mobile games, survival mode and a balance editor. Bugs mostly show up when a lot of mods and add-ons are used at once. |
| **Nintendo 3DS** (homebrew) | Nearly complete | Full hunts, touch menus, bottom-screen map, sound, saves, trophy room, add-on and standalone packs, `.cia` installs. Tested on real 3DS and 2DS hardware. Like PC, bugs mostly come from packing in too many add-ons or mods. |
| **Wii U** (homebrew) | Demo, being optimized | Stock Carnivores 2 only: menu, all five maps, the nine animals, six weapons, calls, radar, GamePad map. Runs on a real Wii U under Aroma (`.wuhb`), and exits to the HOME menu cleanly. About 30 FPS standing still, 19 FPS while turning, with a brief stutter each time you walk into a new map cell; work continues ([results](wiiu-ver/perf-probe/hardware-logs/2026-10-30-resident-scenery-results.md)). Known issues: weapon selection, the gun being hidden by scenery, the scope and menu resolution. |
| **PC VR** (OpenXR) | Early first draft | Menus and the hunt setup on a panel, walking the areas, animals that hunt you. No weapons yet. Tested with a PS VR2 on Linux through Monado. |
| **PS5 / PS VR2** | Not started | Research notes only (`psvr2-ver/gameplan.md`). |

---

## What you need

- **Your own Carnivores 2 install**: the folder that contains `HUNTDAT/`. ReShade or other wrapper DLLs
  in it are ignored.
- **Rust**: stable for PC (<https://rustup.rs>). The 3DS and Wii U builds also need nightly; see their
  sections.
- Optional: mods you've downloaded, each in its own folder (or as a `.zip`; the launcher also takes `.rar`
  and `.7z`). Carnivores: Ice Age and the mobile games' APKs can be imported too.

On Linux, the PC build needs the ALSA and udev headers:

```sh
sudo apt install build-essential pkg-config libasound2-dev libudev-dev   # Debian / Ubuntu
sudo dnf install gcc pkgconf alsa-lib-devel systemd-devel                 # Fedora
sudo pacman -S base-devel alsa-lib                                        # Arch
```

---

## The easy way: the launcher (GUI)

The launcher sets up the game, manages mods, plays on PC, and builds 3DS and Wii U installs, without the
command line.

```sh
git clone <this repository> c2rs
cd c2rs/pc-ver
cargo build --release          # builds c2-pc, the launchers and c2-vr into pc-ver/target/release/
cargo run --release -p c2-launcher
```

Three builds of the same app:

| | |
|---|---|
| `c2-launcher` | everything: play on PC, plus the 3DS and Wii U sections if you turn them on (it asks the first time) |
| `c2-3ds-builder` | 3DS installs only (`cargo run --release -p c2-launcher --bin c2-3ds-builder`) |
| `c2-wiiu-builder` | Wii U installs only (`--bin c2-wiiu-builder`) |

The tabs:

- **The game**: drop in, browse to, or paste the path of your Carnivores 2 (or Carnivores: Ice Age) folder;
  it's checked for `HUNTDAT`. **Base overlays** go on top of it, such as the Modder's Edition release, which
  ships changed files only.
- **Add-ons**: add mod folders or archives (`.zip`, `.rar`, `.7z`), switch each on or off, and change their
  order. **Standalone games** (whole games made from Carnivores 2) go here too, and are combined beside
  the base game. You can also import animals from Ice Age or from the mobile games' APKs.
- **Balance**: every area, animal and weapon with its price and score, plus the starting points. Edit them to
  taste, or to make several mods fit together. Where the PC and mobile versions differ, pick which one to
  use. Your changes are kept as a balance file over the game, never written into it.
- **Play**: **PLAY** builds the combined game (cached until something changes) and starts it.
  **PLAY IN VR** starts the same game in a headset.
- **3DS**: choose Old or New 3DS, an install name and your SD card (or a folder). **Build for 3DS** cooks
  the packs (one for the original game or the base with its add-ons, and one per standalone game that's
  on) and copies the app into `3ds/<name>/`. It can also make a `.cia` with your own home-menu icon and
  banner (see [.cia builds](#cia-builds-optional)).
- **Wii U**: choose your SD card and output format. Build a `.wuhb` for the Aroma Wii U Menu
  (`wuhbtool` from devkitPro `wut-tools` is required), or an `.rpx` for the Homebrew Launcher.
  Both formats copy the original Carnivores 2 files the app needs into
  `wiiu/apps/carnivores-probe/`. Add-ons and other games are listed as "not on the Wii U yet".
- **Settings**: the tools folder (where `c2-pc`, `c2-cook`, the 3DS and Wii U apps, `wuhbtool`, `makerom` and `bannertool`
  are found), showing the 3DS / Wii U sections, and engine options.

The launcher runs the other programs for you. It finds them beside itself, in the tools folder, or in this
source tree's build folders. So for the 3DS and Wii U tabs, build those apps first (below).

---

## PC from the command line

```sh
cd pc-ver
cargo run --release -p c2-pc -- play PATH/TO/Carnivores2
```

Register a hunter (or pick one copied from your install), then Hunt, Options, Trophy, Statistics and
Credits, as in the original. Your game folder is never written to. Saves go to
`~/.local/share/c2rs/` (the first run copies your game's saves).

With mods:

```sh
cargo run --release -p c2-pc -- play GAME --mod ADDON_DIR --mod ADDON.zip              # add-ons
cargo run --release -p c2-pc -- play GAME --overlay "PATH/Carnivores 2" --mod ADDON    # on the Modder's Edition
cargo run --release -p c2-pc -- play GAME --standalone GAME_DIR                         # a standalone game beside the base
cargo run --release -p c2-pc -- play GAME --recipe ../recipes/c2-mee-addons.toml --mods DOWNLOADS --overlay "PATH/Carnivores 2"
```

**Controls:** keyboard and mouse follow your bindings (Options > Controls). A gamepad works too: left
stick moves, right stick looks, ZL draws, ZR fires, L binoculars, R map, A jump, B run, X call, Y next
weapon, D-pad up crouch / down change call, + pause, − exit. In the menus, the left stick moves a pointer,
A clicks and B goes back.

**PC VR** (early): `cargo run --release -p c2-vr -- play GAME` with an OpenXR runtime running (SteamVR,
Monado, WiVRn). `c2-vr check` tells you which runtime it found. Plan and status:
`pc-ver/crates/c2-vr/GAMEPLAN.md`.

More: [docs/MODDING.md](docs/MODDING.md) (mods, recipes, balance files, standalone games) and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) (viewers, debug modes, design).

---

## Nintendo 3DS from the command line

Needs devkitPro (devkitARM, libctru, citro3d), Rust nightly with `rust-src` (selected by
`3ds-ver/rust-toolchain.toml`), `cargo-3ds` (`cargo install cargo-3ds`) and libclang. Details:
[docs/3DS.md](docs/3DS.md).

```sh
cd 3ds-ver

# 1. Cook your game into a pack (runs on the PC). Use --target n3ds for a New 3DS.
cargo run --release -p c2-cook -- --game PATH/TO/Carnivores2 --target o3ds --out c2rs.c2pk
#    add-ons:  --overlay "PATH/Carnivores 2" --mod ADDON --mod ADDON ...   (or --recipe R.toml --mods DIR)

# 2. Build the app.
export DEVKITPRO=/opt/devkitpro DEVKITARM=/opt/devkitpro/devkitARM
cargo 3ds build --release -p c2-3ds        # -> target/armv6k-nintendo-3ds/release/c2-3ds.3dsx

# 3. Optional: check a pack on the PC the way the 3DS loads it.
cargo run --release -p c2-cook -- --check c2rs.c2pk
```

On the SD card, put `c2-3ds.3dsx` in `/3ds/` and the pack(s) in `/3ds/c2rs/`. Start it from the Homebrew
Launcher; with more than one pack, it asks which to play. Each pack keeps its own saves. Packs are made
from your own files: keep them to yourself.

The launcher's 3DS tab does all of this for you, and installs under a name of your choosing
(`sdmc:/3ds/<name>/`), so several installs can sit side by side.

### `.cia` builds (optional)

A `.cia` installs the game to the home menu (with FBI or a similar title manager). It needs two tools that
are **not part of this project**; download them yourself:
- **makerom**, from Project_CTR: <https://github.com/3DSGuy/Project_CTR/releases>
- **bannertool**: <https://github.com/Steveice10/bannertool/releases>

Put them beside the launcher or in its tools folder (on Linux and macOS, `chmod +x makerom bannertool`),
then use the 3DS tab, or:

```sh
c2-3ds-builder --cia NAME --tools DIR --out SD_DIR [--target o3ds|n3ds] [--icon PNG] [--banner PNG] [--sound WAV]
```

Each install name gets its own title ID and reads its packs from `sdmc:/3ds/NAME/`. The icon (48x48),
banner (256x128) and banner sound are your own. Without them, they're made from the game's own menu picture.

---

## Wii U from the command line (demo)

The Wii U version is a demo: stock Carnivores 2 only, tested in Cemu and on a real Wii U under Aroma. It's a C
frontend on devkitPro's WUT with the shared Rust code linked in. You need:

- Rust nightly with `rust-src`
- Docker, for the `devkitpro/devkitppc` image
- the third-party tools fetched into `wiiu-ver/tools/`, as described in
  [wiiu-ver/tools/README.md](wiiu-ver/tools/README.md)

From the repository root:

```sh
cargo +nightly build --release \
  --manifest-path wiiu-ver/perf-probe/Cargo.toml \
  --target wiiu-ver/perf-probe/powerpc-wiiu.json \
  -Z json-target-spec -Z build-std=core,alloc,compiler_builtins
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$(pwd)/wiiu-ver/perf-probe:/src" -w /src devkitpro/devkitppc \
  sh -c '/opt/devkitpro/devkitPPC/bin/powerpc-eabi-objcopy --remove-section .note.GNU-stack target/powerpc-wiiu/release/libc2_wiiu_terrain_probe.a target/wiiu-terrain.a && /opt/devkitpro/portlibs/wiiu/bin/powerpc-eabi-cmake -S . -B build && cmake --build build'

# Copy the game files it needs to the SD card (or use the launcher's Wii U tab):
python3 wiiu-ver/perf-probe/stage_assets.py PATH/TO/Carnivores2/HUNTDAT SD/wiiu/apps/carnivores-probe
```

Run `wiiu-ver/perf-probe/build/c2-wiiu-perf-probe.rpx` in Cemu, or from the Homebrew Launcher, with the
files above on the SD card. For the Aroma Wii U Menu, package it as `carnivores-probe.wuhb` with `wuhbtool`
(the launcher's Wii U tab does this). Hardware frame-time captures and their results are in
[wiiu-ver/perf-probe/hardware-logs/](wiiu-ver/perf-probe/hardware-logs/). Controls and the current state: [wiiu-ver/perf-probe/README.md](wiiu-ver/perf-probe/README.md);
the plan: [docs/WIIU.md](docs/WIIU.md) and [wiiu-ver/gameplan.md](wiiu-ver/gameplan.md).

---

## Recipes: sharing a mod setup

A recipe (`recipes/*.toml`) describes a modded game **without containing any of it**. Each mod is listed
with its authors' credit, its download page and the SHA-256 of every file used, plus the load order,
conflict choices and any small patches of our own. Anyone can rebuild the exact same game from their own
downloads. Recipes are used from the command line for now; the launcher's Add-ons tab takes the same
downloads directly.

**Using a recipe:**

1. **Pick a recipe** from the table below and open it. Each `[[mods]]` entry has the mod's `credit` and its
   download page (`url`).
2. **Download every mod** listed, from its page. **Unpack each one into its own folder**, all inside one
   downloads folder (`DOWNLOADS` below). The folder names don't matter: mods are recognised by their files.
3. **Get the base game and anything else the recipe's header asks for:**
   - your Carnivores 2 folder (`GAME`), or Carnivores: Ice Age for `ice-age-android.toml`;
   - for recipes on the Modder's Edition Engine, its [1.1.1 release](https://www.moddb.com/mods/carnivores-modders-edition/downloads/carnivores-modders-edition-111), unpacked. Use the folder
     `Carnivores Modders Edition 1.1.1 Standalone/Carnivores 2` as the `--overlay`;
   - for the `*-android` recipes and `c2-everything.toml`, import the mobile games' animals from your own
     APK first, into `DOWNLOADS`. The exact `c2-pc mobile ...` command is in the recipe's header.
4. **Check** that everything is there and matches:
   ```sh
   cd pc-ver
   cargo run --release -p c2-pc -- recipe verify ../recipes/c2-mee-addons.toml --base GAME --overlay "PATH/Carnivores 2" --mods DOWNLOADS
   ```
   It lists anything missing or different, with the link to get it.
5. **Play it**, or build it into a folder, or cook it for the 3DS:
   ```sh
   cargo run --release -p c2-pc -- play GAME --overlay "PATH/Carnivores 2" --recipe ../recipes/c2-mee-addons.toml --mods DOWNLOADS
   cargo run --release -p c2-pc -- recipe build ../recipes/c2-mee-addons.toml --base GAME --overlay "PATH/Carnivores 2" --mods DOWNLOADS --out DIR
   ```
   The 3DS command for each recipe is in its header.

The recipes here:

| Recipe | What |
|---|---|
| `c2-mee-addons.toml` | ten add-on animals on the Modder's Edition Engine 1.1.1 |
| `c2-everything.toml` | every add-on, plus the mobile Dinosaur Hunter's and Ice Age's animals (from your own APKs) |
| `c2-android.toml` | Carnivores 2 with the mobile Dinosaur Hunter's animals and classic survival (your APK) |
| `ice-age-android.toml` | Carnivores: Ice Age with the mobile Ice Age's animals (your APK) |
| `example.toml` | the format, not a real pack |
| `bases/*.toml` | hashes of the retail Carnivores 2 and Ice Age, to check a base game |

To make your own from mods you have (it pins every file they use):
`c2-pc recipe new MOD_DIR... --base GAME --id my-pack --out my-pack.toml`. See
[docs/MODDING.md](docs/MODDING.md#recipes).

---

## Repository layout

| | |
|---|---|
| `pc-ver/` | Cargo workspace (stable Rust): the shared crates (`c2-formats`, `c2-vfs`, `c2-mods`, `c2-core`), the PC game (`c2-pc`), the launchers, PC VR (`c2-vr`), gamepads, and the parity tools |
| `3ds-ver/` | the 3DS app (`c2-3ds`) and the pack cooker (`c2-cook`); nightly, reuses `pc-ver/crates` |
| `wiiu-ver/` | the Wii U port: a series of probes for testing and the playable demo (`perf-probe`) |
| `psvr2-ver/` | PS VR2 research notes |
| `recipes/` | mod recipes: credits, links and hashes only |
| `docs/` | [MODDING](docs/MODDING.md), [3DS](docs/3DS.md), [WIIU](docs/WIIU.md), [FORMATS](docs/FORMATS.md) (file formats), [DECISIONS](docs/DECISIONS.md) (every design choice and deviation), [DEVELOPMENT](docs/DEVELOPMENT.md) (spec and conventions) |
| `source/` | **gitignored**: your own game, mods and reference material (see DEVELOPMENT.md) |

---

## Legal

- This is an unofficial fan project, not affiliated with or endorsed by Action Forms, Tatem Games, or any
  mod author.
- No game data, mod files, or original source code are in this repository, and none ever will be.
  You supply your own copy of the game and download mods from their authors.
- The original Carnivores 2 C++ source, released publicly in 2013, was used **as a reference only**
  to match behaviour and file formats. This is a rewrite, not a translation. The same goes for the
  Modder's Edition Engine's public source and manual.
- The code in this repository is under the [MIT License](LICENSE).

## Thanks

- **Action Forms**, for Carnivores 2 and for releasing its source.
- **The Carnivores modding community**: every add-on, map, skin and standalone game author, and the
  creators of the **Modder's Edition Engine** and its modernized fork. The recipes credit each mod's authors
  and link to their pages; please download from them and support their work.
- The homebrew toolchains this builds on: **devkitPro** (devkitARM, devkitPPC, libctru, citro3d, WUT),
  the **rust3ds** project, **rust-wiiu**, and **Monado** for open-source VR on Linux.
- Built with the help of **Claude** and **ChatGPT**
