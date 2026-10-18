# Modding

This build supports **add-on mods**: data-only mods that add or replace files
under `HUNTDAT/` and/or change `_RES.TXT` entries on top of vanilla (new or
reworked dinosaurs, weapons, areas, textures, menus). Whole-game total
conversions are out of scope. Recipes (sharing a mod combination by URL and
file hash) arrive in M5.

## Installing a mod
Put each mod in its own folder (for example under `source/mods/`). The folder
must contain the mod's `HUNTDAT` folder, either directly or inside one
subfolder. A mod shipped as a complete copy of the game is fine: files
identical to vanilla, executables, DLLs, logs and map-editor side files are
ignored.

## Playing with mods
```
cd pc-ver
cargo run --release -p c2-pc -- play ../source/c2-original --mod ../source/mods/MyMod --mod ../source/mods/Other
```
Mods load in the order given (a mod is moved after any mod it `requires`).
The combined game is built in the data directory
(`~/.local/share/c2rs/packs/<mods>/`), linking to your files rather than
copying them; your game and mod folders are never written. Its
`MERGE_REPORT.md` lists the load order, every conflict and who won, every
`_RES.TXT` change and every file taken from a mod.

To build a combined game folder somewhere else:
```
cargo run --release -p c2-pc -- mod build MOD_DIR... --base ../source/c2-original --out DIR
```

## How mods are combined
- **Files:** the last mod that provides a file wins. Two mods providing the
  same file is reported as a conflict.
- **`_RES.TXT`:** merged entry by entry, not as a whole file, so two mods can
  each change a different dinosaur or weapon. Entries are matched by `name`.
  A changed entry modifies that slot; an entry renamed in place (a T-Rex slot
  that now holds a different animal) replaces the slot, because saves and the
  menu refer to dinosaurs by position; anything else is added. `prices` lines
  are matched by key and position. Two mods changing the same entry
  differently is a conflict; the later mod wins.
- The merged `_RES.TXT` is written in a normalised layout (comments dropped).

## `mod.toml`
Optional. Without one, the manifest is inferred. To see (or save) the
inferred one:
```
cargo run --release -p c2-pc -- mod detect MOD_DIR --base ../source/c2-original           # print
cargo run --release -p c2-pc -- mod detect MOD_DIR --base ../source/c2-original --write   # save MOD_DIR/mod.toml
```
`--write` is the only command that writes into a mod folder, and it refuses
to overwrite an existing `mod.toml`. Fields (all optional):

```toml
[mod]
id = "example-raptors"          # default: from the folder name
name = "Example Raptor Pack"
version = "1.0"
authors = ["Original Author"]
source_url = "https://example.com/mod-page"

[files]                         # empty = detect against vanilla
adds = ["HUNTDAT/NEWRAPTOR.CAR"]
replaces = ["HUNTDAT/MENU/MENUM.TGA"]

[res]                           # informational; the merge reads _RES.TXT itself
adds_dinos = ["newraptor"]
modifies_dinos = []
adds_weapons = []
modifies_weapons = []

[deps]
requires = ["other-mod-id"]     # must also be loaded; loads first
conflicts = ["bad-mod-id"]      # refuse to load together

[engine]
features = []                   # reserved for exe-patch features
```

## Checking a mod's files
`c2-parity scan <dir>` parses every `.CAR`, `.3DF`, `.MAP`, `.RSC` and
`_RES.TXT` under a directory and reports anything malformed.
