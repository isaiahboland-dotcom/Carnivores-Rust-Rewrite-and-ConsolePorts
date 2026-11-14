//! The terminal's screens and what each action does to them: the boot log,
//! the numbered main menu, the hunt setup tabs with their budget, the field
//! manual and the deploy sequence. Time only moves through [`Terminal::step`],
//! so the same actions and steps always give the same state.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::{earnings_factor, Catalog, Loadout, Rules, Tab};

/// Seconds between boot log lines, and the pause after the last.
pub const BOOT_LINE_S: f32 = 0.12;
pub const BOOT_HOLD_S: f32 = 0.5;
/// Seconds the deploy sequence runs before the hunt starts.
pub const DEPLOY_S: f32 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Boot,
    Main,
    Setup,
    Manual,
    Deploy,
}

/// The main menu's entries, numbered from 1 in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainItem {
    Hunt,
    Manual,
    Trophy,
    Options,
    Hunters,
    Classic,
    Quit,
}

impl MainItem {
    pub const ALL: [MainItem; 7] = [
        MainItem::Hunt,
        MainItem::Manual,
        MainItem::Trophy,
        MainItem::Options,
        MainItem::Hunters,
        MainItem::Classic,
        MainItem::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MainItem::Hunt => "HUNT SETUP",
            MainItem::Manual => "FIELD MANUAL",
            MainItem::Trophy => "TROPHY ROOM",
            MainItem::Options => "SYSTEM OPTIONS",
            MainItem::Hunters => "CHANGE HUNTER",
            MainItem::Classic => "CLASSIC MENU",
            MainItem::Quit => "LOG OFF",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    /// Setup: the previous / next tab.
    Left,
    Right,
    /// Main: open the entry; setup: pick or drop the focused row.
    Confirm,
    Back,
    Tab(Tab),
    /// Focus row `i` of the current list (main menu or tab), e.g. hover.
    Focus(usize),
    /// Focus and confirm row `i`, e.g. a click.
    Pick(usize),
    /// The next time of day the island offers.
    CycleTime,
    SetTime(usize),
    Deploy,
    /// Skip the boot log or the deploy sequence.
    Skip,
}

/// What the platform must do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Deploy(Loadout),
    TrophyRoom,
    Options,
    /// The hunter list (sign in as another, register, delete).
    Hunters,
    ClassicMenu,
    Quit,
}

/// Why a deploy is refused, or a pick was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    Funds,
    NotAuthorised,
    LimitAnimals(usize),
    LimitWeapons(usize),
    NoTargets,
    NoArms,
    TimeUnavailable,
}

impl Notice {
    pub fn text(self) -> String {
        match self {
            Notice::Funds => "INSUFFICIENT FUNDS".into(),
            Notice::NotAuthorised => "NOT AUTHORISED ON THIS ISLAND".into(),
            Notice::LimitAnimals(n) => format!("LIMIT REACHED: {n} TARGETS PER HUNT"),
            Notice::LimitWeapons(n) => format!("LIMIT REACHED: {n} WEAPONS PER HUNT"),
            Notice::NoTargets => "SELECT AT LEAST ONE TARGET".into(),
            Notice::NoArms => "SELECT AT LEAST ONE WEAPON".into(),
            Notice::TimeUnavailable => "NO TIME OF DAY AVAILABLE ON THIS ISLAND".into(),
        }
    }
}

/// The field manual's text (controls are drawn by each platform).
pub const MANUAL: [&str; 12] = [
    "DINOHUNT CORP // FIELD MANUAL",
    "",
    "AREA     choose the island you are flown to.",
    "TARGETS  animals you hold a permit for. Kills of",
    "         unlisted animals earn half.",
    "ARMS     weapons issued for the hunt.",
    "GEAR     accessories. Radar x0.70, cover scent",
    "         x0.80 and camouflage x0.85 on earnings.",
    "",
    "The budget meter shows your account after the",
    "permits. Islands may restrict what is issued.",
    "Unlisted items are marked RESTRICTED.",
];

#[derive(Debug, Clone)]
pub struct Terminal {
    pub cat: Catalog,
    pub rules: Rules,
    /// The hunter's account (`Save::score`).
    pub score: i32,
    /// The tranquilizer option (earnings ×1.25).
    pub tranq: bool,
    pub screen: Screen,
    pub tab: Tab,
    pub main_row: usize,
    /// The focused row of each tab.
    pub rows: [usize; 4],
    pub area: usize,
    /// Picks of targets, arms and gear, by catalog index.
    picks: [Vec<bool>; 3],
    /// 0 dawn, 1 day, 2 night.
    pub time: usize,
    pub notice: Option<Notice>,
    pub boot_lines: Vec<String>,
    clock: f32,
    outcome: Option<Outcome>,
}

