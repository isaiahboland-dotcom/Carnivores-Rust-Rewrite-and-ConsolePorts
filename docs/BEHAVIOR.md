# Advanced Dinosaur Behavior and AI (design)

The launcher's advanced instance setting **Advanced Dinosaur Behavior and
AI** (recipe editor, Advanced tab) collects every animal behavior that goes
beyond the original game. Its first part, predation, is implemented
([MODDING.md](MODDING.md#predation-carnivores-hunt-herbivores)), and so is
awareness, a slice of phase 3 that came first
([MODDING.md](MODDING.md#awareness-suspicion-investigating-searching);
see "Awareness" under Phases). Everything
else here is a plan: nothing below is implemented unless it says so. Status
is tracked in [ROADMAP.md](ROADMAP.md); decisions go in
[DECISIONS.md](DECISIONS.md).

## Ground rules

- **Opt-in, per part and per species.** With the setting off, or with a
  part off, the animals run the original AIs unchanged. The regression
  scenarios hold vanilla to that ([REGRESSION.md](REGRESSION.md)). Play
  Vanilla leaves the whole section out ([RECIPE-SETTINGS.md](RECIPE-SETTINGS.md)).
- **The original AIs stay the base.** As predation does today, new
  behavior steers the existing AIs (the original's and the Modder's
  Edition's, Ice Age included) by giving them a target, a point to reach or
  a point to flee, instead of replacing an AI family. Animation clips,
  speeds and kills stay the species' own.
- **Explicit values win.** Resolution order for every value: the recipe's
  per-species value, then the species' archetype, then auto mode's guess
  (only when auto is on), then the original behavior.
- **Deterministic.** All random choices draw from the hunt's seeded random
  numbers, so a regression run with behavior on repeats exactly.
- **Side files only.** The recipe's section is written into the built game
  as a side file every platform reads; no new keys in `_RES.TXT`
  ([EXPANSION.md](EXPANSION.md#ground-rules)).
- **Not universal.** No behavior is applied to every species by default.
  Friendliness toward the player in particular is never applied to
  predators without an explicit per-species opt-in, and never by auto mode.

## Foundations already in the engine

| Foundation | Where | Reused for |
|---|---|---|
| Sight, smell and hearing per species (`_RES.TXT` `look`, `smell`, `hear`), wind, cover scent, camouflage, stealth crouch | `CheckAfraid` port, `game/wildlife.rs` | Senses (below); stance response |
| Noise alarms (`MakeNoise`) | `game/wildlife.rs` | Sound investigation |
| Dialect packs: leader, shared alert and attack | `dino/herd.rs` (`Pack`) | Herds |
| Animal-to-animal targets, food chain, scavenging, defensive charges, face-offs | `dino/predation*` | Relationships, pursuit, scavenging, territorial defense |
| Bounded fleeing (edge, cliff and water aware) | `dino/flee.rs` | Fleeing, retreat, migration paths |
| Event reactions (design) | [EXPANSION.md](EXPANSION.md#event-reactions) | Hesitation, mourning, warning displays |

## Recipe layout

`[settings.predation]` keeps its name and format, so existing recipes keep
loading; it is the predation part of this setting. The new parts go in
`[settings.behavior]`, written into the built game as
`HUNTDAT/_BEHAVIOR.TXT`. A draft (names may change before phase 1 lands):

```toml
[settings.behavior]
enabled = true
auto = false                  # guess archetypes for unconfigured species (see Auto mode)

[[settings.behavior.animals]]
name = "Parasaurolophus"      # _RES.TXT name, case ignored
archetype = "herd_grazer"
senses = { sight = 1.2, hearing = 1.5, smell = 1.0 }   # multipliers on the species' own values
needs = { thirst = true, rest = true }
habitat = { prefers = ["shore", "open"], avoids = ["steep"] }
herd = { with = ["Parasaurolophus", "Stegosaurus"], juveniles = ["Para Juvenile"], defense = "circle" }
friends = ["Stegosaurus"]     # herd, flee and defend together
foes = []                     # predation's food chain still applies
hunter = { start = "wary", eligible = true }   # the hunter relationship; see 2.2

[[settings.behavior.animals]]
name = "Velociraptor"
archetype = "pack_hunter"
capabilities = { stalk = true, ambush = false, search = true, retreat = true }
```

Per-species tables are optional; a species without one keeps its original
behavior unless auto mode is on.

## 2.1 Generalized creature behavior

### Archetypes

An archetype is a named set of default capabilities and values, drawn from
how the classic dinosaurs and the Ice Age animals already behave. A species
gets one archetype; any capability can then be turned on or off for it.

| Archetype | Based on | Default capabilities |
|---|---|---|
| `grazer` | Carnivores 2 herbivores that flee the hunter | flee, investigate (cautious), wander, needs |
| `herd_grazer` | the same, in dialect packs | as `grazer`, plus herd, juveniles, circle defense |
| `defender` | the original's Triceratops (charges when close) | territorial defense, flee when far, herd |
| `pursuit_predator` | raptor-family and Allosaurus AIs | pursuit, lost-target search, scavenging, retreat |
| `pack_hunter` | raptors in dialect packs | as `pursuit_predator`, plus herd and coordinated pursuit |
| `ambush_predator` | (new) | stalking, ambush, sound investigation, hesitation |
| `apex` | T-Rex AIs | pursuit, territorial defense, scavenging, no retreat |
| `ice_age_grazer` / `ice_age_predator` | the Ice Age AI family (`AiInfo::ice_age`) | as the C2 counterparts, with Ice Age movement kept |

Fish, flyers, brachiosaurs and ambients take no part unless a later phase
lists them; their AIs have no target to steer (as with predation today).

### Capabilities

Each capability is a small state with an entry condition, a target and an
exit. None plays a clip the species lacks; missing clips fall back to walk
or idle.

- **Flee:** as the original, through bounded fleeing when that option is on.
- **Cautious investigation:** a herbivore that half-notices something (a
  sense value below the alarm threshold) turns toward it, approaches up to
  a set distance, and flees on full alarm or returns to wandering after a
  timeout.
- **Territorial defense:** charges an intruder (another species on its
  foe list, or the player) inside a radius, then breaks off after a time,
  generalizing predation's defensive charge.
- **Pursuit:** the existing predation chase, with its 10-cell lead point.
- **Scavenging:** the existing predation scavenger, extended to any body
  (not only herbivores) when configured.
- **Stalking:** a predator that has sensed prey but is not seen by it walks
  (not runs), keeps to the prey's back arc, and stops when the prey looks
  toward it; it charges inside a set distance.
- **Ambush:** a predator waits near cover or water for a set time, still,
  and charges prey that comes inside a set distance.
- **Sound investigation:** a shot, a call or a fight heard but not located
  sends the animal to the sound's position, scaled by hearing.
- **Lost-target search:** a predator that loses its target goes to the
  last-known position, then searches in widening arcs for a set time.
- **Hesitation:** a short pause (idle or event-reaction clip) before
  committing to a charge or a flight; length set per species.
- **Retreat:** a predator below a health fraction, or outmatched (a face-off
  it loses), breaks off and moves away using bounded fleeing.

### Senses

Each species' detection uses its own `_RES.TXT` sight, smell and hearing
values through the original's formula, scaled by per-species multipliers
and an **alertness** value (how far below the alarm threshold the animal
still notices and investigates). Optional explicit **detection ranges**
in cells cap each sense. Without values the species keeps its original
senses. Animal-to-animal sensing keeps predation's cost limits (no look
traces; doubled distance for cover).

### Needs, travel and habitat

- **Hunger, thirst and rest** are slow meters per animal, filled by
  grazing (herbivores), a kill or a body (carnivores), standing in shallow
  water, and resting (idle clips). A low meter picks the next wander target:
  food, water or a resting spot.
- **Habitat preferences:** each area is classified once at hunt start
  (water and shore cells, open ground, dense objects, steep slopes, height
  bands) into a coarse grid, cached per area; animals pick targets in
  preferred cells and avoid avoided ones.
- **Migration:** a herd whose need is unmet nearby travels together to a
  preferred cell farther away. Travel uses walk speed and the flee path
  checks (edges, cliffs, water), never forced through blocked cells.
- **Wandering and resting** follow the original when no need is pressing.

### Herds

- **Membership:** the species itself, plus any species in its `herd.with`
  list (cross-species herds need the listing on both sides). Dialect packs
  keep their leader and shared alert and attack; species without packs
  form herds at placement from nearby members.
- **Juveniles:** a juvenile is a listed species (a smaller add-on or
  variant) or, when a species has none, a member scaled below a set size.
  Juveniles keep close to an adult, follow the herd, and on alarm run
  behind the nearest adult instead of away from the threat.
- **Circle defense:** when a threat (a foe species or the hunter) comes
  inside the defense radius and the herd has juveniles, adults face it in a
  ring around the juveniles, charge a threat that closes in (territorial
  defense), and the ring breaks into flight when an adult dies, the threat
  leaves, or a set time passes.
- **Coordination:** pack hunters share their target; members approach from
  different sides (spaced headings around the prey).

### Relationships

`friends`, `foes` and neutral (everything else) per species, by `_RES.TXT`
name, including modded species. These lists are for animal-to-animal
interaction only; the hunter has his own relationship (2.2).

- **Friend:** affects only who herds together, and who flees from or
  defends against danger together. Friends join one herd, share alarms,
  run from a threat as a group, and join a circle defense or charge
  alongside each other. Nothing else changes.
- **Foe:** a carnivore hunts it, a herbivore flees or charges it
  (by its archetype); this widens predation's food chain, which still
  applies to species not listed.
- **Neutral:** ignored, as in the original.

A one-sided listing applies only to the side that lists it. A pair listed
as both friend and foe, or friends where one hunts the other on the food
chain, is a recipe error the launcher reports.

### Auto mode

A convenience that guesses an archetype, senses and relationships for
species without explicit values, from the AI number (stock and dialect AI
tables), diet, size and hunt price. The launcher shows the guess greyed out
next to each species, with a warning: *guesses may be unrealistic for
unfamiliar modded creatures; set values explicitly where it matters.* Any
explicit value replaces the guess. Auto mode never enables player trust
and never turns a herbivore into a predator.

## 2.2 Player–creature relationships

### The hunter relationship

Separate from the animal-to-animal lists, each species can have a
relationship with the hunter, so a modder can make animals that like the
hunter's company. An animal is **wary** (the original behavior),
**trusted** or **friendly** toward the hunter. A species starts at a
configured level (`hunter.start`, default `wary`) and an eligible one can
move up through coexistence (below).

- **Attraction:** trusted and friendly animals are drawn to the hunter.
  They pick wander targets near him and keep within a follow band (default
  8 to 20 cells; friendly animals keep closer than trusted ones), without
  crowding him inside the band's inner edge.
- **Danger:** on alarm (a predator, a foe, a shot not from the hunter) they
  flee as usual. If the hunter moves the same way and stays within the
  follow band, they keep with him and settle near him once the danger has
  passed; if he does not follow, they run without him and return to
  wandering where they end up.
- **Herds:** an attracted animal takes its herd along only as far as the
  herd's own rules allow; the herd's friends keep their own relationship
  with the hunter.

### Coexistence and trust

An optional per-species behavior: sustained, nonthreatening proximity
moves an eligible animal from **wary** to **trusted** to **friendly**.

- **Eligibility:** only species with `hunter.eligible = true` (or a
  `hunter.start` above wary). Off for all species by default; auto mode
  never sets it; a carnivore needs `hunter.predator_ok = true` as well,
  and never becomes trusted or friendly while it is hunting the player.
- **Conditions (all needed while the timer runs):** within a distance band
  (default 6 to 24 cells), crouched (stealth crouch), no weapon fired,
  not running, no call used, and the animal not alarmed.
- **Time:** `hunter.time` seconds of uninterrupted conditions to reach
  friendly (default 300, the proposed 5 minutes); trusted at half.
- **Effects:** trusted: proximity alone no longer alarms it (shots,
  wounds and threat actions still do), and it is mildly attracted (above).
  Friendly: also no alarm from standing nearby or walking, and it keeps
  closer. Neither changes what it does when shot.
- **Scoring:** none. Killing a trusted or friendly animal scores and
  counts as a trophy exactly as any other kill of its species.
- **Threat actions:** firing within hearing, hitting the animal or a herd
  member, running inside the band's inner edge, standing up inside it,
  using a predator call, or a herd member being killed by the player.
- **Reset on threat (configurable):** `reset` (back to wary, default),
  `pause` (timer stops, keeps progress), or `decay` (loses progress at a
  set rate). Aggression (hitting or killing a herd member) always resets
  and marks the animal **hostile** (flees or, for defenders, charges
  sooner) for the rest of the hunt.
- **Persistence:** per animal, for the hunt (default). Cross-hunt
  memory per species needs a save format change and is a later option.

### Responses to stance, weapons, sound, scent and history

Per species, optional multipliers on detection and reaction:

- **Stance:** standing, crouched, in water.
- **Weapon:** holstered vs drawn (a shot is always a threat action).
- **Sound:** shots, calls (attractive or alarming per call), fights.
- **Scent:** with or without cover scent, upwind or downwind.
- **Previous interactions:** wounded animals and herds that lost a member
  react earlier and flee farther (or, for defenders, charge).

Missing values keep the original reaction.

## Platforms and cost

- **PC and Android:** every part.
- **PC VR:** follows PC once VR hunting has weapons.
- **3DS and Wii U:** a reduced set (senses, relationships, herds without
  circle defense, trust), no needs or migration; habitat grids only if the
  memory budget allows ([3DS.md](3DS.md)). Parts not supported are
  ignored, and the launcher says so per platform.
- Goals are re-picked on predation's 300 ms schedule; a spatial grid stays
  deferred until measurements need it ([PREDATION-PERF.md](PREDATION-PERF.md)).

## Phases

1. **Schema and relationships:** `[settings.behavior]`, `_BEHAVIOR.TXT`
   (parser, writer, round trip), archetypes as data, senses multipliers,
   friend/foe lists folded into predation's relation table, launcher panel
   rows. Auto mode's guess shown read-only.
2. **Herds:** cross-species membership, juveniles, circle defense,
   coordinated pack pursuit.
3. **Predator capabilities:** stalking, ambush, sound investigation,
   lost-target search, hesitation, retreat.
4. **Needs and habitat:** meters, area classification, migration.
5. **The hunter relationship:** starting levels, attraction and following,
   coexistence trust, and responses to stance, weapons, sound and scent.
6. **Auto mode** turned on as a setting, with the warning.

Each phase lands on PC first, with its regression tests, before console
support.

### Awareness (implemented ahead of phase 1)

`[settings.behavior.awareness]` and `_BEHAVIOR.TXT` exist with this one
part (`c2_formats::behavior`, `c2_mods::behavior`, `c2_core::dino::aware`):
suspicion, cautious investigation, a predator's last-known position and its
lost-target search, and stealth crouch that counts only before pursuit. It
covers phase 3's lost-target search and the first half of its sound
investigation (a shot re-points an alarmed animal; a calm one is still
alarmed by it as in the original). Phase 1 extends the same file and
section with archetypes and senses; the per-animal `animals` tables there
should fold these three values in rather than keep two lists. Stalking,
ambush, hesitation and retreat remain.

## Regression tests

- Vanilla unchanged: with the section absent or `enabled = false`, the
  state hashes of the published baselines are identical.
- `_BEHAVIOR.TXT` round trip; unknown keys reported; bad names warned (as
  recipe validation does today).
- Resolution order: explicit beats archetype beats auto beats original.
- Auto mode never sets trust and never makes a herbivore a predator.
- Friend/foe: friends join one herd and flee or defend together; a
  friend-and-foe pair, or friends where one hunts the other, is rejected.
- Hunter relationship: trusted and friendly animals stay in the follow
  band; on alarm they keep with a hunter who follows and leave one who
  does not; killing one scores as usual.
- Herds: juveniles end up inside the ring on alarm; the ring breaks on an
  adult's death.
- Trust: the timer reaches friendly only under all conditions; each threat
  action applies the configured reset; aggression always resets; predators
  need the explicit opt-in.
- Determinism: two runs with the same seed and settings give the same
  state hash.

## Open questions

- Whether juveniles need their own models or can rely on scaled variants
  ([EXPANSION.md](EXPANSION.md#add-on-variants-sharing-a-menu-animal-implementation-contract)).
- Circle defense clips: the original herbivores have no facing or warning
  clip; idle or event-reaction clips may have to stand in.
- How far 3DS and Wii U can go once the memory planner (3DS-LOAD) lands.
