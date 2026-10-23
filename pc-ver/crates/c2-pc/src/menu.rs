//! The menus: player registration, main menu with statistics, hunt setup,
//! options, credits and the quit prompt.
//!
//! The released source has no menu code (the original menu was a separate
//! front end that started the game with command-line arguments). These
//! screens are rebuilt from its art in `HUNTDAT/MENU`: each is a picture,
//! its lit version (`*_ON.TGA`) and a half-resolution hit map (`*_MAP.RAW`,
//! one region id per pixel); a hovered or selected region shows its lit
//! pixels. Text, lists and their layout are reconstructions.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use c2_core::area::TimeOfDay;
use c2_formats::menu_txt::{parse_c2map, MenuArea, MenuData};
use c2_formats::res_txt::ResTxt;
use c2_formats::sav::{KeyMap, Save};
use winit::keyboard::KeyCode;

use crate::area_view::find_file;
use crate::dinos::load_res;
use crate::gpu::Gpu;
use crate::hud::{Hud, Sprite};
use crate::hunt_view::{HuntSetup, HuntView};
use crate::input::Input;
use crate::keys;
use crate::save;
use crate::scene::{MakeScene, Scene};
use crate::settings::PcSettings;

/// The menus are drawn on an 800x600 canvas, scaled to fit the window.
const CW: f32 = 800.0;
const CH: f32 = 600.0;

type Rect = (f32, f32, f32, f32);

/// Each huntable's `basescore`: its `_RES.TXT` entry (matched by name),
/// with the game's "when picked" changes applied (`overwrite charN`).
fn hunt_scores(huntdat: &Path, menu: &MenuData) -> Vec<Option<f32>> {
    let Some(res) = find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())
    else {
        return vec![None; menu.huntables.len()];
    };
    let chars = res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or_default();
    let unquote = |v: &str| v.trim().trim_matches('\'').to_ascii_lowercase();
    menu.huntables
        .iter()
        .enumerate()
        .map(|(k, h)| {
            let name = h.name.to_ascii_lowercase();
            let b = chars
                .iter()
                .find(|b| b.get(&["name"]).map(unquote).as_deref() == Some(&name))?;
            let picked = c2_formats::mee::Hunt {
                area: Some(0),
                time: 1,
                picked: 1u128.checked_shl(k as u32).unwrap_or(0),
            };
            let b = c2_formats::mee::resolve(b, &picked);
            b.get(&["basescore"]).map(c2_formats::res_txt::c_atof)
        })
        .collect()
}

/// Dialect games: per huntable (menu slot), the areas (index in `maps`)
/// it spawns on when picked; `None` for an original game (all of them).
/// Read once per area with every slot picked.
fn hunt_homes(huntdat: &Path, menu: &MenuData, maps: &[PathBuf]) -> Option<Vec<Vec<usize>>> {
    let res = find_file(huntdat, "_RES.TXT")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())?;
    if !c2_formats::mee::is_dialect(&res) {
        return None;
    }
    let mut homes = vec![Vec::new(); menu.huntables.len()];
    for (a, map) in maps.iter().enumerate() {
        let stem = map
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let h = c2_formats::mee::Hunt {
            area: c2_formats::mee::area_of(&stem),
            time: 1,
            picked: u128::MAX,
        };
        let t = c2_formats::mee::Tables::read(&res, &h);
        for c in &t.characters {
            if c.spawn.is_empty() && c.packs.is_empty() {
                continue;
            }
            let slot = c
                .block
                .get(&["callNo"])
                .map(c2_formats::res_txt::c_atoi)
                .filter(|&n| n >= 0);
            if let Some(v) = slot.and_then(|n| homes.get_mut(n as usize)) {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        }
    }
    Some(homes)
}

fn inside(p: Option<(f32, f32)>, r: Rect) -> bool {
    p.is_some_and(|(x, y)| x >= r.0 && y >= r.1 && x < r.0 + r.2 && y < r.1 + r.3)
}

/// A screen picture, its lit regions and hit map.
#[derive(Default)]
struct Board {
    base: Option<usize>,
    /// Lit cut-out per region id: image and its canvas rectangle.
    lit: Vec<Option<(usize, Rect)>>,
    /// 400x300 region ids.
    hit: Vec<u8>,
}

fn read_tga(dir: &Path, name: &str) -> Option<c2_formats::tga::Picture> {
    let p = find_file(dir, name)?;
    match c2_formats::tga::parse(&std::fs::read(&p).ok()?) {
        Ok(t) => Some(t),
        Err(e) => {
            tracing::warn!("{}: {e}", p.display());
            None
        }
    }
}

impl Board {
    fn load(hud: &mut Hud, gpu: &Gpu, dir: &Path, base: &str, on: &str, map: &str) -> Self {
        let mut b = Board {
            base: read_tga(dir, base).map(|t| {
                let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
                hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, true)
            }),
            lit: Vec::new(),
            hit: find_file(dir, map)
                .and_then(|p| std::fs::read(p).ok())
                .filter(|h| h.len() == 400 * 300)
                .unwrap_or_default(),
        };
        let Some(on) = read_tga(dir, on).filter(|t| t.width == 800 && t.height == 600) else {
            return b;
        };
        if b.hit.is_empty() {
            return b;
        }
        let rgba = c2_core::color::image_to_rgba8(&on.pixels, false);
        // Bounding box of each region, in hit-map pixels.
        let mut bb = vec![(usize::MAX, usize::MAX, 0, 0); 256];
        for (i, &r) in b.hit.iter().enumerate() {
            if r != 0 {
                let (x, y) = (i % 400, i / 400);
                let e = &mut bb[r as usize];
                *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
            }
        }
        b.lit = vec![None; 256];
        for (r, &(x0, y0, x1, y1)) in bb.iter().enumerate() {
            if x0 == usize::MAX {
                continue;
            }
            let (px, py, w, h) = (x0 * 2, y0 * 2, (x1 - x0 + 1) * 2, (y1 - y0 + 1) * 2);
            let mut cut = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let (cx, cy) = (px + x, py + y);
                    if b.hit[cy / 2 * 400 + cx / 2] as usize == r {
                        let s = (cy * 800 + cx) * 4;
                        let d = (y * w + x) * 4;
                        cut[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
                    }
                }
            }
            let id = hud.add_image_filtered(gpu, w as u32, h as u32, &cut, true);
            b.lit[r] = Some((id, (px as f32, py as f32, w as f32, h as f32)));
        }
        b
    }

    /// The region under canvas point `p` (0 = none).
    fn region(&self, p: Option<(f32, f32)>) -> u8 {
        let Some((x, y)) = p else { return 0 };
        if !(0.0..CW).contains(&x) || !(0.0..CH).contains(&y) || self.hit.is_empty() {
            return 0;
        }
        self.hit[(y as usize / 2) * 400 + x as usize / 2]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Register,
    Waiver,
    Delete,
    Main,
    Quit,
    Credits,
    Setup,
    Options,
    Loading,
}

/// Hunt setup choices.
#[derive(Debug, Clone, Default)]
struct Choice {
    area: usize,
    /// Bit per dinosaur (AI 10 + i) / weapon / accessory.
    dinos: u128,
    weapons: u128,
    equip: u32,
    /// 0 dawn, 1 day, 2 night.
    time: usize,
    observer: bool,
    /// The list item whose picture and description show: (column, row).
    focus: Option<(usize, usize)>,
}

/// Accessories in the equipment list, with their picture and description.
const EQUIPMENT: [(&str, &str); 4] = [
    ("Camouflage", "CAMOFLAG.NFO"),
    ("Radar", "RADAR.NFO"),
    ("Cover Scent", "SCENT.NFO"),
    ("Double Ammo", "DOUBLE.NFO"),
];

