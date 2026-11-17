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
first). ZIP limits and generated-install recovery are documented in
[Security hardening notes](SECURITY-HARDENING.md). A mod shipped as a complete copy of the game is fine: files
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
  A folder with its files under `HUNTDAT` and no `_RES.TXT` of its own is
  read the same way when a readme beside it holds the entry.
- An entry written for an older Modder's Edition (`clone`, a `spawninfo`
  with its own `spawnmax` / `spawnrate`, `idleAnim`) is read in today's
  keys; the merge report says so.
- An add-on with entries for several games: on a Modder's Edition base the
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

### Skins and rarities
In the Modder's Edition dialect a skin is a character entry of its own
(its own `.car`), and everything about where and how often it spawns is
written with spawn groups and conditional blocks. This engine reads them
as the Modder's Edition engine does:
- **Rarity** is `spawnratio` within a `spawninfo { spawngroup = N }`: a
  skin with 0.05 beside one with 1 is picked about 1 time in 21.
- **Chance or fixed share.** A spawn group with `randomise = TRUE` picks
  each animal by ratio at random. Without it, entries are taken in turn,
  each once its accumulated ratio reaches 1, so which skins spawn depends
  only on how many the group spawns that hunt. A rare skin that should
  sometimes appear and sometimes not belongs in a randomised group.
- **Map-exclusive skins:** put the skin's `spawninfo` inside
  `overwrite area3 { ... }` and give it none outside, and it spawns on
  area 3 only. `overwrite area1 area4 { ... }` applies on either area.
- **Different rarities per island:** one `overwrite` per area (or set of
  areas), each with its own `spawninfo` and `spawnratio`. An `overwrite`
  replaces the earlier `spawninfo` blocks; an `addition` adds to them.
- **Conditions combine** as in the engine: areas listed together are
  either-or, as are times and `charN` (picked menu slots); different kinds
  must all hold (`overwrite area2 time2` = area 2 at night). Matching is by
  substring, so `area10` also matches area 1 (the tenth area is `area0`).
- **More than 128 entries.** The Modder's Edition engine stops at 128
  character entries. This engine has no such limit; a game that needs more
  can ship the extra skins as an add-on that `requires` the base game.

To see what a game's tables come to without playing, `c2-parity spawns`
places each area and time of day many times (seeds 1..N, the first
landing, by default every menu animal picked) and prints, per character,
the share of hunts it appeared in and its mean count:
```
cargo run --release -p c2-parity -- spawns GAME_DIR [--areas 1,2|all] [--times 0,1,2|all] [--picked slots|all|none] [--hunts 200]
```
A `-` means the character never spawned there: a map-exclusive skin shows
`-` in every other area's column.

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

## More than 254 kinds of object in an area
An area made for this engine can go past the `.MAP`'s 254 kinds of
object with two side files beside it, `AREAn.OBX` and `AREAn.RSX`,
which the original engines never open. Make the extra objects in copies
of the area with any map editor, then join them:
```
cargo run --release -p c2-pc -- area-layers AREA2.MAP --layer AREA2B.MAP --layer AREA2C.MAP --out DIR
```
and ship the two files from `DIR` in the mod's `HUNTDAT/AREAS/`. Details
in [EXPANSION.md](EXPANSION.md) ("More objects per area").

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

