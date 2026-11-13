# Playing on PC

This guide covers installing the PC build, the launcher, the command line, controls and the
optional PC settings. For the project overview, see the [README](../README.md).

You need your own Carnivores 2 install: the folder that contains `HUNTDAT/`. The bundles and
the source tree contain no game or mod files.

## Install

Download the bundle for your system from the
[v0.5.3 release](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/releases/tag/v0.5.3):

- **Windows (64-bit):** [windows.zip](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/releases/download/v0.5.3/windows.zip). See [Windows builds](WINDOWS.md) for details.
- **Linux (x86-64, glibc 2.35 or newer; Ubuntu 22.04 or newer):** [linux.tar](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/releases/download/v0.5.3/linux.tar)

Extract the **whole archive** into a writable folder and keep the executables, `recipes/` and the
other included files together. Then:

- **Windows:** open `c2-launcher.exe`.
- **Linux:** run it from a terminal:
  ```sh
  mkdir -p c2rs
  tar -xf ~/Downloads/linux.tar -C c2rs
  cd c2rs
  ./c2-launcher
  ```

Choose **New instance**, select your own game folder as the base game, save, then press **Play**.

Linux still needs desktop/audio libraries and graphics drivers. Runtime packages and
console-packaging requirements are in the [bundle guide](BUNDLE.md).

### Build from source

Install stable Rust from <https://rustup.rs> and Git. On Linux, install the ALSA and udev headers
and build tools:

```sh
sudo apt install build-essential pkg-config libasound2-dev libudev-dev   # Debian / Ubuntu
sudo dnf install gcc pkgconf alsa-lib-devel systemd-devel                 # Fedora
sudo pacman -S base-devel alsa-lib                                        # Arch
```

Then clone and build:

```sh
git clone https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts.git c2rs
cd c2rs/pc-ver
cargo build --release          # builds c2-pc, the launchers and c2-vr into pc-ver/target/release/
cargo run --release -p c2-launcher --bin c2-launcher   # or ./target/release/c2-launcher
```

Windows source builds need Visual Studio Build Tools; see [Windows builds](WINDOWS.md).

## The launcher

The launcher sets up the game, manages mods, plays on PC, and provides the 3DS and Wii U packaging
interfaces. It opens on **Instances**: one box per instance (a whole game: a base game, its add-ons
and its choices), plus **New instance**. Each box shows the base game, the number of add-ons, a
yellow **⚠ N missing** when files can't be found, and the time played (or **Unplayed**).
**Import recipe…** copies in a recipe someone shared. **Settings** is the only other tab.

- **Play** checks the instance's downloads and base game, builds the combined game (cached until
  something changes) and starts it. Missing downloads are listed with their download pages.
- **Build** asks for a platform: **Windows** (experimental), **3DS** or **Wii U**. Output, packaging
  and artwork choices are remembered separately for each instance.
- **Options** (or a right click on the box): **Play Vanilla**, **Edit**, **Clone**, **Fix missing**,
  **Export…** (the recipe to share, with its base game's hash list and patches), **Play in VR** and
  **Delete**. **Fix missing** looks through the mod and add-on folders set in Settings and keeps
  what it finds.
- **Play Vanilla** plays the instance without its enhanced rules (predation, custom flee limits,
  spawn rules, locked settings), with the same prices and saves; its hover text lists what is left
  out. It is greyed out for an instance that has none. The instance itself is not changed
  ([what is kept](RECIPE-SETTINGS.md)).
- After **Import recipe…**, a recipe with enhanced rules lists them, each with **Remove** (taken
  out of the instance) and, for predation and custom flee limits, **Restore defaults** (kept on,
  with the built-in values). Both save the recipe the way **Edit** does, keeping its comments.

**Edit** has these pages, with **Save**, **Save and exit**, **Discard changes** and **Cancel** at
the bottom:

- **Base game:** the name and game folder: your Carnivores 2 (or Ice Age) folder, a game built from
  your APK (**Build from my APK…**), or a standalone mod used as the base game on its own. The
  folder is checked against the base game's file hashes. Pinned overlays (such as the Modder's
  Edition), **Pick parts**, and the **Menu** choice live here too.
- **Add-ons:** standalone games (combined beside the base; **Pick parts** keeps only the areas,
  animals and weapons you want) and add-ons in load order, each with its folder. Animals can also be
  imported from Ice Age or the mobile games' APKs. Drop folders on the window to add them.
- **Balance:** PC or phone rules, and every area, animal and weapon with its price and score, plus
  the starting points. **Refresh list** reads them again after other pages change. Changes stay in
  the instance and are never written into the game.

**Launch protection:** each instance keeps its own game folder. Play and the builds stop when that
folder's files don't match the base game the recipe names (its SHA-256 hashes). Use the right
folder, or tick **Use this game anyway** when it is another version of the same game; the instance
remembers that choice for that folder. A recipe whose base game hash list (`recipes/bases/`) is
missing or broken is not played at all: restore the list, or remove `base` from the recipe to allow
any game.

## Command line

With a downloaded bundle, run these from the extracted folder. Replace `cargo run --release -p c2-pc --`
with `./c2-pc` (or `.\c2-pc.exe` on Windows). Bundled recipes are in `recipes/`.

```sh
cargo run --release -p c2-pc -- play PATH/TO/Carnivores2
```