/// One list in the hunt setup screen.
struct Column {
    rect: Rect,
    names: Vec<String>,
    prices: Vec<i32>,
    pics: Vec<Option<usize>>,
    /// Description per item (dinosaurs: metric, then US units).
    texts: Vec<Vec<String>>,
    texts_us: Vec<Vec<String>>,
    /// First row shown (lists longer than the box scroll with the wheel).
    scroll: usize,
    /// Item shown on each row: maps, animals and weapons cheapest first
    /// (picks still go by item, so slots and saves are unaffected).
    order: Vec<usize>,
}

const ROW_H: f32 = 19.0;

impl Column {
    /// Rows that fit in the box.
    fn visible(&self) -> usize {
        (((self.rect.3 - 8.0) / ROW_H) as usize).max(1)
    }
}

/// Options panel rows.
#[derive(Debug, Clone, Copy)]
enum Opt {
    Slider(&'static str, fn(&mut Save) -> &mut i32),
    Toggle(&'static str, [&'static str; 2], fn(&mut Save) -> &mut i32),
    /// A slider for a PC-only setting (`settings::PcSettings`).
    PcSlider(&'static str, fn(&mut PcSettings) -> &mut i32),
    /// An on/off PC-only setting.
    PcToggle(&'static str, fn(&mut PcSettings) -> &mut i32),
    /// Removes every trophy not locked (asked first: a second click).
    ClearTrophies,
}

const GAME_OPTS: [Opt; 7] = [
    Opt::Slider("Aggressiveness", |s| &mut s.options.agres),
    Opt::Slider("Density", |s| &mut s.options.dens),
    Opt::Slider("Sensitivity", |s| &mut s.options.sens),
    Opt::Toggle("Measurement", ["Metric", "US"], |s| &mut s.options.sys),
    Opt::PcToggle("Blood", |p| &mut p.blood),
    Opt::PcToggle("Death animation", |p| &mut p.death_anim),
    Opt::ClearTrophies,
];

const VIDEO_OPTS: [Opt; 3] = [
    Opt::Slider("View range", |s| &mut s.options.view_r),
    Opt::PcSlider("Render distance", |p| &mut p.render),
    Opt::Slider("Brightness", |s| &mut s.options.brightness),
];

const MOUSE_OPTS: [Opt; 2] = [
    Opt::Slider("Mouse sensitivity", |s| &mut s.options.mouse_sens),
    Opt::Toggle("Reverse mouse", ["Off", "On"], |s| {
        &mut s.options.reverse_mouse
    }),
];

/// Bindable actions, in `KeyMap` order.
/// Picks one binding out of the key map.
type KeyField = fn(&mut KeyMap) -> &mut i32;

const ACTIONS: [(&str, KeyField); 17] = [
    ("Forward", |k| &mut k.forward),
    ("Backward", |k| &mut k.backward),
    ("Look up", |k| &mut k.up),
    ("Look down", |k| &mut k.down),
    ("Turn left", |k| &mut k.left),
    ("Turn right", |k| &mut k.right),
    ("Fire", |k| &mut k.fire),
    ("Get weapon", |k| &mut k.show),
    ("Step left", |k| &mut k.strafe_left),
    ("Step right", |k| &mut k.strafe_right),
    ("Strafe", |k| &mut k.strafe),
    ("Jump", |k| &mut k.jump),
    ("Run", |k| &mut k.run),
    ("Crouch", |k| &mut k.crouch),
    ("Call", |k| &mut k.call),
    ("Change call", |k| &mut k.change_call),
    ("Binoculars", |k| &mut k.binoculars),
];

const GAME_PANEL: Rect = (48.0, 76.0, 324.0, 190.0);
const VIDEO_PANEL: Rect = (48.0, 346.0, 324.0, 190.0);
const KEYS_PANEL: Rect = (432.0, 76.0, 320.0, 460.0);
const OPT_ROW: f32 = 22.0;
const SLIDER_X: f32 = 220.0;

/// Colours (RGBA).
const TEXT: [f32; 4] = [0.62, 0.82, 0.5, 1.0];
const HOVER: [f32; 4] = [0.9, 1.0, 0.75, 1.0];
const PICKED: [f32; 4] = [1.0, 0.85, 0.3, 1.0];
const DIM: [f32; 4] = [0.36, 0.42, 0.3, 1.0];
const SAND: [f32; 4] = [0.9, 0.78, 0.6, 1.0];
const BLUE: [f32; 4] = [0.55, 0.78, 0.95, 1.0];
const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub struct Menu {
    hud: Hud,
    game_dir: PathBuf,
    huntdat: PathBuf,
    start_score: i32,
    main: Board,
    stats_board: Board,
    quit_board: Board,
    reg: Board,
    waiver: Board,
    delete: Board,
    setup: Board,
    options: Board,
    credits: Board,
    slider: Option<usize>,
    knob: Option<usize>,
    loading: Option<usize>,
    columns: Vec<Column>,
    day_texts: Vec<Vec<String>>,
    tranq_text: Vec<String>,
    fog_text: Vec<String>,
    /// Render distance and fog (not in the original save).
    pc: PcSettings,
    observe_text: Vec<String>,
    areas: Vec<PathBuf>,
    players: Vec<Save>,
    /// Index into `players` of the signed-in hunter.
    player: Option<usize>,
    screen: Screen,
    /// The statistics panel is open on the main menu.
    stats: bool,
    /// Registration: typed name and highlighted list entry.
    name: String,
    picked: Option<usize>,
    choice: Choice,
    /// Options: the action waiting for a key, and the slider being dragged.
    binding: Option<usize>,
    drag: Option<Opt>,
    /// Hunt to start after the loading picture has been shown.
    launch: Option<HuntSetup>,
    shown_loading: bool,
    quit: bool,
    /// Last drawn window size, for mapping the cursor to the canvas.
    screen_size: (u32, u32),
    cursor: Option<(f32, f32)>,
    /// The experimental mobile layout's art, when it's on, and its page
    /// (0 maps, 1 animals, 2 weapons and gear).
    mobile: Option<mobile::Art>,
    mobile_page: usize,
    /// The signed-in hunter's record (`trophy0N.rec`), read when the
    /// statistics open.
    record: c2_core::record::Record,
    /// "Remove unlocked trophies" was clicked once (a second click does it).
    clear_armed: bool,
}

fn read_text(dir: Option<&Path>, name: &str) -> Vec<String> {
    dir.and_then(|d| find_file(d, name))
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| {
            String::from_utf8_lossy(&b)
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Re-flows description text for a panel. The files are broken for a wider
/// font: long lines are joined into paragraphs, short ones ("Name : ...",
/// blank lines) kept, then everything is wrapped to `width`.
fn wrap(hud: &Hud, px: f32, width: f32, lines: &[String]) -> Vec<String> {
    let mut paras: Vec<String> = Vec::new();
    let mut joining = false;
    for l in lines {
        let l = l.trim();
        match paras.last_mut() {
            Some(p) if joining && !l.is_empty() => {
                p.push(' ');
                p.push_str(l);
            }
            _ => paras.push(l.to_string()),
        }
        joining = l.len() > 30;
    }
    let mut out = Vec::new();
    for l in &paras {
        let mut cur = String::new();
        for w in l.split(' ') {
            let next = if cur.is_empty() {
                w.to_string()
            } else {
                format!("{cur} {w}")
            };
            if hud.label_width(px, &next) > width && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
                cur = w.to_string();
            } else {
                cur = next;
            }
        }
        out.push(cur);
    }
    out
}

/// Menu style (`--menu-art`): a game folder whose menu frames, hit maps
/// and slider are used (the original's, a mod's, Ice Age's); the pictures
/// and texts stay the game's.
pub static MENU_ART: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The experimental mobile layout of the hunt setup (`--mobile-menu`).
pub static MOBILE_MENU: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

mod mobile;

impl Menu {
    /// `game_dir`: the install (or its `HUNTDAT`). `player`: sign straight in
    /// as this hunter (back from a hunt).
    pub fn new(gpu: &Gpu, game_dir: &Path, player: Option<i32>) -> Result<Self> {
        let (game_dir, huntdat) = match find_file(game_dir, "HUNTDAT") {
            Some(h) => (game_dir.to_path_buf(), h),
            None => (
                game_dir.parent().unwrap_or(Path::new(".")).to_path_buf(),
                game_dir.to_path_buf(),
            ),
        };
        // A readable `_RES.TXT` is what makes this a game folder.
        load_res(&huntdat).context("reading the game data (is this a game folder?)")?;
        let dir = find_file(&huntdat, "MENU").unwrap_or_else(|| huntdat.join("MENU"));
        let mut hud = Hud::new(gpu);
        // The style's frames where it has them (else the game's).
        let art = MENU_ART
            .get()
            .and_then(|d| find_file(d, "HUNTDAT").or_else(|| Some(d.clone())))
            .and_then(|h| find_file(&h, "MENU"))
            .filter(|m| find_file(m, "MENUM.TGA").is_some())
            .unwrap_or_else(|| dir.clone());
        let mut board =
            |base: &str, on: &str, map: &str| Board::load(&mut hud, gpu, &art, base, on, map);
        let main = board("MENUM.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW");
        let stats_board = board("MENUS.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW");
        let quit_board = board("MENUQ.TGA", "MENUQ_ON.TGA", "MQ_MAP.RAW");
        let reg = board("MENUR.TGA", "MENUR_ON.TGA", "MR_MAP.RAW");
        let waiver = board("MENUL.TGA", "MENUL_ON.TGA", "ML_MAP.RAW");
        let delete = board("MENUD.TGA", "MENUD_ON.TGA", "MD_MAP.RAW");
        let setup = board("MENU2.TGA", "MENU2_ON.TGA", "M2_MAP.RAW");
        let options = board("OPT_OFF.TGA", "OPT_ON.TGA", "OPT_MAP.RAW");
        let credits = board("CREDITS.TGA", "", "");
        let mut pic = |name: &str, linear: bool| {
            read_tga(&art, name)
                .or_else(|| read_tga(&dir, name))
                .map(|t| {
                    let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
                    hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, linear)
                })
        };
        let slider = pic("SL_BAR.TGA", true);
        let knob = pic("SL_BUT.TGA", true);
        let loading = pic("LOADING.TGA", true);

        let pics_dir = find_file(&dir, "PICS");
        let txt = find_file(&dir, "TXT");
        // A picture by name in `MENU/PICS`, or by a path from the game
        // folder (`huntdat\\menu\\pics\\x.tga`, as data may give).
        let mut view = |name: String| {
            let t = if name.contains(['/', '\\']) {
                c2_formats::tga::parse(&std::fs::read(find_file(&game_dir, &name)?).ok()?).ok()?
            } else {
                read_tga(pics_dir.as_deref()?, &name)?
            };
            let rgba = c2_core::color::image_to_rgba8(&t.pixels, false);
            Some(hud.add_image_filtered(gpu, t.width as u32, t.height as u32, &rgba, true))
        };
        // The hunt lists: `_MENU.TXT` if the game has one (Modder's Edition
        // Engine), else `_RES.TXT`; see `c2_formats::menu_txt`.
        let menu_src = find_file(&huntdat, "_MENU.TXT")
            .or_else(|| find_file(&huntdat, "_RES.TXT"))
            .and_then(|p| std::fs::read(p).ok())
            .map(|d| String::from_utf8_lossy(&d).into_owned())
            .unwrap_or_default();
        let mut menu = MenuData::from_res(&ResTxt::parse(&menu_src)?);

        // Locations: the `prices` areas whose .MAP exists (slot 6 may be
        // `external.map`), then `.c2map` descriptors; without any `prices`
        // areas, AREA1.. as found.
        let areas_dir = find_file(&huntdat, "AREAS");
        let map_of = |stem: &str| find_file(areas_dir.as_deref()?, &format!("{stem}.MAP"));
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
                });
            }
        }
        if let Some(dir) = areas_dir.as_deref() {
            let mut c2maps: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("c2map"))
                })
                .collect();
            c2maps.sort();
            for p in c2maps {
                if let Some(a) = std::fs::read(&p)
                    .ok()
                    .and_then(|d| parse_c2map(&String::from_utf8_lossy(&d)))
                {
                    menu.add_c2map(a);
                }
            }
        }
        let mut area_list: Vec<(MenuArea, PathBuf)> = Vec::new();
        for a in &menu.areas {
            let map = map_of(&a.project)
                .or_else(|| (a.number == Some(6)).then(|| map_of("external")).flatten());
            if let Some(m) = map {
                area_list.push((a.clone(), m));
            }
        }
        let areas: Vec<PathBuf> = area_list.iter().map(|(_, m)| m.clone()).collect();
        // A path from the data (`huntdat\\menu\\pics\\x.tga`), from the game folder.
        let game_path = |p: &str| find_file(&game_dir, p);
        let mut cols = Vec::new();
        let area_texts: Vec<Vec<String>> = area_list
            .iter()
            .map(|(a, _)| match (&a.text, a.number) {
                (Some(t), _) => game_path(t)
                    .and_then(|p| std::fs::read(p).ok())
                    .map(|d| {
                        String::from_utf8_lossy(&d)
                            .lines()
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                (None, Some(n)) => read_text(txt.as_deref(), &format!("AREA{n}.TXT")),
                (None, None) => Vec::new(),
            })
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (12.0, 376.0, 182.0, 170.0),
            names: area_list
                .iter()
                .zip(&area_texts)
                .enumerate()
                .map(|(i, ((a, _), t))| {
                    a.name
                        .clone()
                        .or_else(|| t.first().filter(|l| !l.is_empty()).cloned())
                        .unwrap_or_else(|| format!("Area {}", i + 1))
                })
                .collect(),
            prices: area_list.iter().map(|(a, _)| a.price).collect(),
            pics: area_list
                .iter()
                .map(|(a, _)| match (&a.pic, a.number) {
                    (Some(p), _) => view(p.clone()),
                    (None, Some(n)) => view(format!("AREA{n}.TGA")),
                    (None, None) => None,
                })
                .collect(),
            texts: area_texts.clone(),
            texts_us: area_texts,
        });
        // Huntables by menu slot (slot n = `DINOn+1.TGA` unless `pic` says).
        let dinos: Vec<String> = menu.huntables.iter().map(|d| d.name.clone()).collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (210.0, 376.0, 184.0, 170.0),
            prices: menu.huntables.iter().map(|d| d.price).collect(),
            pics: menu
                .huntables
                .iter()
                .enumerate()
                .map(|(k, d)| match &d.pic {
                    Some(p) => view(p.clone()),
                    None => view(format!("DINO{}.TGA", k + 1)),
                })
                .collect(),
            texts: (1..=dinos.len())
                .map(|n| read_text(txt.as_deref(), &format!("DINO{n}.TXM")))
                .collect(),
            texts_us: (1..=dinos.len())
                .map(|n| read_text(txt.as_deref(), &format!("DINO{n}.TXU")))
                .collect(),
            names: dinos,
        });
        let weapons: Vec<String> = menu.weapons.iter().map(|w| w.name.clone()).collect();
        let wtexts: Vec<Vec<String>> = (1..=weapons.len())
            .map(|n| read_text(txt.as_deref(), &format!("WEAPON{n}.TXT")))
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (410.0, 376.0, 184.0, 170.0),
            prices: menu.weapons.iter().map(|w| w.price).collect(),
            pics: (1..=weapons.len())
                .map(|n| view(format!("WEAPON{n}.TGA")))
                .collect(),
            texts: wtexts.clone(),
            texts_us: wtexts,
            names: weapons,
        });
        let etexts: Vec<Vec<String>> = EQUIPMENT
            .iter()
            .map(|(_, f)| read_text(txt.as_deref(), f))
            .collect();
        cols.push(Column {
            scroll: 0,
            order: Vec::new(),
            rect: (610.0, 376.0, 184.0, 170.0),
            names: EQUIPMENT.iter().map(|(n, _)| n.to_string()).collect(),
            prices: (0..4)
                .map(|i| menu.accessory_prices.get(i).copied().unwrap_or(0))
                .collect(),
            pics: (1..=4).map(|n| view(format!("EQUIP{n}.TGA"))).collect(),
            texts: etexts.clone(),
            texts_us: etexts,
        });
        // Each huntable's points, from its `_RES.TXT` entry as when picked
        // (descriptions differ between games; this is what the hunt scores).
        let scores = hunt_scores(&huntdat, &menu);
        if let Some(c) = cols.get_mut(1) {
            for (k, sc) in scores.iter().enumerate() {
                let Some(sc) = sc else { continue };
                // Whole points, as the hunt adds them.
                let line = format!(
                    "Score when bagged: {} (tranquilized: {}, no trophy)",
                    *sc as i32,
                    (sc * 1.25) as i32
                );
                for t in [c.texts.get_mut(k), c.texts_us.get_mut(k)]
                    .into_iter()
                    .flatten()
                {
                    t.push(String::new());
                    t.push(line.clone());
                }
            }
        }
        // Where each comes from (combined games: an animal's own game's
        // maps). Picked, it comes on any map (`spawn_picked_anywhere`).
        let homes = hunt_homes(&huntdat, &menu, &areas);
        if let (Some(h), Some(c)) = (&homes, cols.get_mut(1)) {
            let area_names: Vec<String> = area_list
                .iter()
                .map(|(a, _)| a.name.clone().unwrap_or_else(|| a.project.clone()))
                .collect();
            for (k, list) in h.iter().enumerate() {
                if list.len() == areas.len() {
                    continue;
                }
                let names: Vec<&str> = list
                    .iter()
                    .filter_map(|&a| area_names.get(a).map(String::as_str))
                    .collect();
                let line = if names.is_empty() {
                    "Spawns on no map by itself.".to_string()
                } else {
                    format!("Native to: {}", names.join(", "))
                };
                for t in [c.texts.get_mut(k), c.texts_us.get_mut(k)]
                    .into_iter()
                    .flatten()
                {
                    t.push(String::new());
                    t.push(line.clone());
                }
            }
        }
        // Rows by price (stable, so equal prices keep their order); the
        // equipment keeps its fixed order.
        for (ci, c) in cols.iter_mut().enumerate() {
            c.order = (0..c.names.len()).collect();
            if ci < 3 {
                c.order
                    .sort_by_key(|&i| c.prices.get(i).copied().unwrap_or(0));
            }
        }
        // Wrap descriptions to the info panel.
        let info_w = 340.0;
        for c in &mut cols {
            for t in c.texts.iter_mut().chain(c.texts_us.iter_mut()) {
                *t = wrap(&hud, 12.0, info_w, t);
            }
        }
        let day_texts = (1..=3)
            .map(|n| {
                wrap(
                    &hud,
                    12.0,
                    info_w,
                    &read_text(txt.as_deref(), &format!("DAY{n}.NFO")),
                )
            })
            .collect();
        let tranq_text = wrap(&hud, 12.0, info_w, &read_text(txt.as_deref(), "TRANQ.NFO"));
        let fog_text = wrap(
            &hud,
            12.0,
            info_w,
            &[
                "Fog".to_string(),
                String::new(),
                "Hunt in the original game's view range and fog, whatever the render distance \
                 in Options says, at any time of day."
                    .to_string(),
            ],
        );
        let observe_text = wrap(
            &hud,
            12.0,
            info_w,
            &read_text(txt.as_deref(), "OBSERVE.NFO"),
        );

        let players = save::players(&game_dir);
        let signed_in = player.and_then(|r| players.iter().position(|p| p.reg_number == r));
        let mobile = MOBILE_MENU
            .get()
            .copied()
            .unwrap_or(false)
            .then(|| mobile::Art::new(&mut hud, gpu));
        let mut m = Self {
            mobile,
            mobile_page: 0,
            record: Default::default(),
            clear_armed: false,
            hud,
            game_dir,
            huntdat,
            start_score: menu.start,
            main,
            stats_board,
            quit_board,
            reg,
            waiver,
            delete,
            setup,
            options,
            credits,
            slider,
            knob,
            loading,
            columns: cols,
            day_texts,
            tranq_text,
            fog_text,
            pc: PcSettings::load(),
            observe_text,
            areas,
            players,
            player: signed_in,
            screen: if signed_in.is_some() {
                Screen::Main
            } else {
                Screen::Register
            },
            stats: false,
            name: String::new(),
            picked: None,
            choice: Choice {
                time: 1,
                ..Default::default()
            },
            binding: None,
            drag: None,
            launch: None,
            shown_loading: false,
            quit: false,
            screen_size: (800, 600),
            cursor: None,
        };
        m.restore_choice();
        Ok(m)
    }

    /// Opens a screen directly (screenshots): signs in the first hunter.
    pub fn show(&mut self, screen: &str) -> Result<()> {
        if self.player.is_none() && !self.players.is_empty() {
            self.sign_in(0);
        }
        self.screen = match screen {
            "main" => Screen::Main,
            "stats" => {
                self.stats = true;
                if let Some(p) = self.me() {
                    self.record = crate::save::load_record(p.reg_number);
                }
                Screen::Main
            }
            "hunt" => {
                // Something picked, to show the lists and account.
                self.choice.dinos = 1;
                self.choice.weapons = 1;
                self.choice.focus = Some((1, 0));
                Screen::Setup
            }
            "options" => Screen::Options,
            "credits" => Screen::Credits,
            "register" => Screen::Register,
            "quit" => Screen::Quit,
            _ => anyhow::bail!("unknown screen {screen:?}"),
        };
        Ok(())
    }

    /// Plays scripted input (screenshots, testing), steps separated by `;`:
    /// `X,Y` clicks that canvas point, `'text` types, `enter` / `esc` press keys.
    pub fn script(&mut self, gpu: &Gpu, steps: &str) -> Result<()> {
        for step in steps.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let (s, ox, oy) = self.canvas();
            let mut input = Input::default();
            if let Some(t) = step.strip_prefix('\'') {
                input.typed = t.to_string();
            } else if step == "enter" || step == "esc" {
                let k = if step == "enter" {
                    KeyCode::Enter
                } else {
                    KeyCode::Escape
                };
                input.key(k, true);
            } else {
                let v: Vec<f32> = step
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .with_context(|| format!("bad script step {step:?}"))?;
                anyhow::ensure!(v.len() == 2, "bad script step {step:?}");
                input.cursor = Some((ox + v[0] * s, oy + v[1] * s));
                input.clicked = true;
                input.mouse_down = true;
            }
            self.update(gpu, 0.0, &input);
        }
        Ok(())
    }

    fn me(&self) -> Option<&Save> {
        self.player.and_then(|i| self.players.get(i))
    }

    fn me_mut(&mut self) -> Option<&mut Save> {
        self.player.and_then(|i| self.players.get_mut(i))
    }

    fn sign_in(&mut self, i: usize) {
        self.player = Some(i);
        self.restore_choice();
        self.screen = Screen::Main;
    }

    /// Accessories are kept in the save (`ScentMode`, `CamoMode`, `RadarMode`,
    /// `Tranq`); everything else starts fresh.
    fn restore_choice(&mut self) {
        let Some(o) = self.me().map(|s| s.options) else {
            return;
        };
        let mut equip = 0;
        for (bit, on) in [o.camo, o.radar, o.scent].into_iter().enumerate() {
            if on != 0 {
                equip |= 1 << bit;
            }
        }
        self.choice.equip = equip;
        self.choice.observer = false;
        // Drop accessories the hunter can no longer afford.
        if self.remaining() < 0 {
            self.choice.equip = 0;
        }
    }

    fn store_me(&self) {
        if let Some(s) = self.me() {
            if let Err(e) = save::store(s) {
                tracing::error!("{e:#}");
            }
        }
    }

    fn tranq(&self) -> bool {
        self.me().is_some_and(|s| s.options.tranq != 0)
    }

    fn picked_mask(&self, col: usize) -> u128 {
        let c = &self.choice;
        match col {
            0 => 1u128.checked_shl(c.area as u32).unwrap_or(0),
            1 => c.dinos,
            2 => c.weapons,
            _ => u128::from(c.equip),
        }
    }

    /// Account left after paying for the choices.
    fn remaining(&self) -> i32 {
        let score = self.me().map_or(0, |s| s.score);
        if self.choice.observer {
            return score;
        }
        let mut spent = 0;
        for (ci, c) in self.columns.iter().enumerate() {
            let mask = self.picked_mask(ci);
            for (i, p) in c.prices.iter().enumerate() {
                if i < 128 && mask & (1 << i) != 0 {
                    spent += p;
                }
            }
        }
        score - spent
    }

    fn can_hunt(&self) -> bool {
        self.choice.area < self.areas.len()
            && (self.choice.observer || (self.choice.dinos != 0 && self.choice.weapons != 0))
            && self.remaining() >= 0
    }

    /// Canvas mapping for the last drawn window size: (scale, x0, y0).
    fn canvas(&self) -> (f32, f32, f32) {
        // Not drawn yet (a test script): the canvas's own size.
        if self.screen_size.0 == 0 || self.screen_size.1 == 0 {
            return (1.0, 0.0, 0.0);
        }
        let (w, h) = (self.screen_size.0 as f32, self.screen_size.1 as f32);
        let s = (w / CW).min(h / CH);
        (s, (w - CW * s) / 2.0, (h - CH * s) / 2.0)
    }

    fn row_at(&self, col: usize) -> Option<usize> {
        let c = &self.columns[col];
        let (x, y) = self.cursor?;
        if !inside(self.cursor, c.rect) {
            return None;
        }
        let _ = x;
        let r = ((y - c.rect.1 - 4.0) / ROW_H).floor();
        if r < 0.0 || r as usize >= c.visible() {
            return None;
        }
        c.order.get(r as usize + c.scroll).copied()
    }

    // ---------------------------------------------------------------- input

    fn update_register(&mut self, input: &Input, region: u8) {
        for ch in input.typed.chars() {
            if self.name.len() < 24 && (ch.is_ascii_graphic() || ch == ' ') {
                self.name.push(ch);
                self.picked = None;
            }
        }
        if input.pressed(KeyCode::Backspace) {
            self.name.pop();
            self.picked = None;
        }
        // The hunter list.
        if input.clicked {
            for i in 0..self.players.len() {
                if inside(self.cursor, (304.0, 368.0 + i as f32 * 18.0, 192.0, 18.0)) {
                    self.picked = Some(i);
                    self.name = self.players[i].player_name();
                }
            }
        }
        let ok = (input.clicked && region == 1) || input.pressed(KeyCode::Enter);
        if ok {
            let name = self.name.trim().to_string();
            if let Some(i) = self.players.iter().position(|p| p.player_name() == name) {
                self.sign_in(i);
            } else if !name.is_empty() {
                self.screen = Screen::Waiver;
            }
        }
        if input.clicked && region == 2 && self.picked.is_some() {
            self.screen = Screen::Delete;
        }
        if input.pressed(KeyCode::Escape) && self.player.is_some() {
            self.screen = Screen::Main;
        }
    }

    fn update_waiver(&mut self, input: &Input, region: u8) {
        if input.clicked && region == 1 {
            let used: Vec<i32> = self.players.iter().map(|p| p.reg_number).collect();
            let Some(reg) = (0..save::MAX_PLAYERS).find(|r| !used.contains(r)) else {
                tracing::warn!("all {} player slots are used", save::MAX_PLAYERS);
                self.screen = Screen::Register;
                return;
            };
            let p = save::new_player(self.name.trim(), reg, self.start_score);
            if let Err(e) = save::store(&p) {
                tracing::error!("{e:#}");
            }
            self.players.push(p);
            self.players.sort_by_key(|p| p.reg_number);
            let i = self.players.iter().position(|p| p.reg_number == reg);
            if let Some(i) = i {
                self.sign_in(i);
            }
        } else if (input.clicked && region == 2) || input.pressed(KeyCode::Escape) {
            self.screen = Screen::Register;
        }
    }

    fn update_delete(&mut self, input: &Input, region: u8) {
        if input.clicked && region == 1 {
            if let Some(i) = self.picked.take() {
                let p = self.players.remove(i);
                if let Err(e) = save::delete(p.reg_number) {
                    tracing::error!("{e:#}");
                }
                self.player = None;
                self.name.clear();
            }
            self.screen = Screen::Register;
        } else if (input.clicked && region == 2) || input.pressed(KeyCode::Escape) {
            self.screen = Screen::Register;
        }
    }

    fn update_main(&mut self, input: &Input, region: u8) {
        if input.pressed(KeyCode::Escape) {
            self.screen = Screen::Quit;
            return;
        }
        if !input.clicked {
            return;
        }
        // The name field: change hunter.
        if inside(self.cursor, (84.0, 4.0, 272.0, 34.0)) {
            self.name = self.me().map(|p| p.player_name()).unwrap_or_default();
            self.picked = self.player;
            self.screen = Screen::Register;
            return;
        }
        match region {
            1 => self.screen = Screen::Setup,
            2 => self.screen = Screen::Options,
            3 => {
                if let Some(trophy) =
                    find_file(&self.huntdat, "AREAS").and_then(|a| find_file(&a, "TROPHY.MAP"))
                {
                    self.start(trophy, TimeOfDay::Day, 0, 0, false, false);
                }
            }
            4 => self.screen = Screen::Credits,
            5 => self.screen = Screen::Quit,
            6 => {
                self.stats = !self.stats;
                if let (true, Some(p)) = (self.stats, self.me()) {
                    self.record = crate::save::load_record(p.reg_number);
                }
            }
            _ => {}
        }
    }

    fn update_setup(&mut self, input: &Input, region: u8) {
        if input.pressed(KeyCode::Escape) || (input.clicked && region == 7) {
            self.save_equipment();
            self.screen = Screen::Main;
            return;
        }
        // The wheel scrolls the list under the cursor.
        if input.scroll != 0.0 {
            let cursor = self.cursor;
            for c in &mut self.columns {
                if inside(cursor, c.rect) {
                    let max = c.names.len().saturating_sub(c.visible());
                    let step = if input.scroll > 0.0 { -1 } else { 1 };
                    c.scroll = c.scroll.saturating_add_signed(step).min(max);
                }
            }
        }
        // Hovering a list item shows it.
        for col in 0..self.columns.len() {
            if let Some(r) = self.row_at(col) {
                self.choice.focus = Some((col, r));
            }
        }
        if !input.clicked {
            return;
        }
        match region {
            1..=3 => self.choice.time = region as usize - 1,
            4 => {
                self.pc.fog = i32::from(self.pc.fog == 0);
                self.pc.store();
            }
            5 => {
                if let Some(s) = self.me_mut() {
                    s.options.tranq = i32::from(s.options.tranq == 0);
                }
            }
            6 => self.toggle_observer(),
            8 if self.can_hunt() => self.launch_hunt(),
            _ => {}
        }
        for col in 0..self.columns.len() {
            if let Some(r) = self.row_at(col) {
                self.pick(col, r);
            }
        }
    }

    /// Observer mode on or off (off: picks the account can't pay go).
    fn toggle_observer(&mut self) {
        self.choice.observer = !self.choice.observer;
        if !self.choice.observer && self.remaining() < 0 {
            self.choice.dinos = 0;
            self.choice.weapons = 0;
            self.choice.equip = 0;
        }
    }

    /// Picks (or drops) item `r` of list `col`, unless the account can't
    /// pay for it.
    fn pick(&mut self, col: usize, r: usize) {
        // Picks are bit masks; animals go to the hunt as bits 10 and up of
        // 128.
        let max = match col {
            1 => 118,
            2 => 128,
            _ => 32,
        };
        if r >= max {
            return;
        }
        let bit = 1u128 << r;
        let price = self.columns[col].prices[r];
        let old = self.choice.clone();
        match col {
            0 => self.choice.area = r,
            1 => self.choice.dinos ^= bit,
            2 => self.choice.weapons ^= bit,
            _ => self.choice.equip ^= bit as u32,
        }
        // Can't spend more than the account holds.
        if self.remaining() < 0 && price > 0 {
            self.choice = Choice {
                focus: Some((col, r)),
                ..old
            };
        }
    }

    /// Starts the hunt the choices describe.
    fn launch_hunt(&mut self) {
        self.save_equipment();
        let c = self.choice.clone();
        let time = [TimeOfDay::Dusk, TimeOfDay::Day, TimeOfDay::Night][c.time];
        let (targets, weapons) = if c.observer {
            (0, 0)
        } else {
            (c.dinos << 10, c.weapons)
        };
        let map = self.areas[c.area].clone();
        let double = !c.observer && c.equip & 8 != 0;
        self.start(map, time, targets, weapons, double, c.observer);
    }

    /// Keeps the accessory choice in the save, as the original does.
    fn save_equipment(&mut self) {
        let e = self.choice.equip;
        let observer = self.choice.observer;
        if let Some(s) = self.me_mut() {
            let on = |bit: u32| i32::from(!observer && e & bit != 0);
            s.options.camo = on(1);
            s.options.radar = on(2);
            s.options.scent = on(4);
        }
        self.store_me();
    }

    fn start(
        &mut self,
        map: PathBuf,
        time: TimeOfDay,
        targets: u128,
        weapons: u128,
        double_ammo: bool,
        observer: bool,
    ) {
        let Some(reg) = self.me().map(|s| s.reg_number) else {
            return;
        };
        self.launch = Some(HuntSetup {
            game_dir: self.game_dir.clone(),
            map,
            time,
            seed: None,
            targets,
            weapons,
            double_ammo,
            observer,
            reg,
            from_menu: true,
            pc: self.pc,
        });
        self.shown_loading = false;
        self.screen = Screen::Loading;
    }

    /// The option rows of each panel with their rectangles.
    fn option_rows(&self) -> Vec<(Opt, Rect)> {
        let mut out = Vec::new();
        for (panel, opts) in [(GAME_PANEL, &GAME_OPTS[..]), (VIDEO_PANEL, &VIDEO_OPTS[..])] {
            for (i, o) in opts.iter().enumerate() {
                out.push((
                    *o,
                    (panel.0, panel.1 + i as f32 * OPT_ROW, panel.2, OPT_ROW),
                ));
            }
        }
        let y0 = KEYS_PANEL.1 + ACTIONS.len() as f32 * 20.0 + 12.0;
        for (i, o) in MOUSE_OPTS.iter().enumerate() {
            out.push((
                *o,
                (KEYS_PANEL.0, y0 + i as f32 * OPT_ROW, KEYS_PANEL.2, OPT_ROW),
            ));
        }
        out
    }

    fn slider_value(&self, r: Rect) -> Option<i32> {
        let (x, _) = self.cursor?;
        let x0 = r.0 + r.2 - 128.0 - 8.0;
        Some((((x - x0 - 4.0) / 120.0) * 255.0).clamp(0.0, 255.0) as i32)
    }

    fn update_options(&mut self, input: &Input, region: u8) {
        if let Some(a) = self.binding {
            let vk = if input.clicked && input.cursor.is_some() {
                Some(keys::VK_LBUTTON)
            } else if input.right_clicked {
                Some(keys::VK_RBUTTON)
            } else {
                input
                    .last_pressed
                    .filter(|&k| k != KeyCode::Escape)
                    .and_then(keys::vk_of)
            };
            if input.pressed(KeyCode::Escape) {
                self.binding = None;
            } else if let Some(vk) = vk {
                if let Some(s) = self.me_mut() {
                    *(ACTIONS[a].1)(&mut s.options.keys) = vk;
                }
                self.binding = None;
            }
            return;
        }
        if input.pressed(KeyCode::Escape) || (input.clicked && region == 4) {
            self.store_me();
            self.pc.store();
            self.screen = Screen::Main;
            return;
        }
        if !input.mouse_down {
            self.drag = None;
        }
        let rows = self.option_rows();
        if input.clicked {
            let on_clear = rows
                .iter()
                .any(|(o, r)| matches!(o, Opt::ClearTrophies) && inside(self.cursor, *r));
            if on_clear {
                if self.clear_armed {
                    if let Some(s) = self.me_mut() {
                        c2_core::profile::clear_unlocked(s);
                    }
                    self.store_me();
                }
                self.clear_armed = !self.clear_armed;
                return;
            }
            self.clear_armed = false;
            for (i, _) in ACTIONS.iter().enumerate() {
                let r = (
                    KEYS_PANEL.0,
                    KEYS_PANEL.1 + i as f32 * 20.0,
                    KEYS_PANEL.2,
                    20.0,
                );
                if inside(self.cursor, r) {
                    self.binding = Some(i);
                    return;
                }
            }
            for (o, r) in &rows {
                if !inside(self.cursor, *r) {
                    continue;
                }
                match o {
                    Opt::Slider(..) | Opt::PcSlider(..) => self.drag = Some(*o),
                    Opt::Toggle(_, _, f) => {
                        if let Some(s) = self.me_mut() {
                            let v = f(s);
                            *v = i32::from(*v == 0);
                        }
                    }
                    Opt::PcToggle(_, f) => {
                        let v = f(&mut self.pc);
                        *v = i32::from(*v == 0);
                    }
                    Opt::ClearTrophies => {}
                }
            }
        }
        if let Some(Opt::PcSlider(name, f)) = self.drag {
            let r = rows.iter().find_map(|(o, r)| match o {
                Opt::PcSlider(n, _) if *n == name => Some(*r),
                _ => None,
            });
            if let Some(v) = r.and_then(|r| self.slider_value(r)) {
                *f(&mut self.pc) = v;
            }
        }
        if let Some(Opt::Slider(name, f)) = self.drag {
            let r = rows.iter().find_map(|(o, r)| match o {
                Opt::Slider(n, _) if *n == name => Some(*r),
                _ => None,
            });
            if let Some(v) = r.and_then(|r| self.slider_value(r)) {
                if let Some(s) = self.me_mut() {
                    *f(s) = v;
                }
            }
        }
    }

    // ---------------------------------------------------------------- drawing

    fn text(&self, out: &mut Vec<Sprite>, x: f32, y: f32, px: f32, color: [f32; 4], t: &str) {
        let (s, ox, oy) = self.canvas();
        let (x, y) = (ox + x * s, oy + y * s);
        out.extend(self.hud.label(x + s, y + s, px * s, SHADOW, t));
        out.extend(self.hud.label(x, y, px * s, color, t));
    }

    fn text_width(&self, px: f32, t: &str) -> f32 {
        self.hud.label_width(px, t)
    }

    fn image(&self, out: &mut Vec<Sprite>, id: Option<usize>, r: Rect) {
        let Some(id) = id else { return };
        let (s, ox, oy) = self.canvas();
        out.push(Sprite {
            size: Some([r.2 * s, r.3 * s]),
            ..Sprite::new(id, ox + r.0 * s, oy + r.1 * s, 1.0)
        });
    }

    fn image_native(&self, out: &mut Vec<Sprite>, id: Option<usize>, x: f32, y: f32) {
        let Some((id, w, h)) = id.and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h))) else {
            return;
        };
        self.image(out, Some(id), (x, y, w as f32, h as f32));
    }

    /// The board, with `lit` regions shown lit.
    fn board(&self, out: &mut Vec<Sprite>, b: &Board, lit: &[u8]) {
        self.image(out, b.base, (0.0, 0.0, CW, CH));
        for &r in lit {
            if let Some(Some((id, rect))) = b.lit.get(r as usize) {
                self.image(out, Some(*id), *rect);
            }
        }
    }

    fn draw_register(&self, out: &mut Vec<Sprite>, region: u8) {
        self.board(out, &self.reg, &[region]);
        let caret = if (self.frame_time() * 2.0) as i32 % 2 == 0 {
            "_"
        } else {
            ""
        };
        self.text(
            out,
            306.0,
            322.0,
            12.0,
            BLUE,
            &format!("{}{caret}", self.name),
        );
        for (i, p) in self.players.iter().enumerate() {
            let y = 370.0 + i as f32 * 18.0;
            let hover = inside(self.cursor, (304.0, y - 2.0, 192.0, 18.0));
            let c = if self.picked == Some(i) {
                PICKED
            } else if hover {
                HOVER
            } else {
                BLUE
            };
            self.text(out, 308.0, y, 12.0, c, &p.player_name());
        }
    }

    fn frame_time(&self) -> f32 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| (d.as_millis() % 100_000) as f32 / 1000.0)
    }

    fn draw_main(&self, out: &mut Vec<Sprite>, region: u8) {
        let b = if self.stats {
            &self.stats_board
        } else {
            &self.main
        };
        let mut lit = vec![region];
        if self.stats {
            lit.push(6);
        }
        self.board(out, b, &lit);
        let Some(p) = self.me() else { return };
        self.text(out, 92.0, 14.0, 16.0, SAND, &p.player_name());
        self.text(out, 474.0, 14.0, 16.0, SAND, &p.score.to_string());
        if !self.stats {
            return;
        }
        let us = p.options.sys != 0;
        let rank = ["Beginner", "Advanced", "Expert"]
            .get(p.rank.clamp(0, 2) as usize)
            .copied()
            .unwrap_or("");
        for (y0, st, extra) in [
            (74.0, &p.last, None),
            (204.0, &p.total, Some(format!("Rank: {rank}"))),
        ] {
            // `path` is in half metres (distance / 128 units).
            let metres = st.path * 2.0;
            let path = if us {
                format!("{:.2} mi", metres / 1609.3)
            } else {
                format!("{:.2} km", metres / 1000.0)
            };
            let t = st.time.max(0.0) as i32;
            let lines = [
                format!("Path: {path}"),
                format!("Time: {}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60),
                format!("Shots: {}", st.shots_made),
                format!("Success: {}", st.success),
            ];
            for (i, l) in lines.iter().chain(extra.iter()).enumerate() {
                self.text(out, 612.0, y0 + i as f32 * 18.0, 12.0, SAND, l);
            }
        }
        // Records (ours: `trophy0N.rec`).
        use c2_core::record::Mode;
        let r = &self.record;
        let dist = |m: f32| {
            if us {
                format!("{:.0} ft", m / 0.3)
            } else {
                format!("{m:.0} m")
            }
        };
        let short = |s: &str| s.chars().take(12).collect::<String>();
        let mut lines = vec![("Records".to_string(), PICKED)];
        if let Some(k) = r.longest() {
            lines.push((format!("Longest: {}", dist(k.range)), SAND));
            lines.push((
                format!("  {} ({})", short(&k.animal), short(&k.weapon)),
                TEXT,
            ));
        }
        if let Some(k) = r.best() {
            lines.push((format!("Best kill: {}", k.score), SAND));
            lines.push((format!("  {}", short(&k.animal)), TEXT));
        }
        lines.push((format!("Kills: {} hunt", r.count(Mode::Hunt)), SAND));
        lines.push((format!("  {} survival", r.count(Mode::Survival)), SAND));
        lines.push((format!("Best wave: {}", r.best_wave), SAND));
        let h = lines.len() as f32 * 18.0 + 10.0;
        self.fill(out, (604.0, 322.0, 186.0, h), [0.0, 0.0, 0.0, 0.65]);
        for (i, (l, c)) in lines.iter().enumerate() {
            self.text(out, 612.0, 330.0 + i as f32 * 18.0, 12.0, *c, l);
        }
    }

    fn draw_setup(&self, out: &mut Vec<Sprite>, region: u8) {
        let c = &self.choice;
        let mut lit = vec![region, c.time as u8 + 1];
        if self.tranq() {
            lit.push(5);
        }
        if c.observer {
            lit.push(6);
        }
        self.board(out, &self.setup, &lit);
        // The original's empty box beside the times of day: the fog toggle.
        let on = self.pc.fog != 0;
        let fc = if on || region == 4 { PICKED } else { DIM };
        self.text(
            out,
            301.0 - self.text_width(14.0, "FOG") / 2.0,
            304.0,
            14.0,
            fc,
            "FOG",
        );
        let score = self.me().map_or(0, |s| s.score);
        let left = self.remaining();
        for (x, v) in [(332.0, score), (408.0, left)] {
            let t = v.to_string();
            let col = if v < 0 { [1.0, 0.3, 0.2, 1.0] } else { TEXT };
            self.text(
                out,
                x + (62.0 - self.text_width(14.0, &t)) / 2.0,
                42.0,
                14.0,
                col,
                &t,
            );
        }
        let us = self.me().is_some_and(|s| s.options.sys != 0);
        for (ci, col) in self.columns.iter().enumerate() {
            let mask = self.picked_mask(ci);
            let hover_row = self.row_at(ci);
            let shown = col.scroll..col.scroll + col.visible();
            for (row, &i) in col.order.iter().enumerate() {
                if !shown.contains(&row) {
                    continue;
                }
                let name = &col.names[i];
                let y = col.rect.1 + 6.0 + (row - col.scroll) as f32 * ROW_H;
                let picked = i < 128 && mask & (1 << i) != 0;
                let price = col.prices[i];
                let affordable = c.observer || picked || left - price >= 0;
                let color = if picked {
                    PICKED
                } else if !affordable {
                    DIM
                } else if hover_row == Some(i) {
                    HOVER
                } else {
                    TEXT
                };
                let p = price.to_string();
                let w = self.text_width(12.0, &p);
                // Long names give way to the price.
                let room = col.rect.2 - 22.0 - w;
                let mut name = name.clone();
                while self.text_width(12.0, &name) > room && name.pop().is_some() {}
                self.text(out, col.rect.0 + 6.0, y, 12.0, color, &name);
                self.text(out, col.rect.0 + col.rect.2 - 8.0 - w, y, 12.0, color, &p);
            }
            // More rows above / below: scroll with the wheel.
            let mid = col.rect.0 + col.rect.2 / 2.0 - 4.0;
            if col.scroll > 0 {
                self.text(out, mid, col.rect.1 - 6.0, 12.0, TEXT, "^");
            }
            if col.scroll + col.visible() < col.names.len() {
                self.text(out, mid, col.rect.1 + col.rect.3 - 8.0, 12.0, TEXT, "v");
            }
        }
        // The view and info panels: the focused item, or a hovered toggle.
        let (pic, text): (Option<usize>, &[String]) = match region {
            1..=3 => (None, &self.day_texts[region as usize - 1]),
            4 => (None, &self.fog_text),
            5 => (None, &self.tranq_text),
            6 => (None, &self.observe_text),
            _ => match c.focus {
                Some((ci, r)) => {
                    let col = &self.columns[ci];
                    let t = if us { &col.texts_us } else { &col.texts };
                    (
                        col.pics.get(r).copied().flatten(),
                        t.get(r).map_or(&[][..], |v| &v[..]),
                    )
                }
                None => (None, &self.day_texts[c.time]),
            },
        };
        self.image_native(out, pic, 38.0, 74.0);
        for (i, l) in text.iter().enumerate().take(11) {
            self.text(out, 416.0, 80.0 + i as f32 * 17.0, 12.0, TEXT, l);
        }
        if !self.can_hunt() && region == 8 {
            let why = if self.choice.dinos == 0 {
                "Choose dinosaurs to hunt"
            } else {
                "Choose a weapon"
            };
            self.text(out, 420.0, 566.0, 12.0, HOVER, why);
        }
    }

    fn draw_options(&self, out: &mut Vec<Sprite>, region: u8) {
        self.board(out, &self.options, &[region]);
        let Some(s) = self.me() else { return };
        let mut copy = s.clone();
        for (o, r) in self.option_rows() {
            let hover = inside(self.cursor, r);
            let color = if hover { SAND } else { [0.85, 0.7, 0.5, 1.0] };
            match o {
                Opt::Slider(name, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = *f(&mut copy);
                    let x0 = r.0 + r.2 - 128.0 - 8.0;
                    self.image_native(out, self.slider, x0, r.1 + 4.0);
                    let kx = x0 + 4.0 + v.clamp(0, 255) as f32 * 120.0 / 255.0 - 4.0;
                    self.image_native(out, self.knob, kx, r.1 + 5.0);
                }
                Opt::PcSlider(name, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = *f(&mut self.pc.clone());
                    let x0 = r.0 + r.2 - 128.0 - 8.0;
                    self.image_native(out, self.slider, x0, r.1 + 4.0);
                    let kx = x0 + 4.0 + v.clamp(0, 255) as f32 * 120.0 / 255.0 - 4.0;
                    self.image_native(out, self.knob, kx, r.1 + 5.0);
                }
                Opt::Toggle(name, labels, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = labels[usize::from(*f(&mut copy) != 0)];
                    self.text(out, r.0 + SLIDER_X, r.1 + 4.0, 12.0, SAND, v);
                }
                Opt::PcToggle(name, f) => {
                    self.text(out, r.0, r.1 + 4.0, 12.0, color, name);
                    let v = ["Off", "On"][usize::from(*f(&mut self.pc.clone()) != 0)];
                    self.text(out, r.0 + SLIDER_X, r.1 + 4.0, 12.0, SAND, v);
                }
                Opt::ClearTrophies => {
                    let n = copy
                        .bodies
                        .iter()
                        .filter(|b| b.ctype != 0 && !b.locked())
                        .count();
                    let t = if self.clear_armed {
                        format!("Click again: remove {n}")
                    } else {
                        "Remove unlocked trophies".into()
                    };
                    let c = if self.clear_armed { PICKED } else { color };
                    self.text(out, r.0, r.1 + 4.0, 12.0, c, &t);
                }
            }
        }
        for (i, (name, f)) in ACTIONS.iter().enumerate() {
            let y = KEYS_PANEL.1 + i as f32 * 20.0;
            let r = (KEYS_PANEL.0, y, KEYS_PANEL.2, 20.0);
            let hover = inside(self.cursor, r);
            let color = if hover { SAND } else { [0.85, 0.7, 0.5, 1.0] };
            self.text(out, r.0, y + 4.0, 12.0, color, name);
            let key = if self.binding == Some(i) {
                "press a key...".to_string()
            } else {
                keys::name(*f(&mut copy.options.keys))
            };
            let kc = if self.binding == Some(i) {
                PICKED
            } else {
                SAND
            };
            self.text(out, r.0 + 170.0, y + 4.0, 12.0, kc, &key);
        }
    }

    fn draw_loading(&self, out: &mut Vec<Sprite>) {
        let Some((id, w, h)) = self
            .loading
            .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
        else {
            return;
        };
        // The top half is the picture; the bottom half its filled progress bar.
        let (s, ox, oy) = self.canvas();
        let (x, y) = ((CW - w) / 2.0, (CH - h / 2.0) / 2.0);
        out.push(Sprite {
            uv: [0.0, 0.0, 1.0, 0.5],
            size: Some([w * s, h / 2.0 * s]),
            ..Sprite::new(id, ox + x * s, oy + y * s, 1.0)
        });
    }
}

