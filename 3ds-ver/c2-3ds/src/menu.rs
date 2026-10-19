//! The hunt setup, touch-first (the owner's design, after the mobile game):
//! the bottom screen walks through three steps, the top screen sums up the
//! picks.
//!
//! 1. Area: a hexagon grid of the areas (and the trophy room), and the time
//!    of day as sun / sunset / moon.
//! 2. Animals: a hexagon grid of the huntable species (several can be
//!    picked).
//! 3. Weapons and gear: a hexagon grid of both, tranquilizer and observer
//!    toggles, then Hunt.
//!
//! Prices, points and what is remembered follow `c2-pc`'s `menu.rs`
//! (`Menu.cpp`): picks can't cost more than the hunter's score; camouflage,
//! radar, cover scent and the tranquilizer are kept in the save.

use c2_core::area::TimeOfDay;
use c2_core::dino::DinoInfo;
use c2_core::game::WeaponInfo;
use c2_formats::sav::Save;
use c2_vfs::Vfs;

use crate::draw2d::{in_hex, inside, Canvas, Rgba, WHITE};
use crate::gpu::{Gpu, Texture, BOTTOM_W, TOP_W};
use crate::hunt::{PackInfo, Setup};
use crate::ui::load;

/// Accessories (`EQUIPn.TGA`), in `prices.acces` order.
const EQUIPMENT: [&str; 4] = ["Camouflage", "Radar", "Cover Scent", "Double Ammo"];
/// Time-of-day choices as the original lists them (`dtm=`).
const TIMES: [(TimeOfDay, &str); 3] = [
    (TimeOfDay::Dusk, "Dawn"),
    (TimeOfDay::Day, "Day"),
    (TimeOfDay::Night, "Night"),
];

const BAR_Y: f32 = 212.0;
const BACK: [f32; 4] = [4.0, BAR_Y, 76.0, 24.0];
const NEXT: [f32; 4] = [240.0, BAR_Y, 76.0, 24.0];

const BG: Rgba = [16, 22, 16, 255];
const PANEL: Rgba = [32, 44, 30, 255];
const GOLD: Rgba = [240, 200, 70, 255];
const TEXT: Rgba = [160, 210, 130, 255];
const DIM: Rgba = [110, 110, 110, 255];
const RED: Rgba = [240, 80, 60, 255];
const DIMMED: Rgba = [140, 140, 140, 255];
/// Space between hexagons.
const GAP: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Area,
    Animals,
    Gear,
}

struct Item {
    name: String,
    price: i32,
    pic: Option<Texture>,
}

/// One cell of a hexagon grid: centre and radius.
type Cell = (f32, f32, f32);

/// `n` flat-topped hexagons in rows of `cols`, centred across the bottom
/// screen, the first row's centres at `top + h/2`.
fn grid(n: usize, cols: usize, r: f32, top: f32) -> Vec<Cell> {
    // Laid out for r + GAP/2, drawn at r.
    let rr = r + GAP / 2.0;
    let h = rr * 3f32.sqrt();
    let cols = cols.min(n).max(1);
    let width = (cols - 1) as f32 * 1.5 * rr + 2.0 * rr;
    let x0 = (BOTTOM_W as f32 - width) / 2.0 + rr;
    (0..n)
        .map(|i| {
            let (c, row) = (i % cols, i / cols);
            let y = top + h / 2.0 + row as f32 * h + if c % 2 == 1 { h / 2.0 } else { 0.0 };
            (x0 + c as f32 * 1.5 * rr, y, r)
        })
        .collect()
}

pub struct Menu {
    c: Canvas,
    info: PackInfo,
    pub save: Save,
    step: Step,
    /// Areas (`AREA1`..), then the trophy room if packed.
    areas: Vec<(String, Item)>,
    /// Huntable species: (AI, `_RES.TXT` character index, item).
    dinos: Vec<(i32, usize, Item)>,
    /// Characters always loaded (hunter, ambient animals).
    always: Vec<usize>,
    weapons: Vec<Item>,
    equip: Vec<Item>,
    area: usize,
    time: usize,
    /// Bits by position in `dinos`, `weapons`, `equip`.
    picked_dinos: u32,
    picked_weapons: u32,
    picked_equip: u32,
    observer: bool,
    /// The last item tapped, for its name and price under the grid.
    focus: Option<String>,
    message: Option<String>,
}

