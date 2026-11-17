//! The DinoHunt Corp terminal ("DHC OS") on the 3DS: an opt-in look for
//! the hunt setup (Settings > DHC). The steps stay the 3DS menu's (area,
//! animals, weapons and gear, then the hunt options), as pages with Back
//! and Next (L / R): the bottom screen lists the step's items, the top
//! screen shows the focused one's picture and description, the budget and
//! the earnings. Picks, prices, the three-pick limit, memory and the hunt
//! start are the menu's own; the recipe's island rules (`_DHC.TXT`,
//! `c2_core::dhc`) decide what each area issues.

use std::sync::atomic::{AtomicBool, Ordering};

use c2_core::dhc::{Catalog, Dhc, Item as CItem, Limits, Rules as DhcRules, Tab};
use c2_vfs::Vfs;

use super::{Menu, Pick, Step, EQUIPMENT, EXTRA_AMMO};
use crate::draw2d::inside;
use crate::hunt::Setup;

/// The boot log has been shown this run.
static BOOTED: AtomicBool = AtomicBool::new(false);

/// Frames each boot log line takes, and the hold after the last.
pub(super) const BOOT_LINE_FRAMES: u32 = 7;
const BOOT_HOLD_FRAMES: u32 = 30;

/// List rows on the bottom screen.
pub(super) const LIST_Y: f32 = 20.0;
pub(super) const ROW: f32 = 16.0;
pub(super) const ROWS: usize = 11;
/// The bar: Back, the middle button, Next.
pub(super) const T_BACK: [f32; 4] = [4.0, 212.0, 76.0, 24.0];
pub(super) const T_MID: [f32; 4] = [86.0, 212.0, 148.0, 24.0];
pub(super) const T_NEXT: [f32; 4] = [240.0, 212.0, 76.0, 24.0];

/// One row of the gear page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GearRow {
    Weapon(usize),
    Equip(usize),
    Ammo,
    Tranq,
    Observe,
}

/// The buttons the terminal reads this frame (the menu loop fills it).
#[derive(Debug, Clone, Copy, Default)]
pub struct Keys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    /// L let go without SELECT (L+SELECT is the screenshot).
    pub l: bool,
    pub r: bool,
    pub start: bool,
}

/// The terminal's state beside the menu's.
pub(super) struct Term {
    /// The focused row of the area, animal and gear lists.
    pub rows: [usize; 3],
    pub rules: DhcRules,
    pub cat: Catalog,
    /// The boot log and the frames it has run (`None`: done).
    pub boot: Option<(Vec<String>, u32)>,
    /// The focused item's description, and which item it is.
    pub desc: Option<((Step, usize), Vec<String>)>,
    pub notice: Option<String>,
}

impl Term {
    /// Before the menu's lists exist ([`Term::new`] follows).
    pub(super) fn placeholder() -> Self {
        Term {
            rows: [0; 3],
            rules: DhcRules::default(),
            cat: Catalog::default(),
            boot: None,
            desc: None,
            notice: None,
        }
    }

    /// The terminal's state for the menu's lists; `on`: the terminal is in
    /// use (the boot log plays, once a run).
    pub(super) fn new(vfs: &Vfs, m: &Menu, on: bool) -> Self {
        let item = |name: &str, price: i32| CItem::new(name, price);
        let cat = Catalog {
            areas: m
                .areas
                .iter()
                .map(|(stem, it)| CItem {
                    stem: stem.clone(),
                    ..item(&it.name, it.price)
                })
                .collect(),
            animals: m
                .dinos
                .iter()
                .map(|(_, it)| item(&it.name, it.price))
                .collect(),
            weapons: m
                .weapons
                .iter()
                .map(|it| item(&it.name, it.price))
                .collect(),
            equipment: m.equip.iter().map(|it| item(&it.name, it.price)).collect(),
        };
        let inst = crate::instance_rules::get();
        let times = (0..3)
            .filter(|&t| inst.time_allowed(t))
            .fold(0u8, |acc, t| acc | 1 << t);
        let rules = DhcRules {
            dhc: vfs.read(c2_formats::dhc::PATH).ok().map(|b| Dhc::parse(&b)),
            limits: Limits {
                times,
                ..Limits::default()
            },
        };
        let boot = (on && !BOOTED.swap(true, Ordering::Relaxed))
            .then(|| (c2_core::dhc::terminal::boot_log(&cat, &rules), 0));
        Term {
            rows: [0; 3],
            rules,
            cat,
            boot,
            desc: None,
            notice: None,
        }
    }
}