impl Scene for Menu {
    fn title(&self) -> String {
        "Carnivores 2 (c2rs)".into()
    }

    fn update(&mut self, _gpu: &Gpu, _dt: f32, input: &Input) {
        let (s, ox, oy) = self.canvas();
        self.cursor = input.cursor.map(|(x, y)| ((x - ox) / s, (y - oy) / s));
        let p = self.cursor;
        match self.screen {
            Screen::Register => {
                let r = self.reg.region(p);
                self.update_register(input, r);
            }
            Screen::Waiver => {
                let r = self.waiver.region(p);
                self.update_waiver(input, r);
            }
            Screen::Delete => {
                let r = self.delete.region(p);
                self.update_delete(input, r);
            }
            Screen::Main => {
                let r = self.main.region(p);
                self.update_main(input, r);
            }
            Screen::Quit => {
                let r = self.quit_board.region(p);
                if (input.clicked && r == 1) || input.pressed(KeyCode::KeyY) {
                    self.quit = true;
                } else if (input.clicked && r == 2)
                    || input.pressed(KeyCode::KeyN)
                    || input.pressed(KeyCode::Escape)
                {
                    self.screen = Screen::Main;
                }
            }
            Screen::Credits => {
                if input.clicked || input.pressed(KeyCode::Escape) || input.pressed(KeyCode::Enter)
                {
                    self.screen = Screen::Main;
                }
            }
            Screen::Setup if self.mobile.is_some() => self.update_mobile(input),
            Screen::Setup => {
                let r = self.setup.region(p);
                self.update_setup(input, r);
            }
            Screen::Options => {
                let r = self.options.region(p);
                self.update_options(input, r);
            }
            Screen::Loading => {}
        }
    }

    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        self.screen_size = screen;
        let mut out = Vec::new();
        let p = self.cursor;
        match self.screen {
            Screen::Register => self.draw_register(&mut out, self.reg.region(p)),
            Screen::Waiver => self.board(&mut out, &self.waiver, &[self.waiver.region(p)]),
            Screen::Delete => {
                self.board(&mut out, &self.delete, &[self.delete.region(p)]);
                if let Some(name) = self.picked.and_then(|i| self.players.get(i)) {
                    let t = format!("Delete {}?", name.player_name());
                    let x = 400.0 - self.text_width(14.0, &t) / 2.0;
                    self.text(&mut out, x, 388.0, 14.0, BLUE, &t);
                }
            }
            Screen::Main => self.draw_main(&mut out, self.main.region(p)),
            Screen::Quit => {
                let r = self.quit_board.region(p);
                self.board(&mut out, &self.quit_board, &[r]);
            }
            Screen::Credits => self.board(&mut out, &self.credits, &[]),
            Screen::Setup if self.mobile.is_some() => self.draw_mobile(&mut out),
            Screen::Setup => self.draw_setup(&mut out, self.setup.region(p)),
            Screen::Options => self.draw_options(&mut out, self.options.region(p)),
            Screen::Loading => {
                self.draw_loading(&mut out);
                self.shown_loading = true;
            }
        }
        self.hud.draw(gpu, pass, screen, &out);
    }

    fn handles_escape(&self) -> bool {
        true
    }

    fn wants_exit(&self) -> bool {
        self.quit
    }

    fn next(&mut self) -> Option<MakeScene> {
        if !self.shown_loading {
            return None;
        }
        let setup = self.launch.take()?;
        Some(Box::new(move |gpu| {
            Ok(Box::new(HuntView::load(gpu, setup)?) as Box<dyn Scene>)
        }))
    }
}
