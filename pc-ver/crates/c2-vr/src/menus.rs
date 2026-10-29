//! The menu screens on the panel, as on PC (`c2-pc menu.rs`): the main
//! screen and the hunt setup, from the game's own pictures, hit maps and
//! texts, drawn on the `canvas`. A controller's ray points (the item under
//! it is focused, the trigger takes it); the D-pad or the left stick moves
//! the focus, A or + takes it, B goes back. The focus is a gold box.
//!
//! In VR so far: HUNT (the setup: area, animals, time of day, observer
//! mode), TROPHY and QUIT. Options, credits, statistics, fog, the
//! tranquilizer, weapons and equipment are dimmed (no guns in VR yet), and
//! hunts cost nothing (there's no account in VR yet).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use c2_core::area::TimeOfDay;
use c2_formats::bits::Bits;
use c2_formats::menu_txt::{MenuArea, MenuData};
use c2_formats::res_txt::ResTxt;

use crate::canvas::{self, text_width, Canvas, Rect, Rgba};
use crate::panel::find_ci;
use crate::scene::{Input, Ray};

/// `c2-pc menu.rs`'s colours.
const TEXT: [f32; 4] = [0.62, 0.82, 0.5, 1.0];
const PICKED: [f32; 4] = [1.0, 0.85, 0.3, 1.0];
const DIM: [f32; 4] = [0.36, 0.42, 0.3, 1.0];
const HOVER: [f32; 4] = [0.9, 1.0, 0.75, 1.0];
/// The focus box (the launcher's gold).
const GOLD: [f32; 4] = [0.94, 0.78, 0.27, 1.0];

/// Main screen regions (`MAIN_MAP.RAW`).
const HUNT: u8 = 1;
const OPTIONS: u8 = 2;
const TROPHY: u8 = 3;
const CREDITS: u8 = 4;
const QUIT: u8 = 5;
const STATS: u8 = 6;
/// Top to bottom; the D-pad steps through these.
const MAIN_OFFERED: [u8; 3] = [HUNT, TROPHY, QUIT];

/// Setup screen regions (`M2_MAP.RAW`): dawn, day, night, fog,
/// tranquilizer, observer, back, hunt.
const FOG: u8 = 4;
const TRANQ: u8 = 5;
const OBSERVER: u8 = 6;
const BACK: u8 = 7;
const GO: u8 = 8;

/// The lists' boxes and rows (`c2-pc menu.rs`).
const AREAS_RECT: Rect = (12.0, 376.0, 182.0, 170.0);
const DINOS_RECT: Rect = (210.0, 376.0, 184.0, 170.0);
const WEAPONS_RECT: Rect = (410.0, 376.0, 184.0, 170.0);
const EQUIP_RECT: Rect = (610.0, 376.0, 184.0, 170.0);
const ROW_H: f32 = 19.0;
const ROWS: usize = 8;
const EQUIPMENT: [&str; 4] = ["Camouflage", "Radar", "Cover Scent", "Double Ammo"];
/// The info panel's text width.
const INFO_W: f32 = 340.0;

/// A menu screen's pictures (`*.TGA`, `*_ON.TGA`, `*_MAP.RAW`).
struct Board {
    base: Rgba,
    on: Rgba,
    /// Region ids, 400x300.
    hit: Vec<u8>,
    /// Each region's bounding box.
    boxes: Vec<Option<Rect>>,
}

fn read_tga(path: &Path) -> Result<Rgba> {
    let t = c2_formats::tga::parse(
        &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
    )
    .with_context(|| format!("{}", path.display()))?;
    Ok(Rgba {
        w: t.width as usize,
        h: t.height as usize,
        px: c2_core::color::image_to_rgba8(&t.pixels, false),
    })
}

