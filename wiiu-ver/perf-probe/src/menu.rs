//! The Wii U menus: the 3DS hunt setup (hexagon grids for areas and animals,
//! times of day, weapons and gear) redrawn for a 16:9 TV and the GamePad,
//! with the original menu pictures and descriptions in an info panel.
//! Independent of GX2; C feeds it buttons and touches and shows the canvas.
use crate::hunt_setup::HuntChoice;
use crate::ui::{in_hex, measure, rect, Canvas, Clip, Font, Image, Rect, Rgba, TileCache, HEIGHT, WIDTH};
use core::cell::RefCell;
use alloc::{format, string::String, vec::Vec};

/// Files the menu reads, in slot order: pictures, then descriptions.
pub const FILES: [&str; 63] = [
    "MENUM.TGA", "AREA1.TGA", "AREA2.TGA", "AREA3.TGA", "AREA4.TGA", "AREA5.TGA",
    "DINO1.TGA", "DINO2.TGA", "DINO3.TGA", "DINO4.TGA", "DINO5.TGA", "DINO6.TGA",
    "DINO7.TGA", "DINO8.TGA", "DINO9.TGA", "WEAPON1.TGA", "WEAPON2.TGA", "WEAPON3.TGA",
    "WEAPON4.TGA", "WEAPON5.TGA", "WEAPON6.TGA", "EQUIP1.TGA", "EQUIP2.TGA", "EQUIP3.TGA",
    "EQUIP4.TGA",
    "AREA1.TXT", "AREA2.TXT", "AREA3.TXT", "AREA4.TXT", "AREA5.TXT",
    "DINO1.TXM", "DINO2.TXM", "DINO3.TXM", "DINO4.TXM", "DINO5.TXM", "DINO6.TXM",
    "DINO7.TXM", "DINO8.TXM", "DINO9.TXM",
    "DINO1.TXU", "DINO2.TXU", "DINO3.TXU", "DINO4.TXU", "DINO5.TXU", "DINO6.TXU",
    "DINO7.TXU", "DINO8.TXU", "DINO9.TXU",
    "WEAPON1.TXT", "WEAPON2.TXT", "WEAPON3.TXT", "WEAPON4.TXT", "WEAPON5.TXT", "WEAPON6.TXT",
    "CAMOFLAG.NFO", "RADAR.NFO", "SCENT.NFO", "DOUBLE.NFO", "TRANQ.NFO", "OBSERVE.NFO",
    "DAY1.NFO", "DAY2.NFO", "DAY3.NFO",
];
pub const PICTURES: usize = 25;
const PIC_MENU: usize = 0;
const PIC_AREA: usize = 1;
const PIC_DINO: usize = 6;
const PIC_WEAPON: usize = 15;
const PIC_EQUIP: usize = 21;
const TXT_AREA: usize = 0;
const TXT_DINO_METRIC: usize = 5;
const TXT_DINO_US: usize = 14;
const TXT_WEAPON: usize = 23;
/// Camouflage, radar, cover scent, double ammo, tranquilizer, observer.
const TXT_GEAR: usize = 29;
const TXT_TIME: usize = 35;

const AREAS: [&str; 5] = ["Delphaeus Hills", "Fort Ciskin", "Vengar Fjords", "Manya Jungle", "Mount Ravan"];
const ANIMALS: [&str; 9] = [
    "Parasaurolophus", "Ankylosaurus", "Stegosaurus", "Allosaurus", "Chasmosaurus",
    "Velociraptor", "Spinosaurus", "Ceratosaurus", "Tyrannosaurus",
];
const WEAPONS: [&str; 6] = ["Pistol", "Shotgun", "Double-barrel shotgun", "Crossbow", "Rifle", "Sniper rifle"];
const EQUIPMENT: [&str; 4] = ["Camouflage", "Radar", "Cover scent", "Double ammo"];
const TIMES: [&str; 3] = ["Dawn", "Day", "Night"];

const BG: Rgba = [16, 22, 16, 255];
const PANEL: Rgba = [32, 44, 30, 255];
const EDGE: Rgba = [90, 110, 68, 255];
const GOLD: Rgba = [240, 200, 70, 255];
const TEXT: Rgba = [160, 210, 130, 255];
const WHITE: Rgba = [255, 255, 255, 255];
const DIM: Rgba = [120, 128, 112, 255];
const RED: Rgba = [240, 80, 60, 255];
const SHADOW: Rgba = [0, 0, 0, 200];

