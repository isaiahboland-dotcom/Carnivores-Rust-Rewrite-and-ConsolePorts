# Expansion features (design)

Planned features that let a mod go past what the original engines can do,
while its original release keeps working in them. Nothing here is
implemented yet unless it says so. Status is tracked in
[ROADMAP.md](ROADMAP.md).

## Ground rules

- **The original release stays untouched.** A mod's `_RES.TXT`, maps and
  models stay valid for its own engine. Everything only this engine reads
  ships beside them: in an add-on that `requires` the base game
  ([MODDING.md](MODDING.md), `mod.toml`), or in side files the original
  never opens.
- **No new keys in a Modder's Edition `_RES.TXT`.** Its parser matches
  keys by substring (`strstr`), so a new key containing an old one (say
  `skinfile`, which contains `file`) would be misread by that engine, and
  an unknown `{ }` block ends the entry early. New data goes in files of
  its own.
- **Vanilla behaviour by default.** A game without expansion data plays
  exactly as in its own engine; the regression scenarios hold it to that
  ([REGRESSION.md](REGRESSION.md)). Player-facing changes are launcher
  options, off by default.

## Spawn groups at random by default

Today, as in the Modder's Edition engine, a spawn group without
`randomise = TRUE` takes its entries in turn by ratio, so a rare skin
appears only once the group has spawned enough animals
([MODDING.md](MODDING.md), "Skins and rarities").

Flipping the default for every game would change how existing releases
play compared with their own engine. Instead:

- A game opts in through its expansion: `[engine] spawn_groups = "random"`
  in `mod.toml`. Every group is then randomised unless it says
  `randomise = FALSE`. That key is already valid in the Modder's Edition
  dialect, so the base release can carry it unchanged.
- Players get the same switch as an instance rule (launcher, Advanced).

## Skins with their own animations and sounds

Already possible: each skin is a character entry with its own `.car`
(animations and sounds) and its own `killtype` / `deathtype` blocks, so
males and females can kill, die and call differently today.

Planned: a compact skin list per animal, so one entry carries several
skins instead of one copy of every value per skin. Each skin names its
model and may change any value of the entry it belongs to (speeds, kill
types, sounds), and has its own rarity and conditions:

```
skin {
  file = 'models/stego_alb_f.car'
  spawnratio = 0.05
  overwrite area3 area5 { spawnratio = 0.2 }
}
```

A skin counts as its animal for trophies, calls and the menu. Which skin
was shot is kept with the trophy (the save format has spare fields).

## More animations per model, joined by name

A `.car` stores its animation and sound counts in its header, so it can
hold more than 64 of each; the fixed part is the 64-slot table at the end
of the file that says which sound each animation plays. Planned:

- An entry may name extra models: their animations and sounds are
  appended to the main model's, their sound tables re-pointed at the
  appended sounds, and the table grows past 64. The extra models must
  have the same vertex count. The hunter-model merge already does this
  for kill animations (`c2-mods/src/hunter.rs`).
- Animations may be named instead of numbered (`runAnim = 'Run_limp'`):
  `.car` animations carry names, so a second model's labels match up
  without anyone counting slots.

## More objects per area

**Implemented on PC** (hunt, area viewer, VR, regression runs). The
`.MAP` stores one byte of object per cell, with two values reserved, so an
area holds at most 254 kinds of object. The `.RSC` itself has no such
limit, but the original engines stop at 256 models.

- **Side files** beside the area, which only this engine reads:
  `AREAn.OBX` (two bytes of object per cell, plus the object heights) and
  `AREAn.RSX` (models numbered after the `.RSC`'s). Formats are in
  `c2-formats/src/obx.rs`. The `.MAP` and `.RSC` stay as they are, and the
  original engines go on using them.
- **Authoring** with the existing map editors: save the same area two or
  four times, each copy with up to 254 kinds of object, then join them:
  ```
  c2-pc area-layers AREA2.MAP --layer AREA2B.MAP --layer AREA2C.MAP --out DIR
  ```
  Each layer's models are numbered after the ones before it; a model
  identical to an earlier one reuses its number, so trees every copy
  shares are stored once. Where a layer puts a different object on a
  cell that already has one, the later layer wins and the command counts
  it. Each object keeps its own layer's object height. Landing markers
  come from the base only.
- **In memory** object indices are 16-bit everywhere (`c2_formats::map`:
  `NO_OBJECT`, `LANDING`); the `.MAP` writer stores indices past 253 as no
  object, so such an area needs its side files.
- **Not yet:** 3DS packs and the Wii U keep the `.MAP`'s objects (the
  cooker packs the `.MAP` and `.RSC` only), as their memory budgets would
  need checking first.

## Event reactions

Animals that react to what happens around them with an animation and
sound of their own: a herbivore that finds a dead one of its kind and
mourns, a carnivore that roars over a kill, a herd that freezes at a
distant shot. Off by default (launcher, Advanced: "Event reactions"),
and only animals given reactions by their game or an add-on use them.

```
reaction {
  event    = 'sees_dead_same_species'
  anim     = 'Mourn'
  sound    = 'mourn.wav'
  chance   = 0.5
  radius   = 3000
  cooldown = 60
}
```

- **Events** (first set): sees a dead animal of its kind, sees one of its
  pack killed, hears a shot within range, sees the hunter, smells a
  carnivore, finishes eating, the time of day changes.
- **Effect:** plays the animation (by name, from the animal's models,
  see above) and the sound, then returns to what the AI was doing.
  Reactions only replace idle and wandering behaviour, never fleeing or
  attacking, so hunting stays the same.
- **Determinism:** reactions draw from the hunt's seeded random numbers,
  so regression runs with reactions on repeat exactly.