impl Terminal {
    /// A terminal over `cat`; `boot`: start with the boot log.
    pub fn new(cat: Catalog, rules: Rules, score: i32, boot: bool) -> Self {
        let picks = [
            vec![false; cat.animals.len()],
            vec![false; cat.weapons.len()],
            vec![false; cat.equipment.len()],
        ];
        let boot_lines = boot_log(&cat, &rules);
        let mut t = Terminal {
            cat,
            rules,
            score,
            tranq: false,
            screen: if boot { Screen::Boot } else { Screen::Main },
            tab: Tab::Area,
            main_row: 0,
            rows: [0; 4],
            area: 0,
            picks,
            time: 1,
            notice: None,
            boot_lines,
            clock: 0.0,
            outcome: None,
        };
        t.fit_time();
        t
    }

    /// Seconds into the current screen.
    pub fn clock(&self) -> f32 {
        self.clock
    }

    /// Boot log lines shown so far.
    pub fn boot_shown(&self) -> usize {
        ((self.clock / BOOT_LINE_S) as usize + 1).min(self.boot_lines.len())
    }

    /// The deploy sequence's progress, 0..=1.
    pub fn deploy_progress(&self) -> f32 {
        (self.clock / DEPLOY_S).clamp(0.0, 1.0)
    }

    pub fn take_outcome(&mut self) -> Option<Outcome> {
        self.outcome.take()
    }

    fn go(&mut self, s: Screen) {
        self.screen = s;
        self.clock = 0.0;
        self.notice = None;
    }

    /// Moves time on by `dt` seconds.
    pub fn step(&mut self, dt: f32) {
        self.clock += dt.max(0.0);
        match self.screen {
            Screen::Boot => {
                let end = self.boot_lines.len() as f32 * BOOT_LINE_S + BOOT_HOLD_S;
                if self.clock >= end {
                    self.go(Screen::Main);
                }
            }
            Screen::Deploy if self.clock >= DEPLOY_S => self.finish_deploy(),
            _ => {}
        }
    }

    fn finish_deploy(&mut self) {
        self.outcome = Some(Outcome::Deploy(self.loadout()));
        self.go(Screen::Setup);
    }

    pub fn act(&mut self, a: Action) {
        match self.screen {
            Screen::Boot => self.go(Screen::Main),
            Screen::Deploy => {
                if matches!(a, Action::Skip | Action::Confirm) {
                    self.finish_deploy();
                } else if a == Action::Back {
                    self.go(Screen::Setup);
                }
            }
            Screen::Manual => {
                if matches!(a, Action::Back | Action::Confirm | Action::Pick(_)) {
                    self.go(Screen::Main);
                }
            }
            Screen::Main => self.act_main(a),
            Screen::Setup => self.act_setup(a),
        }
    }

    fn act_main(&mut self, a: Action) {
        let n = MainItem::ALL.len();
        match a {
            Action::Up => self.main_row = self.main_row.saturating_sub(1),
            Action::Down => self.main_row = (self.main_row + 1).min(n - 1),
            Action::Focus(i) if i < n => self.main_row = i,
            Action::Pick(i) if i < n => {
                self.main_row = i;
                self.open(MainItem::ALL[i]);
            }
            Action::Confirm => self.open(MainItem::ALL[self.main_row.min(n - 1)]),
            // As the original's Escape on its main menu: the quit prompt.
            Action::Back => self.outcome = Some(Outcome::Quit),
            _ => {}
        }
    }

    fn open(&mut self, item: MainItem) {
        match item {
            MainItem::Hunt => self.go(Screen::Setup),
            MainItem::Manual => self.go(Screen::Manual),
            MainItem::Trophy => self.outcome = Some(Outcome::TrophyRoom),
            MainItem::Options => self.outcome = Some(Outcome::Options),
            MainItem::Hunters => self.outcome = Some(Outcome::Hunters),
            MainItem::Classic => self.outcome = Some(Outcome::ClassicMenu),
            MainItem::Quit => self.outcome = Some(Outcome::Quit),
        }
    }