impl Board {
    fn load(dir: &Path, base: &str, on: &str, map: &str) -> Result<Self> {
        let file = |n: &str| find_ci(dir, n).ok_or_else(|| anyhow!("no MENU/{n}"));
        let hit = std::fs::read(file(map)?)?;
        if hit.len() != 400 * 300 {
            bail!("{map}: {} bytes, not 400x300", hit.len());
        }
        let base = read_tga(&file(base)?)?;
        if (base.w, base.h) != (canvas::W, canvas::H) {
            bail!("the menu pictures aren't 800x600");
        }
        let mut bb = vec![None::<[usize; 4]>; 256];
        for (i, &r) in hit.iter().enumerate() {
            if r != 0 {
                let (x, y) = (i % 400, i / 400);
                let b = bb[r as usize].get_or_insert([x, y, x, y]);
                *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
        Ok(Self {
            on: read_tga(&file(on)?)?,
            base,
            boxes: bb
                .into_iter()
                .map(|b| {
                    b.map(|[x0, y0, x1, y1]| {
                        let f = |v: usize| (v * 2) as f32;
                        (f(x0), f(y0), f(x1 + 1 - x0), f(y1 + 1 - y0))
                    })
                })
                .collect(),
            hit,
        })
    }

    fn region(&self, c: (f32, f32)) -> u8 {
        let (x, y) = (c.0 as usize / 2, c.1 as usize / 2);
        if c.0 < 0.0 || c.1 < 0.0 || x >= 400 || y >= 300 {
            return 0;
        }
        self.hit[y * 400 + x]
    }

    fn rect(&self, region: u8) -> Option<Rect> {
        self.boxes.get(region as usize).copied().flatten()
    }
}

/// A list item in the setup.
struct Item {
    name: String,
    price: i32,
    pic: Option<Rgba>,
    text: Vec<String>,
}

struct List {
    rect: Rect,
    items: Vec<Item>,
    /// Items by row: cheapest first (`c2-pc menu.rs`).
    order: Vec<usize>,
    scroll: usize,
}

impl List {
    fn new(rect: Rect, items: Vec<Item>) -> Self {
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&i| items[i].price);
        Self {
            rect,
            items,
            order,
            scroll: 0,
        }
    }

    /// Row `row`'s box.
    fn row_rect(&self, row: usize) -> Rect {
        let y = self.rect.1 + 4.0 + (row as f32 - self.scroll as f32) * ROW_H;
        (self.rect.0 + 2.0, y, self.rect.2 - 4.0, ROW_H)
    }

    fn visible(&self) -> std::ops::Range<usize> {
        self.scroll..(self.scroll + ROWS).min(self.order.len())
    }

    fn row_of(&self, item: usize) -> Option<usize> {
        self.order.iter().position(|&i| i == item)
    }

    /// Scrolls so `item` shows.
    fn show(&mut self, item: usize) {
        if let Some(r) = self.row_of(item) {
            if r < self.scroll {
                self.scroll = r;
            } else if r >= self.scroll + ROWS {
                self.scroll = r + 1 - ROWS;
            }
        }
    }

    fn draw(&self, out: &mut Canvas, color: impl Fn(usize) -> [f32; 4]) {
        for row in self.visible() {
            let i = self.order[row];
            let it = &self.items[i];
            let (x, y, w, _) = self.row_rect(row);
            let c = color(i);
            let p = it.price.to_string();
            let pw = text_width(12.0, &p);
            let mut name = it.name.clone();
            while text_width(12.0, &name) > w - 18.0 - pw && name.pop().is_some() {}
            out.text(x + 4.0, y + 2.0, 12.0, c, &name);
            out.text(x + w - 6.0 - pw, y + 2.0, 12.0, c, &p);
        }
        let mid = self.rect.0 + self.rect.2 / 2.0 - 4.0;
        if self.scroll > 0 {
            out.text(mid, self.rect.1 - 6.0, 12.0, TEXT, "^");
        }
        if self.scroll + ROWS < self.order.len() {
            out.text(mid, self.rect.1 + self.rect.3 - 8.0, 12.0, TEXT, "v");
        }
    }
}

fn inside(c: (f32, f32), r: Rect) -> bool {
    c.0 >= r.0 && c.1 >= r.1 && c.0 < r.0 + r.2 && c.1 < r.1 + r.3
}

fn center(r: Rect) -> (f32, f32) {
    (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0)
}

/// What can have the focus in the setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    /// Dawn, day, night.
    Time(usize),
    Observer,
    Area(usize),
    Dino(usize),
    Back,
    Go,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Setup,
}

/// A hunt the setup asks for.
#[derive(Clone, Debug)]
pub struct Launch {
    pub map: PathBuf,
    pub time: TimeOfDay,
    /// Bit per AI (`TargetDino`).
    pub targets: Bits,
    pub observer: bool,
}