fn title(vfs: &Vfs, file: &str, fallback: String) -> String {
    vfs.read(&format!("HUNTDAT/MENU/TXT/{file}"))
        .ok()
        .and_then(|d| {
            String::from_utf8_lossy(&d)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string)
        })
        .unwrap_or(fallback)
}

impl Menu {
    /// The menu, with the hunter's save (a new hunter gets `prices.start`).
    pub fn new(vfs: &Vfs, info: PackInfo) -> anyhow::Result<Self> {
        let res = c2_core::load::res(vfs)?;
        let prices = res.prices.clone().unwrap_or_default();
        let save = crate::save::load(prices.start.unwrap_or(100));
        let price = |v: &Vec<i32>, i: usize| v.get(i).copied().unwrap_or(0);
        let mut areas = Vec::new();
        for (i, name) in info.areas.iter().enumerate() {
            let item = if name.eq_ignore_ascii_case("TROPHY") {
                Item {
                    name: "Trophies".into(),
                    price: 0,
                    pic: None,
                }
            } else {
                let n = i + 1;
                Item {
                    name: title(vfs, &format!("AREA{n}.TXT"), format!("Area {n}")),
                    price: price(&prices.area, i),
                    pic: load(vfs, &format!("C2RS/MENU/AREA{n}.TX")),
                }
            };
            areas.push((name.clone(), item));
        }
        let infos: Vec<DinoInfo> = res.characters.iter().map(DinoInfo::from_def).collect();
        // Huntable: AI 10 on, in order (`DINOn.TGA`, `prices.dino`).
        let mut dinos = Vec::new();
        for (k, ai) in (10..32).enumerate() {
            let Some(ci) = infos.iter().position(|d| d.ai == ai) else {
                break;
            };
            dinos.push((
                ai,
                ci,
                Item {
                    name: infos[ci].name.clone(),
                    price: price(&prices.dino, k),
                    pic: load(vfs, &format!("C2RS/MENU/DINO{}.TX", k + 1)),
                },
            ));
        }
        let always = (0..infos.len()).filter(|&i| infos[i].ai < 10).collect();
        let weapons = res
            .weapons
            .iter()
            .enumerate()
            .map(|(i, w)| Item {
                name: WeaponInfo::from_def(w).name,
                price: price(&prices.weapon, i),
                pic: load(vfs, &format!("C2RS/MENU/WEAPON{}.TX", i + 1)),
            })
            .collect();
        let equip = EQUIPMENT
            .iter()
            .enumerate()
            .map(|(i, n)| Item {
                name: n.to_string(),
                price: price(&prices.accessory, i),
                pic: load(vfs, &format!("C2RS/MENU/EQUIP{}.TX", i + 1)),
            })
            .collect();
        // Accessories are kept in the save (`CamoMode`, `RadarMode`,
        // `ScentMode`).
        let o = save.options;
        let picked_equip = [o.camo, o.radar, o.scent]
            .iter()
            .enumerate()
            .filter(|(_, &on)| on != 0)
            .fold(0, |m, (i, _)| m | 1 << i);
        let mut m = Self {
            c: Canvas::new(load(vfs, "C2RS/FONT.TX")),
            info,
            save,
            step: Step::Area,
            areas,
            dinos,
            always,
            weapons,
            equip,
            area: 0,
            time: 1,
            picked_dinos: 0,
            picked_weapons: 0,
            picked_equip,
            observer: false,
            focus: None,
            message: None,
        };
        if m.remaining() < 0 {
            m.picked_equip = 0;
        }
        Ok(m)
    }

    fn trophy(&self) -> bool {
        self.areas
            .get(self.area)
            .is_some_and(|a| a.0.eq_ignore_ascii_case("TROPHY"))
    }

    fn sum(items: &[Item], mask: u32) -> i32 {
        items
            .iter()
            .enumerate()
            .filter(|(i, _)| *i < 32 && mask & (1 << i) != 0)
            .map(|(_, it)| it.price)
            .sum()
    }

    /// Points left after paying for the picks (none needed to observe).
    fn remaining(&self) -> i32 {
        if self.observer || self.trophy() {
            return self.save.score;
        }
        let area = self.areas.get(self.area).map_or(0, |a| a.1.price);
        let dinos: i32 = self
            .dinos
            .iter()
            .enumerate()
            .filter(|(i, _)| self.picked_dinos & (1 << i) != 0)
            .map(|(_, d)| d.2.price)
            .sum();
        self.save.score
            - area
            - dinos
            - Self::sum(&self.weapons, self.picked_weapons)
            - Self::sum(&self.equip, self.picked_equip)
    }