/// Buttons, as C passes them (edges, one bit each).
pub mod button {
    pub const UP: u32 = 1;
    pub const DOWN: u32 = 2;
    pub const LEFT: u32 = 4;
    pub const RIGHT: u32 = 8;
    pub const A: u32 = 16;
    pub const B: u32 = 32;
    pub const PLUS: u32 = 64;
    pub const L: u32 = 128;
    pub const R: u32 = 256;
}

/// What the menu asks C to do.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    None = 0,
    Hunt = 1,
    Exit = 2,
    Restart = 3,
    MainMenu = 4,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Title,
    Area,
    Animals,
    Gear,
    Settings,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Id {
    Play,
    Settings,
    Exit,
    Area(usize),
    Time(usize),
    Animal(usize),
    Weapon(usize),
    /// Camouflage, radar, cover scent, double ammo.
    Equip(usize),
    Tranq,
    Observer,
    Back,
    Next,
    RunSpeed,
    Ambient,
    Blood,
    DeathCam,
    Units,
    Restart,
    MainMenu,
}

pub struct Menu {
    pub page: Page,
    pub focus: Id,
    pub choice: HuntChoice,
    pub action: Action,
    pub loading: bool,
    message: Option<&'static str>,
    death: (u32, u32),
    font: Font,
    pics: Vec<Option<Image>>,
    texts: Vec<String>,
    tiles: RefCell<TileCache>,
}

const SETUP: [Page; 3] = [Page::Area, Page::Animals, Page::Gear];
const BACK: Rect = rect(32.0, 648.0, 200.0, 60.0);
const NEXT: Rect = rect(1048.0, 648.0, 200.0, 60.0);
const INFO: Rect = rect(800.0, 100.0, 448.0, 524.0);
const SETTINGS_ROWS: [Id; 5] = [Id::RunSpeed, Id::Ambient, Id::Blood, Id::DeathCam, Id::Units];

fn flag(v: u32) -> &'static str {
    if v != 0 { "On" } else { "Off" }
}

/// The original descriptions break lines by hand: join prose lines into
/// paragraphs, keep "Key : value" lines and blank lines as breaks.
pub fn reflow(text: &str) -> String {
    let mut out = String::new();
    let mut prose = false;
    for line in text.lines().map(str::trim_end) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push_str(if out.ends_with('\n') { "\n" } else { "\n\n" });
            }
            prose = false;
        } else if trimmed.contains(" : ") || trimmed.contains(": ") && trimmed.len() < 40 {
            if prose {
                out.push('\n');
            }
            out.push_str(trimmed);
            out.push('\n');
            prose = false;
        } else {
            if prose {
                out.push(' ');
            } else if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(trimmed);
            prose = true;
        }
    }
    String::from(out.trim_end())
}

impl Menu {
    pub fn new(font: Font) -> Self {
        Menu {
            page: Page::Title,
            focus: Id::Play,
            choice: HuntChoice::default(),
            action: Action::None,
            loading: false,
            message: None,
            death: (0, 0),
            font,
            pics: (0..PICTURES).map(|_| None).collect(),
            texts: (PICTURES..FILES.len()).map(|_| String::new()).collect(),
            tiles: RefCell::new(TileCache::default()),
        }
    }

    /// A file from `FILES`; returns false if a picture does not decode.
    pub fn set_file(&mut self, slot: usize, bytes: &[u8]) -> bool {
        if slot < PICTURES {
            *self.tiles.borrow_mut() = TileCache::default();
            self.pics[slot] = Image::from_tga(bytes);
            self.pics[slot].is_some()
        } else if slot < FILES.len() {
            self.texts[slot - PICTURES] = reflow(&String::from_utf8_lossy(bytes));
            true
        } else {
            false
        }
    }

    pub fn open_title(&mut self) {
        self.page = Page::Title;
        self.focus = Id::Play;
        self.action = Action::None;
        self.loading = false;
        self.message = None;
    }

    pub fn open_death(&mut self, shots: u32, kills: u32) {
        self.page = Page::Death;
        self.focus = Id::Restart;
        self.action = Action::None;
        self.loading = false;
        self.death = (shots, kills);
    }

    fn pic(&self, slot: usize) -> Option<&Image> {
        self.pics.get(slot).and_then(Option::as_ref)
    }

    // ---- Settings file ------------------------------------------------

