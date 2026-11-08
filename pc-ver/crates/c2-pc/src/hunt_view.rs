//! `c2-pc hunt <AREA.MAP>`: hunting an area (M3, in progress).
//!
//! The simulation lives in `c2_core::game::Hunt`; this file maps input,
//! drives fixed ticks and draws the world, animals, weapon and HUD.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{
    GameEvent, Hunt, HuntInput, HuntOptions, Outcome, SurvivalKind, SurvivalOptions, WeaponInfo,
    Weapons,
};
use c2_core::player::{Controls, Player};
use c2_core::rng::MsRand;
use c2_formats::car::Car;
use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::area_view::{find_file, AreaView};
use crate::audio::Audio;
use crate::dinos::{load_res, load_ship, load_species, DinoAssets};
use crate::effects_view::EffectsView;
use crate::gpu::Gpu;
use crate::hud::{load_tga, Hud, Sprite};
use crate::hunt_kit::{Buttons, HuntKit};
use crate::input::Input;
use crate::keys;
use crate::menu::kit::{GOLD, LIGHT};
use crate::scene::Scene;
use crate::sounds::{get, SoundBank};
use crate::weapons::{load_weapons, WeaponAssets};
use c2_core::ship::ship_sound;
use c2_core::sound::SoundCmd;

mod debug;
mod draw;
mod extras;
mod gauges;
mod lights;
pub(crate) use lights::bought as lights_bought;
mod load;
mod map;
mod map_viewport;
mod new_hunt;
mod picker;
mod prompts;
mod regress;
mod survival_hud;
mod touch;
mod trophy;
mod update;

use picker::{Pick, HOLD_TO_PICK};
use trophy::Ask;

/// A key's name as the bindings show it.
fn keys_name(k: KeyCode) -> String {
    keys::vk_of(k).map_or_else(|| format!("{k:?}"), keys::name)
}

/// Gamepad: a stick past this walks (as a key would).
const PAD_STEP: f32 = 0.33;
/// Gamepad: the right stick full over, in mouse pixels a second.
const PAD_LOOK: f32 = 900.0;

/// Simulation step. The original runs on variable `TimeDt`; a fixed step keeps
/// the simulation deterministic (needed for logic parity later).
const TICK_MS: u32 = 10;
/// The most animals in a survival wave on the PC.
const SURVIVAL_MAX: usize = 12;

/// What to hunt with. The original menu passes these to the game on its
/// command line (`prj=`, `din=`, `wep=`, `dtm=`, `-double`, `-observ`); the
/// accessories come from the player's save.
#[derive(Debug, Clone)]
pub struct HuntSetup {
    /// The game install (`HUNTDAT`'s parent).
    pub game_dir: PathBuf,
    /// `HUNTDAT/AREAS/AREAn.MAP`, or `TROPHY.MAP` for the trophy room.
    pub map: PathBuf,
    pub time: TimeOfDay,
    /// Fixed RNG seed (reproducible runs); `None` = from the clock.
    pub seed: Option<u32>,
    /// `TargetDino`: bit per AI.
    pub targets: c2_formats::bits::Bits,
    /// `WeaponPres`: bit per `_RES.TXT` weapon.
    pub weapons: u128,
    pub double_ammo: bool,
    pub night_vision: bool,
    pub flashlight: bool,
    pub observer: bool,
    /// A survival instead of a hunt (README M13).
    pub survival: Option<SurvivalKind>,
    /// The player (`trophy0N.sav`).
    pub reg: i32,
    /// Return to the menu afterwards (otherwise close the window).
    pub from_menu: bool,
    /// Render distance and fog (PC-only settings).
    pub pc: crate::settings::PcSettings,
}

impl HuntSetup {
    /// The original enables trophy mode when the project name contains "trophy".
    pub fn trophy(&self) -> bool {
        self.map
            .file_stem()
            .is_some_and(|s| s.to_string_lossy().to_ascii_lowercase().contains("trophy"))
    }
}

