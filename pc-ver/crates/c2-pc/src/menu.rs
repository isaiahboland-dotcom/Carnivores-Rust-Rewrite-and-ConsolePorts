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
use c2_core::game::SurvivalKind;
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
use c2_formats::bits::Bits;

mod board;
mod draw;
pub(crate) mod kit;
mod layout;
mod load;
mod main_menu;
mod mobile;
mod option_scroll;
mod options;
mod register;
mod roster;
mod scene;
mod setup;
mod tranq_ask;

use board::*;
use draw::wrap;
use layout::*;
pub use options::RESOLUTION;
use options::*;
use roster::*;
pub use roster::{dialect_res, menu_data, roamer_slots};

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
    dinos: Bits,
    weapons: u128,
    equip: u32,
    /// 0 dawn, 1 day, 2 night.
    time: usize,
    observer: bool,
    /// A survival instead of a hunt (README M13): free, the animals come.
    survival: Option<SurvivalKind>,
    /// The list item whose picture and description show: (column, row).
    focus: Option<(usize, usize)>,
    /// The list (1 animals, 2 weapons) whose last pick was refused for
    /// being at `--max-picks`; shown until the next pick.
    full: Option<usize>,
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

impl Column {
    /// Rows that fit in the box.
    fn visible(&self) -> usize {
        (((self.rect.3 - 8.0) / ROW_H) as usize).max(1)
    }
}

pub struct Menu {
    hud: Hud,
    game_dir: PathBuf,
    huntdat: PathBuf,
    /// Stock animals are present, so Classic can be offered.
    classic_survival: bool,
    /// Which survival? (more than one offered).
    survival_pick: bool,
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
    /// The phone kit's loading board (an APK-built game).
    kit_loading: Option<Board>,
    columns: Vec<Column>,
    day_texts: Vec<Vec<String>>,
    tranq_text: Vec<String>,
    fog_text: Vec<String>,
    /// Render distance and fog (not in the original save).
    pc: PcSettings,
    /// The menu's own animals; the roaming add-ons follow them.
    menu_animals: usize,
    observe_text: Vec<String>,
    areas: Vec<PathBuf>,
    players: Vec<Save>,
    /// Index into `players` of the signed-in hunter.
    player: Option<usize>,
    screen: Screen,
    /// The statistics panel is open on the main menu.
    stats: bool,
    /// The phone's MORE page of the main menu (an APK-built game): the
    /// hunter, options, credits, quit.
    more: bool,
    more_board: Board,
    /// Registration: typed name and highlighted list entry.
    name: String,
    picked: Option<usize>,
    choice: Choice,
    /// Options: the action waiting for a key, and the slider being dragged.
    binding: Option<usize>,
    /// The phone-style options' tab: game, controls, video.
    opt_tab: usize,
    game_scroll: option_scroll::Scroll,
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
    /// The animals' page in twenties (the phone layout, modded rosters).
    comb_page: usize,
    /// The phone game's interface art (a game built from an APK).
    kit: Option<kit::Kit>,
    /// The signed-in hunter's record (`trophy0N.rec`), read when the
    /// statistics open.
    record: c2_core::record::Record,
    /// "Remove unlocked trophies" was clicked once (a second click does it).
    clear_armed: bool,
    /// Multi ammo's Half / Full tranq question is open.
    tranq_ask: bool,
}

/// Menu style (`--menu-art`): a game folder whose menu frames, hit maps
/// and slider are used (the original's, a mod's, Ice Age's); the pictures
/// and texts stay the game's.
pub static MENU_ART: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The experimental mobile layout of the hunt setup (`--mobile-menu`).
pub static MOBILE_MENU: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Most animals, and most weapons, one hunt can take (`--max-picks N`; the
/// Android build passes 8). Unset: no limit beyond the account.
pub static MAX_PICKS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

impl Menu {
    /// Opens a screen directly (screenshots): signs in the first hunter.
    pub fn show(&mut self, screen: &str) -> Result<()> {
        if self.player.is_none() && !self.players.is_empty() {
            self.sign_in(0);
        }
        self.screen = match screen {
            "main" => Screen::Main,
            "more" => {
                self.more = self.kit.is_some();
                Screen::Main
            }
            "stats" => {
                self.stats = true;
                self.more = self.kit.is_some();
                if let Some(p) = self.me() {
                    self.record = crate::save::load_record(p.reg_number);
                }
                Screen::Main
            }
            "hunt" => {
                // Something picked, to show the lists and account.
                self.choice.dinos = Bits::bit(0);
                self.choice.weapons = 1;
                self.choice.focus = Some((1, 0));
                Screen::Setup
            }
            "options" => Screen::Options,
            "credits" => Screen::Credits,
            "register" => Screen::Register,
            "loading" => Screen::Loading,
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
}
