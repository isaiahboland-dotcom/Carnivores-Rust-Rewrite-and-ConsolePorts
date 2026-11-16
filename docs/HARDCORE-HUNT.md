# Hardcore hunt

An opt-in ruleset for hunts, set per instance in the recipe editor's Advanced
page ("Hardcore hunt"). It is **off by default** and only the launcher turns it
on. With the DHC OS terminal, a hardcore hunt is its own entry under the
regular hunt setup, and the regular hunt never plays the ruleset. With the
section absent or off, nothing differs. Each rule is independent, so a recipe can take any
subset. Not part of the original game; Play Vanilla leaves the whole section
out.

### Where it starts

- **DHC OS terminal:** main menu `HUNT SETUP` (regular), `HARDCORE HUNT` (this
  ruleset; same setup tabs, with the ruleset's gear bans and weapon cap), and
  `SURVIVAL` (the survival modes). The hardcore entry appears only when the
  recipe turns a rule on.
- **Original menu** (terminal off, or "Classic menu"): it has no second slot,
  so an enabled ruleset applies to every hunt there.
- `c2-pc --hardcore` starts a hunt with the ruleset from the command line.

Hardcore *survival* (the endless-night survival kind) is a separate, existing
mode and is not changed.

Status: implemented on PC (and Android, which runs the PC game; no touch
buttons yet). 3DS, Wii U and VR read the rules file and ignore the `hunt`
lines, so hunts there are regular hunts; `c2_mods::validate` warns. Play
testing is pending ([testing-needed.md](../testing-needed.md)). Decisions are
recorded in [DECISIONS.md](DECISIONS.md) ("Hardcore hunt ruleset").

## Recipe

```toml
[settings.hunt]
enabled = true
no_night_vision = true
no_radar = true
no_cover_scent = true            # the flashlight is always kept
max_firearms = 2
ammo_drops = 3                   # placed from the hunt's seed
hesitation_s = 10
back_angle = 90                  # degrees; past this the back is turned
extraction = { arrive_s = 5, hover_s = 30, cooldown_s = 30, min_distance = 48 }
experimental_knife = true        # unfinished: the knife needs this switch too
knife = { dodge = true }
second_predator = { window_s = 30, min_price = 300 }

[[settings.hunt.predators]]      # per species; unlisted: see "Reactions"
name = "Velociraptor"
reaction = "circle_lunge"        # or "circle_eat", "ignore"
```

The build writes it as `hunt <key> <values>` lines in `HUNTDAT/_RULES.TXT`
(`c2_formats::hardcore`). Engines skip `hunt` lines they do not know.

## Rules

### Equipment

- `no_night_vision`, `no_radar`, `no_cover_scent`: the hunt menu does not take
  the item and the hunt starts without it. The flashlight is always available.
- `max_firearms`: at most this many weapons picked in the hunt menu; a hunt
  started another way keeps the first ones. The knife does not count.

### Drop-off and pickup zones

With `extraction`, the hunt opens on the map and nothing moves until two spots
are picked: the **drop-off** (the hunter is put there) and the **pickup** zone
(6 cells in radius). Both snap to the nearest dry ground. The pickup must be at
least `min_distance` cells (48: about 190 m) from the drop-off. Dangerous
animals standing within 30 cells of the chosen drop-off are moved out to that
distance, as the original keeps them from the landing.

### Extraction

1. The player calls the ship from anywhere with **G**, or with **Esc** while standing in the pickup zone (outside the zone Esc opens the exit prompt, which forfeits).
2. It flies to the pickup zone, taking `arrive_s` (5 s).
3. It hovers for `hover_s` (30 s). If the player is inside the zone at any
   moment while it hovers, the player escapes: the hunt ends as a normal exit,
   with trophies and points kept.
4. If the hover ends with no player, the ship leaves. The cooldown
   (`cooldown_s`, 30 s) starts the moment it leaves.

The ship cannot be called while it is collecting a trophy; trophy pickups wait
while it flies an extraction. It is the game's own pickup ship model and
sounds.

**Leaving by the exit prompt forfeits the hunt**: it ends as a death does
(this hunt's trophies and points are discarded). The prompt says so.

### Ammunition drops

`ammo_drops` supply drops lie 24 to 140 cells from the drop-off, on dry ground,
at places drawn from the hunt's seed. Walking over one adds one full load of
rounds to every carried weapon's reserve. PC shows them on the map and as a
marker with its distance within 48 cells. They have no 3D model yet.

### Knife (experimental; will be completed later)

Off unless `experimental_knife = true` is set (the launcher's "Enable the
experimental knife"), even with the ruleset on. A `knife` section alone is
ignored. See "Deferred".

- In hand when every carried firearm is empty (chambered, reserve and spare
  magazine all zero). Fire stabs: the nearest animal within reach in front of
  the hunter loses 2 health.
- Knife kills give **no points and no trophy**, do not count toward the success
  statistic and do not call the ship.
- The knife is built into the engine, not a `_RES.TXT` weapon, so saved trophy
  weapon indices are unchanged. PC draws a blade in code; there is no model or
  sound.
- `dodge` (PC: **F**, with a strafe key for the side; right otherwise): a
  2-cell side-step. No strike lands for 0.6 s; the next dodge is 3 s later.
  Animals that were attacking circle again for half their time.

### Predator hesitation

A hunting carnivore that comes within its circling distance of the hunter
(about 5 to 14 cells, by its size) does not attack at once. For `hesitation_s`
(10 s) it circles him, slower than it runs. It attacks when:

- the hunter's back is turned to it: the angle between his view direction and
  the direction to the animal is more than `back_angle` (90 degrees); or
- its time is up.

| Reaction | With the hunter's back turned |
|---|---|
| `circle_lunge` | it lunges: goes for him at 1.5 times its running speed |
| `circle_eat` | it attacks at its normal speed with its normal kill |
| `ignore` | no hesitation: the original attack |

A lunge kills as the animal's normal attack does, unless dodged.

#### Reactions

The recipe editor lists the instance's animals with a reaction each. Unlisted
animals: carnivores that hunt the hunter get `circle_lunge`; the T-Rex and any
carnivore whose model reaches further than 900 units get `circle_eat`;
everything else gets `ignore`.

### Second predator

Inside `window_s` (30 s) of an encounter starting, or while one is under way,
another carnivore that reaches the hunter attacks at once if its hunt-menu
price is above `min_price` (300). Otherwise it hesitates like the first, and
attacks when the hunter's back is turned to it.

With the original game's prices (10 to 500), only the T-Rex (500) is above
300; the Ceratosaurus costs exactly 300 and hesitates.

## Implementation

- `c2_formats::hardcore`: the rules-file lines. `c2_mods::hunt`: the recipe
  section.
- `c2_core::dino::hesitate`: hesitation. No AI is changed: while an animal
  circles, the herd step hands its AI a waypoint on a circle round the hunter
  (chased, never struck). `HunterView::back_turned_to` is the back-arc test,
  to be shared with [BEHAVIOR.md](BEHAVIOR.md)'s stalking.
- `c2_core::game::hardcore`: zones, extraction, drops, knife and dodge;
  `Hunt::start_hardcore` turns the ruleset on.
- `c2-pc/src/hunt_view/hardcore.rs`: map picking, keys and HUD.
  `c2-launcher/src/recipe_edit/hunt.rs`: the editor panel.

## Deferred

The knife is unfinished and stays behind its own switch until it is completed:

- a knife model and sounds;
- knife kills that are replaced by a new animal, as firearm kills are;
- a stamina cost for the dodge;
- gamepad and touch bindings for the knife and the dodge.

## Not done

- 3DS, Wii U and VR; gamepad and touch bindings for the ship and the dodge.
- A model and sound for the drops.
- Multi ammo: a drop adds rounds of the kind loaded.

## Regression tests

- `c2-formats` `hardcore::tests`: rules-file round trip, unknown lines skipped,
  clamping, defaults.
- `c2-mods` `hunt::tests`, `classic::tests`, `validate::tests`: recipe to rules
  file, off writes nothing, mistyped keys reported, Play Vanilla omits the
  section, range, name and platform warnings.
- `c2-core` `dino::hesitate::tests`: back arc, circling and timeout, each
  reaction, second predator above and at the price and outside the window,
  dodge, renumbering.
- `c2-core` `dhc::tests`: the hardcore and survival entries appear only when
  offered, the hardcore setup's own limits, a regular setup without them,
  survival's modes.
- `c2-core` `game::tests::hardcore`: zones before anything moves, the ship's
  timings, escape at any moment of the hover and not after it, no call during
  the cooldown, forfeit, equipment, drops from the seed, knife only at zero
  ammunition, knife kill scores nothing, dodge, and a Velociraptor circling in
  a whole hunt.