pub enum Action {
    None,
    Hunt(Launch),
    Trophy,
    Quit,
}

pub struct Menus {
    screen: Screen,
    main: Board,
    setup: Board,
    /// The main screen's focus (a region).
    main_sel: u8,
    areas: List,
    dinos: List,
    maps: Vec<PathBuf>,
    weapons: List,
    equipment: List,
    day_texts: Vec<Vec<String>>,
    observe_text: Vec<String>,
    // The setup's choices (`c2-pc menu.rs: Choice`).
    time: usize,
    observer: bool,
    area: usize,
    /// Bit per menu slot.
    dinos_picked: Bits,
    focus: Focus,
    /// The list item whose picture and text show.
    shown: Option<Focus>,
    note: Option<&'static str>,
    pointer: Option<(f32, f32)>,
    /// The D-pad and left stick held (each steps once per push).
    dpad_was: [i8; 2],
    stick_was: [i8; 2],
    dirty: bool,
}

fn read_text(dir: Option<&Path>, name: &str) -> Vec<String> {
    dir.and_then(|d| find_ci(d, name))
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect()
        })
        .unwrap_or_default()
}

impl Menus {
    /// From the game's `HUNTDAT` folders (overlays first).
    pub fn load(huntdat: &[PathBuf]) -> Result<Self> {
        let dir = huntdat
            .iter()
            .filter_map(|h| find_ci(h, "MENU"))
            .find(|m| find_ci(m, "MENUM.TGA").is_some())
            .ok_or_else(|| anyhow!("no HUNTDAT/MENU/MENUM.TGA"))?;
        let main = Board::load(&dir, "MENUM.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW")?;
        let setup = Board::load(&dir, "MENU2.TGA", "MENU2_ON.TGA", "M2_MAP.RAW")?;
        let pics = find_ci(&dir, "PICS");
        let txt = find_ci(&dir, "TXT");
        let pic = |name: &str| {
            pics.as_deref()
                .and_then(|p| find_ci(p, name))
                .and_then(|p| read_tga(&p).ok())
        };
        let src = huntdat
            .iter()
            .find_map(|h| find_ci(h, "_MENU.TXT").or_else(|| find_ci(h, "_RES.TXT")))
            .and_then(|p| std::fs::read(p).ok())
            .map(|d| String::from_utf8_lossy(&d).into_owned())
            .unwrap_or_default();
        let mut menu = MenuData::from_res(&ResTxt::parse(&src)?);
        let areas_dir = huntdat.iter().find_map(|h| find_ci(h, "AREAS"));
        let map_of = |stem: &str| find_ci(areas_dir.as_deref()?, &format!("{stem}.MAP"));
        if menu.areas.is_empty() {
            for n in 1..=9 {
                if map_of(&format!("area{n}")).is_none() {
                    break;
                }
                menu.areas.push(MenuArea {
                    name: None,
                    project: format!("area{n}"),
                    price: 0,
                    number: Some(n),
                    pic: None,
                    text: None,
                    snow: false,
                });
            }
        }
        let wrap = |t: Vec<String>| canvas::wrap(12.0, INFO_W, &t);
        let (mut areas, mut maps) = (Vec::new(), Vec::new());
        for a in &menu.areas {
            let Some(map) = map_of(&a.project) else {
                continue;
            };
            let text = a
                .number
                .map(|n| read_text(txt.as_deref(), &format!("AREA{n}.TXT")))
                .unwrap_or_default();
            areas.push(Item {
                name: a
                    .name
                    .clone()
                    .or_else(|| text.first().filter(|l| !l.is_empty()).cloned())
                    .unwrap_or_else(|| a.project.clone()),
                price: a.price,
                pic: a.number.and_then(|n| pic(&format!("AREA{n}.TGA"))),
                text: wrap(text),
            });
            maps.push(map);
        }
        if areas.is_empty() {
            bail!("no areas in the game");
        }
        let dinos = menu
            .huntables
            .iter()
            .enumerate()
            .map(|(k, d)| Item {
                name: d.name.clone(),
                price: d.price,
                pic: pic(&format!("DINO{}.TGA", k + 1)),
                text: wrap(read_text(txt.as_deref(), &format!("DINO{}.TXM", k + 1))),
            })
            .collect();
        let plain = |name: &str, price: i32| Item {
            name: name.to_string(),
            price,
            pic: None,
            text: Vec::new(),
        };
        let weapons = menu
            .weapons
            .iter()
            .map(|w| plain(&w.name, w.price))
            .collect();
        let equipment = EQUIPMENT
            .iter()
            .enumerate()
            .map(|(i, n)| plain(n, menu.accessory_prices.get(i).copied().unwrap_or(0)))
            .collect();
        let mut equipment = List::new(EQUIP_RECT, equipment);
        equipment.order = (0..EQUIPMENT.len()).collect();
        Ok(Self {
            screen: Screen::Main,
            main,
            setup,
            main_sel: HUNT,
            areas: List::new(AREAS_RECT, areas),
            dinos: List::new(DINOS_RECT, dinos),
            maps,
            weapons: List::new(WEAPONS_RECT, weapons),
            equipment,
            day_texts: (1..=3)
                .map(|n| wrap(read_text(txt.as_deref(), &format!("DAY{n}.NFO"))))
                .collect(),
            observe_text: wrap(read_text(txt.as_deref(), "OBSERVE.NFO")),
            time: 1,
            observer: false,
            area: 0,
            dinos_picked: Bits::NONE,
            focus: Focus::Area(0),
            shown: None,
            note: None,
            pointer: None,
            dpad_was: [0; 2],
            stick_was: [0; 2],
            dirty: true,
        })
    }

