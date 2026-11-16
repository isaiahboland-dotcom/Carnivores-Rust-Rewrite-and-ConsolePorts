//! The DinoHunt Corp terminal ("DHC OS") on the PC: an opt-in front end
//! for the main menu and hunt setup (`c2_core::dhc`). It lists what the
//! original menu loaded (names, prices, pictures, descriptions) under the
//! recipe's `_DHC.TXT` rules and starts the hunt the same way the original
//! menu does. Keyboard, mouse and the pad's pointer drive it.
//!
//! Off (the default), in Play Vanilla, or after "Classic menu" this
//! session, the original menu shows. On, it also takes over from the phone
//! (APK) menus; the hunt keeps the phone's touch prompts.

use std::sync::atomic::{AtomicBool, Ordering};

use c2_core::dhc::terminal::{Action, Outcome, Screen as TScreen, Terminal};
use c2_core::dhc::{Catalog, Dhc, Item, Limits, Loadout, Rules, Tab};

use super::*;

mod draw;
mod login;
mod options;
pub(super) use options::term_layout;
pub(crate) mod palette;

/// The boot log has been shown this run.
static BOOTED: AtomicBool = AtomicBool::new(false);
/// "Classic menu" was picked this run: the original menu until restart.
static CLASSIC_THIS_RUN: AtomicBool = AtomicBool::new(false);

/// The terminal and how its lists map to the menu's columns.
pub(super) struct TermView {
    pub(super) term: Terminal,
    /// Per tab: catalog index → the column's item index.
    pub(super) map: [Vec<usize>; 4],
    /// The pointer last frame (hovering focuses only when it moves).
    pub(super) last_cursor: Option<(f32, f32)>,
    /// Per tab: the list's first shown row (the wheel scrolls it without
    /// moving the focus).
    pub(super) first: [usize; 4],
}

/// The game's `_DHC.TXT`, when the recipe has one.
fn read_rules(huntdat: &Path) -> Option<Dhc> {
    find_file(
        huntdat,
        c2_formats::dhc::PATH.trim_start_matches("HUNTDAT/"),
    )
    .and_then(|p| std::fs::read(p).ok())
    .map(|b| Dhc::parse(&b))
}

/// Whether the terminal's look is in use with settings `pc`: its option
/// on, not Play Vanilla, not "Classic menu" this run. The hunt's exit
/// prompt follows it too.
pub(crate) fn terminal_look(pc: &crate::settings::PcSettings) -> bool {
    pc.dhc_terminal != 0 && !crate::classic::on() && !CLASSIC_THIS_RUN.load(Ordering::Relaxed)
}

/// The menu screen's key for each tab, `1`..`4`.
const TAB_KEYS: [KeyCode; 4] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
];

const MAIN_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

impl Menu {
    /// Whether the menus take the terminal's look (the hunter list too).
    /// It overrides the phone (APK) menus when its option is on.
    pub(super) fn terminal_style(&self) -> bool {
        terminal_look(&self.pc)
    }

    /// Whether the terminal stands in for the main menu now.
    pub(super) fn terminal_on(&self) -> bool {
        self.terminal_style() && self.me().is_some()
    }

    /// The hunter list is the first screen: the boot log plays before it
    /// (once a run).
    pub(super) fn boot_before_login(&mut self) {
        if self.terminal_style() && !BOOTED.swap(true, Ordering::Relaxed) {
            self.term = Some(self.build_terminal(true));
        }
    }

    /// The main menu: the terminal's, or the original's.
    pub(super) fn go_home(&mut self) {
        if self.terminal_on() {
            self.enter_terminal();
        } else {
            self.screen = Screen::Main;
        }
    }

    /// Opens the terminal (its boot log the first time this run).
    pub(super) fn enter_terminal(&mut self) {
        let boot = !BOOTED.swap(true, Ordering::Relaxed);
        let (score, tranq) = (self.me().map_or(0, |s| s.score), self.tranq());
        let survival = self.survival_labels();
        match &mut self.term {
            Some(v) => {
                v.term.score = score;
                v.term.tranq = tranq;
                v.term.survival = survival;
                // Back from options or the quit prompt: the main menu.
                if !matches!(v.term.screen, TScreen::Main | TScreen::Setup) {
                    v.term.act(Action::Back);
                }
            }
            None => self.term = Some(self.build_terminal(boot)),
        }
        self.screen = Screen::Terminal;
    }

