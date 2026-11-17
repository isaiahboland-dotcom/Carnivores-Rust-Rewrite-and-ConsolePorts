//! Starting a hunt from a cooked pack and turning 3DS input into the
//! simulation's controls.

use c2_formats::bits::Bits;

use anyhow::Result;
use c2_core::area::{Area, TimeOfDay};
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::trophy_room;
use c2_core::game::{
    GameEvent, Hunt, HuntInput, HuntOptions, SurvivalKind, SurvivalOptions, Weapons,
};
use c2_core::load;
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use c2_formats::sav::Save;
use c2_vfs::Vfs;

use crate::budget::{Plan, Request};
use crate::rules::{HuntRules, Roam, Role, Rules};
use crate::world::DrawInfo;
use c2_formats::rules::Rules as InstRules;

/// Simulation step: 20 ms, half the PC's rate. Every animal costs about
/// 0.1 ms a step on an Old 3DS; the original updates once per frame with
/// any `TimeDt`, so the game plays the same, and the camera is
/// interpolated between steps.
pub const TICK_MS: u32 = 20;
/// Most ticks run in one frame (40 ms of game time).
pub const MAX_TICKS: u32 = 4;

use crate::pack_info::kv;
pub use crate::pack_info::{PackInfo, PACK_OLDEST, PACK_VERSION};

impl PackInfo {
    /// How to draw `area` (`C2RS/AREAn/INFO.TXT`).
    pub fn draw_info(&self, vfs: &Vfs, area: &str) -> DrawInfo {
        let m = kv(&String::from_utf8_lossy(
            &vfs.read(&format!("C2RS/{area}/INFO.TXT"))
                .unwrap_or_default(),
        ));
        let num = |k: &str, d: usize| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        DrawInfo {
            area: area.to_string(),
            tile: num("terrain_tile", 64),
            cols: num("atlas_cols", 16),
            view: self.view,
            detail: self.detail.min(self.view),
            object_view: self.object_view,
            terrain_count: num("terrain_count", 1),
            all_sprites: false,
        }
    }
}

/// What the menu picked (the original menu passes these to the game on its
/// command line: `prj=`, `din=`, `wep=`, `dtm=`, `-double`, `-observ`).
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    /// `AREA1`.., or `TROPHY` for the trophy room.
    pub area: String,
    pub time: TimeOfDay,
    /// Bit 10 + menu slot per target (`TargetDino`; in the original the
    /// slot is the AI - 10, so the bit is the AI).
    pub targets: Bits,
    /// Roaming add-ons picked (no menu slot of their own); their call
    /// slots are among the targets.
    pub roamers: Vec<Roam>,
    /// Target calls the pack has no sound for (add-ons without one): no
    /// button, and ZR doesn't call them. Filled when the hunt loads.
    pub silent_calls: Vec<i32>,
    /// Bit per carried weapon.
    pub weapons: Bits,
    pub double_ammo: bool,
    pub observer: bool,
    /// Extra ammo gear (3DS only): refills to carry, and what it does to
    /// kill scores.
    pub resupplies: u32,
    pub score_factor: f32,
    /// Settings (3DS only): small ambient animals at most (0 = none), and
    /// the animals' running speed (0.5..=1).
    pub max_ambient: i32,
    pub dino_run_k: f32,
    /// Setting: unpicked animals the map's own tables may bring at most
    /// (0: none); memory can allow fewer (`budget.rs`).
    pub secret_max: u32,
    /// Bounded fleeing (experimental here): scared animals stay on the map
    /// and a flight has limits (`_FLEE.TXT`, else built-in ones).
    pub bounded_flee: bool,
    /// Settings: blood, and the death camera.
    pub blood: bool,
    pub death_anim: bool,
    /// Hunt options: "3DS friendly" (animals notice and chase the hunter
    /// from a shorter range), stealth crouch (ZL toggles; crouched: gyro
    /// aim, half as visible and loud), and 4 random ambient species
    /// (modded games).
    pub friendly: bool,
    pub stealth: bool,
    /// Settings: the map as the mobile game draws it; teleporting by the
    /// map.
    pub map_mobile: bool,
    pub teleport: bool,
    /// Settings: the controls along the bottom of the top screen.
    pub hints: bool,
    pub random_ambients: bool,
    /// Hunt option: trees and plants drawn as their sprites, near too.
    pub tree_sprites: bool,
    /// The crouch button (setting).
    pub crouch_key: crate::settings::CrouchKey,
    /// Gyro aim: view degrees per console degree (setting).
    pub gyro_k: f32,
    /// A survival instead of a hunt (README M13).
    pub survival: Option<SurvivalKind>,
    /// Multi ammo's Half tranq (with the tranquilizer): each weapon's
    /// rounds split between lethal ones and darts, swapped by the bottom
    /// screen's ammo switch.
    pub half_tranq: bool,
    /// Advanced observer mode (with the observer): unlimited instant
    /// teleports, flying and stopping time.
    pub free_observer: bool,
    /// The DHC OS terminal's look (setting): the exit prompt takes it.
    pub dhc_terminal: bool,
}

