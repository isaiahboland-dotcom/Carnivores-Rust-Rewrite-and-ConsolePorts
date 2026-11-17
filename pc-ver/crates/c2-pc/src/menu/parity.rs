//! `c2-pc menu-parity`: the DHC OS terminal and the original hunt setup,
//! given the same picks, must start the same hunt, or both refuse it
//! (DHC-OS phase 2; `c2-regress menu-parity` runs it). Each case is played
//! through both front ends of one loaded menu: the terminal through its
//! actions and its deploy, the original menu through its picks and its
//! hunt start. The hunts they would start (`HuntSetup`) are compared.
//! The last cases add island rules (`_DHC.TXT`, made up here) to one area,
//! so the original menu's island filter is checked against the terminal's.
//!
//! The hunter's account is raised in memory so every pick is affordable;
//! a hunt start stores the save, so run it on a scratch data folder.

use c2_core::dhc::terminal::{Action, Outcome as TOutcome};
use c2_core::dhc::{Dhc, Island, Tab};
use serde_json::{json, Value};

use super::*;

/// One set of picks, by the menu's item indices.
#[derive(Debug, Clone)]
struct Case {
    name: &'static str,
    area: usize,
    animals: Vec<usize>,
    weapons: Vec<usize>,
    equipment: Vec<usize>,
    time: usize,
    observer: bool,
    /// The cases with the made-up island rules.
    island: bool,
}

/// What of a hunt start is compared.
#[derive(Debug, Clone, PartialEq)]
struct Started {
    map: String,
    time: String,
    targets: Vec<usize>,
    weapons: u128,
    double_ammo: bool,
    night_vision: bool,
    flashlight: bool,
    observer: bool,
    hardcore: bool,
    /// The accessories (camouflage, radar, cover scent go to the hunt in
    /// the save, not in `HuntSetup`): the menu's `equip` bits.
    equip: u32,
}

impl Started {
    fn of(h: &HuntSetup, equip: u32) -> Self {
        Started {
            map: h
                .map
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_uppercase())
                .unwrap_or_default(),
            time: format!("{:?}", h.time),
            targets: h.targets.iter().collect(),
            weapons: h.weapons,
            double_ammo: h.double_ammo,
            night_vision: h.night_vision,
            flashlight: h.flashlight,
            observer: h.observer,
            hardcore: h.hardcore,
            equip,
        }
    }

    fn json(s: &Option<Self>) -> Value {
        match s {
            None => Value::Null,
            Some(s) => json!({
                "map": s.map,
                "time": s.time,
                "targets": s.targets,
                "weapons": format!("{:#x}", s.weapons),
                "double_ammo": s.double_ammo,
                "night_vision": s.night_vision,
                "flashlight": s.flashlight,
                "observer": s.observer,
                "hardcore": s.hardcore,
                "equip": format!("{:#x}", s.equip),
            }),
        }
    }
}

/// One case's result.
pub struct CaseResult {
    pub name: &'static str,
    pub same: bool,
    pub json: Value,
}

/// The made-up island: `stem` issues only the first animal and weapon,
/// camouflage, and night.
fn island_rules(stem: &str, animal: &str, weapon: &str) -> Dhc {
    Dhc {
        islands: vec![Island {
            area: stem.into(),
            animals: vec![animal.into()],
            weapons: vec![weapon.into()],
            equipment: vec!["Camouflage".into()],
            times: 0b100,
        }],
        ..Dhc::default()
    }
}

/// `c2-pc menu-parity <GAME_DIR>`: runs the cases offscreen, prints one
/// line each and writes them as JSON to `out`; an error when any differ.
pub(crate) fn run(game: &Path, out: Option<&Path>) -> Result<()> {
    let (gpu, _target) = Gpu::offscreen((800, 600))?;
    let mut m = Menu::new(&gpu, game, None)?;
    let results = m.parity()?;
    let mut differ = 0;
    for r in &results {
        let mark = if r.same { "same" } else { "DIFFERENT" };
        println!("{mark:>9}  {}  {}", r.name, r.json);
        differ += usize::from(!r.same);
    }
    if let Some(p) = out {
        let all: Vec<Value> = results.iter().map(|r| r.json.clone()).collect();
        std::fs::write(p, serde_json::to_string_pretty(&all)?)
            .with_context(|| format!("writing {}", p.display()))?;
    }
    anyhow::ensure!(
        differ == 0,
        "{differ} of {} cases start a different hunt from the terminal and the original menu",
        results.len()
    );
    println!(
        "{} cases: the terminal and the original menu agree",
        results.len()
    );
    Ok(())
}

impl Menu {
    /// Runs every case; `Err` when the menu can't be set up.
    pub fn parity(&mut self) -> Result<Vec<CaseResult>> {
        if self.player.is_none() {
            if self.players.is_empty() {
                self.name = "Parity".into();
                self.accept_waiver();
            } else {
                self.sign_in(0);
            }
        }
        let me = self.me_mut().context("no hunter could be signed in")?;
        me.score = 1_000_000;
        // The regular hunt on both sides (the original menu applies an
        // instance's hardcore ruleset to every hunt only without the
        // terminal: `hardcore_applies`).
        self.pc.dhc_terminal = 1;
        self.pc.remember_hunt = 0;
        let cases = self.parity_cases()?;
        let own_rules = self.dhc.take();
        let mut out = Vec::new();
        for c in &cases {
            self.dhc = if c.island {
                let stem = self.area_stem(c.area);
                let first = |col: usize| self.columns[col].order.first().copied().unwrap_or(0);
                let name = |col: usize| self.columns[col].names[first(col)].clone();
                Some(island_rules(&stem, &name(1), &name(2)))
            } else {
                own_rules.clone()
            };
            let terminal = self.via_terminal(c);
            let menu = self.via_menu(c);
            let same = terminal == menu;
            out.push(CaseResult {
                name: c.name,
                same,
                json: json!({
                    "case": c.name,
                    "same": same,
                    "terminal": Started::json(&terminal),
                    "menu": Started::json(&menu),
                }),
            });
        }
        self.dhc = own_rules;
        self.term = None;
        Ok(out)
    }