    fn build_terminal(&self, boot: bool) -> TermView {
        let mut map: [Vec<usize>; 4] = Default::default();
        let mut lists: [Vec<Item>; 4] = Default::default();
        for (ci, col) in self.columns.iter().enumerate().take(4) {
            for &i in &col.order {
                let mut it = Item::new(&col.names[i], col.prices[i]);
                if ci == 0 {
                    it.stem = self
                        .areas
                        .get(i)
                        .and_then(|p| p.file_stem())
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
                lists[ci].push(it);
                map[ci].push(i);
            }
        }
        let [areas, animals, weapons, equipment] = lists;
        let cat = Catalog {
            areas,
            animals,
            weapons,
            equipment,
        };
        // The limits the original setup screen applies.
        let inst = crate::instance_rules::get();
        let hc = &inst.hunt;
        let cap = MAX_PICKS.get().copied();
        let firearms = hc.max_firearms.map(|n| n as usize);
        let max_weapons = match (cap, firearms) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let barred_cols = [
            (1, hc.no_radar),
            (2, hc.no_cover_scent),
            (4, hc.no_night_vision),
        ];
        let barred_equipment = barred_cols
            .into_iter()
            .filter(|&(_, on)| on)
            .filter_map(|(col, _)| map[3].iter().position(|&i| i == col))
            .collect();
        let times = (0..3)
            .filter(|&t| inst.time_allowed(t))
            .fold(0u8, |m, t| m | 1 << t);
        // A regular hunt only has the menu's own limits; the hardcore
        // hunt's entry adds the ruleset's (and exists only when the
        // recipe turned the ruleset on).
        let rules = Rules {
            dhc: read_rules(&self.huntdat),
            limits: Limits {
                times,
                max_animals: cap,
                max_weapons: cap,
                barred_equipment: Vec::new(),
            },
        };
        let hardcore = Limits {
            times,
            max_animals: cap,
            max_weapons,
            barred_equipment,
        };
        let score = self.me().map_or(0, |s| s.score);
        let mut term = Terminal::new(cat, rules, score, boot);
        if !hc.is_empty() {
            term.offer_hardcore(hardcore);
        }
        term.survival = self.survival_labels();
        term.tranq = self.tranq();
        // The accessories the save keeps, and the last area and time.
        let c = &self.choice;
        let back = |tab: usize, i: usize| map[tab].iter().position(|&x| x == i);
        // The last hunt's animals and weapons come back only when asked.
        let again = self.pc.remember_hunt != 0;
        let picked = |tab: usize, has: &dyn Fn(usize) -> bool| -> Vec<usize> {
            (0..map[tab].len())
                .filter(|&k| again && has(map[tab][k]))
                .collect()
        };
        let keep = Loadout {
            area: back(0, c.area).unwrap_or(0),
            animals: picked(1, &|i| c.dinos.has(i)),
            weapons: picked(2, &|i| c.weapons >> i & 1 != 0),
            equipment: (0..32)
                .filter(|&b| c.equip & (1 << b) != 0)
                .filter_map(|b| back(3, b))
                .collect(),
            time: c.time,
            observer: c.observer,
            hardcore: false,
        };
        term.restore(&keep);
        TermView {
            term,
            map,
            last_cursor: None,
            first: [0; 4],
        }
    }

    /// Opens the terminal for a screenshot: `terminal` (main menu),
    /// `terminal-boot`, `terminal-hunt` (the setup's targets, two picks).
    pub(super) fn show_terminal(&mut self, which: &str) {
        self.pc.dhc_terminal = 1;
        self.term = Some(self.build_terminal(which == "terminal-boot"));
        BOOTED.store(true, Ordering::Relaxed);
        self.screen = Screen::Terminal;
        match which {
            "terminal-login" | "terminal-delete" => {
                self.picked = (!self.players.is_empty()).then_some(0);
                self.name = self.me().map(|p| p.player_name()).unwrap_or_default();
                self.screen = if which == "terminal-login" {
                    Screen::Register
                } else {
                    Screen::Delete
                };
            }
            "terminal-options" => self.screen = Screen::Options,
            "terminal-quit" => self.screen = Screen::Quit,
            "terminal-waiver" => {
                self.name = "Newcomer".into();
                self.screen = Screen::Waiver;
            }
            _ => {}
        }
        if which == "terminal-hunt" {
            if let Some(v) = &mut self.term {
                let t = &mut v.term;
                t.step(1.0);
                for a in [
                    Action::Pick(0),
                    Action::Tab(Tab::Arms),
                    Action::Pick(0),
                    Action::Tab(Tab::Targets),
                    Action::Pick(0),
                ] {
                    t.act(a);
                }
            }
        }
    }

    pub(super) fn update_terminal(&mut self, dt: f32, input: &Input) {
        let tranq = self.tranq();
        let hits = self.terminal_hits();
        let Some(v) = &mut self.term else {
            self.screen = Screen::Main;
            return;
        };
        let moved = self.cursor != v.last_cursor;
        v.last_cursor = self.cursor;
        let t = &mut v.term;
        // The question for the tranquilizer's ammo is up: it takes the input.
        t.tranq = tranq;
        t.step(dt);
        let mut acts = Vec::new();
        let key = |k| input.pressed(k);
        if t.screen == TScreen::Boot || t.screen == TScreen::Deploy {
            if input.last_pressed.is_some() || input.clicked {
                acts.push(if key(KeyCode::Escape) {
                    Action::Back
                } else {
                    Action::Skip
                });
            }
        } else {
            for (k, a) in [
                (KeyCode::ArrowUp, Action::Up),
                (KeyCode::ArrowDown, Action::Down),
                (KeyCode::ArrowLeft, Action::Left),
                (KeyCode::ArrowRight, Action::Right),
                (KeyCode::Tab, Action::Right),
                (KeyCode::Enter, Action::Confirm),
                (KeyCode::NumpadEnter, Action::Confirm),
                (KeyCode::Space, Action::Confirm),
                (KeyCode::Escape, Action::Back),
                (KeyCode::KeyT, Action::CycleTime),
                (KeyCode::KeyQ, Action::ToggleTranq),
                (KeyCode::KeyO, Action::ToggleObserver),
                (KeyCode::KeyF, Action::Deploy),
                (KeyCode::F5, Action::Deploy),
            ] {
                if key(k) {
                    acts.push(a);
                }
            }
            if t.screen == TScreen::Setup {
                for (i, &k) in TAB_KEYS.iter().enumerate() {
                    if key(k) {
                        acts.push(Action::Tab(Tab::ALL[i]));
                    }
                }
            } else if matches!(t.screen, TScreen::Main | TScreen::Survival) {
                for (i, &k) in MAIN_KEYS.iter().enumerate() {
                    if key(k) {
                        acts.push(Action::Pick(i));
                    }
                }
            }
            // The wheel scrolls the setup's lists, leaving the focus; the
            // main menu's rows all show, so there it moves the focus.
            if t.screen != TScreen::Setup {
                if input.scroll > 0.0 {
                    acts.push(Action::Up);
                } else if input.scroll < 0.0 {
                    acts.push(Action::Down);
                }
            }
            // The pointer: hover focuses, a click picks.
            for (r, hit) in &hits {
                if !inside(self.cursor, *r) {
                    continue;
                }
                match *hit {
                    Hit::Row(i) if input.clicked => acts.push(Action::Pick(i)),
                    Hit::Row(i) if moved => acts.push(Action::Focus(i)),
                    Hit::Tab(tab) if input.clicked => acts.push(Action::Tab(tab)),
                    Hit::Time if input.clicked => acts.push(Action::CycleTime),
                    Hit::Tranq if input.clicked => acts.push(Action::ToggleTranq),
                    Hit::Observer if input.clicked => acts.push(Action::ToggleObserver),
                    Hit::Deploy if input.clicked => acts.push(Action::Deploy),
                    Hit::Back if input.clicked => acts.push(Action::Back),
                    _ => {}
                }
            }
        }
        let (tab, before) = (t.tab.index(), t.rows[t.tab.index()]);
        for a in acts {
            t.act(a);
        }
        // Keep a focus the keys moved in view; the wheel scrolls apart.
        if t.screen == TScreen::Setup {
            let (vis, n) = (draw::visible_rows(), t.cat.list(t.tab).len());
            let (now, row) = (t.tab.index(), t.rows[t.tab.index()]);
            let first = &mut v.first[now];
            if now != tab || row != before {
                *first = (*first).clamp((row + 1).saturating_sub(vis), row);
            }
            if input.scroll > 0.0 {
                *first = first.saturating_sub(1);
            } else if input.scroll < 0.0 {
                *first += 1;
            }
            *first = (*first).min(n.saturating_sub(vis));
        }
        if let Some(o) = t.take_outcome() {
            self.terminal_outcome(o);
        }
    }

    fn terminal_outcome(&mut self, o: Outcome) {
        match o {
            Outcome::Deploy(l) => self.deploy(&l),
            Outcome::TrophyRoom => {
                if let Some(trophy) =
                    find_file(&self.huntdat, "AREAS").and_then(|a| find_file(&a, "TROPHY.MAP"))
                {
                    self.start(trophy, TimeOfDay::Day, Bits::NONE, 0, false, false);
                }
            }
            Outcome::ToggleTranq => {
                self.toggle_tranq();
                let tranq = self.tranq();
                if let Some(v) = &mut self.term {
                    v.term.tranq = tranq;
                }
            }
            Outcome::Survival(i) => {
                if let Some(&kind) = self.survival_modes().get(i) {
                    self.start_survival(kind);
                }
            }
            Outcome::Options => self.screen = Screen::Options,
            Outcome::Hunters => {
                self.name = self.me().map(|p| p.player_name()).unwrap_or_default();
                self.picked = self.player;
                self.screen = Screen::Register;
            }
            Outcome::ClassicMenu => {
                CLASSIC_THIS_RUN.store(true, Ordering::Relaxed);
                self.screen = Screen::Main;
            }
            Outcome::Quit => self.screen = Screen::Quit,
        }
    }

    /// The survival modes' labels, as `survival_modes` lists them.
    pub(super) fn survival_labels(&self) -> Vec<String> {
        self.survival_modes()
            .iter()
            .map(|k| {
                match k {
                    SurvivalKind::Classic => "CLASSIC SURVIVAL",
                    SurvivalKind::Modded => "MODDED SURVIVAL",
                    SurvivalKind::Hardcore => "HARDCORE SURVIVAL",
                }
                .to_string()
            })
            .collect()
    }

    /// Starts the hunt `l` describes, through the original menu's start.
    fn deploy(&mut self, l: &Loadout) {
        let Some(v) = &self.term else { return };
        let col = |tab: usize, i: usize| v.map[tab].get(i).copied();
        let mut c = Choice {
            area: col(0, l.area).unwrap_or(0),
            time: l.time.min(2),
            observer: l.observer,
            hardcore: l.hardcore,
            ..Default::default()
        };
        for i in l.animals.iter().filter_map(|&i| col(1, i)) {
            c.dinos.set(i);
        }
        for i in l.weapons.iter().filter_map(|&i| col(2, i)) {
            c.weapons |= 1u128.checked_shl(i as u32).unwrap_or(0);
        }
        for i in l.equipment.iter().filter_map(|&i| col(3, i)) {
            c.equip |= 1u32.checked_shl(i as u32).unwrap_or(0);
        }
        // The original menu's checks still apply (a refusal stays put).
        self.choice = c;
        if self.can_hunt() {
            self.launch_hunt();
        } else {
            tracing::warn!("terminal loadout refused by the hunt setup's checks");
        }
    }
}

/// What a spot on the terminal does when pointed at.
#[derive(Debug, Clone, Copy)]
pub(super) enum Hit {
    Row(usize),
    Tab(Tab),
    Time,
    Tranq,
    Observer,
    Deploy,
    Back,
}