/// `s` broken into lines of at most `n` characters, at spaces.
pub(super) fn wrap(lines: &[String], n: usize) -> Vec<String> {
    let mut out = Vec::new();
    for l in lines {
        let mut cur = String::new();
        for w in l.split_whitespace() {
            if !cur.is_empty() && cur.len() + 1 + w.len() > n {
                out.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(&w.chars().take(n).collect::<String>());
        }
        out.push(cur);
    }
    out
}

impl Menu {
    /// The terminal look is on for this screen.
    pub fn terminal_active(&self) -> bool {
        self.settings.dhc_terminal
            && self.survival.is_none()
            && matches!(self.step, Step::Area | Step::Animals | Step::Gear)
    }

    /// The boot log is running.
    pub fn terminal_booting(&self) -> bool {
        self.term.boot.is_some()
    }

    /// Which list the step shows (0 areas, 1 animals, 2 gear).
    pub(super) fn term_list(&self) -> usize {
        match self.step {
            Step::Animals => 1,
            Step::Gear => 2,
            _ => 0,
        }
    }

    /// The animals listed (indices in `dinos`), as the grid lists them.
    pub(super) fn term_animals(&self) -> Vec<usize> {
        (0..self.dinos.len())
            .filter(|&i| {
                !self.hidden_dinos.has(i)
                    && (self.settings.all_huntable || !matches!(self.dinos[i].0, Pick::Roamer(_)))
            })
            .collect()
    }

    pub(super) fn gear_rows(&self) -> Vec<GearRow> {
        let mut v: Vec<GearRow> = (0..self.weapons.len()).map(GearRow::Weapon).collect();
        v.extend((0..self.equip.len()).map(GearRow::Equip));
        v.extend([GearRow::Ammo, GearRow::Tranq, GearRow::Observe]);
        v
    }

    /// How many rows the step's list has.
    pub(super) fn term_len(&self) -> usize {
        match self.term_list() {
            0 => self.areas.len(),
            1 => self.term_animals().len(),
            _ => self.gear_rows().len(),
        }
    }

    /// The first row shown, keeping the focused one in view.
    pub(super) fn term_first(&self) -> usize {
        let row = self.term.rows[self.term_list()];
        (row / ROWS) * ROWS
    }

    /// Whether the island allows item `i` of `tab` (catalog index).
    pub(super) fn island_allows(&self, tab: Tab, i: usize) -> bool {
        self.term.rules.allows(&self.term.cat, self.area, tab, i)
    }

    pub(super) fn time_offered(&self, t: usize) -> bool {
        self.term.rules.allows_time(&self.term.cat, self.area, t)
    }

    /// Whether the picks break the area's island rules (picks kept from
    /// another area): the hexagon menu refuses that hunt start as the
    /// terminal does. An observer carries no weapons; a survival and the
    /// trophy room have no island.
    pub(super) fn island_forbids(&self) -> bool {
        let Some(isl) = self.term.rules.island(&self.term.cat, self.area) else {
            return false;
        };
        if self.survival.is_some() {
            return false;
        }
        let barred = |tab: Tab, mask: &c2_formats::bits::Bits| {
            mask.iter().any(|i| !self.island_allows(tab, i))
        };
        !isl.allows_time(self.time)
            || barred(Tab::Targets, &self.picked_dinos)
            || (!self.observer && barred(Tab::Arms, &self.picked_weapons))
            || barred(Tab::Gear, &self.picked_equip)
    }

    /// Drops the picks the area's island does not allow; keeps the time
    /// offered.
    pub(super) fn prune_island(&mut self) {
        for i in 0..self.dinos.len() {
            if self.picked_dinos.has(i) && !self.island_allows(Tab::Targets, i) {
                self.picked_dinos.clear(i);
            }
        }
        for i in 0..self.weapons.len() {
            if self.picked_weapons.has(i) && !self.island_allows(Tab::Arms, i) {
                self.picked_weapons.clear(i);
            }
        }
        for i in 0..self.equip.len() {
            if self.picked_equip.has(i) && !self.island_allows(Tab::Gear, i) {
                self.picked_equip.clear(i);
            }
        }
        if !self.time_offered(self.time) {
            if let Some(t) = [1, 0, 2].into_iter().find(|&t| self.time_offered(t)) {
                self.time = t;
            }
        }
    }

    fn cycle_time(&mut self) {
        for k in 1..=3 {
            let t = (self.time + k) % 3;
            if self.time_offered(t) {
                self.time = t;
                return;
            }
        }
    }

    /// Picks (or drops) row `row` of the step's list.
    fn term_pick(&mut self, row: usize) {
        self.term.notice = None;
        self.message = None;
        let restricted = "NOT AUTHORISED ON THIS ISLAND".to_string();
        match self.term_list() {
            0 => {
                if let Some(a) = self.areas.get(row) {
                    let price_now = self.areas.get(self.area).map_or(0, |a| a.1.price);
                    if a.1.price > price_now && self.remaining() + price_now - a.1.price < 0 {
                        self.term.notice = Some("INSUFFICIENT FUNDS".into());
                        return;
                    }
                    self.area = row;
                    self.prune_island();
                }
            }
            1 => {
                let Some(&i) = self.term_animals().get(row) else {
                    return;
                };
                if !self.picked_dinos.has(i) && !self.island_allows(Tab::Targets, i) {
                    self.term.notice = Some(restricted);
                    return;
                }
                self.focus = Some(self.toggle(Step::Animals, i, false));
            }
            _ => {
                let Some(&g) = self.gear_rows().get(row) else {
                    return;
                };
                match g {
                    GearRow::Weapon(i) | GearRow::Equip(i) => {
                        let (tab, gear, on) = match g {
                            GearRow::Weapon(_) => (Tab::Arms, false, self.picked_weapons.has(i)),
                            _ => (Tab::Gear, true, self.picked_equip.has(i)),
                        };
                        if !on && !self.island_allows(tab, i) {
                            self.term.notice = Some(restricted);
                            return;
                        }
                        self.focus = Some(self.toggle(Step::Gear, i, gear));
                    }
                    GearRow::Ammo => self.focus = Some(self.cycle_extra_ammo()),
                    GearRow::Tranq => {
                        let t = &mut self.save.options.tranq;
                        *t = i32::from(*t == 0);
                        self.tranq_ask = *t != 0 && self.settings.multi_ammo;
                    }
                    GearRow::Observe => self.observer = !self.observer,
                }
            }
        }
    }

    fn term_move(&mut self, d: i32) {
        let n = self.term_len();
        let r = &mut self.term.rows[self.term_list()];
        *r = (*r as i32 + d).clamp(0, n.saturating_sub(1) as i32) as usize;
    }

    /// One frame of input; `Some` starts the hunt.
    pub fn term_input(&mut self, k: Keys, tap: Option<(f32, f32)>) -> Option<Setup> {
        if let Some((lines, f)) = &mut self.term.boot {
            *f += 1;
            let end = lines.len() as u32 * BOOT_LINE_FRAMES + BOOT_HOLD_FRAMES;
            if *f >= end || k.a || k.b || k.start || k.x || k.y || k.r || k.l || tap.is_some() {
                self.term.boot = None;
            }
            return None;
        }
        // Multi ammo's question: Half (A, or the left button) or Full (Y).
        if self.tranq_ask {
            if let Some((x, y)) = tap {
                self.answer_tranq(x, y);
            } else if k.a || k.y {
                self.answer_tranq_key(k.a);
            }
            return None;
        }
        if k.start {
            self.start_button();
            return None;
        }
        if k.up {
            self.term_move(-1);
        }
        if k.down {
            self.term_move(1);
        }
        if k.left {
            self.term_move(-(ROWS as i32));
        }
        if k.right {
            self.term_move(ROWS as i32);
        }
        if k.x && self.step == Step::Area {
            self.cycle_time();
        }
        if k.a {
            let row = self.term.rows[self.term_list()];
            self.term_pick(row);
        }
        if k.b || k.l {
            self.term.notice = None;
            self.back();
            return None;
        }
        if k.r {
            return self.term_next();
        }
        let (x, y) = tap?;
        if inside(T_BACK, x, y) {
            self.back();
            return None;
        }
        if inside(T_NEXT, x, y) {
            return self.term_next();
        }
        if inside(T_MID, x, y) && self.step == Step::Area {
            self.cycle_time();
            return None;
        }
        if (LIST_Y..LIST_Y + ROWS as f32 * ROW).contains(&y) {
            let row = self.term_first() + ((y - LIST_Y) / ROW) as usize;
            if row < self.term_len() {
                self.term.rows[self.term_list()] = row;
                self.term_pick(row);
            }
        }
        None
    }

    fn term_next(&mut self) -> Option<Setup> {
        self.term.notice = None;
        self.message = None;
        self.next()
    }

    /// The focused row's catalog tab and index, for the island check.
    pub(super) fn term_focus(&self) -> Option<(Tab, usize)> {
        let row = self.term.rows[self.term_list()];
        match self.term_list() {
            0 => (row < self.areas.len()).then_some((Tab::Area, row)),
            1 => self.term_animals().get(row).map(|&i| (Tab::Targets, i)),
            _ => match self.gear_rows().get(row)? {
                GearRow::Weapon(i) => Some((Tab::Arms, *i)),
                GearRow::Equip(i) => Some((Tab::Gear, *i)),
                _ => None,
            },
        }
    }

    /// The pictures the terminal shows: the picked area's and the focused
    /// item's; and the focused item's description, read when it changes.
    pub(super) fn term_sync(&mut self, vfs: &Vfs) {
        let list = self.term_list();
        let row = self.term.rows[list];
        let (area_on, dino_on, gear_on) = match list {
            0 => (Some(row), None, None),
            1 => (None, self.term_animals().get(row).copied(), None),
            _ => (None, None, self.gear_rows().get(row).copied()),
        };
        let picked = self.area;
        for (i, a) in self.areas.iter_mut().enumerate() {
            a.1.want_pic(vfs, i == picked || area_on == Some(i));
        }
        for (i, d) in self.dinos.iter_mut().enumerate() {
            d.1.want_pic(vfs, dino_on == Some(i));
        }
        for (i, w) in self.weapons.iter_mut().enumerate() {
            w.want_pic(vfs, gear_on == Some(GearRow::Weapon(i)));
        }
        for (i, e) in self.equip.iter_mut().enumerate() {
            e.want_pic(vfs, gear_on == Some(GearRow::Equip(i)));
        }
        let key = (self.step, row);
        if self.term.desc.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let us = self.save.options.sys != 0;
        let read = |file: &str| -> Vec<String> {
            vfs.read(&format!("HUNTDAT/MENU/TXT/{file}"))
                .map(|d| {
                    String::from_utf8_lossy(&d)
                        .lines()
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let text = match list {
            0 => self
                .areas
                .get(row)
                .map(|(stem, _)| read(&format!("{stem}.TXT")))
                .unwrap_or_default(),
            1 => match self
                .term_animals()
                .get(row)
                .and_then(|&i| self.dinos.get(i))
            {
                Some((Pick::Slot(k), _)) => {
                    let ext = if us { "TXU" } else { "TXM" };
                    let t = read(&format!("DINO{}.{ext}", k + 1));
                    if c2_formats::menu_txt::is_blank(&t) {
                        self.rules
                            .menu
                            .huntables
                            .get(*k)
                            .map(|h| h.fallback_text(us))
                            .unwrap_or_default()
                    } else {
                        t
                    }
                }
                Some((Pick::Roamer(r), it)) => {
                    vec![format!("{}: {}", it.name, self.roam_text(r.char))]
                }
                None => Vec::new(),
            },
            _ => match self.gear_rows().get(row) {
                Some(GearRow::Weapon(i)) => read(&format!("WEAPON{}.TXT", i + 1)),
                Some(GearRow::Equip(i)) => {
                    let file = match EQUIPMENT.get(*i) {
                        Some(&"Camouflage") => "CAMOFLAG.NFO",
                        Some(&"Radar") => "RADAR.NFO",
                        _ => "SCENT.NFO",
                    };
                    read(file)
                }
                Some(GearRow::Ammo) => {
                    let mut v =
                        vec!["Extra ammunition (3DS): more reloads for every weapon.".to_string()];
                    for (k, (p, n, f)) in EXTRA_AMMO.iter().enumerate() {
                        let pen = if *f < 1.0 { ", kills -5%" } else { "" };
                        v.push(format!("x{}: {n} refill(s), {p} pts{pen}", k + 1));
                    }
                    v
                }
                Some(GearRow::Tranq) => read("TRANQ.NFO"),
                Some(GearRow::Observe) => read("OBSERVE.NFO"),
                None => Vec::new(),
            },
        };
        self.term.desc = Some((key, text));
    }
}
