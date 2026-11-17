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
mod last_hunt;
mod layout;
mod load;
mod main_menu;
mod mobile;
mod option_scroll;
mod options;
mod parity;
mod register;
mod roster;
mod scene;
mod setup;
mod specimens;
mod terminal;
mod tranq_ask;
pub(crate) use parity::run as run_parity;
pub(crate) use terminal::crt::ALL_LAYERS as CRT_ALL_LAYERS;
pub(crate) use terminal::{palette, terminal_look};

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
    /// The DinoHunt Corp terminal (`terminal`), standing in for the main
    /// menu and hunt setup when its option is on.
    Terminal,
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
    /// A hardcore hunt: started from the terminal's hardcore entry (the
    /// original menu has none, so there the instance's ruleset applies to
    /// every hunt, see `Menu::hardcore_applies`).
    hardcore: bool,
    /// The list item whose picture and description show: (column, row).
    focus: Option<(usize, usize)>,
    /// The list (1 animals, 2 weapons) whose last pick was refused for
    /// being at `--max-picks`; shown until the next pick.
    full: Option<usize>,
    /// The last pick was refused by the area's island rules (`_DHC.TXT`);
    /// shown until the next pick.
    barred: bool,
}

/// Accessories in the equipment list, with their picture and description.
const EQUIPMENT: [(&str, &str); 4] = [
    ("Camouflage", "CAMOFLAG.NFO"),
    ("Radar", "RADAR.NFO"),
    ("Cover Scent", "SCENT.NFO"),
    ("Double Ammo", "DOUBLE.NFO"),
];

/// The gear page's advanced binoculars: `equip` bit, and price in points
/// (not in the original; offered with or without dynamic time).
const ADVANCED_BINOCULARS: u32 = 1 << 6;
const ADVANCED_BINOCULARS_PRICE: i32 = 40;
/// The advanced binoculars are desktop-only: Android neither lists nor
/// applies them.
const OFFERS_ADVANCED_BINOCULARS: bool = !cfg!(target_os = "android");

/// The gear page's scuba gear: `equip` bit, and price in points (not in
/// the original; offered with or without dynamic time, Android too, only
/// where the instance's rules list it: [`scuba_offered`]).
const SCUBA: u32 = 1 << 7;
const SCUBA_PRICE: i32 = 30;

/// Whether the game in force offers scuba gear (`_RULES.TXT`'s `scuba`
/// line, the launcher's Advanced page; hidden by default).
fn scuba_offered() -> bool {
    crate::instance_rules::get().scuba
}

/// The gear page's extras, after the accessories: night vision and the
/// flashlight (dynamic time only), the advanced binoculars (not on
/// Android), then the scuba gear (where offered).
fn gear_order(dynamic_time: bool, scuba: bool) -> Vec<usize> {
    (0..4).chain(gear_extras(dynamic_time, scuba)).collect()
}

/// The gear list items past the four accessories, in page order.
fn gear_extras(dynamic_time: bool, scuba: bool) -> Vec<usize> {
    let lights: &[usize] = if dynamic_time { &[4, 5] } else { &[] };
    let binoculars: &[usize] = if OFFERS_ADVANCED_BINOCULARS {
        &[6]
    } else {
        &[]
    };
    let scuba: &[usize] = if scuba { &[7] } else { &[] };
    lights
        .iter()
        .chain(binoculars)
        .chain(scuba)
        .copied()
        .collect()
}