Register a hunter (or pick one copied from your install), then Hunt, Options, Trophy, Statistics and
Credits, as in the original. Your game folder is never written to. Saves go to `~/.local/share/c2rs/`
(the first run copies your game's saves). On Windows they live under `%APPDATA%\c2rs`.

With mods:

```sh
cargo run --release -p c2-pc -- play GAME --mod ADDON_DIR --mod ADDON.zip              # add-ons
cargo run --release -p c2-pc -- play GAME --overlay "PATH/Carnivores 2" --mod ADDON    # on the Modder's Edition
cargo run --release -p c2-pc -- play GAME --standalone GAME_DIR                         # a standalone game beside the base
cargo run --release -p c2-pc -- play GAME --recipe ../recipes/c2-mee-addons.toml --mods DOWNLOADS --overlay "PATH/Carnivores 2"
```

Add `--classic` to a `--recipe` command to play it as **Play Vanilla**: the same game, prices and
saves, without the recipe's enhanced rules (predation, custom flee limits, spawn rules, locked
settings) and with the opt-in enhancements in Options (teleport, dynamic time, multi ammo and the
like) at their defaults. Your recipe, settings and the normal build are left as they are; the
classic build has its own folder. What is kept and what is turned off: [RECIPE-SETTINGS.md](RECIPE-SETTINGS.md).

More on mods, recipes and balance files: [MODDING.md](MODDING.md).

## Controls

Keyboard and mouse follow your bindings (Options > Controls). Gamepads on PC and Android use
Xbox/Steam Deck labels: left stick moves (click to sprint), right stick looks (click to cycle
lights), LT draws, RT fires, LB binoculars, RB map, A jump, B supported firing modes, X call, Y next
weapon. Hold X/Y for the call/weapon picker; inside them, right-stick click changes the flashlight
beam and B swaps split lethal/tranq ammo. D-pad up crouches, down changes calls, left reloads, right
racks; Start pauses, Select opens the exit prompt. Advanced Observer uses A double-tap for flight and
X to stop or resume time. In menus, A confirms and B goes back. Full mapping, including Android's
default-on **Fire reloads when empty** and the optional picker pause: [pc-ver/CONTROLLER.md](../pc-ver/CONTROLLER.md).
Nintendo ports keep their own mappings.

## PC options

- **Map zoom / pan** (Options > Video, off by default): with the map open, use the wheel to zoom at
  the pointer, right-drag to pan, and Home to restore the full map.
- **Dynamic time** adds an optional 60-minute day/night cycle, starting at the chosen time. Night
  equipment is selected under Equipment before the hunt: **N** toggles night vision and **L** the
  flashlight (only one at a time). Dynamic-time night kills earn +20% while night vision is off; the
  flashlight keeps that bonus. **K** switches the flashlight beam between wide and a narrow one that
  reaches farther (remembered for the next hunt). Details: [DYNAMIC_TIME.md](DYNAMIC_TIME.md).
- **Multi ammo** (Options > Game, off by default): picking the tranquilizer asks **Half tranq** (each
  weapon's rounds split between lethal rounds and darts; **T** swaps them, and the other kind loads
  when one runs out; the HUD shows "lethal:" and "tranq:" rows) or **Full tranq** (darts only, as the
  original).
- **Advanced observer mode** (Options > Game): in observer hunts, teleport by the map with no wait or
  cooldown, **F** flies (jump rises, crouch sinks), **T** stops time for camera shots.
- **Bounded fleeing** (Options > Video, off by default; experimental on 3DS, where it is in the
  hunt options): scared herbivores and wary animals keep to the map and run clear of cliffs and
  water instead of off the edge, a flight ends after a limited distance or time, and repeated
  noises cannot keep an animal afraid. Limits come from the instance's recipe (see
  [MODDING.md](MODDING.md#flee-limits-bounded-fleeing)) or built-in defaults.
- **Predation** is an advanced setting for instances (launcher: the recipe editor's Advanced tab),
  off unless a recipe turns it on. See [MODDING.md](MODDING.md#predation-carnivores-hunt-herbivores).

### Survival

Survival offers **Classic** (stock animals; hidden on standalone rosters) and **Modded** (eligible
stock and installed mod animals). Neither requires an APK. Waves use hunt-menu prices and a separate
budget that grows 10% per round. The starting budget defaults to twice the cheapest eligible
dangerous animal's price (minimum 10). PC Game options can override it; packs can set
`survival_start` in the menu script's `prices` section on all platforms.

Choosing Classic or Modded starts immediately on Mount Ravan in the mobile survival ravine, with a
shotgun only. Waves come from the fixed approach; ordinary map wildlife is removed. Movement remains
enabled. Console memory and wave limits still apply.

**Hardcore survival** (PC; off by default, not part of the original game): turn on Game options >
"Enable hardcore survival mode" and a third choice appears. It has Modded's waves, but at a night
that never ends, as pitch black as the dynamic day cycle's midnight, with the flashlight, the radar
and the shotgun.

## PC VR (early)

`cargo run --release -p c2-vr -- play GAME` with an OpenXR runtime running (SteamVR, Monado or
WiVRn). `c2-vr check` reports which runtime it found. Plan and status:
`pc-ver/crates/c2-vr/GAMEPLAN.md`.