    /// Linear memory the picks need, against what the console has (KiB).
    fn memory(&self) -> (usize, usize) {
        let i = &self.info;
        let mut kb = i.cost("hunt") + self.areas.get(self.area).map_or(0, |a| i.cost(&a.0));
        for &c in &self.always {
            kb += i.cost(&format!("char{c}"));
        }
        for (k, d) in self.dinos.iter().enumerate() {
            if self.picked_dinos & (1 << k) != 0 {
                kb += i.cost(&format!("char{}", d.1));
            }
        }
        for w in 0..self.weapons.len() {
            if self.picked_weapons & (1 << w) != 0 {
                kb += i.cost(&format!("wpn{w}"));
            }
        }
        (kb, i.linear_kb)
    }

    fn can_hunt(&self) -> Result<(), &'static str> {
        if self.trophy() {
            return Ok(());
        }
        if !self.observer && (self.picked_dinos == 0 || self.picked_weapons == 0) {
            return Err("Pick an animal and a weapon");
        }
        if self.remaining() < 0 {
            return Err("Not enough points");
        }
        let (need, have) = self.memory();
        if need > have {
            return Err("Too much for this console");
        }
        Ok(())
    }

    fn setup(&self) -> Setup {
        let targets = self
            .dinos
            .iter()
            .enumerate()
            .filter(|(k, _)| self.picked_dinos & (1 << k) != 0)
            .fold(0u32, |m, (_, d)| m | 1 << d.0);
        Setup {
            area: self
                .areas
                .get(self.area)
                .map_or("AREA1".into(), |a| a.0.clone()),
            time: TIMES[self.time].0,
            targets: if self.observer { 0 } else { targets },
            weapons: if self.observer {
                0
            } else {
                self.picked_weapons
            },
            double_ammo: !self.observer && self.picked_equip & 8 != 0,
            observer: self.observer,
        }
    }

    /// Keeps the accessory choice in the save, as the original does.
    fn store_equipment(&mut self) {
        let e = if self.observer { 0 } else { self.picked_equip };
        let o = &mut self.save.options;
        o.camo = i32::from(e & 1 != 0);
        o.radar = i32::from(e & 2 != 0);
        o.scent = i32::from(e & 4 != 0);
        if let Err(e) = crate::save::store(&self.save) {
            self.message = Some(format!("save failed: {e}"));
        }
    }

    /// Toggles bit `i` of `mask` unless that costs more than is left.
    fn toggle(&mut self, which: Step, i: usize, gear: bool) -> String {
        let (name, price) = match (which, gear) {
            (Step::Animals, _) => (self.dinos[i].2.name.clone(), self.dinos[i].2.price),
            (_, false) => (self.weapons[i].name.clone(), self.weapons[i].price),
            (_, true) => (self.equip[i].name.clone(), self.equip[i].price),
        };
        let mask = match (which, gear) {
            (Step::Animals, _) => &mut self.picked_dinos,
            (_, false) => &mut self.picked_weapons,
            (_, true) => &mut self.picked_equip,
        };
        *mask ^= 1 << i;
        let on = *mask & (1 << i) != 0;
        if on && self.remaining() < 0 {
            let mask = match (which, gear) {
                (Step::Animals, _) => &mut self.picked_dinos,
                (_, false) => &mut self.picked_weapons,
                (_, true) => &mut self.picked_equip,
            };
            *mask ^= 1 << i;
            self.message = Some("Not enough points".into());
        }
        format!("{name}  {price} pts")
    }

    fn area_cells(&self) -> Vec<Cell> {
        grid(self.areas.len(), 3, 34.0, 2.0)
    }

    fn time_cells() -> [Cell; 3] {
        [
            (116.0, 176.0, 16.0),
            (160.0, 176.0, 16.0),
            (204.0, 176.0, 16.0),
        ]
    }

    fn dino_cells(&self) -> Vec<Cell> {
        grid(self.dinos.len(), 5, 32.0, 2.0)
    }

    fn gear_cells(&self) -> Vec<Cell> {
        grid(self.weapons.len() + self.equip.len(), 5, 30.0, 2.0)
    }

    const TRANQ: [f32; 4] = [56.0, 178.0, 96.0, 24.0];
    const OBSERVE: [f32; 4] = [168.0, 178.0, 96.0, 24.0];

    /// A tap at bottom-screen (x, y); `Some` = start this hunt.
    pub fn tap(&mut self, x: f32, y: f32) -> Option<Setup> {
        self.message = None;
        if inside(BACK, x, y) {
            self.back();
            return None;
        }
        if inside(NEXT, x, y) {
            return self.next();
        }
        let hit = |cells: &[Cell]| {
            cells
                .iter()
                .position(|&(cx, cy, r)| in_hex(cx, cy, r, x, y))
        };
        match self.step {
            Step::Area => {
                if let Some(i) = hit(&self.area_cells()) {
                    self.area = i;
                    let a = &self.areas[i].1;
                    self.focus = Some(format!("{}  {} pts", a.name, a.price));
                } else if let Some(t) = hit(&Self::time_cells()) {
                    self.time = t;
                }
            }
            Step::Animals => {
                if let Some(i) = hit(&self.dino_cells()) {
                    self.focus = Some(self.toggle(Step::Animals, i, false));
                }
            }
            Step::Gear => {
                if let Some(i) = hit(&self.gear_cells()) {
                    let nw = self.weapons.len();
                    self.focus = Some(if i < nw {
                        self.toggle(Step::Gear, i, false)
                    } else {
                        self.toggle(Step::Gear, i - nw, true)
                    });
                } else if inside(Self::TRANQ, x, y) {
                    let t = &mut self.save.options.tranq;
                    *t = i32::from(*t == 0);
                } else if inside(Self::OBSERVE, x, y) {
                    self.observer = !self.observer;
                }
            }
        }
        None
    }

    /// B / the Back button.
    pub fn back(&mut self) {
        self.step = match self.step {
            Step::Area | Step::Animals => Step::Area,
            Step::Gear => Step::Animals,
        };
    }

    /// A / the Next (or Hunt) button.
    pub fn next(&mut self) -> Option<Setup> {
        match self.step {
            Step::Area if !self.trophy() => self.step = Step::Animals,
            Step::Animals => self.step = Step::Gear,
            _ => match self.can_hunt() {
                Ok(()) => {
                    self.store_equipment();
                    return Some(self.setup());
                }
                Err(e) => self.message = Some(e.into()),
            },
        }
        None
    }

    pub fn set_message(&mut self, m: String) {
        self.message = Some(m);
    }

    /// Both screens for this frame (call between `Gpu::begin` and `end`).
    pub fn draw(&mut self, gpu: &mut Gpu) {
        self.c.new_frame();
        self.draw_top(gpu);
        self.draw_bottom(gpu);
    }

    fn draw_hex_item(c: &mut Canvas, gpu: &mut Gpu, cell: Cell, it: &Item, on: bool, afford: bool) {
        let (cx, cy, r) = cell;
        if on {
            c.hex(cx, cy, r + 3.0, GOLD);
        }
        c.hex(cx, cy, r, PANEL);
        c.flush(gpu);
        let tint = if afford || on { WHITE } else { DIMMED };
        if let Some(t) = &it.pic {
            // Pictures are 357x208 fitted to 128 (or 256) wide: their top
            // 0.58 of the canvas; show the middle 2/3 of their width.
            c.hex_image(gpu, t, cell, [0.17, 0.0, 0.83, 0.58], tint);
        } else {
            let short: String = it.name.chars().take(8).collect();
            c.text_c(gpu, cx, cy - 4.0, 1.0, tint, &short);
        }
        if it.price > 0 {
            let p = format!("{}", it.price);
            let w = p.len() as f32 * 8.0 + 4.0;
            c.rect(
                [cx - w / 2.0, cy + r * 0.866 - 11.0, w, 10.0],
                [0, 0, 0, 170],
            );
            c.text_c(
                gpu,
                cx,
                cy + r * 0.866 - 10.0,
                1.0,
                if afford || on { GOLD } else { DIM },
                &p,
            );
        }
    }

    /// Sun, sunset or moon, drawn with shapes.
    fn time_icon(c: &mut Canvas, (cx, cy, r): Cell, which: usize) {
        match which {
            0 => {
                // Sunset: half a sun on the horizon.
                c.disc(cx, cy + 4.0, r * 0.6, [250, 140, 50, 255]);
                c.rect([cx - r, cy + 4.0, 2.0 * r, r * 0.7], [70, 40, 50, 255]);
                c.rect([cx - r, cy + 3.0, 2.0 * r, 2.0], [250, 190, 90, 255]);
            }
            1 => {
                for k in 0..8 {
                    let a = k as f32 / 8.0 * std::f32::consts::TAU;
                    let (s, co) = a.sin_cos();
                    c.disc(
                        cx + co * r * 0.8,
                        cy + s * r * 0.8,
                        2.5,
                        [250, 220, 80, 255],
                    );
                }
                c.disc(cx, cy, r * 0.5, [255, 230, 90, 255]);
            }
            _ => {
                c.disc(cx, cy, r * 0.6, [220, 225, 240, 255]);
                c.disc(cx + r * 0.3, cy - r * 0.2, r * 0.55, PANEL);
            }
        }
    }

    fn draw_bottom(&mut self, gpu: &mut Gpu) {
        gpu.begin_2d(Some([BG[0], BG[1], BG[2]]));
        let left = self.remaining();
        let step = self.step;
        let mut c = std::mem::replace(&mut self.c, Canvas::new(None));
        match step {
            Step::Area => {
                for (i, cell) in self.area_cells().into_iter().enumerate() {
                    let it = &self.areas[i].1;
                    let afford = it.price <= self.save.score;
                    Self::draw_hex_item(&mut c, gpu, cell, it, i == self.area, afford);
                }
                for (i, cell) in Self::time_cells().into_iter().enumerate() {
                    let (cx, cy, r) = cell;
                    if i == self.time {
                        c.hex(cx, cy, r + 3.0, GOLD);
                    }
                    c.hex(cx, cy, r, PANEL);
                    Self::time_icon(&mut c, cell, i);
                }
                c.flush(gpu);
            }
            Step::Animals => {
                for (i, cell) in self.dino_cells().into_iter().enumerate() {
                    let on = self.picked_dinos & (1 << i) != 0;
                    let afford = self.dinos[i].2.price <= left;
                    Self::draw_hex_item(&mut c, gpu, cell, &self.dinos[i].2, on, afford);
                }
            }
            Step::Gear => {
                let nw = self.weapons.len();
                for (i, cell) in self.gear_cells().into_iter().enumerate() {
                    let (it, on) = if i < nw {
                        (&self.weapons[i], self.picked_weapons & (1 << i) != 0)
                    } else {
                        (
                            &self.equip[i - nw],
                            self.picked_equip & (1 << (i - nw)) != 0,
                        )
                    };
                    Self::draw_hex_item(&mut c, gpu, cell, it, on, it.price <= left);
                }
                let tranq = self.save.options.tranq != 0;
                for (r, on, label) in [
                    (Self::TRANQ, tranq, "Tranq"),
                    (Self::OBSERVE, self.observer, "Observe"),
                ] {
                    c.rect(r, if on { GOLD } else { PANEL });
                    c.flush(gpu);
                    let col = if on { [0, 0, 0, 255] } else { WHITE };
                    c.text_c(gpu, r[0] + r[2] / 2.0, r[1] + 8.0, 1.0, col, label);
                }
            }
        }
        // What was tapped, or a message; then the bar.
        let line = self.message.clone().or_else(|| self.focus.clone());
        if let Some(l) = line {
            let col = if self.message.is_some() { RED } else { TEXT };
            let l: String = l.chars().take(38).collect();
            c.text_c(gpu, 160.0, 200.0, 1.0, col, &l);
        }
        let next = match step {
            Step::Area if self.trophy() => "Enter",
            Step::Gear => "HUNT",
            _ => "Next",
        };
        if step != Step::Area {
            c.rect(BACK, PANEL);
        }
        let ready = step != Step::Gear || self.can_hunt().is_ok();
        c.rect(NEXT, if ready { [60, 110, 60, 255] } else { PANEL });
        c.flush(gpu);
        if step != Step::Area {
            c.text_c(
                gpu,
                BACK[0] + BACK[2] / 2.0,
                BAR_Y + 8.0,
                1.0,
                WHITE,
                "Back",
            );
        }
        c.text_c(gpu, NEXT[0] + NEXT[2] / 2.0, BAR_Y + 8.0, 1.0, WHITE, next);
        let pts = format!("{left} pts");
        c.text_c(
            gpu,
            160.0,
            BAR_Y + 8.0,
            1.0,
            if left < 0 { RED } else { GOLD },
            &pts,
        );
        self.c = c;
    }

    fn draw_top(&mut self, gpu: &mut Gpu) {
        gpu.begin_2d(None);
        let mut c = std::mem::replace(&mut self.c, Canvas::new(None));
        let name = String::from_utf8_lossy(&self.save.player_name_raw)
            .trim_end_matches('\0')
            .to_string();
        let ranks = ["Beginner", "Advanced", "Expert"];
        let rank = ranks
            .get(self.save.rank.clamp(0, 2) as usize)
            .unwrap_or(&"");
        c.text(gpu, 8.0, 6.0, 1.0, TEXT, &format!("{name} ({rank})"));
        let pts = format!("Points: {} / {}", self.remaining(), self.save.score);
        c.text(
            gpu,
            TOP_W as f32 - 8.0 - pts.len() as f32 * 8.0,
            6.0,
            1.0,
            GOLD,
            &pts,
        );

        // The area and the time of day.
        if let Some((_, a)) = self.areas.get(self.area) {
            if let Some(t) = &a.pic {
                c.image(
                    gpu,
                    t,
                    [8.0, 22.0, 192.0, 112.0],
                    [0.0, 0.0, 1.0, 0.58],
                    WHITE,
                );
            } else {
                c.rect([8.0, 22.0, 192.0, 112.0], PANEL);
            }
            let n: String = a.name.chars().take(24).collect();
            c.text(gpu, 8.0, 140.0, 1.0, WHITE, &n);
        }
        if !self.trophy() {
            Self::time_icon(&mut c, (24.0, 168.0, 16.0), self.time);
            c.flush(gpu);
            c.text(gpu, 46.0, 164.0, 1.0, WHITE, TIMES[self.time].1);
        }

        // What is picked, with prices.
        let mut y = 22.0;
        let mut list = |c: &mut Canvas, gpu: &mut Gpu, head: &str, items: Vec<(String, i32)>| {
            c.text(gpu, 212.0, y, 1.0, TEXT, head);
            y += 10.0;
            if items.is_empty() {
                c.text(gpu, 220.0, y, 1.0, DIM, "-");
                y += 10.0;
            }
            for (n, p) in items {
                let n: String = n.chars().take(16).collect();
                c.text(gpu, 220.0, y, 1.0, WHITE, &n);
                if p > 0 {
                    let ps = p.to_string();
                    c.text(gpu, 392.0 - ps.len() as f32 * 8.0, y, 1.0, GOLD, &ps);
                }
                y += 9.0;
            }
            y += 4.0;
        };
        if self.trophy() {
            list(&mut c, gpu, "Your trophies", Vec::new());
        } else if self.observer {
            list(&mut c, gpu, "Observer: just watch", Vec::new());
        } else {
            let pick = |items: &[Item], mask: u32| -> Vec<(String, i32)> {
                items
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, it)| (it.name.clone(), it.price))
                    .collect()
            };
            let dinos: Vec<(String, i32)> = self
                .dinos
                .iter()
                .enumerate()
                .filter(|(i, _)| self.picked_dinos & (1 << i) != 0)
                .map(|(_, d)| (d.2.name.clone(), d.2.price))
                .collect();
            list(&mut c, gpu, "Animals", dinos);
            list(
                &mut c,
                gpu,
                "Weapons",
                pick(&self.weapons, self.picked_weapons),
            );
            let mut gear = pick(&self.equip, self.picked_equip);
            if self.save.options.tranq != 0 {
                gear.push(("Tranquilizer".into(), 0));
            }
            list(&mut c, gpu, "Gear", gear);
        }

        // Will it fit?
        let (need, have) = self.memory();
        let (msg, col) = match (self.step, self.can_hunt()) {
            (_, Ok(())) if self.step == Step::Gear || self.trophy() => (
                format!("Ready. Memory {}/{} MiB", need / 1024, have / 1024),
                TEXT,
            ),
            (Step::Gear, Err(e)) => (e.to_string(), if need > have { RED } else { DIM }),
            (Step::Area, _) => ("Pick a place and the time of day".into(), DIM),
            (Step::Animals, _) => ("Pick the animals to hunt".into(), DIM),
            (Step::Gear, _) => (String::new(), DIM),
        };
        c.text(gpu, 8.0, 226.0, 1.0, col, &msg);
        self.c = c;
    }
}
