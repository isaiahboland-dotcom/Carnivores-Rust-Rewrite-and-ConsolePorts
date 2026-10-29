# Modding

This build supports **add-on mods**: data-only mods that add or replace files
under `HUNTDAT/` and/or change `_RES.TXT` entries on top of vanilla (new or
reworked dinosaurs, weapons, areas, textures, menus). Game folders made for
the community Modder's Edition Engine are read in its own `_RES.TXT`
dialect, and whole standalone games made from Carnivores 2 can be played on
their own or combined beside the base game (see "Standalone games" below).
A mod combination can be shared as a
**recipe**: credits, download URLs and file hashes, never the mods themselves.

## Installing a mod
Put each mod in its own folder (for example under `source/mods/`). The folder
must contain the mod's `HUNTDAT` folder, either directly or inside one
subfolder. A `.zip` works too, laid out the same way inside, and needn't be
unpacked (other archive types, such as `.rar` or `.7z`, must be extracted
first). A mod shipped as a complete copy of the game is fine: files
identical to vanilla, executables, DLLs, logs and map-editor side files are
ignored.

### Loose add-ons
Most add-ons are shipped as loose files with instructions ("drop the `.car`
in HUNTDAT, paste this entry into `_RES.TXT` over the Spinosaurus, use our
hunter model"). Give such a folder as a mod as it is; it is installed the
way the instructions would have you do it, and the merge report says what
was decided:
- A folder holding `MENU`, `SOUNDFX`, `WEAPONS` or `AREAS` stands for
  `HUNTDAT`. Other files named like a game file replace it (`CALL7_A.WAV`);
  a new `.car` goes in `HUNTDAT`; `DINOn`/`CALLn`/`WEAPONn` menu files and
  call sounds go to their folders. Pictures and anything else are left out.
- The `{ ... }` entries are read from its text files (readmes included).
  An add-on with entries for several games: on a Modder's Edition base the
  one introduced as for the Modder's Edition is used, else the first.
  Trophy-room entries and "Legacy" folders are skipped; of a Modder's
  Edition folder and a plain one, the one matching the base is used.
- Menu art for slot n (`DINO7.TGA`) means the entry was made to replace
  that slot ("paste it over the Spinosaurus"). Instead it is **added in a
  slot of its own**, next to the original: its menu pictures, texts and
  calls are renumbered to the new slot, its slot conditions follow, and it
  is priced like the slot it was made for. `WEAPONn` the same for weapons.
  Exception: an animal on the original game's data (not the Modder's
  Edition) takes the slot over, since that engine places animals by AI
  number. The menu lists scroll with the mouse wheel; up to 22 animals.
  An entry named like an existing one changes it; anything else is added.
- A hunter model (`hunter...car`) carrying the add-on's kill animations
  is **merged**: the extra animations of every add-on's hunter are added to
  the game's hunter and each add-on's `hunterAnim` numbers are moved to
  match, so several such add-ons work together.
- Fix-ups: an entry naming a model the add-on doesn't ship uses its
  closest-named one; an entry for the original game that takes over a slot
  on a Modder's Edition base keeps that slot's spawn tables; an add-on with
  no entry at all (a skin pack) only adds its new files.