    /// The pointer, canvas units.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.pointer
    }

    /// The canvas changed since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// A step this frame: -1 / 1 per axis, once per push of the D-pad or
    /// the stick.
    fn steps(&mut self, input: &Input) -> (i32, i32) {
        let edge = |now: i8, was: &mut i8| {
            let s = if now != *was { now } else { 0 };
            *was = now;
            i32::from(s)
        };
        let [dx, dy] = input.pad.dpad;
        let stick = |v: f32, was: i8| {
            if v.abs() > 0.6 {
                v.signum() as i8
            } else if v.abs() < 0.3 {
                0
            } else {
                was
            }
        };
        let [sx, sy] = input.pad.walk;
        let s = [stick(sx, self.stick_was[0]), stick(sy, self.stick_was[1])];
        let x = edge(dx, &mut self.dpad_was[0]) + edge(s[0], &mut self.stick_was[0]);
        let y = edge(dy, &mut self.dpad_was[1]) + edge(s[1], &mut self.stick_was[1]);
        // Canvas y grows downwards.
        (x.signum(), -y.signum())
    }

    /// This frame's input; `pick` says where a ray meets the panel.
    pub fn update(&mut self, input: &Input, pick: impl Fn(&Ray) -> Option<(f32, f32)>) -> Action {
        use c2_pad::Action as Pad;
        let mut take = false;
        self.pointer = None;
        // The right hand first: it's the one pointing, usually.
        for hand in [1, 0] {
            let Some(c) = input.aim[hand].and_then(|r| pick(&r)) else {
                continue;
            };
            self.pointer = Some(c);
            let hovered = self.hover(c);
            take = hovered && input.select[hand];
            break;
        }
        let (sx, sy) = self.steps(input);
        if sx != 0 || sy != 0 {
            self.step(sx, sy);
        }
        if take || input.pad.pressed(Pad::Jump) || input.pad.pressed(Pad::Pause) {
            return self.take();
        }
        if input.pad.pressed(Pad::Run) && self.screen == Screen::Setup {
            self.screen = Screen::Main;
            self.dirty = true;
        }
        Action::None
    }

    fn set_focus(&mut self, f: Focus) {
        if self.focus != f {
            self.focus = f;
            self.note = None;
            if matches!(f, Focus::Area(_) | Focus::Dino(_)) {
                self.shown = Some(f);
            }
            match f {
                Focus::Area(i) => self.areas.show(i),
                Focus::Dino(i) => self.dinos.show(i),
                _ => {}
            }
            self.dirty = true;
        }
    }

    /// The pointer over `c`: focuses what's there. Whether anything is.
    fn hover(&mut self, c: (f32, f32)) -> bool {
        match self.screen {
            Screen::Main => {
                let r = self.main.region(c);
                if MAIN_OFFERED.contains(&r) {
                    if self.main_sel != r {
                        self.main_sel = r;
                        self.dirty = true;
                    }
                    return true;
                }
                false
            }
            Screen::Setup => match self.focus_at(c) {
                Some(f) => {
                    self.set_focus(f);
                    true
                }
                None => false,
            },
        }
    }

    /// The setup's focusable things with their boxes (lists: the rows
    /// showing).
    fn targets(&self) -> Vec<(Focus, Rect)> {
        let mut v = Vec::new();
        for (f, r) in [
            (Focus::Time(0), 1),
            (Focus::Time(1), 2),
            (Focus::Time(2), 3),
            (Focus::Observer, OBSERVER),
            (Focus::Back, BACK),
            (Focus::Go, GO),
        ] {
            if let Some(b) = self.setup.rect(r) {
                v.push((f, b));
            }
        }
        for row in self.areas.visible() {
            v.push((Focus::Area(self.areas.order[row]), self.areas.row_rect(row)));
        }
        for row in self.dinos.visible() {
            v.push((Focus::Dino(self.dinos.order[row]), self.dinos.row_rect(row)));
        }
        v
    }

    fn focus_at(&self, c: (f32, f32)) -> Option<Focus> {
        self.targets()
            .into_iter()
            .find(|(_, r)| inside(c, *r))
            .map(|(f, _)| f)
    }

    fn focus_rect(&self) -> Option<Rect> {
        let f = self.focus;
        match f {
            Focus::Area(i) => self.areas.row_of(i).map(|r| self.areas.row_rect(r)),
            Focus::Dino(i) => self.dinos.row_of(i).map(|r| self.dinos.row_rect(r)),
            _ => self
                .targets()
                .into_iter()
                .find(|(t, _)| *t == f)
                .map(|(_, r)| r),
        }
    }

    /// Moves the focus one step (canvas directions).
    fn step(&mut self, dx: i32, dy: i32) {
        if self.screen == Screen::Main {
            if dy != 0 {
                let i = MAIN_OFFERED
                    .iter()
                    .position(|&r| r == self.main_sel)
                    .unwrap_or(0) as i32;
                let n = MAIN_OFFERED.len() as i32;
                self.main_sel = MAIN_OFFERED[(i + dy).rem_euclid(n) as usize];
                self.dirty = true;
            }
            return;
        }
        // Up and down a list go row by row (scrolling); off its ends, and
        // sideways, to the nearest thing that way.
        if dy != 0 && dx == 0 {
            let (list, wrap): (&List, fn(usize) -> Focus) = match self.focus {
                Focus::Area(_) => (&self.areas, Focus::Area),
                Focus::Dino(_) => (&self.dinos, Focus::Dino),
                _ => (&self.areas, Focus::Area),
            };
            let item = match self.focus {
                Focus::Area(i) | Focus::Dino(i) => Some(i),
                _ => None,
            };
            if let Some(row) = item.and_then(|i| list.row_of(i)) {
                let next = row as i32 + dy;
                if (0..list.order.len() as i32).contains(&next) {
                    let f = wrap(list.order[next as usize]);
                    self.set_focus(f);
                    return;
                }
            }
        }
        let Some(here) = self.focus_rect() else {
            return;
        };
        let from = center(here);
        // In line with the focus (sharing its rows, or its columns), else
        // within 45 degrees of the push; nothing further off (BACK's left
        // isn't DAWN).
        let in_line = |r: Rect| {
            if dx != 0 {
                r.1 < here.1 + here.3 && here.1 < r.1 + r.3
            } else {
                r.0 < here.0 + here.2 && here.0 < r.0 + r.2
            }
        };
        let (dx, dy) = (dx as f32, dy as f32);
        let best = self
            .targets()
            .into_iter()
            .filter(|(f, _)| *f != self.focus)
            .filter_map(|(f, r)| {
                let (cx, cy) = center(r);
                let (ox, oy) = (cx - from.0, cy - from.1);
                let along = ox * dx + oy * dy;
                let across = (ox * dy - oy * dx).abs();
                let tier = if in_line(r) {
                    0
                } else if across <= along {
                    1
                } else {
                    return None;
                };
                (along > 1.0).then_some((f, (tier, along + 2.0 * across)))
            })
            .min_by(|a, b| a.1 .0.cmp(&b.1 .0).then(a.1 .1.total_cmp(&b.1 .1)));
        if let Some((f, _)) = best {
            self.set_focus(f);
        }
    }

    /// The focused thing is taken.
    fn take(&mut self) -> Action {
        self.dirty = true;
        if self.screen == Screen::Main {
            return match self.main_sel {
                HUNT => {
                    self.screen = Screen::Setup;
                    Action::None
                }
                TROPHY => Action::Trophy,
                QUIT => Action::Quit,
                _ => Action::None,
            };
        }
        match self.focus {
            Focus::Time(t) => self.time = t,
            Focus::Observer => self.observer = !self.observer,
            Focus::Area(i) => self.area = i,
            Focus::Dino(i) => {
                if self.dinos_picked.has(i) {
                    self.dinos_picked.clear(i);
                } else {
                    self.dinos_picked.set(i);
                }
            }
            Focus::Back => self.screen = Screen::Main,
            Focus::Go => {
                if !self.observer && self.dinos_picked.is_empty() {
                    self.note = Some("Choose dinosaurs to hunt");
                    return Action::None;
                }
                let Some(map) = self.maps.get(self.area).cloned() else {
                    return Action::None;
                };
                // `c2-pc menu.rs: launch_hunt`: an observer's hunt has no
                // targets (the original places a random mix).
                return Action::Hunt(Launch {
                    map,
                    time: [TimeOfDay::Dusk, TimeOfDay::Day, TimeOfDay::Night][self.time],
                    targets: if self.observer {
                        Bits::NONE
                    } else {
                        self.dinos_picked.up(10)
                    },
                    observer: self.observer,
                });
            }
        }
        Action::None
    }

    /// Draws the screen showing.
    pub fn compose(&self, out: &mut Canvas) {
        match self.screen {
            Screen::Main => {
                let b = &self.main;
                out.board(
                    &b.base,
                    &b.on,
                    &b.hit,
                    &[self.main_sel],
                    &[OPTIONS, CREDITS, STATS],
                );
                if let Some(r) = b.rect(self.main_sel) {
                    out.outline(r, 4.0, GOLD);
                }
            }
            Screen::Setup => self.compose_setup(out),
        }
    }

    fn compose_setup(&self, out: &mut Canvas) {
        let b = &self.setup;
        let mut lit = vec![self.time as u8 + 1];
        if self.observer {
            lit.push(OBSERVER);
        }
        if self.focus == Focus::Back {
            lit.push(BACK);
        }
        if self.focus == Focus::Go {
            lit.push(GO);
        }
        out.board(&b.base, &b.on, &b.hit, &lit, &[FOG, TRANQ]);
        self.areas
            .draw(out, |i| if i == self.area { PICKED } else { TEXT });
        self.dinos.draw(out, |i| {
            if self.dinos_picked.has(i) {
                PICKED
            } else {
                TEXT
            }
        });
        self.weapons.draw(out, |_| DIM);
        self.equipment.draw(out, |_| DIM);
        let (wx, wy, ww, wh) = WEAPONS_RECT;
        let note = "Not in VR yet";
        out.text(
            wx + (ww - text_width(12.0, note)) / 2.0,
            wy + wh - 22.0,
            12.0,
            HOVER,
            note,
        );
        // The view and info panels: the focused toggle's text, or the
        // last list item focused.
        let (pic, text) = match (self.focus, self.shown) {
            (Focus::Time(t), _) => (None, &self.day_texts[t]),
            (Focus::Observer, _) => (None, &self.observe_text),
            (_, Some(Focus::Area(i))) => {
                let it = &self.areas.items[i];
                (it.pic.as_ref(), &it.text)
            }
            (_, Some(Focus::Dino(i))) => {
                let it = &self.dinos.items[i];
                (it.pic.as_ref(), &it.text)
            }
            _ => (None, &self.day_texts[self.time]),
        };
        if let Some(p) = pic {
            out.image(p, 38.0, 74.0);
        }
        for (i, l) in text.iter().enumerate().take(11) {
            out.text(416.0, 80.0 + i as f32 * 17.0, 12.0, TEXT, l);
        }
        if let Some(n) = self.note {
            out.text(420.0, 566.0, 12.0, HOVER, n);
        }
        if let Some(r) = self.focus_rect() {
            out.outline(r, 2.0, GOLD);
        }
    }
}