/// The most snowflakes on the 3DS (an area asks for up to 2000).
const SNOW_MAX: usize = 300;

/// The most animals in a survival wave on the 3DS.
pub const SURVIVAL_MAX: usize = 6;

/// The phone's `ctViewR` (`Game::Process`: 19 456 units, fixed): how far
/// animals notice, chase and give up on the hunter with "3DS friendly" off.
pub const MOBILE_VIEW_R: i32 = 76;

impl Setup {
    pub fn trophy(&self) -> bool {
        self.area.eq_ignore_ascii_case("TROPHY")
    }

    /// Whether call id `id` can be called: roamers' slots can't (their
    /// add-ons supply no call; they score +10% instead).
    pub fn callable(&self, id: i32) -> bool {
        !self.roamers.iter().any(|r| r.slot as i32 + 10 == id) && !self.silent_calls.contains(&id)
    }

    pub fn carried(&self) -> Vec<usize> {
        self.weapons.iter().collect()
    }

    /// The instance's secret animals for this area (`_RULES.TXT`), one
    /// entry per species: its character, how many, and the rule's name
    /// for it. None in a survival or the trophy room.
    pub fn rule_secrets(&self, rules: &HuntRules, inst: &InstRules) -> Vec<(usize, usize, String)> {
        if self.trophy() || self.survival.is_some() {
            return Vec::new();
        }
        let names: Vec<String> = rules
            .res
            .characters
            .iter()
            .map(|d| c2_core::dino::DinoInfo::from_def(d).name)
            .collect();
        let mut out: Vec<(usize, usize, String)> = Vec::new();
        for s in inst.secrets_on(&self.area) {
            let Some(c) = c2_core::game::index_named(names.iter().map(String::as_str), &s.name)
            else {
                continue;
            };
            match out.iter_mut().find(|e| e.0 == c) {
                Some(e) => e.1 += s.count as usize,
                None => out.push((c, s.count as usize, s.name.clone())),
            }
        }
        out
    }

    /// The hunt's memory plan (`budget.rs`): which models load and how
    /// many animals of each are placed. `density` is the hunt's option;
    /// `ambient_only`, the species "Random ambients" drew.
    pub fn plan(
        &self,
        info: &PackInfo,
        rules: &HuntRules,
        inst: &InstRules,
        density: i32,
        ambient_only: Option<Vec<usize>>,
    ) -> Plan {
        let secrets = self.rule_secrets(rules, inst);
        crate::budget::plan(
            info,
            rules,
            &Request {
                area: self.area.clone(),
                hunting: !self.trophy() && self.survival.is_none(),
                targets: self.targets,
                roamers: self.roamers.iter().map(|r| r.char).collect(),
                weapons: self.carried(),
                ambient_max: usize::try_from(self.max_ambient).unwrap_or(0),
                ambient_only,
                secret_max: self.secret_max as usize,
                rule_secrets: secrets.iter().map(|s| (s.0, s.1)).collect(),
                // `PlaceCharacters`: 8 + density / 30 and up to 5 more.
                huntable_want: usize::try_from(8 + density / 30 + 5).unwrap_or(8),
                water: info.has_water(&self.area),
            },
        )
    }
}

pub struct Game {
    pub info: PackInfo,
    pub setup: Setup,
    pub draw: DrawInfo,
    pub area: Area,
    pub hunt: Hunt,
    pub cars: Vec<Option<Car>>,
    /// Per character, the name its model is cooked under (`C2RS/M/`).
    pub keys: Vec<String>,
    pub weapon_cars: Vec<Option<Car>>,
    pub events: Vec<GameEvent>,
    acc_ms: f32,
    /// Ticks run by the last `update`.
    pub ticks_last: u32,
    /// `RunMode`.
    pub run: bool,
    /// Stealth crouch: crouched (ZL toggles).
    pub crouched: bool,
    /// Crouch on L: how long it has been held (ms).
    l_ms: f32,
    /// Crouch on D-Pad Up: 0 walk, 1 run, 2 crouch.
    gait: u8,
    /// The trophy room's question waiting for A / B.
    pub ask: Option<TrophyAsk>,
    /// The trophy room's memory plan (`None` outside it).
    pub trophy_plan: Option<trophy_room::RoomPlan>,
    /// The hunt's memory plan: what was loaded and the animals' limits.
    pub plan: Plan,
    /// The gyroscope, on for a stealth hunt: raw-to-degrees/s.
    gyro: Option<f32>,
    /// Extra ammo refills left.
    pub resupplies: u32,
    pub prev_cam: [f32; 3],
}

