//! The headless run: the scenario's hunt ticked as fast as it goes, no
//! renderer, every tick timed.

use std::sync::OnceLock;
use std::time::Instant;

use c2_core::game::GameEvent;
use c2_core::prof::SECTIONS;
use c2_core::scenario::{self, Director, Headless, Inject, Scenario, Script, TICK_MS};
use c2_formats::car::Car;

use crate::game::Game;

pub struct TickSample {
    pub tick: u32,
    pub ms: f64,
    /// `Hunt::tick`'s parts (`prof::SECTIONS`), ms.
    pub sections: [f32; SECTIONS.len()],
}

pub struct Checkpoint {
    pub name: String,
    pub tick: u32,
    pub hash: u64,
}

pub struct RunData {
    /// `None` when set-up failed (see `setup_error`).
    pub director: Option<Director>,
    pub setup_error: Option<String>,
    pub panic: Option<String>,
    pub samples: Vec<TickSample>,
    pub checkpoints: Vec<Checkpoint>,
    /// Species placed, by name.
    pub placed: Vec<String>,
    pub animals_end: usize,
    pub effects_end: usize,
    pub shots: u32,
    pub wall_ms: f64,
}

static START: OnceLock<Instant> = OnceLock::new();

fn clock_ms() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

impl RunData {
    /// A run that has done nothing yet.
    pub fn empty() -> Self {
        RunData {
            director: None,
            setup_error: None,
            panic: None,
            samples: Vec::new(),
            checkpoints: Vec::new(),
            placed: Vec::new(),
            animals_end: 0,
            effects_end: 0,
            shots: 0,
            wall_ms: 0.0,
        }
    }
}

pub fn run(game: &Game, s: &Scenario, inject: Option<Inject>) -> RunData {
    let mut data = RunData::empty();
    let infos = game.weapons.iter().map(|w| w.info.clone()).collect();
    let anims = game.weapons.iter().map(|w| w.anims).collect();
    let room = c2_core::profile::new_player("regress", 0, 0);
    let built = scenario::build(&game.area, game.species.clone(), infos, anims, s, room);
    let mut b = match built {
        Ok(b) => b,
        Err(e) => {
            data.setup_error = Some(e.to_string());
            return data;
        }
    };
    data.placed = b
        .placed
        .iter()
        .map(|&i| game.species[i].info.name.clone())
        .collect();
    b.hunt.prof.clock = Some(clock_ms);
    if let Some(i) = &inject {
        i.adjust_hold(&mut b.hold);
    }
    let cars: Vec<Option<&Car>> = game.cars.iter().map(Option::as_ref).collect();
    let names = game.weapon_names();
    let mut d = Director::new(s, &names, &b.cast);
    let ceiling = s.max_ticks(names.len());
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut fe = Headless;
        let mut out: Vec<GameEvent> = Vec::new();
        while !d.done() {
            if d.tick() >= ceiling {
                d.abort("scenario timeout: whole-run tick ceiling reached");
                break;
            }
            let mut plan = d.plan(&b.hunt, &mut fe);
            if let (Some(a), Script::Kills(k)) = (plan.arrange, &s.script) {
                b.hold.arrange(&mut b.hunt, &game.area, a, k);
            }
            if let Some(i) = &inject {
                i.adjust_plan(&mut plan, &b.hunt);
                i.adjust_hunt(d.tick(), s.warmup_ticks, &mut b.hunt);
            }
            let t0 = Instant::now();
            b.hunt
                .tick(&game.area, &cars, &plan.input, TICK_MS, &mut out);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            out.clear();
            data.samples.push(TickSample {
                tick: d.tick(),
                ms,
                sections: b.hunt.prof.last_tick(),
            });
            let lost = b.hold.apply(&mut b.hunt, &game.area);
            for name in d.observe(&b.hunt, &mut fe, &lost) {
                data.checkpoints.push(Checkpoint {
                    name,
                    tick: d.tick(),
                    hash: scenario::state_hash(&b.hunt),
                });
            }
        }
    }));
    data.wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    if let Err(p) = result {
        let msg = p
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "panic".into());
        d.abort(&format!("panic: {msg}"));
        data.panic = Some(msg);
    }
    data.animals_end = b.hunt.herd.chars.iter().filter(|c| c.health > 0).count();
    data.effects_end = b.hunt.effects.blood.len() + b.hunt.effects.bursts.len();
    data.shots = b.hunt.shots_made;
    data.director = Some(d);
    data
}