    pub fn settings_text(&self) -> String {
        let c = &self.choice;
        format!(
            "area={}\ntime={}\nanimals={}\nweapons={}\nradar={}\ncamo={}\nscent={}\ndouble_ammo={}\ntranq={}\nobserver={}\nrun_pct={}\nblood={}\ndeath_anim={}\nambient={}\nunits_us={}\n",
            c.area, c.time, c.animals, c.weapons, c.radar, c.camo, c.scent, c.double_ammo, c.tranq,
            c.observer, c.run_pct, c.blood, c.death_anim, c.ambient, c.units_us
        )
    }

    /// Unknown keys and bad values are ignored; values are clamped.
    pub fn load_settings(&mut self, text: &str) {
        let c = &mut self.choice;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let Ok(v) = v.trim().parse::<u32>() else { continue };
            let field = match k.trim() {
                "area" => &mut c.area,
                "time" => &mut c.time,
                "animals" => &mut c.animals,
                "weapons" => &mut c.weapons,
                "radar" => &mut c.radar,
                "camo" => &mut c.camo,
                "scent" => &mut c.scent,
                "double_ammo" => &mut c.double_ammo,
                "tranq" => &mut c.tranq,
                "observer" => &mut c.observer,
                "run_pct" => &mut c.run_pct,
                "blood" => &mut c.blood,
                "death_anim" => &mut c.death_anim,
                "ambient" => &mut c.ambient,
                "units_us" => &mut c.units_us,
                _ => continue,
            };
            *field = v;
        }
        c.area = c.area.min(4);
        c.time = c.time.min(2);
        c.animals &= 0x1ff;
        c.weapons &= 0x3f;
        c.run_pct = (c.run_pct / 10 * 10).clamp(50, 100);
        for f in [&mut c.radar, &mut c.camo, &mut c.scent, &mut c.double_ammo, &mut c.tranq, &mut c.observer, &mut c.blood, &mut c.death_anim, &mut c.ambient, &mut c.units_us] {
            *f = (*f != 0) as u32;
        }
    }

    // ---- Layout -------------------------------------------------------

    pub fn items(&self) -> Vec<(Id, Rect)> {
        let mut v = Vec::new();
        match self.page {
            Page::Title => {
                for (i, id) in [Id::Play, Id::Settings, Id::Exit].into_iter().enumerate() {
                    v.push((id, rect(72.0, 330.0 + i as f32 * 84.0, 420.0, 64.0)));
                }
            }
            Page::Area => {
                let (w, h) = (212.0, 184.0);
                let at = [(40.0, 100.0), (268.0, 100.0), (496.0, 100.0), (154.0, 296.0), (382.0, 296.0)];
                for (i, (x, y)) in at.into_iter().enumerate() {
                    v.push((Id::Area(i), rect(x, y, w, h)));
                }
                for i in 0..3 {
                    v.push((Id::Time(i), rect(40.0 + i as f32 * 228.0, 508.0, 212.0, 108.0)));
                }
            }
            Page::Animals => {
                let (w, h) = (196.0, 166.0);
                for i in 0..9 {
                    let (row, col) = (i / 3, i % 3);
                    let shift = if row == 1 { 106.0 } else { 0.0 };
                    v.push((Id::Animal(i), rect(40.0 + shift + col as f32 * 212.0, 98.0 + row as f32 * 176.0, w, h)));
                }
            }
            Page::Gear => {
                for i in 0..6 {
                    let (row, col) = (i / 3, i % 3);
                    let shift = if row == 1 { 93.0 } else { 0.0 };
                    v.push((Id::Weapon(i), rect(40.0 + shift + col as f32 * 186.0, 98.0 + row as f32 * 158.0, 170.0, 148.0)));
                }
                for i in 0..4 {
                    v.push((Id::Equip(i), rect(40.0 + i as f32 * 168.0, 418.0, 150.0, 130.0)));
                }
                v.push((Id::Tranq, rect(40.0, 564.0, 326.0, 56.0)));
                v.push((Id::Observer, rect(382.0, 564.0, 326.0, 56.0)));
            }
            Page::Settings => {
                for (i, id) in SETTINGS_ROWS.into_iter().enumerate() {
                    v.push((id, rect(200.0, 110.0 + i as f32 * 84.0, 880.0, 68.0)));
                }
            }
            Page::Death => {
                v.push((Id::Restart, rect(430.0, 420.0, 420.0, 64.0)));
                v.push((Id::MainMenu, rect(430.0, 500.0, 420.0, 64.0)));
            }
        }
        if SETUP.contains(&self.page) || self.page == Page::Settings {
            v.push((Id::Back, BACK));
        }
        if SETUP.contains(&self.page) {
            v.push((Id::Next, NEXT));
        }
        v
    }

    fn hexagonal(id: Id) -> bool {
        matches!(id, Id::Area(_) | Id::Animal(_) | Id::Weapon(_) | Id::Equip(_))
    }

    fn focus_rect(&self) -> Option<Rect> {
        self.items().into_iter().find(|(id, _)| *id == self.focus).map(|(_, r)| r)
    }

    // ---- Input --------------------------------------------------------

    /// Button edges; returns whether anything changed (redraw).
    pub fn input(&mut self, buttons: u32) -> bool {
        use button::*;
        if buttons == 0 || self.loading {
            return false;
        }
        self.message = None;
        if self.focus == Id::RunSpeed && buttons & (LEFT | RIGHT) != 0 {
            let step = if buttons & LEFT != 0 { -10 } else { 10 };
            self.choice.run_pct = (self.choice.run_pct as i32 + step).clamp(50, 100) as u32;
            return true;
        }
        for (bit, dir) in [(UP, (0, -1)), (DOWN, (0, 1)), (LEFT, (-1, 0)), (RIGHT, (1, 0))] {
            if buttons & bit != 0 {
                self.move_focus(dir);
            }
        }
        if buttons & A != 0 {
            self.activate(self.focus);
        }
        if buttons & B != 0 {
            self.back();
        }
        if buttons & PLUS != 0 && SETUP.contains(&self.page) {
            self.try_hunt();
        }
        if SETUP.contains(&self.page) && buttons & (L | R) != 0 {
            let at = SETUP.iter().position(|p| *p == self.page).unwrap();
            if buttons & L != 0 && at > 0 {
                self.go(SETUP[at - 1]);
            } else if buttons & R != 0 && at + 1 < SETUP.len() {
                self.go(SETUP[at + 1]);
            }
        }
        true
    }

    /// A GamePad touch in logical coordinates; returns whether it hit.
    pub fn touch(&mut self, x: f32, y: f32) -> bool {
        if self.loading {
            return false;
        }
        let hit = self.items().into_iter().find(|(id, r)| {
            if Self::hexagonal(*id) { in_hex(r, x, y) } else { r.contains(x, y) }
        });
        let Some((id, r)) = hit else { return false };
        self.message = None;
        self.focus = id;
        if id == Id::RunSpeed {
            // The left third slows, the right third speeds up, the middle steps.
            let step = if x < r.x + r.w / 3.0 { -10 } else { 10 };
            let pct = self.choice.run_pct as i32 + step;
            self.choice.run_pct = if pct > 100 { 50 } else { pct.max(50) } as u32;
            return true;
        }
        self.activate(id);
        true
    }

    fn move_focus(&mut self, (dx, dy): (i32, i32)) {
        let Some(from) = self.focus_rect() else {
            if let Some((id, _)) = self.items().first() {
                self.focus = *id;
            }
            return;
        };
        let (fx, fy) = from.center();
        let mut best: Option<(f32, Id)> = None;
        for (id, r) in self.items() {
            let (cx, cy) = r.center();
            let (ax, ay) = (cx - fx, cy - fy);
            let along = ax * dx as f32 + ay * dy as f32;
            let across = (ax * dy as f32 - ay * dx as f32).abs();
            if along <= 1.0 {
                continue;
            }
            // Squared, so the nearest row or column wins over a straight
            // line further away (the honeycomb rows are offset).
            let score = along * along + across * across * 4.0;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, id));
            }
        }
        if let Some((_, id)) = best {
            self.focus = id;
        }
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.focus = match page {
            Page::Title => Id::Play,
            Page::Area => Id::Area(self.choice.area as usize),
            Page::Animals => Id::Animal((0..9).find(|i| self.choice.animals & (1 << i) != 0).unwrap_or(0)),
            Page::Gear => Id::Weapon((0..6).find(|i| self.choice.weapons & (1 << i) != 0).unwrap_or(0)),
            Page::Settings => Id::RunSpeed,
            Page::Death => Id::Restart,
        };
    }

    fn back(&mut self) {
        match self.page {
            Page::Area | Page::Settings => {
                let from = if self.page == Page::Settings { Id::Settings } else { Id::Play };
                self.go(Page::Title);
                self.focus = from;
            }
            Page::Animals => self.go(Page::Area),
            Page::Gear => self.go(Page::Animals),
            Page::Title | Page::Death => {}
        }
    }

    fn problem(&self) -> Option<&'static str> {
        if self.choice.animals == 0 {
            Some("Pick at least one animal to hunt")
        } else if self.choice.weapons == 0 && self.choice.observer == 0 {
            Some("Pick a weapon, or Observer mode")
        } else {
            None
        }
    }

    fn try_hunt(&mut self) {
        match self.problem() {
            Some(m) => {
                self.message = Some(m);
                if self.choice.animals == 0 {
                    self.go(Page::Animals);
                }
            }
            None => {
                self.action = Action::Hunt;
                self.loading = true;
            }
        }
    }

    fn activate(&mut self, id: Id) {
        let c = &mut self.choice;
        match id {
            Id::Play => self.go(Page::Area),
            Id::Settings => self.go(Page::Settings),
            Id::Exit => self.action = Action::Exit,
            Id::Area(i) => c.area = i as u32,
            Id::Time(i) => c.time = i as u32,
            Id::Animal(i) => c.animals ^= 1 << i,
            Id::Weapon(i) => c.weapons ^= 1 << i,
            Id::Equip(0) => c.camo ^= 1,
            Id::Equip(1) => c.radar ^= 1,
            Id::Equip(2) => c.scent ^= 1,
            Id::Equip(_) => c.double_ammo ^= 1,
            Id::Tranq => c.tranq ^= 1,
            Id::Observer => c.observer ^= 1,
            Id::Back => self.back(),
            Id::Next => match self.page {
                Page::Area => self.go(Page::Animals),
                Page::Animals if c.animals == 0 => self.message = Some("Pick at least one animal to hunt"),
                Page::Animals => self.go(Page::Gear),
                _ => self.try_hunt(),
            },
            Id::RunSpeed => c.run_pct = if c.run_pct >= 100 { 50 } else { c.run_pct + 10 },
            Id::Ambient => c.ambient ^= 1,
            Id::Blood => c.blood ^= 1,
            Id::DeathCam => c.death_anim ^= 1,
            Id::Units => c.units_us ^= 1,
            Id::Restart => self.action = Action::Restart,
            Id::MainMenu => self.action = Action::MainMenu,
        }
    }

    // ---- Drawing ------------------------------------------------------

    pub fn draw(&self, c: &mut Canvas) {
        c.fill(rect(0.0, 0.0, WIDTH, HEIGHT), BG);
        match self.page {
            Page::Title => self.draw_title(c),
            Page::Settings => self.draw_settings(c),
            Page::Death => self.draw_death(c),
            _ => self.draw_setup(c),
        }
        if self.loading {
            c.fill(rect(0.0, 0.0, WIDTH, HEIGHT), [0, 0, 0, 150]);
            let r = rect(440.0, 300.0, 400.0, 120.0);
            c.fill(r, PANEL);
            c.frame(r, 3.0, GOLD);
            c.text_centered(&self.font, r, 36.0, GOLD, "Loading hunt...");
        }
    }

    fn shadowed(&self, c: &mut Canvas, x: f32, y: f32, size: f32, col: Rgba, s: &str) -> f32 {
        let o = (size / 16.0).max(1.5);
        c.text(&self.font, x + o, y + o, size, SHADOW, s);
        c.text(&self.font, x, y, size, col, s)
    }

    fn shadowed_centered(&self, c: &mut Canvas, r: Rect, size: f32, col: Rgba, s: &str) {
        let w = measure(&self.font, size, s);
        self.shadowed(c, r.x + (r.w - w) * 0.5, r.y + (r.h - size) * 0.5, size, col, s);
    }

    fn button(&self, c: &mut Canvas, r: Rect, label: &str, focused: bool, enabled: bool) {
        c.fill(r, if focused { [52, 70, 44, 255] } else { PANEL });
        c.frame(r, if focused { 4.0 } else { 2.0 }, if focused { WHITE } else { EDGE });
        c.text_centered(&self.font, r, 26.0, if enabled { WHITE } else { DIM }, label);
    }

    fn toggle(&self, c: &mut Canvas, r: Rect, label: &str, on: bool, focused: bool) {
        c.fill(r, if focused { [52, 70, 44, 255] } else { PANEL });
        c.frame(r, if focused { 4.0 } else { 2.0 }, if focused { WHITE } else if on { GOLD } else { EDGE });
        c.text(&self.font, r.x + 18.0, r.y + (r.h - 24.0) * 0.5, 24.0, WHITE, label);
        let state = flag(on as u32);
        let w = measure(&self.font, 24.0, state);
        c.text(&self.font, r.x + r.w - 18.0 - w, r.y + (r.h - 24.0) * 0.5, 24.0, if on { GOLD } else { DIM }, state);
    }

    fn draw_title(&self, c: &mut Canvas) {
        if let Some(art) = self.pic(PIC_MENU) {
            // The Spinosaurus from the original title picture: its right
            // part, below the painted logo and clear of the painted buttons.
            let (kx, ky) = (art.w as f32 / 800.0, art.h as f32 / 600.0);
            let src = rect(360.0 * kx, 180.0 * ky, 440.0 * kx, 420.0 * ky);
            let w = HEIGHT * src.w / src.h;
            c.image_cached(&mut self.tiles.borrow_mut(), PIC_MENU as u32, art, src, rect(WIDTH - w, 0.0, w, HEIGHT), Clip::None, 1.0);
        }
        c.fill(rect(0.0, 0.0, 540.0, HEIGHT), BG);
        c.fade_right(rect(540.0, 0.0, 120.0, HEIGHT), BG);
        self.shadowed(c, 72.0, 110.0, 76.0, GOLD, "CARNIVORES 2");
        self.shadowed(c, 76.0, 200.0, 28.0, TEXT, "Classic, rebuilt for Wii U");
        for (id, r) in self.items() {
            let label = match id {
                Id::Play => "Hunt",
                Id::Settings => "Settings",
                _ => "Exit",
            };
            self.button(c, r, label, self.focus == id, true);
        }
        c.text(&self.font, 72.0, 660.0, 20.0, DIM, "A: select   D-pad: move   or tap the GamePad");
    }

    fn header(&self, c: &mut Canvas, title: &str) {
        c.gradient(rect(0.0, 0.0, WIDTH, 80.0), [36, 50, 32, 255], [22, 30, 20, 255]);
        c.fill(rect(0.0, 80.0, WIDTH, 2.0), EDGE);
        self.shadowed(c, 32.0, 22.0, 34.0, GOLD, title);
    }

    fn draw_setup(&self, c: &mut Canvas) {
        self.header(c, "Hunt setup");
        let mut x = 460.0;
        for (i, (page, name)) in SETUP.iter().zip(["Area", "Animals", "Weapons & gear"]).enumerate() {
            let on = *page == self.page;
            let label = format!("{}. {name}", i + 1);
            let w = c.text(&self.font, x, 26.0, 26.0, if on { GOLD } else { DIM }, &label);
            if on {
                c.fill(rect(x, 60.0, w, 4.0), GOLD);
            }
            x += w + 48.0;
        }
        let ch = &self.choice;
        let observer = ch.observer != 0;
        for (id, r) in self.items() {
            let focused = self.focus == id;
            match id {
                Id::Area(i) => self.hex_item(c, r, PIC_AREA + i, AREAS[i], ch.area == i as u32, focused, true),
                Id::Animal(i) => self.hex_item(c, r, PIC_DINO + i, ANIMALS[i], ch.animals & (1 << i) != 0, focused, true),
                Id::Weapon(i) => self.hex_item(c, r, PIC_WEAPON + i, WEAPONS[i], ch.weapons & (1 << i) != 0, focused, !observer),
                Id::Equip(i) => {
                    let on = [ch.camo, ch.radar, ch.scent, ch.double_ammo][i] != 0;
                    self.hex_item(c, r, PIC_EQUIP + i, EQUIPMENT[i], on, focused, !observer || i == 1)
                }
                Id::Time(i) => self.time_item(c, r, i, ch.time == i as u32, focused),
                Id::Tranq => self.toggle(c, r, "Tranquilizer", ch.tranq != 0, focused),
                Id::Observer => self.toggle(c, r, "Observer mode", observer, focused),
                Id::Back => self.button(c, r, "Back", focused, true),
                Id::Next => {
                    let label = if self.page == Page::Gear { "Hunt!" } else { "Next" };
                    self.button(c, r, label, focused, self.page != Page::Gear || self.problem().is_none());
                }
                _ => {}
            }
        }
        self.draw_info(c);
        // Summary and hints between the bottom buttons.
        c.fill(rect(0.0, 632.0, WIDTH, 2.0), EDGE);
        let mid = rect(250.0, 642.0, 780.0, 36.0);
        match self.message {
            Some(m) => c.text_centered(&self.font, mid, 26.0, RED, m),
            None => {
                let animals = ch.animals.count_ones();
                let weapons = if observer { String::from("observer") } else { format!("{} weapon{}", ch.weapons.count_ones(), if ch.weapons.count_ones() == 1 { "" } else { "s" }) };
                let s = format!("{} - {} - {} animal{} - {}", AREAS[ch.area as usize], TIMES[ch.time as usize], animals, if animals == 1 { "" } else { "s" }, weapons);
                c.text_centered(&self.font, mid, 24.0, TEXT, &s);
            }
        }
        c.text_centered(&self.font, rect(250.0, 682.0, 780.0, 26.0), 18.0, DIM, "A: pick   B: back   L/R: page   +: start the hunt");
    }

    fn hex_item(&self, c: &mut Canvas, r: Rect, pic: usize, name: &str, on: bool, focused: bool, enabled: bool) {
        let bright = if !enabled { 0.25 } else if on { 1.0 } else { 0.5 };
        match self.pic(pic) {
            // Weapons and gear are long: the whole picture across the
            // hexagon, on the picture's own background colour.
            Some(img) if pic >= PIC_WEAPON => {
                let bg = img.px[img.w * img.h / 2 + 6];
                c.hex(r, Some(bg.map(|v| (v as f32 * bright) as u8)), 0.0, bg);
                let h = r.w * img.h as f32 / img.w as f32;
                let dst = rect(r.x, r.y + (r.h - h) * 0.5, r.w, h);
                c.image_cached(&mut self.tiles.borrow_mut(), pic as u32, img, rect(0.0, 0.0, img.w as f32, img.h as f32), dst, Clip::Hex(r), bright);
            }
            // Areas and animals: the middle of the picture fills the hexagon.
            Some(img) => {
                let h = img.h as f32 * 0.92;
                let w = h * r.w / r.h;
                let src = rect((img.w as f32 - w) * 0.5, (img.h as f32 - h) * 0.5, w, h);
                c.image_cached(&mut self.tiles.borrow_mut(), pic as u32, img, src, r, Clip::Hex(r), bright);
            }
            None => c.hex(r, Some(PANEL), 0.0, PANEL),
        }
        let outline = if focused { WHITE } else if on && enabled { GOLD } else { EDGE };
        c.hex(r, None, if focused || on { 5.0 } else { 2.0 }, outline);
        let size = if r.w > 180.0 { 20.0 } else { 17.0 };
        let label = rect(r.x, r.y + r.h - size - 22.0, r.w, size);
        self.shadowed_centered(c, label, size, if enabled { if on { GOLD } else { WHITE } } else { DIM }, name);
    }

    fn time_item(&self, c: &mut Canvas, r: Rect, which: usize, on: bool, focused: bool) {
        c.fill(r, if focused { [52, 70, 44, 255] } else { PANEL });
        c.frame(r, if focused || on { 4.0 } else { 2.0 }, if focused { WHITE } else if on { GOLD } else { EDGE });
        let (cx, cy) = (r.x + 58.0, r.y + r.h * 0.5);
        match which {
            0 => {
                // Dawn: a sun half over the horizon.
                c.disc(cx, cy + 10.0, 26.0, [250, 150, 60, 255]);
                c.fill(rect(cx - 36.0, cy + 10.0, 72.0, 30.0), if focused { [52, 70, 44, 255] } else { PANEL });
                c.fill(rect(cx - 36.0, cy + 10.0, 72.0, 3.0), [250, 150, 60, 255]);
            }
            1 => c.disc(cx, cy, 24.0, [255, 220, 90, 255]),
            _ => {
                c.disc(cx, cy, 24.0, [200, 220, 255, 255]);
                c.disc(cx + 11.0, cy - 7.0, 21.0, if focused { [52, 70, 44, 255] } else { PANEL });
            }
        }
        c.text(&self.font, r.x + 104.0, r.y + (r.h - 30.0) * 0.5, 30.0, if on { GOLD } else { WHITE }, TIMES[which]);
    }

    // The focused item's original picture and description.
    fn draw_info(&self, c: &mut Canvas) {
        let ch = &self.choice;
        let (pic, title, text): (Option<usize>, &str, usize) = match self.focus {
            Id::Area(i) => (Some(PIC_AREA + i), AREAS[i], TXT_AREA + i),
            Id::Time(i) => (Some(PIC_AREA + ch.area as usize), TIMES[i], TXT_TIME + i),
            Id::Animal(i) => (Some(PIC_DINO + i), ANIMALS[i], if ch.units_us != 0 { TXT_DINO_US } else { TXT_DINO_METRIC } + i),
            Id::Weapon(i) => (Some(PIC_WEAPON + i), WEAPONS[i], TXT_WEAPON + i),
            Id::Equip(i) => (Some(PIC_EQUIP + i), EQUIPMENT[i], TXT_GEAR + i),
            Id::Tranq => (None, "Tranquilizer", TXT_GEAR + 4),
            Id::Observer => (None, "Observer mode", TXT_GEAR + 5),
            _ => match self.page {
                Page::Area => (Some(PIC_AREA + ch.area as usize), AREAS[ch.area as usize], TXT_AREA + ch.area as usize),
                _ => (None, "", usize::MAX),
            },
        };
        c.fill(INFO, [10, 14, 10, 255]);
        c.frame(INFO, 2.0, EDGE);
        let mut y = INFO.y + 16.0;
        if let Some(img) = pic.and_then(|p| self.pic(p)) {
            let r = c.image_fit(&mut self.tiles.borrow_mut(), pic.unwrap() as u32, img, rect(INFO.x + 12.0, y, INFO.w - 24.0, 248.0), if self.focus == Id::Time(2) { 0.6 } else { 1.0 });
            y = r.y + r.h + 14.0;
        }
        if title.is_empty() {
            return;
        }
        self.shadowed(c, INFO.x + 20.0, y, 30.0, GOLD, title);
        y += 44.0;
        let Some(body) = self.texts.get(text) else { return };
        // Drop a first line that only repeats the name ("Weapon : Pistol").
        let mut body = body.as_str();
        if let Some((first, rest)) = body.split_once('\n') {
            if first.trim().eq_ignore_ascii_case(title) || first.starts_with("Weapon") || first.starts_with("Accessory") || first.starts_with("Hunting time") || first.starts_with("Name") {
                body = rest.trim_start();
            }
        }
        c.paragraph(&self.font, INFO.x + 20.0, y, INFO.w - 40.0, INFO.y + INFO.h - 12.0, 21.0, TEXT, body);
    }

    fn draw_settings(&self, c: &mut Canvas) {
        self.header(c, "Settings");
        let ch = &self.choice;
        for (id, r) in self.items() {
            let focused = self.focus == id;
            let (label, value, help) = match id {
                Id::RunSpeed => ("Dinosaur run speed", format!("<  {}%  >", ch.run_pct), "Slows every species by the same amount. 100% is the original."),
                Id::Ambient => ("Ambient animals", String::from(flag(ch.ambient)), "The small animals that score nothing."),
                Id::Blood => ("Blood", String::from(flag(ch.blood)), "Blood sprays and the trails of wounded animals."),
                Id::DeathCam => ("Death camera", String::from(flag(ch.death_anim)), "Off: straight to the restart screen when you die."),
                Id::Units => ("Animal descriptions", String::from(if ch.units_us != 0 { "US units" } else { "Metric" }), "Sizes and weights in the animal descriptions."),
                Id::Back => {
                    self.button(c, r, "Back", focused, true);
                    continue;
                }
                _ => continue,
            };
            c.fill(r, if focused { [52, 70, 44, 255] } else { PANEL });
            c.frame(r, if focused { 4.0 } else { 2.0 }, if focused { WHITE } else { EDGE });
            c.text(&self.font, r.x + 24.0, r.y + (r.h - 28.0) * 0.5, 28.0, WHITE, label);
            let w = measure(&self.font, 28.0, &value);
            c.text(&self.font, r.x + r.w - 24.0 - w, r.y + (r.h - 28.0) * 0.5, 28.0, GOLD, &value);
            if focused {
                c.text_centered(&self.font, rect(0.0, 548.0, WIDTH, 30.0), 22.0, TEXT, help);
            }
        }
        c.fill(rect(0.0, 632.0, WIDTH, 2.0), EDGE);
        c.text_centered(&self.font, rect(250.0, 660.0, 780.0, 26.0), 18.0, DIM, "A: change   Left/Right: run speed   B: back (settings are saved)");
    }

    fn draw_death(&self, c: &mut Canvas) {
        let panel = rect(240.0, 120.0, 800.0, 480.0);
        c.fill(panel, PANEL);
        c.frame(panel, 3.0, EDGE);
        self.shadowed_centered(c, rect(panel.x, 160.0, panel.w, 70.0), 64.0, RED, "You were killed");
        c.text_centered(&self.font, rect(panel.x, 250.0, panel.w, 40.0), 32.0, GOLD, "Your hunt has ended");
        let stats = format!("Shots {}   Kills {}", self.death.0, self.death.1);
        c.text_centered(&self.font, rect(panel.x, 316.0, panel.w, 36.0), 28.0, WHITE, &stats);
        for (id, r) in self.items() {
            self.button(c, r, if id == Id::Restart { "Restart hunt" } else { "Main menu" }, self.focus == id, true);
        }
    }
}