/// What the trophy room asks before doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrophyAsk {
    Remove,
    Lock,
}

/// What the hunter does with the 3DS controls.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pad {
    /// Circle Pad, -1..1 (x right, y up).
    pub stick: (f32, f32),
    /// Look: C-Stick, touch drag (pixels this frame).
    pub look: (f32, f32),
    pub turn_left: bool,
    pub turn_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub fire: bool,
    pub run: bool,
    pub jump: bool,
    pub crouch: bool,
    /// ZL pressed this frame (stealth crouch toggles).
    pub crouch_down: bool,
    /// L held, and released this frame (crouch on L: hold / tap).
    pub l_held: bool,
    pub l_up: bool,
    // Pressed this frame:
    pub toggle_weapon: bool,
    pub next_weapon: bool,
    pub binoculars: bool,
    pub call: bool,
    pub pause: bool,
    pub escape: bool,
    pub confirm: bool,
    pub cancel: bool,
    /// Y / X at the leave prompt (or once dead): a new hunt, same setup.
    pub restart: bool,
    /// Pressed this frame: L, R (the trophy room's controls).
    pub l_down: bool,
    pub r_down: bool,
    /// From the bottom screen: a weapon (index), a call (AI).
    pub select_weapon: Option<usize>,
    pub select_call: Option<i32>,
    /// Use one extra ammo refill.
    pub resupply: bool,
    /// From the bottom screen: switch the gun's firing mode.
    pub switch_mode: bool,
}

/// Frees what the 3DS doesn't need once the GPU has its copies.
fn trim(area: &mut Area, cars: &mut [Option<Car>]) {
    area.rsc.textures = Vec::new();
    area.rsc.skies = Default::default();
    area.rsc.random_sounds = Vec::new();
    for a in &mut area.rsc.ambients {
        a.pcm = Vec::new();
    }
    for o in &mut area.rsc.objects {
        o.model.mesh.texture = Vec::new();
        o.sprite = Vec::new();
    }
    area.map.texture2 = Vec::new();
    for (i, l) in area.map.light.iter_mut().enumerate() {
        if i != area.time as usize {
            *l = Vec::new();
        }
    }
    for c in cars.iter_mut().flatten() {
        c.mesh.texture = Vec::new();
        for s in &mut c.sounds {
            s.pcm = Vec::new();
        }
    }
}

fn next_seed(s: u32) -> u32 {
    s.wrapping_mul(1_103_515_245).wrapping_add(12345)
}