    fn area_stem(&self, i: usize) -> String {
        self.areas
            .get(i)
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The cases, from the lists the game has (cheapest items first).
    fn parity_cases(&self) -> Result<Vec<Case>> {
        let order = |col: usize| self.columns.get(col).map_or(&[][..], |c| &c.order[..]);
        let (areas, animals, weapons) = (order(0), order(1), order(2));
        anyhow::ensure!(
            !areas.is_empty() && !animals.is_empty() && !weapons.is_empty(),
            "the game needs an area, an animal and a weapon"
        );
        let a0 = animals[0];
        let a1 = animals.get(1).copied().unwrap_or(a0);
        let w0 = weapons[0];
        let w1 = weapons.get(1).copied().unwrap_or(w0);
        let (first, last) = (areas[0], areas[areas.len() - 1]);
        let isl = areas.get(1).copied().unwrap_or(first);
        let case = |name, area, animals: &[usize], weapons: &[usize], equipment: &[usize]| Case {
            name,
            area,
            animals: animals.to_vec(),
            weapons: weapons.to_vec(),
            equipment: equipment.to_vec(),
            time: 1,
            observer: false,
            island: false,
        };
        let mut v = vec![
            case("basic", first, &[a0], &[w0], &[]),
            Case {
                time: 2,
                ..case(
                    "two_each_and_gear_at_night",
                    last,
                    &[a0, a1],
                    &[w0, w1],
                    &[0, 1, 3],
                )
            },
            Case {
                time: 0,
                observer: true,
                ..case("observer_at_dawn", first, &[a0], &[], &[])
            },
            case("nothing_to_hunt", first, &[], &[w0], &[]),
            case("no_weapon", first, &[a0], &[], &[]),
        ];
        let island = |name, animals: &[usize], equipment: &[usize], time| Case {
            time,
            island: true,
            ..case(name, isl, animals, &[w0], equipment)
        };
        v.push(island("island_allows", &[a0], &[0], 2));
        if a1 != a0 {
            v.push(island("island_bars_an_animal", &[a1], &[], 2));
        }
        v.push(island("island_bars_gear", &[a0], &[1], 2));
        v.push(island("island_bars_the_time", &[a0], &[], 1));
        Ok(v)
    }

    /// Fresh picks: the day, nothing picked, no hunt waiting.
    fn parity_reset(&mut self) {
        self.choice = Choice {
            time: 1,
            ..Default::default()
        };
        self.launch = None;
        self.term = None;
        self.screen = Screen::Setup;
    }

    /// The original menu: its picks, the time and observer toggles, its
    /// hunt start when it allows one.
    fn via_menu(&mut self, c: &Case) -> Option<Started> {
        self.parity_reset();
        self.pick(0, c.area);
        for (col, list) in [(1, &c.animals), (2, &c.weapons), (3, &c.equipment)] {
            for &i in list {
                self.pick(col, i);
            }
        }
        // As a click on a time of day (`update_setup`).
        if self.time_offered(c.time) {
            self.choice.time = c.time;
        }
        if c.observer {
            self.toggle_observer();
        }
        if self.can_hunt() {
            self.launch_hunt();
        }
        let equip = self.choice.equip;
        self.launch.take().map(|h| Started::of(&h, equip))
    }

    /// The terminal: HUNT SETUP, the tabs' picks, the time, observer,
    /// DEPLOY (its sequence skipped), then the platform's deploy.
    fn via_terminal(&mut self, c: &Case) -> Option<Started> {
        self.parity_reset();
        let mut v = self.build_terminal(false);
        let row = |tab: usize, item: usize| v.map[tab].iter().position(|&x| x == item);
        let mut acts = vec![Action::Confirm, Action::Tab(Tab::Area)];
        acts.extend(row(0, c.area).map(Action::Pick));
        for (tab, list) in [
            (Tab::Targets, &c.animals),
            (Tab::Arms, &c.weapons),
            (Tab::Gear, &c.equipment),
        ] {
            acts.push(Action::Tab(tab));
            acts.extend(
                list.iter()
                    .filter_map(|&i| row(tab.index(), i))
                    .map(Action::Pick),
            );
        }
        acts.push(Action::SetTime(c.time));
        if c.observer {
            acts.push(Action::ToggleObserver);
        }
        acts.extend([Action::Deploy, Action::Skip]);
        for a in acts {
            v.term.act(a);
        }
        let out = v.term.take_outcome();
        self.term = Some(v);
        if let Some(TOutcome::Deploy(l)) = out {
            self.deploy(&l);
        }
        let equip = self.choice.equip;
        self.launch.take().map(|h| Started::of(&h, equip))
    }
}