## Modder's Edition add-ons
Add-ons written for the Modder's Edition Engine need its data under them
(download the 1.1.1 release: <https://www.moddb.com/mods/carnivores-modders-edition/downloads/carnivores-modders-edition-111>).
Its release ships only the files it changes, to be copied over the
original game; give that folder as an `--overlay` and it is layered over
the game the same way (its `_RES.TXT` replaces the original's):
```
cargo run --release -p c2-pc -- play ../source/c2-original --overlay "PATH/Carnivores 2" --mod ADDON_DIR --mod OTHER
cargo run --release -p c2-pc -- mod build ADDON_DIR... --base ../source/c2-original --overlay "PATH/Carnivores 2" --out DIR
```

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

## Balance files
Prices and scores of your own, over any game (the launcher's Balance tab
writes one; add-on weapons are often too cheap next to the others):
```toml
start = 500                 # a new hunter's points

[[animals]]
name = "Manospondylus"      # as the menu lists it; a second "Raptor" is "Raptor #2"
price = 400                 # roaming add-ons have no price of their own
score = 45                  # basescore

[[weapons]]
name = "MP5 SD"
price = 180

[[areas]]
name = "Fort Ciskin"
price = 40
```
Only what you list changes; names the game doesn't have are skipped.
`c2-pc balance GAME [--overlay DIR] [--mod DIR]...` prints every current
value in this form. `c2-pc play ... --balance FILE` and
`c2-cook ... --balance FILE` apply it: the rewritten `_RES.TXT` /
`_MENU.TXT` / `.c2map` files go in a layer over the game, never into your
folders.

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

## Recipes
A recipe (`recipes/*.toml`, format in `recipes/example.toml`) names each mod
with its credit, URL and the SHA-256 of every file it uses, plus the load
order, explicit conflict winners and optional text patches of our own.

```
# You: make a recipe from mods you have (pins every file they use)
cargo run --release -p c2-pc -- recipe new MOD_DIR... --base ../source/c2-original --id my-pack --out ../recipes/my-pack.toml
# Anyone: download the listed mods into one folder, then
cargo run --release -p c2-pc -- recipe verify ../recipes/my-pack.toml --base ../source/c2-original --mods DOWNLOADS
cargo run --release -p c2-pc -- play ../source/c2-original --recipe ../recipes/my-pack.toml --mods DOWNLOADS
```
`verify` finds each mod by its `folder` hint, a folder named like its id, or
the folder whose files match the hashes, and lists every missing or different
file with the URL to get it. Only the listed files are used, so a recipe
builds the same game everywhere.

### Standalone games
Whole games made from Carnivores 2 (most need the Modder's Edition as the
overlay) are combined beside the base: each keeps its own areas, animals
(on its own maps) and weapons, renumbered after the base's; add-ons go on
top, on the base's maps.
```
cargo run --release -p c2-pc -- combine GAME_DIR... --base ../source/c2-original --overlay "PATH/Carnivores 2" [--mod ADDON]... [--ambients tied|random] --out DIR
cargo run --release -p c2-pc -- play ../source/c2-original --overlay "PATH/Carnivores 2" --standalone GAME_DIR... [--mod ADDON]...
```
A game folder can be the folder you extracted (the game may be a few
folders down). Games made for the original engine are read as dialect
games: each animal takes the animations and spawns of the base's animal
with the same AI. `COMBINED.md` in the result says what each game got.

In a recipe, `[[overlays]]` (the Modder's Edition release) and
`[[standalones]]` are listed like mods, with their files' paths in each
game's folder; `[combine] ambients = "tied"` (or `"random"`); `[[mods]]`
are then add-ons over the combined game, and patches apply to it:
```
cargo run --release -p c2-pc -- recipe new ADDON... --base ../source/c2-original --overlay "PATH/Carnivores 2" --standalone GAME_DIR... --id my-pack --out my-pack.toml
cargo run --release -p c2-pc -- recipe build my-pack.toml --base ../source/c2-original --mods DOWNLOADS --out DIR
```
`--mods DOWNLOADS` holds each download in a folder of its own; the overlay
comes from the recipe.

Loose add-ons work the same way (`loose = true` on the mod): their files are
listed by their path in the add-on's folder (the models, menu files, sounds,
the hunter model and the text files holding the entries; readmes without an
entry are left out), and the add-on is installed from those files only, so
a download with extra files in it builds the same game. A recipe of
Modder's Edition add-ons needs the same `--overlay` when it is verified and
built. `recipe build ... --out DIR` writes the
combined game to a folder instead of playing it.

- `[[resolve]] path = "HUNTDAT/..."`, `winner = "mod-id"`: that mod's copy of
  the file wins regardless of load order.
- `[[resolve]] entry = "characters:T-Rex"`, `winner = "mod-id"`: only that
  mod's change to the `_RES.TXT` entry applies.
- `[[patches]] target = "_RES.TXT"`, `patch = "patches/x.patch"`: a unified
  diff (`diff -u`, `git diff`) applied after merging; line endings don't
  matter and hunks may have moved. Patches must sit inside the recipe folder.

`base = "c2-vanilla"` is checked against `recipes/bases/c2-vanilla.toml`
(hashes of every vanilla `HUNTDAT` file, from `recipe base`). A different
game release only produces a warning.
A recipe for Carnivores: Ice Age as the base game says `base = "ice-age"`
(`recipe new ... --base-id ice-age`; `recipes/bases/ice-age.toml`, made with
`recipe base --base ICE_AGE --id ice-age`).

Ready-made recipes for the 3DS installs: `recipes/ice-age-android.toml`,
`c2-android.toml` and `c2-everything.toml` (their headers say how to verify,
play and cook each one for the 3DS). The mobile games' animals in them come
from your own APK (`c2-pc mobile`), so their hashes match only the same APK
release.

## Checking a mod's files
`c2-parity scan <dir>` parses every `.CAR`, `.3DF`, `.MAP`, `.RSC` and
`_RES.TXT` under a directory and reports anything malformed.