#[allow(clippy::too_many_arguments)]
/// `ReInitGame`: the hunter at a random landing spot (or the trophy room's
/// door) and the herd placed (or the trophies), as `c2-pc`'s `new_hunt`.
fn new_hunt(
    area: &Area,
    setup: &Setup,
    plan: &Plan,
    species: &[c2_core::dino::Species],
    tables: Option<&c2_formats::mee::Tables>,
    weapon_info: &[(c2_core::game::WeaponInfo, c2_core::game::WeaponAnims)],
    ship_ms: i32,
    seed: u32,
    room: Save,
    portrait_stands: bool,
) -> Hunt {
    let o = room.options;
    let trophy = setup.trophy();
    let mut rng = MsRand::new(seed);
    let player = if trophy {
        Player::new(
            area,
            tables
                .and_then(|t| t.trophy_start)
                .or(area.trophy_start)
                .unwrap_or((76, 70)),
        )
    } else {
        let cell = area
            .landings
            .get(seed as usize % area.landings.len().max(1))
            .copied()
            .unwrap_or((512, 512));
        Player::new(area, cell)
    };
    let mut herd = Herd {
        species: species.to_vec(),
        chars: Vec::new(),
        demo_index: None,
        ..Default::default()
    };
    // Dialect games: the trophy room's spots (where trophies stand, and
    // which a kill takes).
    if let Some(t) = tables {
        herd.trophy_spots = t.trophies.clone();
    }
    herd.portrait_stands = portrait_stands;
    // The memory plan's limits: no more of a species than was planned
    // for, at placement or later.
    herd.caps = plan.caps();
    if trophy {
        herd.place_trophies(area, &room, &mut rng);
    } else if let Some(t) = tables {
        herd.place_mee(area, (player.x, player.z), &mut rng, t, o.dens);
    } else {
        herd.place(
            area,
            (player.x, player.z),
            &mut rng,
            PlaceOptions {
                density: o.dens,
                opt_agres: o.agres,
                targets: setup.targets,
                // The plan's: the setting, or fewer when memory is short.
                max_ambient: Some(i32::try_from(plan.ambients_max).unwrap_or(0)),
            },
        );
    }
    let carried = if trophy || setup.observer {
        Bits::NONE
    } else {
        setup.weapons
    };
    let (infos, anims): (Vec<_>, Vec<_>) = weapon_info.iter().cloned().unzip();
    let weapons = Weapons::new(infos, anims, carried, setup.double_ammo);
    let opts = HuntOptions {
        targets: setup.targets,
        radar: o.radar != 0,
        opt_agres: o.agres,
        opt_sens: o.sens,
        // ctViewR: the phone's fixed 76 cells; "3DS friendly", the PC's
        // lowest view-range option (42 cells); 60 in the trophy room.
        view_r: if trophy {
            60
        } else if setup.friendly {
            42
        } else {
            MOBILE_VIEW_R
        },
        render_r: 0,
        camo: o.camo != 0,
        scent: o.scent != 0,
        tranq: o.tranq != 0 && !setup.half_tranq,
        half_tranq: o.tranq != 0 && setup.half_tranq,
        // The PC's "Map puts the gun away" setting; the 3DS map is on the
        // bottom screen.
        map_holsters: false,
        free_observer: setup.observer && setup.free_observer,
        observer: setup.observer,
        trophy,
        score_factor: setup.score_factor,
        dino_run_k: setup.dino_run_k,
        blood: setup.blood,
        death_anim: setup.death_anim,
        afraid_every: 2,
        notice_k: if setup.friendly {
            crate::settings::FRIENDLY_NOTICE
        } else {
            1.0
        },
        stealth: setup.stealth,
        teleport: setup.teleport,
        // No buttons left for the dialect's reload and rack: on their own.
        auto_chamber: true,
        dynamic_night: None,
        night_vision_active: false,
        advanced_binoculars: false,
        scuba: false,
        distant_every: crate::settings::distant_every(),
        no_call_bonus: setup.roamers.iter().map(|r| r.slot + 10).collect(),
    };
    let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
    hunt.ship_anim_ms = ship_ms;
    // The area's snow (Ice Age), thinner than on PC.
    hunt.effects.snow = tables
        .and_then(|t| t.snow)
        .filter(|_| !setup.trophy())
        .map(|s| c2_core::effects::Snowfall::new(s, SNOW_MAX));
    // Survival: the waves the menu prices buy, in Mount Ravan's ravine.
    if let Some(kind) = setup.survival {
        hunt.start_survival(area, SurvivalOptions::pick(&hunt, kind, SURVIVAL_MAX));
    }
    hunt
}