    fn act_setup(&mut self, a: Action) {
        let len = self.cat.list(self.tab).len();
        let row = &mut self.rows[self.tab.index()];
        match a {
            Action::Up => *row = row.saturating_sub(1),
            Action::Down => *row = (*row + 1).min(len.saturating_sub(1)),
            Action::Left => self.tab = Tab::ALL[(self.tab.index() + 3) % 4],
            Action::Right => self.tab = Tab::ALL[(self.tab.index() + 1) % 4],
            Action::Tab(t) => self.tab = t,
            Action::Focus(i) if i < len => *row = i,
            Action::Pick(i) if i < len => {
                *row = i;
                self.toggle(self.tab, i);
            }
            Action::Confirm => {
                let i = *row;
                self.toggle(self.tab, i);
            }
            Action::Back => {
                self.go(Screen::Main);
            }
            Action::CycleTime => self.cycle_time(),
            Action::SetTime(t) if self.rules.allows_time(&self.cat, self.area, t) => self.time = t,
            Action::Deploy => match self.deploy_problem() {
                None => self.go(Screen::Deploy),
                Some(n) => self.notice = Some(n),
            },
            _ => {}
        }
    }

    pub fn picked(&self, tab: Tab, i: usize) -> bool {
        match tab {
            Tab::Area => i == self.area,
            _ => self.picks[tab.index() - 1].get(i).copied().unwrap_or(false),
        }
    }

    fn picked_list(&self, tab: Tab) -> Vec<usize> {
        (0..self.cat.list(tab).len())
            .filter(|&i| self.picked(tab, i))
            .collect()
    }

    pub fn allowed(&self, tab: Tab, i: usize) -> bool {
        self.rules.allows(&self.cat, self.area, tab, i)
    }

    /// What the picks cost.
    pub fn spent(&self) -> i32 {
        Tab::ALL
            .into_iter()
            .flat_map(|t| {
                self.picked_list(t)
                    .into_iter()
                    .map(move |i| self.cat.list(t)[i].price)
            })
            .sum()
    }

    /// The account after the picks.
    pub fn remaining(&self) -> i32 {
        self.score - self.spent()
    }

    /// Whether picking item `i` of `tab` (not yet picked) is affordable.
    pub fn affordable(&self, tab: Tab, i: usize) -> bool {
        let price = self.cat.list(tab).get(i).map_or(0, |it| it.price);
        if self.picked(tab, i) || price <= 0 {
            return true;
        }
        let swap = if tab == Tab::Area {
            self.cat.areas.get(self.area).map_or(0, |a| a.price)
        } else {
            0
        };
        self.remaining() + swap - price >= 0
    }

    /// The kill-score multiplier the gear and tranquilizer give.
    pub fn earnings(&self) -> f32 {
        let names = self
            .picked_list(Tab::Gear)
            .into_iter()
            .map(|i| self.cat.equipment[i].name.as_str());
        earnings_factor(names, self.tranq)
    }

    fn toggle(&mut self, tab: Tab, i: usize) {
        self.notice = None;
        if !self.allowed(tab, i) {
            self.notice = Some(Notice::NotAuthorised);
            return;
        }
        if !self.affordable(tab, i) {
            self.notice = Some(Notice::Funds);
            return;
        }
        if tab == Tab::Area {
            self.area = i;
            self.prune();
            return;
        }
        let adding = !self.picked(tab, i);
        let count = self.picked_list(tab).len();
        let cap = match tab {
            Tab::Targets => self.rules.limits.max_animals.map(Notice::LimitAnimals),
            Tab::Arms => self.rules.limits.max_weapons.map(Notice::LimitWeapons),
            _ => None,
        };
        if let Some(n @ (Notice::LimitAnimals(c) | Notice::LimitWeapons(c))) = cap {
            if adding && count >= c {
                self.notice = Some(n);
                return;
            }
        }
        if let Some(p) = self.picks[tab.index() - 1].get_mut(i) {
            *p = adding;
        }
    }

    /// Drops picks the island does not allow, and keeps the time offered.
    fn prune(&mut self) {
        for tab in [Tab::Targets, Tab::Arms, Tab::Gear] {
            for i in 0..self.cat.list(tab).len() {
                if !self.allowed(tab, i) {
                    self.picks[tab.index() - 1][i] = false;
                }
            }
        }
        self.fit_time();
    }

    fn fit_time(&mut self) {
        if !self.rules.allows_time(&self.cat, self.area, self.time) {
            if let Some(t) = [1, 0, 2]
                .into_iter()
                .find(|&t| self.rules.allows_time(&self.cat, self.area, t))
            {
                self.time = t;
            }
        }
    }

