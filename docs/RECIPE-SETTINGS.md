# Recipe settings and Play Vanilla

This document lists every setting that can change how a hunt plays: the
recipe's fields (`recipes/*.toml`) and the player's settings (`c2rs.cfg`).
For each one it states what **Play Vanilla** (classic mode) does with it.

Play Vanilla plays an instance's game (its base, overlays, standalones and
add-ons, with its prices and menu) without the enhanced rules. It does not
change the recipe file, the player's settings file or the enhanced pack, and
it uses the same saves as the enhanced game. The reasons are in
[DECISIONS.md](DECISIONS.md) ("Play Vanilla: a classic session of an
instance").

Treatments:

- **Kept**: the classic build or session uses the value as the recipe or the
  player set it.
- **Off**: the classic build leaves it out, or the classic session reads the
  built-in default (the original game's behaviour). Nothing is written back.

## Recipe fields

| Field | What it is | Play Vanilla |
|---|---|---|
| `[recipe] id` | instance identity; also the save instance | Kept |
| `[recipe] name`, `version` | display name; the author's free-text version (never compared) | Kept |
| `[recipe] schema` | recipe file format version (phase 1) | Kept |
| `[recipe] base` | expected base game | Kept |
| `[recipe.skip]` | base game areas, huntables and weapons left out | Kept |
| `[[overlays]]`, `[[standalones]]`, `[combine]` | releases and games combined with the base | Kept |
| `[[mods]]`, `[load_order]`, `[[resolve]]`, `[[patches]]` | add-ons, their order and conflict choices | Kept |
| `[[variants]]` | skin and character variants sharing a menu slot | Kept |
| `[settings] menu`, `mobile_card_layout` | which menu screens and layout | Kept |
| `[settings.pictures]`, `[settings.tuning]` | PC or phone pictures and game rules | Kept |
| `[settings] three_ds_target`, `ice_age_title` | console build options | Kept (not used on PC) |
| `[settings.balance]` | prices, base scores, starting points | Kept |
| `[[settings.weapon_restrictions]]` | per-weapon ammo and trophy eligibility | Kept |
| `[settings.rules] times` | times of day offered | Kept |
| `[settings.rules] hidden` | animals kept off the hunt menu | Kept |
| `[settings.predation]` | Advanced Dinosaur Behavior and AI: carnivores hunt herbivores | **Off** (no `_PREDATION.TXT`) |
| `[settings.flee]` | custom bounded-flee limits | **Off** (no `_FLEE.TXT`) |
| `[settings.hunt]` | hardcore hunt ruleset ([HARDCORE-HUNT.md](HARDCORE-HUNT.md)) | **Off** (no `hunt` lines in `_RULES.TXT`) |
| `[settings.dhc]` | DHC OS terminal rules: discovery, licence costs, island allow-lists ([MODDING.md](MODDING.md#dhc-os-terminal-rules)) | **Off** (no `_DHC.TXT`; the terminal never shows) |
| `[settings.world]` | zones, player starts and extraction points on the maps ([MODDING.md](MODDING.md#world-zones-player-starts-and-extraction-points)) | **Off** (no `_WORLD.TXT`) |
| `[[settings.world.missions]]` | fixed challenges that use those points ([MODDING.md](MODDING.md#missions)) | **Off** (with `[settings.world]`) |
| `[[settings.rules.areas]]` | per-area huntable caps | **Off** |
| `[[settings.rules.secrets]]` | secret animals spawned per area | **Off** |
| `[settings.rules.locks]` | settings the instance fixes | **Off** (no locks in the classic `_RULES.TXT`; see below) |

The classic `_RULES.TXT` holds only `times` and `hidden`, in the existing
format. When neither is set, the classic pack has no `_RULES.TXT`.

Locks are dropped as a whole. A lock that forces an enhanced feature on
(`dynamic_time`, `advanced_observer`, `multi_ammo`, `teleport`,
`all_huntable` set to true) is listed among the recipe's advanced sections.
The others (`mobile_radar`, `secret_dinos`, `survival_budget`, or a feature
forced off) do not make a game enhanced; in a classic session the player's
own value applies to the settings they name, or the default where the table
below says **Off**.

## Player settings (`c2rs.cfg`)

| Key | Default | What it is | Play Vanilla |
|---|---|---|---|
| `render_distance` | 0 | render distance slider | Kept |
| `fog` | 0 | original view range and fog | Kept |
| `blood` | 1 | blood sprays and trails | Kept |
| `death_animation` | 1 | death camera | Kept |
| `model_distance` | 0 | model/sprite switch distance | Kept |
| `map_heading` | 0 | map shows the hunter's heading | Kept |
| `map_mobile` | 0 | mobile map style | Kept |
| `button_hints` | 0 | button hints | Kept |
| `high_end_view` | 0 | far ground levels, object fades | Kept |
| `pretty_underwater` | 1 | underwater fog and tint | Kept |
| `classic_filter` | 0 | point-sampled textures | Kept |
| `render_scale` | 255 | Android resolution slider | Kept |
| `pause_pickers` | 1 | pickers pause gameplay | Kept |
| `map_holsters` | 1 | map puts the gun away | Kept |
| `fire_reloads_empty` | 0 (Android 1) | fire reloads an empty MEE weapon | Kept |
| `secret_dinos` | 10 | unselected huntable animals per map | Kept |
| `survival_budget` | 0 | survival starting points | Kept |
| `narrow_flashlight` | 0 | flashlight beam | Kept (no effect without dynamic time or hardcore survival) |
| `half_tranq` | 1 | multi-ammo answer | Kept (no effect without multi ammo) |
| `teleport` | 0 | teleport by the map | **Off** |
| `advanced_observer` | 0 | free-flying observer | **Off** |
| `multi_ammo` | 0 | tranquilizer splits | **Off** |
| `bounded_flee` | 0 | bounded fleeing | **Off** |
| `all_huntable` | 0 | every huntable animal in the menu | **Off** |
| `radar_water` | 0 | fish and Mosasaurus on the radar | **Off** |
| `hardcore_survival` | 0 | hardcore survival offered | **Off** |
| `dynamic_time` | 0 | day/night cycle, night vision, flashlight | **Off** |
| `map_zoom` | 0 | map zoom and panning | **Off** |
| `dhc_terminal` | 0 | the DHC OS terminal as main menu and hunt setup | **Off** (row hidden) |
| `dhc_crt` | 0 | the terminal's CRT scanlines | Kept (no effect: no terminal) |

The classic session applies these as an in-memory override, the way instance
locks already apply (`c2-pc/src/instance_rules.rs`). The override is never
stored: `c2rs.cfg` keeps the player's own values.

## Engine behaviour

Engine fixes and corrections (bug fixes, regression-verified behaviour) have
no switch, and Play Vanilla does not add one. A classic session runs the same
engine as an enhanced one.

## Rulings

- `times` and `hidden` are kept. They shape the menu a modpack presents, so
  they belong to the game rather than to the enhanced rules. Spawn rules
  (`areas`, `secrets`) are the advanced creature rules and are off.
- Dynamic time is off. It is described as visual, but it adds night vision and
  a flashlight as equipment, which changes play.
- `pause_pickers`, `map_holsters`, `fire_reloads_empty` and `secret_dinos`
  keep the player's values. They are comfort settings that default on (or a
  default count of 10), not opt-in enhancements.
- `[[variants]]` is kept. Variants are content (which character fills a menu
  slot), and trophies record them.

## Saves

A classic session uses the instance's save (`trophy0N.sav` under the recipe
ID). The two sessions are compatible:

- The score is one cumulative integer. Prices and base scores come from the
  balance, which stays applied, so earning and spending rates match.
- The rank is recomputed from the score when a hunt ends; there is no separate
  rank state.
- Trophy eligibility comes from the tranquilizer and the weapon's
  `c2rs_allow_trophy` flag in `_RES.TXT`, which weapon restrictions write and
  which stays applied.
- Trophies are stored by animal and weapon index. Classic builds keep every
  animal, weapon and their order, so the indices match.
- No unlock, licence or discovery state exists. If one is added (for example
  licences for secret animals), it must be revisited before classic sessions
  share saves with it.

## Pack folders

A classic build is written to `packs/recipe-<slug>.classic`, beside the
enhanced `packs/recipe-<slug>`. The pack cache replaces a folder whose inputs
changed, so sharing one folder would rebuild and replace the enhanced pack on
every switch. Recipe slugs contain only `a-z`, `0-9` and `-`, so the suffix
cannot collide with another recipe.

## Platform support

What each platform does with the enhanced rules today, and where Play Vanilla
is available. `c2-pc` and `c2-cook` warn when a recipe uses a feature its
target lacks (`c2_mods::validate`).

| | PC | Android | 3DS | Wii U | VR |
|---|---|---|---|---|---|
| Play Vanilla | Yes: launcher (Options), `play --classic`, `recipe build --classic` | No | No (planned) | No (planned) | No |
| Predation (`_PREDATION.TXT`) | Yes | Yes (the PC game) | Yes | Experimental | Yes |
| Custom flee limits (`_FLEE.TXT`) | Yes, with Bounded fleeing on | Yes (the PC game) | Experimental | No option yet | No |
| Times, hidden animals, locks, spawn rules (`_RULES.TXT`) | Yes | Yes (the PC game) | Yes | Yes | No |
| Hardcore hunt (`hunt` lines in `_RULES.TXT`) | Yes | Yes (the PC game; no touch buttons) | No: regular hunts | No: regular hunts | No: regular hunts |
| Variants | Yes | Yes (the PC game) | No: the cooker refuses them | No: refused at install | Not checked |
| DHC OS terminal (`_DHC.TXT`) | Yes, with its option on | Untested (the PC game; no touch layout yet) | Yes, with Settings > DHC on (hunt setup pages; untested on hardware) | No: original menu | No |
| World zones (`_WORLD.TXT`) | Data only: built and shipped, not used by hunts yet | Data only | Data only (copied into the pack) | Not shipped | Not used |
| Missions (`mission` entries in `_WORLD.TXT`) | Data only: not listed or played yet (planned in the DHC OS terminal) | Data only | Data only | Hidden: no terminal | Hidden: no terminal |

Console Play Vanilla is planned as a separate task: a second, classic pack
built beside the enhanced one with `recipe build --classic`, in the existing
pack formats. It needs hardware testing.