`[recipe] schema` is the recipe file format, an integer (missing means 1). A
recipe with a newer schema than the build supports is refused with both
numbers named; update the game to load it. `[recipe] version` is your own
version of the pack, free text that is never compared. Keys the build does not
read are ignored with a warning (in the log, and in `c2-cook`'s output), so an
older build can still load a recipe written for a newer one; a mistyped key
(`max_distnace` under `[settings.flee]`) shows up the same way. A top-level
`[missions]` table is reserved: it is accepted, reported as not implemented,
and ignored. Missions are written as
[`[[settings.world.missions]]`](#missions).

The launcher saves a recipe by editing the file in place: comments, key order
and blank lines stay as written, a changed value keeps the comment beside it,
and a removed entry or section goes with the comments inside it. Keys the build
does not read are not kept when it saves.

Building a recipe also checks its settings and warns, without changing the
build, about values outside their range (they are clamped), names the built
game does not have (areas, animals, weapons; those entries are skipped) and
features the target platform lacks (variants on 3DS and Wii U, custom flee
limits on Wii U and VR). Each warning names its key, for example
`settings.balance.animals[1].name`.

A recipe can also hold the choices made for its game, so the launcher's
Instances tab can rebuild it later exactly as it was:
```toml
[settings]
menu = "mobile"              # mod, original, ice-age, mobile, game:<standalone id>
three_ds_target = "o3ds"     # or n3ds

[settings.pictures]          # auto (default), pc or mobile
scope = "mobile"

[settings.tuning]            # auto: the phone's only on an APK base alone
shotgun = "pc"
```
Unset values keep the launcher's choice. The launcher lists every recipe
under `recipes/`, any folder deep. `recipes/instances/testing/` is ignored
by git, for your own.

### Advanced Dinosaur Behavior and AI

An advanced instance setting (launcher: the recipe editor's Advanced tab)
for animal behavior beyond the original game. Every part is opt-in; with
none turned on the animals behave exactly as in the original. Predation
and awareness, below, are the parts implemented today. The planned parts
(archetypes, senses, needs and migration, herds, species relationships,
player trust) are specified in [BEHAVIOR.md](BEHAVIOR.md).

#### Predation (carnivores hunt herbivores)

Off unless the recipe turns it on. Not part of the original game, and
experimental everywhere: on maps where many animals spawn close together,
chases and fights pile up and get chaotic. It is tested on PC; on the 3DS,
Wii U, Android and VR it may also cause lag or game-breaking bugs.
```toml
[settings.predation]
enabled = true
priority = "prey"            # "hunter" (default): animals only while not after you
carnivore_fights = true      # carnivores hunt carnivores at least 10% cheaper
apex = "T-Rex"               # one animal above every other; the rest by price
# chain = ["T-Rex", "Spinosaurus", "Velociraptor"]   # or the whole order, top first

[[settings.predation.animals]]
name = "Velociraptor"
scavenger = true             # eats once from dead herbivores it senses, then runs

[[settings.predation.animals]]
name = "Triceratops"
diet = "herbivore"           # or "carnivore"; omitted: from its AI
defensive = true             # charges carnivores below it for 5-10 s
```
- The food chain is the hunt-menu price (with any balance edits), unless
  `apex` or `chain` is given. Animals not in `chain` rank below it, by price.
- A carnivore hunts herbivores at or below it. Within 10% of each other, the
  cheaper carnivore backs off; at the same price they ignore each other
  (so do two unpriced ones, such as roaming add-ons). Two after one prey:
  the clearly higher one, else the closer one.
- Animals notice each other by their own sight, smell and hearing values, so
  keen prey can run before a predator sees it. Prey runs from a predator
  that is after it, or one ready to hunt nearby (16 cells); one that is
  eating, resting or chasing something else scares it only within 4 cells.
- A predator eating a kill stops when it is shot, hears a shot or notices
  the hunter.
- A body left by a predator is not a trophy and scores nothing. After a
  minute, once you can't see it, it comes back as a new animal of its kind
  somewhere out of sight, so the map's population stays the same. Animals
  you kill don't come back.
- Names are `_RES.TXT` names, case ignored. The built game gets
  `HUNTDAT/_PREDATION.TXT`, which every platform reads; survival and the
  trophy room ignore it.

#### Awareness (suspicion, investigating, searching)

In the original an animal is either calm or alarmed, and an alarmed one
knows exactly where the hunter is for as long as the alarm lasts. Awareness
(experimental; PC and Android) puts states in between. It is off unless the
recipe turns it on, and Play Vanilla leaves it out.

```toml
[settings.behavior.awareness]
enabled = true
suspicion = 160          # half-notices up to this percent of its alarm threshold (100-300; default 160)
investigate_after = 2    # seconds of suspicion before it comes to look (0-30; default 2)
search = 20              # seconds a predator searches; the longest a look lasts (0-300; default 20)

[[settings.behavior.awareness.animals]]
name = "Velociraptor"
search = 40              # unset keys inherit the global value, then the default
```

- **Suspicious:** a calm animal whose sight or smell comes close to its
  alarm threshold, without reaching it, half-notices the hunter. Standing
  up, moving closer or the wind turning can still alarm it outright.
  `suspicion = 100` turns this off for an animal.
- **Investigating:** after `investigate_after` seconds of that, it walks to
  where it noticed him. A herbivore stops 12 cells short; a carnivore goes
  all the way. It is still calm: a look ends when it gets there or after
  `search` seconds.
- **Pursuing:** alarmed, as the original. A carnivore chases the hunter
  while it can see or smell him. When it cannot (he is behind cover, or too
  far), it runs to where it last sensed him.
- **Searching:** there, or after 8 seconds without sensing him, it works
  round that place in widening steps, 3 to 10 cells out. Sensing him again
  resumes the chase; after `search` seconds it gives up and calms.
- **Crouching** (stealth crouch) makes the hunter harder to notice only
  until he is noticed. An alarmed carnivore's senses are tested as if he
  stood, so crouching never ends a chase or a search; breaking its line of
  sight does.
- A shot heard by an alarmed animal moves its idea of where the hunter is
  to where the shot came from.
- The big land animals take part: the stock herbivores and carnivores, and
  a Modder's Edition game's hunting AIs and T-Rex. Ambient animals, fish,
  flyers and brachiosaurs, and a survival wave's animals, behave as before.
- Names are `_RES.TXT` names, case ignored. The built game gets
  `HUNTDAT/_BEHAVIOR.TXT`. The 3DS, Wii U and VR do not read it yet.

### Instance rules (times of day, locked settings, spawn rules)

Also on the recipe editor's Advanced tab (Settings > "Show the Advanced tab
in the instance editor"), with the base game's parts (`[recipe.skip]`).
Nothing set: the game plays as it was.
```toml
[settings.rules]
times = ["day", "night"]         # times of day the hunt menu offers; empty: all three
hidden = ["T-Rex"]               # in the game, but not in the hunt menu
scuba_gear = true                # offer scuba gear on the gear list (hidden by default)

[settings.rules.locks]           # fixed, and removed from the in-game settings
dynamic_time = false
advanced_observer = false
multi_ammo = true
mobile_radar = true
teleport = false
all_huntable = false
secret_dinos = 4                 # unpicked huntables per map (0: none)
survival_budget = 500            # starting survival points (0: from menu prices)

[[settings.rules.areas]]
area = "Vengar Peninsula"        # the area's name, as the balance editor lists it
max_huntable = 3                 # at most 3 of the picked animals spawn here

[[settings.rules.secrets]]
animal = "T-Rex"                 # an unpicked animal that always spawns...
count = 2                        # ...this many times, at random spots
areas = ["Vengar Peninsula"]     # empty: every area
```
- A hardcore instance: hide the carnivores from the hunt menu and give each
  area its own as secrets. Players pick herbivores and track the carnivores
  down.
- At least one time of day stays. Survival keeps its own time.
- A locked setting never overwrites the player's own value in the settings
  file, which every game shares; other instances still get the player's own.
- Secrets are placed where the original places its animals, at least 40
  cells from the hunter, after the game's own secret dinos are counted, so
  they always come. The area caps count the picked animals only.
- Areas and animals are named, not numbered, so the rules survive add-ons
  being reordered. `Raptor #2` is the second animal named Raptor. Names the
  built game doesn't have are skipped.
- The built game gets `HUNTDAT/_RULES.TXT` (areas by map name, `AREA3`).
  PC, Android, 3DS and Wii U read it. The 3DS and Wii U lock only the
  settings they have: mobile map, teleport, all dinos, multi ammo, advanced
  observer and a survival budget above 0, plus dynamic time on the Wii U.
  Secret dinos per map is PC and Android only. On the 3DS each secret animal is loaded
  like a pick and takes memory; on the Wii U it takes a model slot, if one
  is free.
- Ammo per weapon is on the Balance tab (`shots` in `[settings.balance]`).

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

Parts of a game can be left out: `--skip N=area2,dino1,weapon3` (repeatable)
for the `N`th game (from 1), numbers from 1 in that game's own menu order.
What is kept closes up (no gaps in the numbering); the left-out animals'
characters are not added, and their conditions go with them. The
launcher's "Pick parts" under each standalone game does the same. In a
recipe it is the standalone's `skip`:
```toml
[[standalones]]
id = "carnivores-triassic"
skip = { areas = [2], huntables = [1, 2], weapons = [1] }
```
The 3DS and Wii U take each standalone whole, as a pack of its own.

The base game's own parts can be left out the same way, with
`[recipe.skip]` (numbers from 1 in the base's menu order: the game with its
overlays, so the Modder's Edition's list when it is one). This is how an
instance swaps stock content for modded content: leave out a stock map,
animal or weapon and let an add-on or a standalone game stand in for it.
It works whatever the base is (Carnivores 2, the Modder's Edition, an APK
build or a standalone game on its own), with add-ons and with standalone
games combined beside it; it applies on PC, the 3DS and the Wii U. In the
launcher: the instance editor's Base game page, "The base game's parts".
```toml
[recipe]
id = "my-instance"
base = "c2-vanilla"

[recipe.skip]
areas = [2]       # the second stock area
huntables = [9]   # the T-Rex (the ninth stock animal)
weapons = [1, 4]  # the pistol and the X-Bow
```
What is kept is renumbered with no gaps; add-ons and standalone games come
after it, so their own numbers move up by as many as were left out. At least
one area must be left in the whole game (the build stops otherwise). A
left-out animal's character goes with it, also an add-on's change to it.
Files of left-out parts that carry no number (models) stay in the built
folder, unused.

In a recipe, `[[overlays]]` (the Modder's Edition release) and
`[[standalones]]` are listed like mods, with their files' paths in each
game's folder; `[combine] ambients = "tied"` (or `"random"`); `[[mods]]`
are then add-ons over the combined game, and patches apply to it:
```
cargo run --release -p c2-pc -- recipe new ADDON... --base ../source/c2-original --overlay "PATH/Carnivores 2" --standalone GAME_DIR... --id my-pack --out my-pack.toml
cargo run --release -p c2-pc -- recipe build my-pack.toml --base ../source/c2-original --mods DOWNLOADS --out DIR
```
`--mods DOWNLOADS` holds each download in a folder of its own; the overlay
comes from the recipe. `--mods` can be given more than once, and
`--mods ID=FOLDER` names the folder of one entry (`ID` its id) so nothing
else is searched. In the launcher, drop the recipe on the window: the Add-ons
tab lists each entry with its download page and takes a folder for it.

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

### Hardcore hunt

`[settings.hunt]` (the launcher's Advanced page) is an opt-in ruleset for an
instance's hunts: equipment limits, drop-off and pickup zones with extraction
by ship, ammunition drops, a knife, and carnivores that circle before they
attack. Each rule works on its own. PC only so far. The rules, their defaults
and the recipe keys are in [HARDCORE-HUNT.md](HARDCORE-HUNT.md).

### Flee limits (bounded fleeing)

An advanced instance setting on the recipe editor's Advanced tab, opt-in and
not part of the original game. **Bounded fleeing** itself is a switch in the
game's options (off by default); the recipe only sets the limits it uses.
Without a `[settings.flee]` section the built-in limits apply when a player
turns the option on. Experimental on 3DS and Wii U; PC is the tested platform.
```toml
[settings.flee]
enabled = true
max_distance = 24        # cells a flight may cover (4-200; default 24)
max_duration = 20        # seconds a flight may last (2-300; default 20)
chase_extension = 10     # seconds a still-pursuing hunter may add (0-120; default 10)
retrigger_lockout = 4    # seconds repeated noises cannot re-arm fear (0-60; default 4)

[[settings.flee.animals]]
name = "Parasaurolophus"
max_distance = 40        # unset keys inherit the global value, then the default
```
- Scared animals aim inside the map, clear of cliffs, water and prohibited
  areas; a flight ends at the distance or duration cap unless the hunter is
  still closing in, and then by at most `chase_extension`.
- Names are `_RES.TXT` names, case ignored. The built game gets
  `HUNTDAT/_FLEE.TXT`, which the PC and 3DS engines read.
- The launcher shows what the original does for each animal (no distance or
  duration limit; fear timers re-armed by closeness and noise) beside each
  field, whether or not custom limits are on.
- Carnivores that are chasing are never limited; fish, flyers and
  brachiosaurs keep their own movement.

### World zones, player starts and extraction points

`[settings.world]` holds shapes and points drawn on the areas' maps: zones
(circles and polygons), player starts and extraction points. It is an advanced
setting, opt-in and not part of the original game. Zones are used by the
[spawn ecosystem](#spawn-ecosystem); player starts and extraction points are
data for the planned missions, which no hunt uses yet.
```toml
[settings.world]
enabled = true
min_extraction_gap = 0.25    # least start-to-extraction distance, as a
                             # fraction of the map's side (0-1; unset: no rule)

[[settings.world.zones]]
id = "marsh"                 # one word: letters, digits, - and _ (at most 32)
area = "Vengar Peninsula"    # as the balance editor names it
shape = "circle"
center = [512.0, 300.5]
radius = 40.0                # 0.5-1024 cells
tags = ["no_spawn"]

[[settings.world.zones]]
id = "ridge"
area = "Vengar Peninsula"
shape = "polygon"
points = [[100.0, 100.0], [200.0, 100.0], [150.0, 220.0]]

[[settings.world.spawns]]
id = "beach"
area = "Vengar Peninsula"
at = [100.0, 200.0]

[[settings.world.extractions]]
id = "pad"
area = "Vengar Peninsula"
at = [300.0, 400.0]
```
- Coordinates are map cells as floats, `[x, z]`: world units / 256, the unit
  of the Modder's Edition `region{}` rectangles and of the flee limits. A map
  is 1024 cells a side. They are never picture pixels.
- A polygon has 3 to 64 points in order, either winding; the last joins the
  first. Its edges may not cross or touch, and it must enclose some area.
- Zone ids are unique in the recipe; so are start ids, and extraction ids.
  Tags are free words; `no_spawn` makes a no-spawn zone while the
  [spawn ecosystem](#spawn-ecosystem) is on, the others have no meaning yet.
- The recipe is where the entries are written and shared. The build derives
  `HUNTDAT/_WORLD.TXT` from it (one section per map), because packs on the
  consoles and Android never see the recipe; do not edit that file by hand.
- Entries that can't be used are left out of the build and reported as
  warnings: an area the game lacks, an id that is not one word or is used
  before, an unusable shape, a point with no position. A zone or point
  outside the map, and an area where no start is `min_extraction_gap` from
  an extraction point, are warned about and kept.
- Off, absent or empty: no `_WORLD.TXT`, and the build is unchanged. Play
  Vanilla leaves the section out.

#### Spawn ecosystem

`[settings.world.ecosystem]` decides which animals appear in which zone, and
how many. It is opt-in, experimental, and applies on PC and Android (the
validator warns for the other targets). Survival and the trophy room ignore it.
```toml
[settings.world.ecosystem]
enabled = true
max_animals = 40             # living animals on a map (0-255; unset: no limit)

[[settings.world.ecosystem.species]]
name = "Velociraptor"        # as the balance editor names it
max = 4                      # living at once, on any map

[[settings.world.ecosystem.zones]]
zone = "ridge"               # the id of a [[settings.world.zones]] entry
max = 6                      # living animals inside at once
contain = "hard"             # "hard", "soft", or unset
allow = ["Stegosaurus"]      # only these appear here (empty: any animal)
weights = [{ name = "Stegosaurus", weight = 3.0 }]

[[settings.world.ecosystem.zones]]
zone = "marsh"
no_spawn = true              # nothing appears here
avoid = true                 # wandering and fleeing animals keep out

# By time of day (dawn, day, night): only the keys given differ.
[settings.world.ecosystem.night]
max_animals = 20
species = [{ name = "Velociraptor", max = 8 }]
[[settings.world.ecosystem.night.zones]]
zone = "ridge"
max = 2
no_spawn = false
allow = []                   # given and empty: any animal
weights = [{ name = "Stegosaurus", weight = 0.5 }]

# How far from the hunter new animals appear (cells; unset: 40).
[settings.world.ecosystem.safety]
initial_distance = 40        # at the start of a hunt
respawn_distance = 60        # replacements for killed animals
extra_distance = 40          # the instance rules' extra animals
out_of_view = true           # replacements only where he is not looking
species = [{ name = "Tyrannosaurus Rex", distance = 80 }]
```
- The rules steer where the game's own placement puts an animal: at the start
  of a hunt, when a killed animal is replaced, when an ambient animal is moved
  ahead of the hunter, and for the instance rules' extra animals. A refused
  spot is one more spot the placement skips. Nothing already on the map is
  ever removed or moved, and no-spawn zones do not keep animals from walking in.
- A spot inside several zones must be accepted by each of them.
- Caps count living animals; a body does not hold a place. A cap that is
  already reached leaves the animal out (a replacement waits and comes when
  there is room). A species cap covers the species' skin variants together.
- `weights`: how readily an animal appears in the zone, against 1 anywhere
  else on the map (0-10; 0: never). `weight = 3` in one zone makes a spot
  there three times as likely to be taken as a spot outside it. With an
  `allow` list, animals not listed never appear in the zone.
- `contain = "hard"`: animals placed in the zone never cross its edge, even
  when fleeing or chasing (they slide along it). `"soft"`: they wander inside
  and may leave while fleeing or chasing; calm again, they drift back.
  Containment follows the zone an animal was placed in.
- `avoid = true`: animals do not head into the zone, wandering or fleeing
  (with Bounded fleeing on). One already inside may leave. It is not a wall:
  a chase, or a walk to a target on the far side, may still cross it.
  Flights with Bounded fleeing on also stay inside a `hard` zone by choosing
  another way, instead of pressing against its edge.
- **By time of day.** `dawn`, `day` and `night` tables hold what differs at
  that time: `max_animals`, per-species `max`, and per zone `no_spawn`, `max`,
  `allow` and `weights`. A key that is not given is inherited from the main
  rule; a weight or species cap replaces the one for that animal only.
  Containment and `avoid` do not change with the time. The rules of the
  current time apply to each animal as it is placed; with dynamic time that
  follows the clock, and a cap that drops at nightfall never removes an
  animal, it only stops new ones.
- **Spawn safety.** Distances are cells from the hunter, measured as the
  original does (x plus z); unset, each is the original's 40. A species'
  own distance is used when larger. Replacements are looked for within 78
  cells of the hunter, so a `respawn_distance` above about 70 finds few
  spots. `out_of_view` refuses replacement spots within 60 degrees of where
  the hunter faces and inside his view distance.
- Rules that can't be met never hang a hunt: each placement gives up after a
  bounded number of tries and the animal is left out. A replacement that
  finds no room or no spot is tried again every 5 seconds (up to 16 wait).
- A rule for a zone that is not drawn, a second rule for a zone, unknown
  animals and out-of-range numbers are reported as warnings.
- Off or absent, the build has no `eco` lines and hunts are unchanged. Play
  Vanilla leaves the section out with the rest of `[settings.world]`.
- In the launcher: recipe editor, Advanced page, **Spawn ecosystem**: one
  entry per zone drawn in the map editor, **Spawn safety**, and **At dawn /
  By day / At night** for the map cap and each zone's spawning and cap.
  Per-animal caps, allow lists and weights by time are written in the file.

**The map editor.** The launcher's recipe editor draws these for you:
Advanced page, **World zones: map editor**. Choose an area and it shows the
area's overview, the picture the game's map screen uses, read from the
instance's game (the base game must be set; add-ons without a folder yet are
left out).
- **Circle**: press at the centre and drag out to the rim.
- **Polygon**: click each corner in turn; click the first corner again, or
  **Finish**, to close it. An outline whose edges cross is refused.
- **Player start**, **Extraction point**: click the spot.
- **Select / move**: click an entry to select it, then drag its squares (a
  circle's centre and rim, a polygon's corners) or the point itself.
  - Drag inside a polygon to move the whole of it; it stops at the map's
    edges with its shape kept.
  - Drag from an edge of the selected polygon to add a corner there (the
    small dots mark the edges; a polygon holds 64 corners at most).
  - Right-click a corner to remove it. A polygon keeps at least 3.
- The readout under the map gives the cell under the pointer. The list below
  it holds the area's entries; the selected one's id, tags and numbers can be
  typed. Positions are kept to a tenth of a cell.
- The editor changes only the recipe (saved with the editor's **Save**, which
  keeps the file's comments). Entries on areas the instance does not have are
  kept and counted, not shown.

### Missions

A mission is a fixed challenge on one area: the recipe sets the time of day,
the animals on the map, the weapons and the equipment; the hunter enters by
one of the mission's entry points, makes the kills it asks for and leaves by
one of its exit points. Missions are part of `[settings.world]` and use its
player starts and extraction points.

**Status: editor and data only.** Missions are saved in the recipe, checked
and built into `HUNTDAT/_WORLD.TXT`; no game lists or plays them yet. They
will be listed by the [DHC OS terminal](#dhc-os-terminal-rules) only: with the
original menus, and on platforms without the terminal, an instance's missions
are hidden.

```toml
[[settings.world.missions]]
id = "rex-run"                   # letters, digits, - and _
name = "Rex run"                 # shown in the mission list; unset: the id
area = "Vengar Peninsula"        # the area's name, as the balance editor lists it
time = "night"                   # dawn, day (default) or night
entries = ["beach"]              # ids of [[settings.world.spawns]] on that area
exits = ["pad"]                  # ids of [[settings.world.extractions]] there
animals = ["Tyrannosaurus Rex", "Velociraptor"]
weapons = ["Shotgun"]
equipment = ["Radar"]            # as the DHC islands name it; none listed: none
time_limit = 600                 # seconds (30-86400); unset: no limit
requires = ["first-steps"]       # missions to complete before this is offered
kills = [
    { name = "Tyrannosaurus Rex", count = 1 },
    { count = 3 },               # no name: any of the mission's animals
]
```
- The lists are the mission's whole loadout: the player does not pick.
- A mission needs at least one usable entry and one usable exit on its own
  area; without them, on an area the game lacks, or with an id that is not one
  word or is used before, it is left out of the build and warned about.
- Also warned about and kept: an unknown time, animal, weapon or equipment
  name; no animals or weapons; no kill required (reaching an exit then
  completes it); a kill of an animal the mission does not list; a count or
  time limit out of range (clamped); a requirement that names no mission,
  itself, or a mission that requires it back; every mission requiring another
  (none offered at the start); with `min_extraction_gap`, no exit far enough
  from an entry; missions with `[settings.world]` off.
- Play Vanilla leaves the section out.

**The mission editor.** Recipe editor, Advanced page, **Missions**: add a
mission (it starts on the area the map editor shows, with every start and
extraction point drawn there), then set its name, area, time, entry and exit
points, animals, weapons, equipment, time limit, the missions that unlock it
and its kill requirements. Each mission lists what the validator finds wrong
with it. The panel warns when the DHC OS terminal is off, because the
instance's missions would then be hidden.

### Weapon restrictions

`[[settings.weapon_restrictions]]` fixes, per weapon, whether it may fire
tranquilizer darts and whether its kills count as trophies. Unset, a weapon is
as the game has it (both allowed). The build writes `c2rs_allow_tranq` and
`c2rs_allow_trophy` into the weapon's `_RES.TXT` entry; a name that matches no
weapon, or more than one, stops the build. Kept by Play Vanilla. The recipe
editor's Advanced page has a row per weapon (**Weapon restrictions**).

```toml
[[settings.weapon_restrictions]]
name = "Sniper rifle"
allow_tranq = false
allow_trophy = true
```

### DHC OS terminal rules

The **DHC OS terminal** is an optional in-universe front end (DinoHunt Corp)
for the main menu and hunt setup. Players turn it on in the game's options
or the launcher's Menu panel (**DHC OS terminal**, off by default). On, it
replaces whichever menu style is picked, the Android (APK) menus too. It is never shown in Play Vanilla, and
**Classic menu** in the terminal returns to the original menu until the game
restarts. It lists what the original menu lists, with the same pictures,
descriptions, prices and account, and starts the hunt the same way.

A recipe can give the terminal rules, by hand or in the recipe editor's
Advanced page (**DHC OS terminal rules**: the defaults, a row per animal and
a row per area). Without `[settings.dhc]` (or with `enabled = false`) nothing
is written and the terminal offers everything the original menu offers.
```toml
[settings.dhc]
enabled = true
discovery = "sighting"       # off (default), sighting, scan or kill
licence_cost = 200           # score points (0-100000; default 200)

[[settings.dhc.animals]]
name = "Velociraptor"        # _RES.TXT name, case ignored
discovery = "kill"           # unset: the default above
licence_cost = 500           # unset: the default above

[[settings.dhc.islands]]
area = "Vengar Peninsula"    # the area's name, as the balance editor lists it
animals = ["Parasaurolophus", "Pachycephalosaurus"]   # empty: every animal
weapons = ["Pistol", "Shotgun"]                       # empty: every weapon
equipment = ["Radar"]        # Camouflage, Radar, Cover Scent, Double Ammo,
                             # Night Vision, Flashlight; empty: all
times = ["dawn", "day"]      # empty: every time the instance offers
```
- Island lists restrict what the terminal issues on that area: other items
  show as RESTRICTED, and picks made elsewhere that the island does not allow
  are dropped on arrival. They do not change what spawns: that is
  `[settings.rules]` (`hidden`, `secrets`, area caps).
- `discovery` and `licence_cost` are read and validated now; secret-animal
  discovery and licences are a later phase and change nothing yet. Licences
  will cost the original score points, never a second currency.
- The built game gets `HUNTDAT/_DHC.TXT` (areas by map name, `AREA1`). PC
  and 3DS (`c2-cook` packs it; the 3DS applies the island rules in its
  terminal look only, not in the hexagon menu yet). Wii U and VR keep the
  original menu, and the build warns.