impl Game {
    /// Loads what `setup` needs: its area, the hunter, ambient and target
    /// animals, the carried weapons.
    pub fn load(
        vfs: &Vfs,
        info: PackInfo,
        setup: Setup,
        room: Save,
        seed: u32,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Self> {
        // The area first, while the heap is empty: its map is the biggest
        // file, and is briefly in memory twice (file and parsed).
        // `c2-cook`'s `load_peak` repeats these steps; keep them in step.
        let mut setup = setup;
        setup.silent_calls = (10..Bits::LEN as i32)
            .filter(|&ai| setup.targets.has(ai as usize))
            .filter(|ai| vfs.resolve(&format!("C2RS/SND/K{ai}_0.PCM")).is_none())
            .collect();
        progress(&format!("area {}", setup.area));
        let area = load::area_with(vfs, &setup.area, setup.time, true)?;
        progress("rules (_RES.TXT)");
        let menu_rules = Rules::load(vfs)?;
        let rules = menu_rules.hunt_with(&setup.area, setup.time, setup.targets, &setup.roamers);
        progress("animals");
        // The trophy room needs only the species on display (every one a
        // pack with add-ons has won't fit in an Old 3DS). When their models
        // don't all fit in memory (by the cooker's figures, whatever the
        // number of species), none is loaded and the room shows portraits.
        let mut trophies: Vec<usize> = Vec::new();
        let mut trophy_plan = None;
        if setup.trophy() {
            let model = |i: usize| rules.keys.get(i).map(|k| format!("m:{k}"));
            let hunter = (0..rules.roles.len()).filter(|&i| rules.roles[i] == Role::Hunter);
            let base_linear_kb = info.cost("hunt")
                + info.cost(&setup.area)
                + hunter
                    .filter_map(model)
                    .map(|m| info.cost(&m))
                    .sum::<usize>();
            let budget = trophy_room::Budget {
                linear_kb: info.linear_kb,
                heap_kb: info.heap_kb,
                base_linear_kb,
                base_heap_kb: info.heap(&setup.area),
            };
            let cost = |i: usize| {
                let m = model(i).filter(|m| info.costs.contains_key(m))?;
                Some(trophy_room::ModelCost {
                    linear_kb: info.cost(&m),
                    heap_kb: info.heap(&m),
                })
            };
            let has_portrait = |i: usize| {
                rules
                    .keys
                    .get(i)
                    .is_some_and(|k| vfs.resolve(&format!("C2RS/MENU/R/{k}.TX")).is_some())
            };
            let p = trophy_room::plan(
                &room.bodies,
                rules.roles.len(),
                budget,
                &cost,
                &has_portrait,
            );
            trophies = p.load.clone();
            trophy_plan = Some(p);
        }
        // Random ambients: 4 ambient species this hunt (modded games add
        // many; the rest aren't loaded, so they don't spawn).
        let ambients = setup.random_ambients.then(|| {
            let all: Vec<usize> = (0..rules.roles.len())
                .filter(|&i| rules.roles[i] == Role::Ambient)
                .collect();
            pick(&all, 4, seed)
        });
        // The memory plan: the picks, then the instance's secret animals
        // (`_RULES.TXT`) and the map's own roamers, ambient animals and
        // aquatic ones as far as memory goes (`budget.rs`).
        let mut inst = load::rules(vfs);
        let hunting = !setup.trophy() && setup.survival.is_none();
        let plan = setup.plan(&info, &rules, &inst, room.options.dens, ambients);
        let (mut species, cars) = load::species_with(vfs, &rules.res, &|i| {
            if setup.trophy() {
                return trophies.contains(&i) || rules.roles.get(i) == Some(&Role::Hunter);
            }
            plan.loads(i)
        });
        // The secrets as planned: memory can allow fewer than the rules ask.
        if plan.planned {
            let asked = setup.rule_secrets(&rules, &inst);
            inst.secrets = plan
                .rule_secrets
                .iter()
                .filter_map(|&(c, count)| {
                    let name = asked.iter().find(|a| a.0 == c)?.2.clone();
                    Some(c2_formats::rules::Secret {
                        name,
                        count: u32::try_from(count).unwrap_or(u32::MAX),
                        areas: Vec::new(),
                    })
                })
                .collect();
        }
        if let Some(t) = &rules.tables {
            c2_core::mee::tune(&mut species, t);
        }
        c2_core::game::price_species(&mut species, &menu_rules.menu);
        let res = &rules.res;
        progress("weapons");
        let carried = setup.carried();
        let weapons = load::weapons_with(vfs, res, &|i| carried.contains(&i));
        let mut weapon_cars = Vec::new();
        let mut weapon_info = Vec::new();
        for w in weapons {
            weapon_info.push((w.info, w.anims));
            weapon_cars.push(w.car);
        }
        let ship_ms = load::ship_anim_ms(vfs);
        let seed = next_seed(seed);
        let mut hunt = new_hunt(
            &area,
            &setup,
            &plan,
            &species,
            rules.tables.as_ref(),
            &weapon_info,
            ship_ms,
            seed,
            room,
            trophy_plan.as_ref().is_some_and(|p| p.low_memory()),
        );
        if let Some(s) = &mut hunt.survival {
            s.opts.starting_budget =
                crate::instance_rules::survival_budget(menu_rules.menu.survival_budget);
        }
        // Predation (an instance setting, experimental here): the pack's
        // `_PREDATION.TXT`, when its recipe turns it on.
        if !setup.trophy() && setup.survival.is_none() {
            hunt.herd.predation = load::predation(vfs, &hunt.herd.species);
        }
        if setup.bounded_flee {
            // A fixed lockout after each flight: no hunter movement to track.
            let mut table = load::flee(vfs, &hunt.herd.species);
            table.lockout = c2_core::dino::flee::Lockout::Hard {
                ms: c2_core::dino::flee::HARD_LOCK_MS,
            };
            hunt.herd.flee = Some(table);
        }
        // A dialect game's tables can place a dozen ambients or more:
        // the plan's cap (the setting's, or fewer), as the original's
        // placement has.
        if rules.tables.is_some() && !setup.trophy() {
            hunt.herd.cap_placed(plan.ambients_max, |ct| {
                rules.roles.get(ct) == Some(&Role::Ambient)
            });
        }
        // Aquatic animals the plan has room for: in the closest water,
        // and kept near the hunter (`c2_core::game::AquaticRoam`).
        if hunting && !plan.aquatic.is_empty() {
            hunt.roam_aquatics(&area, &plan.aquatic);
        }
        // The instance's spawn rules, after the caps: its extra animals
        // always come.
        if hunting && !inst.is_empty() {
            hunt.apply_spawn_rules(&area, &inst, &setup.area);
        }
        // Shots carry and animals out of sight are re-placed past what is
        // drawn, which can be farther than `view_r`.
        hunt.opts.render_r = info.view as i32;
        hunt.area_no = c2_formats::mee::area_of(&setup.area).map_or(0, |n| n as i32);
        hunt.clock = clock();
        // Snow of the area's own (the phone's Ice Age areas).
        if let (None, Some(s)) = (&hunt.effects.snow, area.snow) {
            hunt.effects.snow = Some(c2_core::effects::Snowfall::new(s, SNOW_MAX));
        }
        // Snow by the area's `.c2map` (Ice Age's areas beside C2's).
        if hunt.effects.snow.is_none()
            && info
                .snow
                .iter()
                .any(|a| a.eq_ignore_ascii_case(&setup.area))
        {
            let s = c2_formats::mee::Snow::ICE_AGE;
            hunt.effects.snow = Some(c2_core::effects::Snowfall::new(s, SNOW_MAX));
        }
        // Start on a call that sounds.
        if !setup.callable(hunt.target_call) {
            if let Some(a) =
                (10..Bits::LEN as i32).find(|&a| setup.targets.has(a as usize) && setup.callable(a))
            {
                hunt.target_call = a;
            }
        }
        anyhow::ensure!(
            hunt.survival.as_ref().is_none_or(|s| s.opts.budget_ready()),
            "Survival needs an attacking animal within the starting budget"
        );
        let keys = rules.keys;
        let resupplies = setup.resupplies;
        let setup_stealth = setup.stealth;
        Ok(Self {
            prev_cam: hunt.camera().0,
            draw: DrawInfo {
                all_sprites: setup.tree_sprites,
                ..info.draw_info(vfs, &setup.area)
            },
            info,
            setup,
            area,
            hunt,
            cars,
            keys,
            weapon_cars,
            events: Vec::new(),
            acc_ms: 0.0,
            ticks_last: 0,
            run: false,
            crouched: false,
            l_ms: 0.0,
            gait: 0,
            ask: None,
            trophy_plan,
            plan,
            gyro: gyro_on(setup_stealth),
            resupplies,
        })
    }

    /// This hunt's kills into the hunter's record.
    pub fn record_kills(&mut self, vfs: &Vfs) -> std::io::Result<()> {
        use c2_core::record::{Kill, Mode};
        let kills = std::mem::take(&mut self.hunt.kills);
        let wave = self.hunt.wave();
        if kills.is_empty() && wave == 0 {
            return Ok(());
        }
        let mode = if self.hunt.survival.is_some() {
            Mode::Survival
        } else {
            Mode::Hunt
        };
        let n = self.hunt.area_no;
        let area = crate::menu::title(vfs, &format!("AREA{n}.TXT"), format!("Area {n}"));
        let (y, mo, d, _, _) = self.hunt.clock;
        let mut r = crate::save::load_record();
        for k in kills {
            r.kills.push(Kill {
                mode,
                animal: self
                    .hunt
                    .herd
                    .species
                    .get(k.ctype)
                    .map_or_else(String::new, |s| s.info.name.clone()),
                score: k.score,
                range: k.range,
                weapon: self
                    .hunt
                    .weapons
                    .info
                    .get(k.weapon)
                    .map_or_else(String::new, |w| w.name.clone()),
                area: area.clone(),
                date: (y << 20) + (mo << 10) + d,
            });
        }
        r.best_wave = r.best_wave.max(wave);
        crate::save::store_record(&r)
    }

    /// Frees load-time data; call once the GPU copies exist.
    pub fn trim(&mut self) {
        trim(&mut self.area, &mut self.cars);
        for c in self.weapon_cars.iter_mut().flatten() {
            c.mesh.texture = Vec::new();
        }
    }

    /// Runs the simulation for `dt` seconds of real time.
    /// The trophy room's buttons: L / R move the trophy shown to the next
    /// free stand that way, D-Pad Down removes it, D-Pad Right locks or
    /// unlocks it (both asked: A yes, B no); X / Y look, as in a hunt (the
    /// owner's ask). They don't do their hunt jobs here. A message to show,
    /// if any.
    pub fn trophy_buttons(&mut self, pad: &mut Pad) -> Option<String> {
        if !self.hunt.opts.trophy {
            return None;
        }
        let mut msg = None;
        let asked = self.ask.is_some();
        if let Some(a) = self.ask {
            if pad.confirm {
                match a {
                    TrophyAsk::Remove => self.hunt.remove_current_trophy(),
                    TrophyAsk::Lock => {
                        let _ = self.hunt.toggle_lock_current();
                    }
                }
                self.ask = None;
            } else if pad.cancel {
                self.ask = None;
            }
        } else if let Some(slot) = self.hunt.trophy_body {
            let locked = self.hunt.room.bodies.get(slot).is_some_and(|b| b.locked());
            for (on, dir) in [(pad.l_down, -1), (pad.r_down, 1)] {
                if on && !self.hunt.move_current_trophy(&self.area, dir) {
                    msg = Some("No free stand that way".into());
                }
            }
            if pad.next_weapon {
                if locked {
                    msg = Some("Locked: D-Pad Right to unlock".into());
                } else {
                    self.ask = Some(TrophyAsk::Remove);
                }
            }
            if pad.toggle_weapon {
                self.ask = Some(TrophyAsk::Lock);
            }
        }
        // Not their hunt jobs.
        (pad.fire, pad.jump, pad.restart) = (false, false, false);
        (pad.next_weapon, pad.toggle_weapon) = (false, false);
        // A / B answered the question (else they answer the leave prompt).
        if asked {
            (pad.confirm, pad.cancel) = (false, false);
        }
        msg
    }

    pub fn update(&mut self, pad: &Pad, dt: f32) {
        use crate::settings::CrouchKey;
        let key = self.setup.crouch_key;
        if pad.run {
            if key == CrouchKey::DpadUp {
                self.gait = (self.gait + 1) % 3;
                self.run = self.gait == 1;
            } else {
                self.run = !self.run;
            }
        }
        // Crouch on L: held a quarter second it crouches (or toggles the
        // stealth crouch), a tap jumps.
        let mut l_jump = false;
        let mut l_crouch = false;
        if key == CrouchKey::L {
            if pad.l_held {
                let was = self.l_ms;
                self.l_ms += dt * 1000.0;
                if was < 250.0 && self.l_ms >= 250.0 && self.setup.stealth {
                    self.crouched = !self.crouched;
                }
                l_crouch = self.l_ms >= 250.0;
            } else {
                l_jump = pad.l_up && self.l_ms < 250.0;
                self.l_ms = 0.0;
            }
        }
        if pad.pause {
            self.hunt.toggle_pause();
        }
        if pad.escape {
            self.hunt.press_escape();
        }
        if self.hunt.exit_prompt {
            if pad.confirm {
                self.hunt.confirm_exit();
            }
            if pad.cancel {
                self.hunt.exit_prompt = false;
            }
        }
        if self.setup.stealth && key == CrouchKey::Zl && pad.crouch_down {
            self.crouched = !self.crouched;
        }
        let crouch = match key {
            CrouchKey::DpadUp => self.gait == 2,
            _ if self.setup.stealth => self.crouched,
            CrouchKey::Zl => pad.crouch,
            CrouchKey::L => l_crouch,
        };
        let jump = match key {
            CrouchKey::L => l_jump,
            _ => pad.jump,
        };
        // Crouched in stealth: aim by turning the console.
        let mut look = pad.look;
        if let (Some(k), true) = (self.gyro, crouch) {
            let mut r = ctru_sys::angularRate { x: 0, z: 0, y: 0 };
            unsafe { ctru_sys::hidGyroRead(&mut r) };
            // Degrees turned this frame, as look units (0.0025 rad each,
            // as the core turns them), scaled by the setting; a hand's
            // tremor (under GYRO_DEADZONE degrees/s) ignored.
            let gain = self.setup.gyro_k;
            let units = |raw: i16| {
                let dps = f32::from(raw) * k;
                let dps = dps.signum() * (dps.abs() - GYRO_DEADZONE).max(0.0);
                dps * gain * dt * (std::f32::consts::PI / 180.0) / 0.0025
            };
            look.0 -= units(r.z);
            look.1 -= units(r.y);
        }
        let (sx, sy) = pad.stick;
        let controls = Controls {
            forward: sy > 0.35,
            backward: sy < -0.35,
            strafe_left: sx < -0.35,
            strafe_right: sx > 0.35,
            turn_left: pad.turn_left,
            turn_right: pad.turn_right,
            look_up: pad.look_up,
            look_down: pad.look_down,
            crouch,
            jump,
            run: self.run,
            mouse: look,
            mouse_sens: f32::from(128u8),
        };
        if let Some(ai) = pad.select_call {
            self.hunt.target_call = ai;
        }
        if pad.resupply && self.resupplies > 0 && self.hunt.death.is_none() {
            self.hunt.weapons.resupply();
            self.resupplies -= 1;
            self.events.push(GameEvent::Message("Ammo refilled".into()));
        }
        let select = if let Some(w) = pad.select_weapon {
            Some(w)
        } else if pad.next_weapon {
            let w = &self.hunt.weapons;
            let carried = self.setup.carried();
            let cur = carried.iter().position(|&i| i == w.current);
            let next = cur.map_or(0, |c| (c + 1) % carried.len().max(1));
            carried.get(next).copied()
        } else {
            None
        };
        let mut input = HuntInput {
            controls,
            fire: pad.fire,
            toggle_weapon: pad.toggle_weapon,
            select_weapon: select,
            toggle_binoculars: pad.binoculars,
            switch_mode: pad.switch_mode,
            call: pad.call && self.setup.callable(self.hunt.target_call),
            ..Default::default()
        };
        // At most `MAX_TICKS` per frame: past that the game slows down
        // instead of spending ever longer catching up (a slow tick would
        // otherwise make the next frame need more of them).
        self.acc_ms = (self.acc_ms + dt * 1000.0).min((MAX_TICKS * TICK_MS) as f32);
        self.ticks_last = 0;
        let cars: Vec<Option<&Car>> = self.cars.iter().map(Option::as_ref).collect();
        while self.acc_ms >= TICK_MS as f32 {
            self.prev_cam = self.hunt.camera().0;
            self.hunt
                .tick(&self.area, &cars, &input, TICK_MS as i32, &mut self.events);
            input.controls.mouse = (0.0, 0.0);
            input.toggle_weapon = false;
            input.select_weapon = None;
            input.toggle_binoculars = false;
            input.switch_mode = false;
            input.call = false;
            self.acc_ms -= TICK_MS as f32;
            self.ticks_last += 1;
        }
    }

    /// Fraction of a tick since the last one (for camera interpolation).
    pub fn alpha(&self) -> f32 {
        self.acc_ms / TICK_MS as f32
    }
}

/// `n` of `from`, picked by `seed` (all of them if there are no more).
pub(crate) fn pick(from: &[usize], n: usize, seed: u32) -> Vec<usize> {
    let mut left = from.to_vec();
    let mut x = seed | 1;
    let mut out = Vec::new();
    while out.len() < n && !left.is_empty() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        out.push(left.swap_remove(x as usize % left.len()));
    }
    out
}

/// Gyro rates below this (degrees/s) are a hand's tremor, not aim.
const GYRO_DEADZONE: f32 = 2.5;

/// Turns the gyroscope on for a stealth hunt: its raw-to-degrees/s factor.
fn gyro_on(stealth: bool) -> Option<f32> {
    if !stealth || unsafe { ctru_sys::HIDUSER_EnableGyroscope() } < 0 {
        return None;
    }
    let mut k = 0.0f32;
    let ok = unsafe { ctru_sys::HIDUSER_GetGyroscopeRawToDpsCoefficient(&mut k) } >= 0;
    Some(if ok && k > 0.0 { k } else { 1.0 / 14.375 })
}

impl Drop for Game {
    fn drop(&mut self) {
        if self.gyro.is_some() {
            let _ = unsafe { ctru_sys::HIDUSER_DisableGyroscope() };
        }
    }
}

/// The date and time (`Hunt::clock`: year, month, day, hour, minute) from
/// the console's clock (local time).
fn clock() -> (i32, i32, i32, i32, i32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970 to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y as i32,
        m as i32,
        d as i32,
        (rem / 3600) as i32,
        (rem / 60 % 60) as i32,
    )
}