/// The `equip` bits a hunt without dynamic time keeps (no lights).
const NO_LIGHTS: u32 = !(16 | 32);

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
    /// Survival is offered at all ([`OFFER_SURVIVAL`], the instance's
    /// rule, or a game built from the APK).
    survival_offered: bool,
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
    /// The terminal-look options' tab: game, video, controls.
    term_opt_tab: usize,
    /// The last input was the gamepad: the terminal's key hints name its buttons.
    pad_used: bool,
    game_scroll: option_scroll::Scroll,
    /// The board's Video panel, likewise.
    video_scroll: option_scroll::Scroll,
    drag: Option<Opt>,
    /// Hunt to start after the loading picture has been shown.
    launch: Option<HuntSetup>,
    shown_loading: bool,
    /// The terminal's own loading log, shown in place of the loading
    /// picture while a hunt or survival run it started loads.
    term_loading: Option<Vec<String>>,
    quit: bool,
    /// Last drawn window size, for mapping the cursor to the canvas.
    screen_size: (u32, u32),
    cursor: Option<(f32, f32)>,
    /// The experimental mobile layout's art, when it's on, and its page
    /// (0 maps, 1 animals, 2 weapons and gear).
    mobile: Option<mobile::Art>,
    mobile_page: usize,
    /// The current sheet of each mobile selection page.
    comb_page: [usize; 3],
    mobile_card_layout: bool,
    /// The phone game's interface art (a game built from an APK).
    kit: Option<kit::Kit>,
    /// The signed-in hunter's record (`trophy0N.rec`), read when the
    /// statistics open.
    record: c2_core::record::Record,
    /// "Remove unlocked trophies" was clicked once (a second click does it).
    clear_armed: bool,
    /// Multi ammo's Half / Full tranq question is open.
    tranq_ask: bool,
    /// The terminal, once opened.
    term: Option<terminal::TermView>,
    /// The field manual's specimen files' facts.
    specimens: specimens::Specimens,
    /// The open file's 3D specimen ("DHC OS 3D specimens"), while it shows.
    viewer: Option<terminal::viewer::Viewer>,
    /// The recipe's `_DHC.TXT` (none in Play Vanilla): its island rules
    /// apply to the original menus as well as the terminal.
    dhc: Option<c2_core::dhc::Dhc>,
    /// The terminal's beeps' sound output, once one has played; closed
    /// when a hunt starts.
    beeper: Option<terminal::beeps::Beeper>,
    /// The terminal's CRT pass, once drawn, and whether this frame's
    /// screen is one of the terminal's (its status bar was drawn).
    crt: Option<terminal::crt::Crt>,
    crt_frame: std::cell::Cell<bool>,
}

/// Menu style (`--menu-art`): a game folder whose menu frames, hit maps
/// and slider are used (the original's, a mod's, Ice Age's); the pictures
/// and texts stay the game's.
pub static MENU_ART: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Survival offered in the menus (`--offer-survival`: the launcher's
/// switch). The instance's `survival` rule and a game built from the
/// mobile APK offer it too; otherwise the menus never show it.
pub static OFFER_SURVIVAL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

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
            // The terminal, on for this run only (not stored).
            "terminal" | "terminal-hunt" | "terminal-boot" | "terminal-login"
            | "terminal-waiver" | "terminal-delete" | "terminal-options" | "terminal-quit"
            | "terminal-manual" | "terminal-manual-3d" => {
                self.show_terminal(screen);
                return Ok(());
            }
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
        // A different hunter: the terminal is rebuilt for them.
        self.term = None;
        self.go_home();
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
        self.recall_hunt();
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

#[cfg(test)]
mod gear_tests {
    use super::{gear_order, ADVANCED_BINOCULARS, NO_LIGHTS, SCUBA};

    #[test]
    #[cfg(not(target_os = "android"))]
    fn advanced_binoculars_and_scuba_are_offered_with_or_without_dynamic_time() {
        assert_eq!(gear_order(false, true), [0, 1, 2, 3, 6, 7]);
        assert_eq!(gear_order(true, true), [0, 1, 2, 3, 4, 5, 6, 7]);
        // Scuba gear only where the instance offers it.
        assert_eq!(gear_order(false, false), [0, 1, 2, 3, 6]);
        assert_eq!(gear_order(true, false), [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(ADVANCED_BINOCULARS, 1 << 6);
        assert_eq!(SCUBA, 1 << 7);
        // Turning dynamic time off drops the lights, not the binoculars or
        // the scuba gear.
        assert_eq!((0b1111_1111 & NO_LIGHTS), 0b1100_1111);
    }

    #[test]
    #[cfg(target_os = "android")]
    fn android_offers_scuba_but_not_advanced_binoculars() {
        assert_eq!(gear_order(false, true), [0, 1, 2, 3, 7]);
        assert_eq!(gear_order(true, true), [0, 1, 2, 3, 4, 5, 7]);
        assert_eq!(gear_order(false, false), [0, 1, 2, 3]);
    }
}
