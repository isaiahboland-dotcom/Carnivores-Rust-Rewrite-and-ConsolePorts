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

## Add-on variants sharing a menu animal (implementation contract)

**Status:** the launcher warns about duplicate add-on dinosaur names in red.
Recipe persistence, private asset/character preservation and per-map runtime
selection are implemented for PC hunts. Explicit recipe groups retain all members
and one shared menu slot; ordinary duplicate names still use replacement behavior.
Initial placement, replacement spawns, predation respawns and survival waves share
the seeded selector. Dedicated editor controls and percentage previews remain pending.

Identity and persistence:

- A menu animal has an explicit stable group ID. Equal display names are a
  warning, never implicit consent to group or rename animals.
- A variant is identified by recipe source ID plus source character ID (the
  trimmed, unquoted, ASCII case-folded name used by `res_merge::block_id`).
  Collections additionally need a stable child source ID. Ambiguous references
  must fail preparation with the matching candidates listed; load order and
  character array indices are not persistent IDs.
- Store explicit groups and members in recipe `[[variants]]` tables; write a
  versioned `HUNTDAT/VARIANTS.TOML` side file when building. Retain source
  character blocks and namespace colliding asset paths before merging. Do not
  add expansion keys to the source `_RES.TXT`. Old recipes keep replacement
  behavior and produce no variant side file.
- A group specifies its menu owner and each member's `source`, `character`,
  `weight` (default 1), and optional `maps`. An omitted map list permits all
  maps; an empty list permits none. Map identities use source ID plus source
  area ID, resolved to packed areas during preparation. Missing members, maps,
  or owners are errors, including members removed through Pick parts.

Probability and eligibility:

- Choose a variant for each individual spawn, after selecting the menu animal;
  variants do not multiply its population or create additional spawn groups.
- Filter members by current map, then normalize their finite, nonnegative
  weights: `P(member | animal, map) = weight / sum(eligible weights)`.
  Zero disables a member. Negative, NaN and infinite values are errors.
  Weights need not add to 100; the UI displays the resulting percentages.
- Per-map weights override the member's default before normalization. A zero
  total means the animal cannot spawn on that map; do not substitute an
  ineligible member or bypass the rule through the selected-animal fallback.
- Use the hunt's seeded RNG and a stable persisted member order. Initial,
  replacement and survival spawning must use the same selector. With no
  variant data, preserve existing RNG consumption and spawn behavior.

The authoring format (edit the recipe TOML, then build/play it on PC) is:

```toml
[[variants]]
id = "rex"
menu_owner = "classic"

[[variants.members]]
id = "classic"
source = "classic-rex"
character = "Tyrannosaurus"
weight = 75

[[variants.members]]
id = "striped"
source = "striped-rex"
character = "Tyrannosaurus"
weight = 25
maps = [{ source = "base", area = "area1" }]
map_weights = [{ map = { source = "base", area = "area1" }, weight = 75 }]
```

On `base:area1` these members have equal probability; elsewhere only `classic`
is eligible. Group and member IDs use lowercase ASCII letters, digits, hyphens
and underscores. Character names retain source spelling; identity comparison
uses the normalization described above. Source IDs are recipe IDs. A collection
member additionally specifies `child`, the exact child folder name, independent
of its generated load-order number.

Current boundaries:

- Members must be huntable land characters (source AI 10–29), supplied by an
  add-on in `mods`. A model-only replacement may inherit its unchanged character
  definition from the base. References to absent or ambiguous sources/characters
  fail the build. Base and standalone characters are not directly addressable
  as members in this first implementation.
- Referenced files, including models in nested conditional blocks, are copied
  under `HUNTDAT/VARIANTS/<group>/<member>/`. Embedded textures, animations and
  sounds remain in the member's CAR. The menu owner's numbered art and call
  recordings are copied to the shared slot after layering, so a later member
  cannot replace them.
- Eligibility supports base maps and direct add-on maps whose area file stems
  are retained. Source map references must exist. Renumbering with Pick parts
  (`recipe.skip`) is explicitly rejected for variant packs. Standalone source
  map remapping is not supported. Changing a member's model through a conditional
  overwrite is rejected at load if it differs from its side-file binding; use
  separate members with map eligibility instead.
- Version 1 of `VARIANTS.TOML` stores validated character/model bindings, rule
  order and stable trophy tokens. The PC loader rejects stale bindings, missing
  member models and unknown versions. The cooker rejects these packs for 3DS
  and Wii U; the Wii U folder loader also refuses the side file.
- New trophies use a tagged identity in the existing save spare fields, derived
  from group/member identity and model content. On reload, character indices
  are recovered from that identity. A missing variant stays recorded but hidden,
  and reappears if its model returns. Original untagged trophies keep their
  original index interpretation. Members share the menu owner's base score.

Calls, trophies and review requirements:

- All members answer the group's call and count as its selected menu target;
  keep their own AI, model, animations and sounds. The menu owner's call art
  and call recording provide one unambiguous picker entry.
- Trophies count and score as the group's animal while retaining the variant
  ID and model identity for display. Existing trophies must retain their
  original interpretation; do not reinterpret saved character indices after
  a recipe changes. Tagged variant trophy remapping and missing-member retention
  implement this for new variant trophies.
- The editor must show effective percentages per map and reject ambiguous or
  invalid configuration before saving/building. Round-trip the recipe through
  editing and pinning. Test duplicate asset paths, map renumbering, exclusions,
  all-zero groups, calls, trophy reloads, seeded selection, and unchanged
  ordinary recipes. Unsupported platform consumers must reject variant packs
  explicitly until they can load the side file.

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