    fn cycle_time(&mut self) {
        for k in 1..=3 {
            let t = (self.time + k) % 3;
            if self.rules.allows_time(&self.cat, self.area, t) {
                self.time = t;
                return;
            }
        }
    }

    /// Why the hunt can't start now (none: it can).
    pub fn deploy_problem(&self) -> Option<Notice> {
        let l = self.loadout();
        if self.cat.areas.is_empty() {
            return Some(Notice::NotAuthorised);
        }
        if !self.rules.allows_time(&self.cat, l.area, l.time) {
            return Some(Notice::TimeUnavailable);
        }
        if l.animals.is_empty() {
            return Some(Notice::NoTargets);
        }
        if l.weapons.is_empty() {
            return Some(Notice::NoArms);
        }
        let lim = &self.rules.limits;
        if let Some(n) = lim.max_animals.filter(|&n| l.animals.len() > n) {
            return Some(Notice::LimitAnimals(n));
        }
        if let Some(n) = lim.max_weapons.filter(|&n| l.weapons.len() > n) {
            return Some(Notice::LimitWeapons(n));
        }
        let forbidden = [
            (Tab::Targets, &l.animals),
            (Tab::Arms, &l.weapons),
            (Tab::Gear, &l.equipment),
        ]
        .into_iter()
        .any(|(t, list)| list.iter().any(|&i| !self.allowed(t, i)));
        if forbidden {
            return Some(Notice::NotAuthorised);
        }
        if self.remaining() < 0 {
            return Some(Notice::Funds);
        }
        None
    }

    pub fn loadout(&self) -> Loadout {
        Loadout {
            area: self.area,
            animals: self.picked_list(Tab::Targets),
            weapons: self.picked_list(Tab::Arms),
            equipment: self.picked_list(Tab::Gear),
            time: self.time,
        }
    }

    /// Picks `l` as far as the rules and the account allow (a loadout kept
    /// from an earlier hunt, or read from a save edited by hand).
    pub fn restore(&mut self, l: &Loadout) {
        if l.area < self.cat.areas.len() {
            self.area = l.area;
        }
        for (tab, list) in [
            (Tab::Targets, &l.animals),
            (Tab::Arms, &l.weapons),
            (Tab::Gear, &l.equipment),
        ] {
            for &i in list {
                if !self.picked(tab, i) {
                    self.toggle(tab, i);
                }
            }
        }
        self.notice = None;
        if self.rules.allows_time(&self.cat, self.area, l.time) {
            self.time = l.time;
        }
        self.fit_time();
    }

    /// The deploy sequence's log, for the loadout picked.
    pub fn deploy_log(&self) -> Vec<String> {
        let l = self.loadout();
        let area = self.cat.areas.get(l.area).map_or("?", |a| a.name.as_str());
        let time = ["DAWN", "DAY", "NIGHT"][l.time.min(2)];
        vec![
            format!("> FLIGHT PLAN FILED: {area} / {time}"),
            format!("> PERMITS ISSUED: {}", l.animals.len()),
            format!("> ARMS CRATED: {}", l.weapons.len()),
            format!("> ACCOUNT DEBITED: {} PTS", self.spent()),
            "> DROPSHIP DEPARTING...".into(),
        ]
    }
}

/// The boot log for a catalog and its rules (each platform draws it).
pub fn boot_log(cat: &Catalog, rules: &Rules) -> Vec<String> {
    let restricted = rules
        .dhc
        .as_ref()
        .map_or(0, |d| d.islands.iter().filter(|i| !i.is_open()).count());
    let mut v = vec![
        "DHC-OS FIELD TERMINAL // DINOHUNT CORP".into(),
        "SECURE LINK ........... OK".into(),
        format!("AREA SURVEYS .......... {}", cat.areas.len()),
        format!("SPECIMEN INDEX ........ {}", cat.animals.len()),
        format!("ARMOURY ............... {}", cat.weapons.len()),
        format!("FIELD EQUIPMENT ....... {}", cat.equipment.len()),
    ];
    match &rules.dhc {
        Some(d) => {
            v.push(format!("RESTRICTED ISLANDS .... {restricted}"));
            v.push(format!("LICENCE FEE ........... {} PTS", d.licence_cost));
        }
        None => v.push("CORPORATE RULES ....... STANDARD".into()),
    }
    v.push("READY.".into());
    v
}