pub struct HuntView {
    view: AreaView,
    hunt: Hunt,
    dinos: DinoAssets,
    weapons: WeaponAssets,
    weapon_infos: Vec<WeaponInfo>,
    weapon_anims: Vec<c2_core::game::WeaponAnims>,
    hud: Hud,
    exit_img: Option<usize>,
    map_img: Option<usize>,
    /// Where the area sits in the map picture (`MAPFRAME.TGA`'s 11, 23).
    map_at: (f32, f32),
    /// The phone game's hunt screen (an APK-built game).
    kit: Option<HuntKit>,
    /// The original's wind indicator and compass (not with the kit, which
    /// has its own), and Caps Lock's toggle that hides them.
    gauges: Option<gauges::Gauges>,
    gauges_hidden: bool,
    /// The map's tick for where the hunter looks (PC setting).
    map_heading: bool,
    /// The map drawn as the mobile game's (PC setting).
    map_mobile: bool,
    /// Teleporting: the marker on the open map (world x, z), and the spot
    /// picked (a circle) waiting for Enter.
    map_cursor: Option<(f32, f32)>,
    /// The window's size at the last draw (taps on the light buttons).
    screen: (u32, u32),
    map_viewport: map_viewport::MapViewport,
    teleport_pick: Option<(f32, f32)>,
    call_imgs: Vec<Option<usize>>,
    /// The phone kit's call pictures (its animals'), by call id 10..
    call_pics: Vec<Option<usize>>,
    /// The trophy plaque (`TROPHY.TGA` in the room, `TROPHY_G.TGA` after a
    /// kill) and the room's "Esc to exit" banner (`TROPHY_E.TGA`).
    plaque_img: Option<usize>,
    pause_img: Option<usize>,
    trophy_exit_img: Option<usize>,
    setup: HuntSetup,
    /// Camera position before the latest tick, for interpolation.
    prev_cam: [f32; 3],
    acc_ms: f32,
    /// `RunMode` (toggled with the run key).
    run: bool,
    /// Crouching from the gamepad (D-pad up toggles it).
    pad_crouch: bool,
    /// The phone's on-screen buttons (`touch`).
    touch: touch::Touch,
    /// The pad was used more lately than the keys and mouse (which hints
    /// to show).
    pad_last: bool,
    /// Key presses seen this frame, applied on the next tick.
    pending: HuntInput,
    events: Vec<GameEvent>,
    message: Option<(String, f32)>,
    /// The last kill's notice (bottom right): animal, points, metres,
    /// tranquilized; seconds left.
    kill: Option<(String, i32, f32, bool, f32, u8)>,
    /// The trophy room's question waiting for Y / N.
    ask: Option<Ask>,
    /// The weapon-pick screen (PC only): open with the highlighted row.
    /// The hunt doesn't tick while it's up.
    picker: Option<usize>,
    /// What the picker picks, and whether it was opened by holding its key
    /// (letting go then picks).
    picker_kind: Pick,
    picker_held: bool,
    /// A pick key held down: which, and for how long (seconds).
    held_pick: Option<(Pick, f32)>,
    /// Seconds left of the new weapon's hexagon (the phone's look).
    weapon_flash: f32,
    seed: u32,
    /// The hunt is over (saved if left alive).
    done: bool,
    audio: Audio,
    effects: EffectsView,
    sounds: SoundBank,
    /// Modder's Edition data: the hunt's spawn tables (`None`: original data).
    mee: Option<c2_formats::mee::Tables>,
    /// The weapon whose rounds are drawn top left, and the lowest they
    /// reached on screen since (the survival box stays under them).
    ammo_low: (usize, f32),
}

impl HuntView {
    /// Dead with the death camera turned off: nothing but the prompt.
    fn blacked_out(&self) -> bool {
        self.hunt.death.is_some() && !self.hunt.opts.death_anim
    }
}

impl Scene for HuntView {
    fn title(&self) -> String {
        let h = &self.hunt;
        let p = &h.player;
        let w = &h.weapons;
        let weapon = match w.pose() {
            Some(_) => format!(
                " — {} ({} left)",
                w.info[w.current].name,
                w.rounds(w.current)
            ),
            None => String::new(),
        };
        let msg = self
            .message
            .as_ref()
            .map(|(m, _)| format!(" — {m}"))
            .unwrap_or_default();
        format!(
            "c2rs hunt {} — {:.0} fps — {} animals, {} bagged{}{}{}{}",
            self.view.name(),
            self.view.fps,
            h.herd
                .chars
                .iter()
                .filter(|c| c.health > 0 && c.ai >= 10)
                .count(),
            h.successes,
            weapon,
            if self.run { " — RUN" } else { "" },
            if p.underwater {
                " — UNDERWATER"
            } else if p.swim {
                " — SWIMMING"
            } else {
                ""
            },
            msg,
        )
    }

    fn update(&mut self, gpu: &Gpu, dt: f32, input: &Input) {
        self.update_hunt(gpu, dt, input);
    }

    fn clear_color(&self) -> [f64; 3] {
        if self.blacked_out() {
            return [0.0; 3];
        }
        self.view.clear_color()
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        self.draw_frame(gpu, pass, screen);
    }

    fn before_update(&mut self, input: &mut Input) {
        self.touch_input(input);
    }

    fn grab_cursor(&self) -> bool {
        true
    }

    fn handles_escape(&self) -> bool {
        true
    }

    fn wants_exit(&self) -> bool {
        self.done && !self.setup.from_menu
    }

    fn next(&mut self) -> Option<crate::scene::MakeScene> {
        if !(self.done && self.setup.from_menu) {
            return None;
        }
        let (game_dir, reg) = (self.setup.game_dir.clone(), self.setup.reg);
        Some(Box::new(move |gpu| {
            Ok(Box::new(crate::menu::Menu::new(gpu, &game_dir, Some(reg))?) as Box<dyn Scene>)
        }))
    }
}
