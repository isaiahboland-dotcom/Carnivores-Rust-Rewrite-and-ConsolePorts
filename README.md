# Carnivores Classic, rebuilt in Rust for PC, 3DS and beyond

A fan-made, ground-up Rust rewrite of **Carnivores 2** (Action Forms, 1999), with a mod system that can combine community mods automatically, and ports to the **Nintendo 3DS**, the **Wii U**(work in progress) and **PC VR** (work in progress).

You need your own copy of Carnivores 2: this project contains **no game files and no mod files**, only
code, documentation and recipes (lists of credits, download links and file hashes).

**v0.4.3:** [Release notes](docs/releases/v0.4.3.md) · [Changelog](CHANGELOG.md) · [Public roadmap](docs/ROADMAP.md)

---

## Why this exists

This is a passion project. Carnivores Classic has been one of my favourite games for a long time. It's a game I grew up with and the overall tense feeling of hunting while potentially being hunted always stuck with me. This project is my way of showing how much I admire the game, and the community that has kept it alive for over twenty-five years!

The goal is to make the game **more accessible while staying faithful to it**. The original game's
behaviour is the vision. Anything this rewrite has that is different, that's a bug or a documented choice
(`docs/DECISIONS.md`), never an "improvement" slipped in quietly. If you are looking for a fully 
modernized version of Carnivores, I urge you to check out [Carnis](https://cce.carnivoressaga.com)! Although we will add some optional 
settings for some QOL changes I wanted to see, we aim to preserve the functionality of the base game
and for it to be accessible on multiple platforms.

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
| **PC** (Linux; Windows experimental) | Playable, feature-rich | The full original game, plus add-ons, Modder's Edition games, standalone games, Carnivores: Ice Age, animals from the mobile games, survival mode and a balance editor. Bugs mostly show up when a lot of mods and add-ons are used at once. Working on smaller bugs and optional quality of life features |
| **Nintendo 3DS** (homebrew) | Nearly complete | Full hunts, touch menus, bottom-screen map, sound, saves, trophy room, add-on and standalone packs, `.cia` installs. Tested on real 3DS and 2DS hardware. Optional settings help with the handheld controls (see [below](#3ds-settings-for-handheld-play)). Like PC, bugs mostly come from packing in too many add-ons or mods. |
| **Wii U** (homebrew) | Demo, being optimized | Stock Carnivores 2 only: an HD menu (TV and GamePad) with dawn/day/night and all the original gear and options, all five maps, the nine animals, six weapons, calls, radar, GamePad map. Runs on a real Wii U under Aroma (`.wuhb`) and exits to the HOME menu cleanly. A steady 30 FPS in open areas; 20-25 FPS in thick jungle, where the GPU is the limit (owner hardware measurements). New and not yet tested on the console: survival, the trophy room, saved trophies and score (the PC's save format), the PC's kill notice and trophy plaque, the hunter record, and an exit prompt and pause. New: any game as its own Wii U Menu entry: add-ons, the Modder's Edition and standalone games such as Triassic and Mandibles. |
| **PC VR** (OpenXR) | Early first draft | Menus and the hunt setup on a panel, walking the areas, animals that hunt you. No weapons yet. Tested with a PS VR2 on Linux through Monado. |
| **PS5 / PS VR2** | Not started | Research notes only (`psvr2-ver/gameplan.md`). Mainly looking at an optimized PSVR2 setup since the PS5 CFW community is already working on translation layers for PC games to run |

The launcher can also rebuild a base game from your own supported Android APK, with add-ons on top.
Mobile conversion and mixed-mod compatibility still need further gameplay testing; see the
[roadmap](docs/ROADMAP.md) for current priorities.

Survival offers **Classic** (stock animals; hidden on standalone rosters) and
**Modded** (eligible stock and installed mod animals). Neither requires an APK.
Waves use hunt-menu prices and a separate budget that grows 10% per round.
The starting budget defaults to twice the cheapest eligible dangerous animal's
price (minimum 10). PC Game options can override it; packs can set
`survival_start` in the menu script's `prices` section on all platforms.
Choosing Classic or Modded starts immediately on Mount Ravan in the mobile
survival ravine, with a shotgun only. Waves come from the fixed approach;
ordinary map wildlife is removed. Movement remains enabled.
Console memory and wave limits still apply.


---

## What you need

- **Your own Carnivores 2 install**: the folder that contains `HUNTDAT/`. ReShade or other wrapper DLLs
  in it are ignored.
- **Building from source:** stable Rust for PC (<https://rustup.rs>). Downloaded Windows
  builds do not need Rust or C++ build tools; see [Windows installation](#windows-installation).
  The 3DS and Wii U source builds also need nightly; see their sections.
- Optional: mods you've downloaded, each in its own folder (or as a `.zip`; extract `.rar` and `.7z` separately
  before adding their folders). Carnivores: Ice Age and the mobile games' APKs can be imported too.

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
git clone https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts.git c2rs
cd c2rs/pc-ver
cargo build --release          # builds c2-pc, the launchers and c2-vr into pc-ver/target/release/
cargo run --release -p c2-launcher --bin c2-launcher   # or ./target/release/c2-launcher
```

Three builds of the same app:

| | |
|---|---|
| `c2-launcher` | play on PC and package Windows instances, with optional 3DS and Wii U sections |
| `c2-3ds-builder` | 3DS installs only (`cargo run --release -p c2-launcher --bin c2-3ds-builder`) |
| `c2-wiiu-builder` | Wii U installs only (`cargo run --release -p c2-launcher --bin c2-wiiu-builder`) |

The launcher opens on **Instances**: a grid of boxes, one per instance (a whole game: a base game, its
add-ons and its choices), sized to the window, plus **New instance**. Each box shows the base game, how
many add-ons, a yellow **⚠ N missing** when its base game or downloads can't be found, and the time
played (or **Unplayed**). **Import recipe…** copies in a recipe someone shared. **Settings** is the only
other tab.

- **Play** checks the instance's downloads and its base game, then builds the combined game (cached
  until something changes) and starts it. Missing downloads are listed with their download pages.
- **Build** asks which platform: **Windows** (experimental), **3DS** or **Wii U**, then shows only that
  builder; **Cancel** at the bottom goes back. Output, packaging and artwork choices are remembered
  separately for each instance. The 3DS and Wii U builders only offer their own platform.
- **Options** (or a right click on the box): **Edit**, **Clone**, **Fix missing**, **Export…** (the recipe
  to share, with its base game's hash list and patches), **Play in VR** and **Delete**. **Fix missing** looks through the mod and add-on folders set in Settings for the instance's
  base game and downloads and keeps what it finds: drop new downloads into those folders, then run it.

**Edit** shows the instance's own pages, with **Save**, **Save and exit**, **Discard changes** and
**Cancel** at the bottom:

- **Base game**: the instance's name and its game folder: your Carnivores 2 (or Ice Age) folder, a game
  built from your APK (**Build from my APK…**), or a standalone mod used as the base game on its own.
  The folder is checked against the base game's file hashes, so a wrong game shows up here. Pinned
  overlays (such as the Modder's Edition), **Pick parts** for the base game, and the **Menu** choice.
- **Add-ons**: standalone games (combined beside the base; **Pick parts** keeps only the areas, animals
  and weapons you want) and add-ons in load order, each with its folder. Animals can also be imported
  from Ice Age or the mobile games' APKs. Drop folders on the window to add them.
- **Balance**: PC or phone rules, and every area, animal and weapon with its price and score, plus the
  starting points. **Refresh list** reads them again after the other pages change. Changes are kept in
  the instance, never written into the game.

**Launch protection:** each instance keeps its own game folder, and Play and the builds stop when that
folder's files don't match the base game the recipe names (its SHA-256 hashes). Use the right folder, or
tick **Use this game anyway** when it is another version of the same game.

## Recipes: sharing a mod setup

A recipe (`recipes/*.toml`) describes a modded game **without containing any of it**. Each mod is listed
with its authors' credit, its download page and the SHA-256 of every file used, plus the load order,
conflict choices and any small patches of our own. Anyone can rebuild the exact same game from their own
downloads. In the launcher, every instance is a recipe: put the file under `recipes/` and it shows on the
Instances screen.

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
| `c2-everything.toml` | every add-on, plus the mobile Dinosaur Hunter's and Ice Age's animals (APKs needed) |
| `c2-android.toml` | Carnivores 2 with the mobile Dinosaur Hunter's animals (APK needed for these animals) |
| `ice-age-android.toml` | Carnivores: Ice Age with the mobile Ice Age's animals (APK needed) |
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

Input limits, generated-install recovery and validation scope:
[Security hardening notes](docs/SECURITY-HARDENING.md).
