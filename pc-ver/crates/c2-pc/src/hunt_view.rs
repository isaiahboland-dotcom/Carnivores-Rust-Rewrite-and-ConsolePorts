//! `c2-pc hunt <AREA.MAP>`: hunting an area (M3, in progress).
//!
//! The simulation lives in `c2_core::game::Hunt`; this file maps input,
//! drives fixed ticks and draws the world, animals, weapon and HUD.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use c2_core::area::TimeOfDay;
use c2_core::dino::{Herd, PlaceOptions};
use c2_core::game::{GameEvent, Hunt, HuntInput, HuntOptions, Outcome, WeaponInfo, Weapons};
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
use crate::input::Input;
use crate::keys;
use crate::scene::Scene;
use crate::sounds::{get, SoundBank};
use crate::weapons::{load_weapons, WeaponAssets};
use c2_core::ship::ship_sound;
use c2_core::sound::SoundCmd;

/// Simulation step. The original runs on variable `TimeDt`; a fixed step keeps
/// the simulation deterministic (needed for logic parity later).
const TICK_MS: u32 = 10;

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
    pub targets: u128,
    /// `WeaponPres`: bit per `_RES.TXT` weapon.
    pub weapons: u128,
    pub double_ammo: bool,
    pub observer: bool,
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
    weapon_anims: Vec<[i32; 4]>,
    hud: Hud,
    exit_img: Option<usize>,
    map_img: Option<usize>,
    call_imgs: Vec<Option<usize>>,
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
    /// Key presses seen this frame, applied on the next tick.
    pending: HuntInput,
    events: Vec<GameEvent>,
    message: Option<(String, f32)>,
    /// The last kill's notice (bottom right): animal, points, metres,
    /// tranquilized; seconds left.
    kill: Option<(String, i32, f32, bool, f32)>,
    /// The weapon-pick screen (PC only): open with the highlighted row.
    /// The hunt doesn't tick while it's up.
    picker: Option<usize>,
    seed: u32,
    /// The hunt is over (saved if left alive).
    done: bool,
    audio: Audio,
    effects: EffectsView,
    sounds: SoundBank,
    /// Modder's Edition data: the hunt's spawn tables (`None`: original data).
    mee: Option<c2_formats::mee::Tables>,
}

/// The Modder's Edition hunt conditions for `setup` (area from the map
/// name, dawn/day/night, the menu slots picked).
fn mee_hunt(setup: &HuntSetup) -> c2_formats::mee::Hunt {
    let stem = setup
        .map
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    c2_formats::mee::Hunt {
        area: c2_formats::mee::area_of(&stem),
        time: match setup.time {
            TimeOfDay::Dusk => 0,
            TimeOfDay::Day => 1,
            TimeOfDay::Night => 2,
        },
        // Menu slot i is target bit 10 + i, so up to 22 slots (add-ons
        // past the engine's 10 get slots of their own).
        picked: setup.targets >> 10,
    }
}

fn menu_pic(hud: &mut Hud, gpu: &Gpu, huntdat: &Path, name: &str) -> Option<usize> {
    let p = find_file(&find_file(huntdat, "MENU")?, name)?;
    let (w, h, rgba) = load_tga(&p).ok()?;
    Some(hud.add_image(gpu, w, h, &rgba))
}

impl HuntView {
    pub fn load(gpu: &Gpu, setup: HuntSetup) -> Result<Self> {
        let mut view = AreaView::load(gpu, &setup.map, setup.time)?;
        view.fly = false;
        let seed = setup.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        });
        let huntdat = setup
            .map
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        // Modder's Edition data is resolved for this hunt first (per-area
        // models, spawn tables); original data is read as it is.
        let res_txt = find_file(&huntdat, "_RES.TXT")
            .and_then(|p| std::fs::read(p).ok())
            .map(|d| c2_formats::res_txt::ResTxt::parse(&String::from_utf8_lossy(&d)))
            .transpose()?
            .filter(c2_formats::mee::is_dialect);
        let (res, mee) = match res_txt {
            Some(r) => {
                let h = mee_hunt(&setup);
                let mut m = c2_core::mee::read(&r, &h);
                c2_core::mee::spawn_picked_anywhere(&r, &h, &mut m.tables, &[]);
                (m.res, Some(m.tables))
            }
            None => (load_res(&huntdat)?, None),
        };
        let (mut species, cars) = load_species(&huntdat, &res)?;
        if let Some(t) = &mee {
            c2_core::mee::tune(&mut species, t);
        }
        let mut hud = Hud::new(gpu);
        let (weapon_infos, weapon_anims, weapons) =
            load_weapons(gpu, view.globals(), &mut hud, &huntdat, &res.weapons);
        let exit_img = menu_pic(&mut hud, gpu, &huntdat, "EXIT.TGA");
        let plaque_img = menu_pic(
            &mut hud,
            gpu,
            &huntdat,
            if setup.trophy() {
                "TROPHY.TGA"
            } else {
                "TROPHY_G.TGA"
            },
        );
        let trophy_exit_img = menu_pic(&mut hud, gpu, &huntdat, "TROPHY_E.TGA");
        let pause_img = menu_pic(&mut hud, gpu, &huntdat, "PAUSE.TGA");
        // The map: the frame with the area overview painted in at (11, 23).
        let map_img = find_file(&huntdat, "MENU")
            .and_then(|m| find_file(&m, "MAPFRAME.TGA"))
            .and_then(|p| c2_formats::tga::parse(&std::fs::read(p).ok()?).ok())
            .map(|mut frame| {
                let px = Hunt::map_image(view.area());
                for y in 0..256 {
                    for x in 0..256 {
                        let (fx, fy) = (x + 11, y + 23);
                        if fx < frame.width && fy < frame.height {
                            frame.pixels[fy * frame.width + fx] = px[y * 256 + x];
                        }
                    }
                }
                let rgba = c2_core::color::image_to_rgba8(&frame.pixels, false);
                hud.add_image(gpu, frame.width as u32, frame.height as u32, &rgba)
            });
        // Call icons (`MENU/PICS/CALLn.TGA`), by call id 10.. (menu slot +
        // 10), for the hunted animals.
        let pics = find_file(&huntdat, "MENU").and_then(|m| find_file(&m, "PICS"));
        let call_imgs: Vec<Option<usize>> = (1..=c2_core::game::MAX_CALL - 10)
            .map(|n| {
                if setup.targets & (1 << (n + 9)) == 0 {
                    return None;
                }
                let p = find_file(pics.as_deref()?, &format!("CALL{n}.TGA"))?;
                let (w, h, rgba) = load_tga(&p).ok()?;
                Some(hud.add_image(gpu, w, h, &rgba))
            })
            .collect();
        let dinos = DinoAssets::new(gpu, view.globals(), cars, load_ship(&huntdat));
        let species_cars: Vec<Option<&Car>> = dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        let sounds = SoundBank::load(
            &huntdat,
            &species_cars,
            &weapons.cars(),
            dinos.ship_car(),
            &view.area().rsc,
            setup.targets,
        );
        let mut hunt = Self::new_hunt(
            &view,
            species,
            &weapon_infos,
            &weapon_anims,
            seed,
            &setup,
            mee.as_ref(),
        );
        hunt.ship_anim_ms = dinos.ship_anim_ms();
        view.radius = hunt.opts.render_r.max(hunt.opts.view_r) as u32;
        // Past the original range, fog only softens the far edge.
        if hunt.opts.render_r > hunt.opts.view_r {
            view.fog_start = 0.85;
        }
        view.brightness = (hunt.room.options.brightness + 128) as f32 / 256.0;
        let prev_cam = hunt.camera().0;
        let effects = EffectsView::new(gpu, view.globals(), &huntdat);
        Ok(Self {
            view,
            hunt,
            dinos,
            weapons,
            weapon_infos,
            weapon_anims,
            hud,
            exit_img,
            map_img,
            call_imgs,
            plaque_img,
            pause_img,
            trophy_exit_img,
            setup,
            prev_cam,
            acc_ms: 0.0,
            run: false,
            pending: HuntInput::default(),
            events: Vec::new(),
            message: None,
            kill: None,
            picker: None,
            seed,
            done: false,
            audio: Audio::new(),
            effects,
            sounds,
            mee,
        })
    }

    /// `ReInitGame`: the saved trophy room is (re)loaded, the hunter put at a
    /// random landing spot and the herd placed (or the trophies, in the room).
    fn new_hunt(
        view: &AreaView,
        species: Vec<c2_core::dino::Species>,
        infos: &[WeaponInfo],
        anims: &[[i32; 4]],
        seed: u32,
        setup: &HuntSetup,
        mee: Option<&c2_formats::mee::Tables>,
    ) -> Hunt {
        let area = view.area();
        let room = crate::save::load(&setup.game_dir, setup.reg);
        let o = &room.options;
        let trophy = setup.trophy();
        let mut rng = MsRand::new(seed);
        let player = if trophy {
            Player::new(area, (76, 70))
        } else {
            let cell = area.landings[seed as usize % area.landings.len().max(1)];
            Player::new(area, cell)
        };
        let mut herd = Herd {
            species,
            chars: Vec::new(),
            demo_index: None,
            ..Default::default()
        };
        if trophy {
            herd.place_trophies(area, &room, &mut rng);
        } else if let Some(t) = mee {
            herd.place_mee(area, (player.x, player.z), &mut rng, t, o.dens);
        } else {
            let opts = PlaceOptions {
                density: o.dens,
                opt_agres: o.agres,
                targets: setup.targets,
                max_ambient: None,
            };
            herd.place(area, (player.x, player.z), &mut rng, opts);
        }
        tracing::info!("placed {} characters", herd.chars.len());
        let carried = if trophy { 0 } else { setup.weapons };
        let weapons = Weapons::new(infos.to_vec(), anims.to_vec(), carried, setup.double_ammo);
        let opts = HuntOptions {
            targets: setup.targets,
            radar: o.radar != 0,
            opt_agres: o.agres,
            opt_sens: o.sens,
            // ctViewR: 42 + OptViewR / 8 * 2 cells; 60 in the trophy room.
            view_r: if trophy { 60 } else { 42 + o.view_r / 8 * 2 },
            render_r: 0,
            camo: o.camo != 0,
            scent: o.scent != 0,
            tranq: o.tranq != 0,
            observer: setup.observer,
            trophy,
            score_factor: 1.0,
            dino_run_k: 1.0,
            blood: setup.pc.blood != 0,
            death_anim: setup.pc.death_anim != 0,
            afraid_every: 1,
        };
        let mut opts = opts;
        opts.render_r = if trophy {
            opts.view_r
        } else {
            setup.pc.render_cells(opts.view_r)
        };
        let mut hunt = Hunt::new(area, player, herd, weapons, rng, opts, room);
        hunt.clock = local_clock();
        hunt
    }

    /// Controls from the player's key bindings (`ProcessControls`).
    fn controls(&self, input: &Input) -> Controls {
        let o = &self.hunt.room.options;
        let k = &o.keys;
        let held = |vk| keys::held(input, vk);
        // The strafe key turns the turn keys into side-steps.
        let strafe = held(k.strafe);
        let mouse = if o.reverse_mouse != 0 {
            (input.mouse_delta.0, -input.mouse_delta.1)
        } else {
            input.mouse_delta
        };
        Controls {
            forward: held(k.forward),
            backward: held(k.backward),
            strafe_left: held(k.strafe_left) || (strafe && held(k.left)),
            strafe_right: held(k.strafe_right) || (strafe && held(k.right)),
            turn_left: !strafe && held(k.left),
            turn_right: !strafe && held(k.right),
            look_up: held(k.up),
            look_down: held(k.down),
            crouch: held(k.crouch),
            jump: held(k.jump),
            run: self.run,
            mouse,
            mouse_sens: o.mouse_sens as f32,
        }
    }

    /// Debug/screenshot helper: simulate `ms` with no input, then stand the
    /// hunter `dist` units from the first animal with AI `ai_id`, facing it;
    /// `after_ms` more pass with the camera fixed.
    pub fn spectate(&mut self, ms: u32, ai_id: i32, dist: f32, after_ms: u32) {
        let idle = HuntInput::default();
        for _ in 0..ms / TICK_MS {
            self.step(&idle);
        }
        let Some(d) = self
            .hunt
            .herd
            .chars
            .iter()
            .find(|ch| ch.ai == ai_id)
            .cloned()
        else {
            tracing::warn!("no character with ai {ai_id}");
            return;
        };
        // Beside it; in the trophy room, where its plaque shows (2.5 cells
        // in front of it).
        let (x, z) = if self.hunt.opts.trophy {
            (d.pos[0] + d.lookx * 640.0, d.pos[2] + d.lookz * 640.0)
        } else {
            (d.pos[0] + d.lookz * dist, d.pos[2] - d.lookx * dist)
        };
        let area = self.view.area();
        let p = &mut self.hunt.player;
        p.x = x;
        p.z = z;
        p.y = area.land_qh(x, z, f32::MAX);
        p.alpha = f32::atan2(d.pos[0] - x, -(d.pos[2] - z));
        p.beta = 0.1;
        let keep = (p.x, p.y, p.z, p.alpha, p.beta);
        for _ in 0..after_ms / TICK_MS {
            self.step(&idle);
            let p = &mut self.hunt.player;
            (p.x, p.y, p.z, p.alpha, p.beta) = keep;
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Debug/screenshot helper: bag the first animal with AI `ai_id`, run
    /// `ms`, then look at the ship from `dist` away.
    pub fn watch_ship(&mut self, ai_id: i32, ms: u32, dist: f32) {
        if let Some(i) = self
            .hunt
            .herd
            .chars
            .iter()
            .position(|c| c.ai == ai_id && c.health > 0)
        {
            self.hunt.debug_bag(self.view.area(), i, &mut self.events);
        }
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        let s = self.hunt.ship.pos;
        tracing::info!(
            "ship state {} at {:?} alpha {:.2}",
            self.hunt.ship.state,
            s,
            self.hunt.ship.alpha
        );
        let p = &mut self.hunt.player;
        p.x = s[0] + dist;
        p.z = s[2];
        p.y = s[1] - 400.0;
        p.alpha = -std::f32::consts::FRAC_PI_2;
        p.beta = 0.0;
        self.prev_cam = self.hunt.camera().0;
    }

    /// Fires once, then runs `ms` (screenshots).
    pub fn fire_and_wait(&mut self, ms: u32) {
        self.step(&HuntInput {
            fire: true,
            ..Default::default()
        });
        for _ in 0..ms / TICK_MS {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }

    /// Opens the map right away (screenshots).
    pub fn open_map(&mut self) {
        let input = HuntInput {
            toggle_map: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Raises the binoculars right away (screenshots).
    pub fn raise_binoculars(&mut self) {
        let input = HuntInput {
            toggle_binoculars: true,
            ..Default::default()
        };
        self.step(&input);
    }

    /// Dead with the death camera turned off: nothing but the prompt.
    fn blacked_out(&self) -> bool {
        self.hunt.death.is_some() && !self.hunt.opts.death_anim
    }

    /// The carried weapons, for the pick screen.
    fn carried(&self) -> Vec<usize> {
        let w = &self.hunt.weapons;
        (0..w.info.len()).filter(|&i| w.carried(i)).collect()
    }

    /// The pick screen's key: Q, or G when Q is bound to something.
    fn pick_key(&self) -> KeyCode {
        let k = &self.hunt.room.options.keys;
        let q = 81;
        let bound = [
            k.forward,
            k.backward,
            k.up,
            k.down,
            k.left,
            k.right,
            k.fire,
            k.show,
            k.strafe_left,
            k.strafe_right,
            k.strafe,
            k.jump,
            k.run,
            k.crouch,
            k.call,
            k.change_call,
            k.binoculars,
        ]
        .contains(&q);
        if bound {
            KeyCode::KeyG
        } else {
            KeyCode::KeyQ
        }
    }

    /// Input while the pick screen is up: arrows/W/S or the wheel move,
    /// 1-9 jump, Enter/click picks, the key again or Esc closes.
    fn update_picker(&mut self, input: &Input, row: usize) {
        let list = self.carried();
        let n = list.len();
        if n == 0 || input.pressed(KeyCode::Escape) || input.pressed(self.pick_key()) {
            self.picker = None;
            return;
        }
        let mut row = row.min(n - 1);
        let up =
            input.pressed(KeyCode::ArrowUp) || input.pressed(KeyCode::KeyW) || input.scroll > 0.0;
        let down =
            input.pressed(KeyCode::ArrowDown) || input.pressed(KeyCode::KeyS) || input.scroll < 0.0;
        if up {
            row = (row + n - 1) % n;
        }
        if down {
            row = (row + 1) % n;
        }
        const DIGITS: [KeyCode; 9] = [
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
        let digit = DIGITS
            .iter()
            .position(|&k| input.pressed(k))
            .filter(|&d| d < n);
        if let Some(d) = digit {
            row = d;
        }
        if digit.is_some() || input.pressed(KeyCode::Enter) {
            let w = &self.hunt.weapons;
            let pick = list.get(row).copied();
            // Picking the weapon in hand leaves it there (selecting it
            // would put it away).
            if !(pick == Some(w.current) && w.pose().is_some()) {
                self.pending.select_weapon = pick;
            }
            self.picker = None;
            return;
        }
        self.picker = Some(row);
    }

    /// The weapon-pick screen: a dimmed world and a list of the carried
    /// weapons with their ammo, scrolled to keep `row` in view.
    fn picker_sprites(&self, row: usize, screen: (u32, u32), scale: f32) -> Vec<Sprite> {
        let mut out = Vec::new();
        let (sw, sh) = (screen.0 as f32, screen.1 as f32);
        out.extend(self.hud.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]));
        let w = &self.hunt.weapons;
        let list = self.carried();
        const ROWS: usize = 12;
        let first = row
            .saturating_sub(ROWS - 1)
            .min(list.len().saturating_sub(ROWS));
        let px = 8.0 * scale;
        let lh = 14.0 * scale;
        let bw = 300.0 * scale;
        let shown = list.len().min(ROWS);
        let bh = (shown as f32 + 3.0) * lh + 8.0 * scale;
        let (x, y) = ((sw - bw) / 2.0, (sh - bh) / 2.0);
        out.extend(self.hud.rect(x, y, bw, bh, [0.05, 0.05, 0.05, 0.85]));
        let white = [0.9, 0.9, 0.9, 1.0];
        let grey = [0.5, 0.5, 0.5, 1.0];
        out.extend(self.hud.label(
            x + 8.0 * scale,
            y + 6.0 * scale,
            px,
            white,
            "Choose a weapon",
        ));
        for (k, &i) in list.iter().enumerate().skip(first).take(ROWS) {
            let ry = y + 6.0 * scale + (k - first + 1) as f32 * lh + 4.0 * scale;
            if k == row {
                out.extend(self.hud.rect(
                    x + 4.0 * scale,
                    ry - 3.0 * scale,
                    bw - 8.0 * scale,
                    lh,
                    [0.2, 0.45, 0.2, 0.9],
                ));
            }
            let shots = w.shots_left.get(i).copied().unwrap_or(0);
            let color = if shots > 0 { white } else { grey };
            let num = if k < 9 {
                format!("{}.", k + 1)
            } else {
                "  ".into()
            };
            let mark = if i == w.current && w.pose().is_some() {
                " *"
            } else {
                ""
            };
            let name = format!("{num} {}{mark}", w.info[i].name);
            out.extend(self.hud.label(x + 10.0 * scale, ry, px, color, &name));
            let ammo = if shots > 0 {
                format!("{shots}")
            } else {
                "empty".into()
            };
            let aw = self.hud.label_width(px, &ammo);
            out.extend(
                self.hud
                    .label(x + bw - aw - 12.0 * scale, ry, px, color, &ammo),
            );
        }
        let hint = "Up/Down or 1-9, Enter to take, Esc to close";
        let hy = y + bh - lh;
        out.extend(self.hud.label(x + 8.0 * scale, hy, px, grey, hint));
        out
    }

    /// Draws weapon `w` right away (screenshots).
    pub fn draw_weapon(&mut self, w: usize) {
        let input = HuntInput {
            select_weapon: Some(w),
            ..Default::default()
        };
        self.step(&input);
        for _ in 0..200 {
            self.step(&HuntInput::default());
        }
        self.prev_cam = self.hunt.camera().0;
    }

    fn step(&mut self, input: &HuntInput) {
        let cars: Vec<Option<&Car>> = self
            .dinos
            .cars
            .iter()
            .map(|c| c.as_ref().map(|(c, _)| c))
            .collect();
        self.prev_cam = self.hunt.camera().0;
        self.hunt.tick(
            self.view.area(),
            &cars,
            input,
            TICK_MS as i32,
            &mut self.events,
        );
    }

    /// A fresh hunt on the same area, reloading the saved trophy room.
    fn restart(&mut self) {
        self.audio.stop_all();
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        let species = std::mem::take(&mut self.hunt.herd.species);
        let ship_ms = self.hunt.ship_anim_ms;
        self.hunt = Self::new_hunt(
            &self.view,
            species,
            &self.weapon_infos,
            &self.weapon_anims,
            self.seed,
            &self.setup,
            self.mee.as_ref(),
        );
        self.hunt.ship_anim_ms = ship_ms;
        self.prev_cam = self.hunt.camera().0;
    }

    fn handle_events(&mut self) {
        self.play_sounds();
        for e in self.events.drain(..) {
            match e {
                GameEvent::Message(m) => self.message = Some((m, 2.0)),
                GameEvent::HunterDied { kind, .. } => {
                    let how = ["eaten", "drowned", "fell", "killed"];
                    let m = format!("You were {}.", how.get(kind as usize).unwrap_or(&"killed"));
                    self.message = Some((m, 6.0));
                }
                GameEvent::Scored {
                    ctype,
                    score,
                    range,
                    tranq,
                } => {
                    let name = self
                        .hunt
                        .herd
                        .species
                        .get(ctype)
                        .map_or_else(String::new, |s| s.info.name.clone());
                    self.kill = Some((name, score, range, tranq, 6.0));
                }
                GameEvent::Killed { trophy: true, .. } => {
                    self.message = Some(("Trophy!".into(), 2.0));
                }
                _ => {}
            }
        }
    }

    /// Plays the sounds for this frame's events (`AddVoice*` call sites).
    fn play_sounds(&mut self) {
        for c in c2_core::sound::commands(&self.events, &self.hunt) {
            match c {
                SoundCmd::Play { id, pos, volume } => {
                    self.audio.play(self.sounds.clip(id), pos, volume)
                }
                SoundCmd::Ambient { zone, volume } => {
                    let clip = match zone {
                        Some(z) => get(&self.sounds.ambients, z),
                        None => self.sounds.underwater.as_ref(),
                    };
                    self.audio.set_ambient(clip, volume);
                }
            }
        }
    }
}

impl HuntView {
    /// `DrawTrophyText`: the trophy's record on its plaque.
    fn plaque_text(&self, slot: usize, x0: f32, y0: f32, scale: f32) -> Vec<Sprite> {
        let Some(b) = self.hunt.room.bodies.get(slot) else {
            return Vec::new();
        };
        let info = self
            .hunt
            .herd
            .species
            .get(b.ctype.max(0) as usize)
            .map(|s| &s.info);
        let (name, mass, length) = info.map_or((String::new(), 0.0, 0.0), |i| {
            (i.name.clone(), i.mass, i.length)
        });
        let weapon = self
            .weapon_infos
            .get(b.weapon.max(0) as usize)
            .map_or("", |w| w.name.as_str());
        let us = self.hunt.room.options.sys != 0;
        let (sc, date, time) = (b.scale, b.date, b.time);
        let (weight, len, range) = if us {
            (
                format!("{:.2}t", mass * sc * sc / 0.907),
                format!("{:.2}ft", length * sc / 0.3),
                format!("{:.1}ft", b.range / 0.3),
            )
        } else {
            (
                format!("{:.2}T", mass * sc * sc),
                format!("{:.2}m", length * sc),
                format!("{:.1}m", b.range),
            )
        };
        let (d, m, y) = (date & 255, (date >> 10) & 255, date >> 20);
        let date = if us {
            format!("{m}.{d}.{y}")
        } else {
            format!("{d}.{m}.{y}")
        };
        let time = format!("{}:{:02}", (time >> 10) & 255, time & 255);
        let lines: [&[(&str, String)]; 5] = [
            &[("Name: ", name)],
            &[("Weight: ", weight + " "), ("Length: ", len)],
            &[
                ("Weapon: ", format!("{weapon}    ")),
                ("Score: ", b.score.to_string()),
            ],
            &[("Range of kill: ", range)],
            &[("Date: ", date + "   "), ("Time: ", time)],
        ];
        let (grey, yellow) = ([0.75, 0.75, 0.75, 1.0], [0.75, 0.75, 0.0, 1.0]);
        let px = 8.0 * scale;
        let mut out = Vec::new();
        for (i, parts) in lines.iter().enumerate() {
            let y = y0 + (18.0 + 16.0 * i as f32) * scale;
            let mut x = x0 + 16.0 * scale;
            for (label, value) in parts.iter() {
                for (t, c) in [(*label, grey), (value.as_str(), yellow)] {
                    out.extend(
                        self.hud
                            .label(x + scale, y + scale, px, [0.0, 0.0, 0.0, 1.0], t),
                    );
                    out.extend(self.hud.label(x, y, px, c, t));
                    x += self.hud.label_width(px, t);
                }
            }
        }
        out
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
                w.info[w.current].name, w.shots_left[w.current]
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
        if let Some(row) = self.picker {
            self.update_picker(input, row);
            self.view.update(gpu, 0.0, &Input::default());
            return;
        }
        let k = self.hunt.room.options.keys;
        let pressed = |vk| keys::pressed(input, vk);
        if input.pressed(self.pick_key())
            && !self.hunt.paused
            && self.hunt.death.is_none()
            && !self.hunt.opts.trophy
            && !self.hunt.opts.observer
        {
            let cur = self.hunt.weapons.current;
            self.picker = Some(self.carried().iter().position(|&i| i == cur).unwrap_or(0));
            self.acc_ms = 0.0;
            return;
        }
        if pressed(k.run) {
            self.run = !self.run;
        }
        if input.pressed(KeyCode::Escape) {
            self.hunt.press_escape();
        }
        if input.pressed(KeyCode::KeyY) || input.pressed(KeyCode::Enter) {
            self.hunt.confirm_exit();
        }
        // Pause; P too (laptops lack the key) unless P is bound to something.
        let p_bound = {
            let k = &self.hunt.room.options.keys;
            [
                k.forward,
                k.backward,
                k.up,
                k.down,
                k.left,
                k.right,
                k.fire,
                k.show,
                k.strafe_left,
                k.strafe_right,
                k.strafe,
                k.jump,
                k.run,
                k.crouch,
                k.call,
                k.change_call,
                k.binoculars,
            ]
            .contains(&80)
        };
        if input.pressed(KeyCode::Pause) || (!p_bound && input.pressed(KeyCode::KeyP)) {
            self.hunt.toggle_pause();
        }
        if input.pressed(KeyCode::KeyN) {
            self.hunt.exit_prompt = false;
        }
        if input.pressed(KeyCode::KeyR) {
            if self.hunt.opts.trophy {
                self.hunt.remove_current_trophy();
            } else if self.hunt.show_exit() {
                self.restart();
            }
        }
        if pressed(k.show) {
            self.pending.toggle_weapon = true;
        }
        const DIGITS: [KeyCode; 6] = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
        ];
        if let Some(w) = DIGITS.iter().position(|&k| input.pressed(k)) {
            self.pending.select_weapon = Some(w);
        }
        if pressed(k.binoculars) {
            self.pending.toggle_binoculars = true;
        }
        if input.pressed(KeyCode::Tab) {
            self.pending.toggle_map = true;
        }
        if pressed(k.call) {
            self.pending.call = true;
        }
        if pressed(k.change_call) {
            self.pending.change_call = true;
        }
        let mut hin = HuntInput {
            controls: self.controls(input),
            fire: keys::held(input, k.fire),
            zoom_in: input.held(KeyCode::Equal) || input.held(KeyCode::NumpadAdd),
            zoom_out: input.held(KeyCode::Minus) || input.held(KeyCode::NumpadSubtract),
            ..self.pending
        };

        self.acc_ms = (self.acc_ms + dt * 1000.0).min(250.0);
        while self.acc_ms >= TICK_MS as f32 {
            self.step(&hin);
            // One-shot inputs apply to the first tick only.
            hin.controls.mouse = (0.0, 0.0);
            hin.toggle_weapon = false;
            hin.select_weapon = None;
            hin.toggle_binoculars = false;
            hin.toggle_map = false;
            hin.call = false;
            hin.change_call = false;
            self.pending = HuntInput::default();
            self.acc_ms -= TICK_MS as f32;
        }
        self.handle_events();
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        if let Some(k) = &mut self.kill {
            k.4 -= dt;
            if k.4 <= 0.0 {
                self.kill = None;
            }
        }

        match self.hunt.outcome() {
            Some(Outcome::Exited) if !self.done => {
                if let Err(e) = crate::save::store(&self.hunt.room) {
                    tracing::error!("{e:#}");
                }
                self.audio.stop_all();
                self.done = true;
            }
            // Dying discards this hunt's trophies (the save isn't written).
            // From the menu, go back to it; otherwise start over.
            Some(Outcome::Died) if !self.done => {
                if self.setup.from_menu {
                    self.audio.stop_all();
                    self.done = true;
                } else {
                    self.restart();
                }
            }
            _ => {}
        }

        let (pos, alpha, beta) = self.hunt.camera();
        let t = self.acc_ms / TICK_MS as f32;
        self.view.cam.pos = Vec3::from(self.prev_cam).lerp(Vec3::from(pos), t);
        // Original yaw/pitch conventions are mirrored relative to the fly cam.
        self.view.cam.yaw = -alpha;
        self.view.cam.pitch = -beta;
        self.view.zoom = self.hunt.zoom();
        self.audio.set_camera(pos, alpha);
        let ship = &self.hunt.ship;
        let engine = (ship.state != -1).then_some(ship.pos);
        self.audio
            .set_engine(get(&self.sounds.ship, ship_sound::ENGINE), engine);
        self.view.update(gpu, dt, input);
    }

    fn clear_color(&self) -> [f64; 3] {
        if self.blacked_out() {
            return [0.0; 3];
        }
        self.view.clear_color()
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        let aspect = screen.0.max(1) as f32 / screen.1.max(1) as f32;
        // Weapon light: 96 + ground light / 4 (`wpnlight`).
        let p = &self.hunt.player;
        let cell = c2_formats::map::MAP_SIZE * (p.z / 256.0) as usize + (p.x / 256.0) as usize;
        let ground = self
            .view
            .area()
            .light_map()
            .get(cell)
            .copied()
            .unwrap_or(128);
        let light = (96.0 + f32::from(ground) / 4.0) / 255.0;
        let (dinos, weapons, hunt, effects) = (
            &mut self.dinos,
            &mut self.weapons,
            &self.hunt,
            &mut self.effects,
        );
        let eye = self.view.cam.pos.to_array();
        let view = self.view.cam.view();
        let max = self.view.radius as f32 * c2_core::CELL;
        let black = hunt.death.is_some() && !hunt.opts.death_anim;
        if !black {
            self.view
                .draw_with(gpu, pass, aspect, &mut |gpu, pass, globals| {
                    dinos.draw(gpu, pass, globals, &hunt.herd, &hunt.ship, eye, max);
                    effects.draw(gpu, pass, globals, &hunt.effects);
                    weapons.draw(gpu, pass, globals, hunt, view, light, aspect);
                });
        }

        // HUD at an integer scale of the original's 640x480 layout.
        let scale = (screen.1 as f32 / 480.0).floor().max(1.0);
        let mut sprites = self.weapons.hud_sprites(&self.hud, &self.hunt, scale);
        // The last kill: a box in the bottom right.
        if let Some((name, score, range, tranq, _)) = &self.kill {
            let us = self.hunt.room.options.sys != 0;
            let dist = if us {
                format!("Range: {:.0} ft", range / 0.3)
            } else {
                format!("Range: {range:.0} m")
            };
            let lines = [
                name.clone(),
                if self.setup.time == TimeOfDay::Night {
                    format!("+{score} points (night +10%)")
                } else {
                    format!("+{score} points")
                },
                dist,
                if *tranq {
                    "Tranquilized (+25%, no trophy)".to_string()
                } else {
                    "Killed".to_string()
                },
            ];
            let px = 8.0 * scale;
            let w = lines
                .iter()
                .map(|l| self.hud.label_width(px, l))
                .fold(0.0, f32::max)
                + 16.0 * scale;
            let h = (lines.len() as f32 * 12.0 + 12.0) * scale;
            let (x, y) = (
                screen.0 as f32 - w - 12.0 * scale,
                screen.1 as f32 - h - 12.0 * scale,
            );
            sprites.extend(self.hud.rect(x, y, w, h, [0.0, 0.0, 0.0, 0.7]));
            for (i, l) in lines.iter().enumerate() {
                let c = if i == 1 {
                    [0.95, 0.85, 0.2, 1.0]
                } else {
                    [0.9, 0.9, 0.9, 1.0]
                };
                let ly = y + (6.0 + 12.0 * i as f32) * scale;
                sprites.extend(self.hud.label(x + 8.0 * scale, ly, px, c, l));
            }
        }
        // Binoculars: what's in the cross-hairs (`Render_LifeInfo`).
        if let Some(li) = self.hunt.life_info(self.view.area()) {
            let green = [0.0, 0.69, 0.0, 1.0];
            let x = screen.0 as f32 / 2.0 + screen.0 as f32 / 64.0;
            let y = screen.1 as f32 / 2.0 + screen.1 as f32 / 6.8;
            let imperial = self.hunt.room.options.sys != 0;
            let lines = if imperial {
                [
                    li.name.clone(),
                    format!("Weight: {:.2}t", li.weight / 0.907),
                    format!("Distance: {}ft", (li.distance * 3.0 / 64.0) as i32),
                ]
            } else {
                [
                    li.name.clone(),
                    format!("Weight: {:.2}T", li.weight),
                    format!("Distance: {}m", (li.distance * 3.0 / 64.0) as i32 / 3),
                ]
            };
            for (i, l) in lines.iter().enumerate() {
                sprites.extend(
                    self.hud
                        .text(x, y + 16.0 * scale * i as f32, scale, green, l),
                );
            }
        }
        // Scope cross-hairs at the view model's aim point (`DrawOpticCross`).
        if self.hunt.zoom() > 1.0 && !self.hunt.bin_mode {
            if let Some(p) = self.weapons.aim_point {
                let far = self.view.radius as f32 * c2_core::CELL;
                let vp = crate::camera::projection(aspect, far * 1.5, self.hunt.zoom()) * view;
                let c = vp * glam::Vec4::new(p[0], p[1], p[2], 1.0);
                if c.w > 0.0 {
                    let sx = (c.x / c.w + 1.0) * 0.5 * screen.0 as f32;
                    let sy = (1.0 - c.y / c.w) * 0.5 * screen.1 as f32;
                    let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
                    if (cx - sx).abs() <= cx && (cy - sy).abs() <= cy / 2.0 {
                        let w = screen.0 as f32 / 12.0;
                        let col = [0.0, 0.0, 0.0625, 0.5];
                        sprites.extend(self.hud.rect(sx - w, sy, 2.0 * w, scale, col));
                        sprites.extend(self.hud.rect(sx, sy - w, scale, 2.0 * w, col));
                    }
                }
            }
        }
        // Map (`DrawHMap`): view range circle, the hunter, radar blips.
        if self.hunt.map_mode {
            if let Some((id, mw, mh)) = self
                .map_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
            {
                let (cx, cy) = (screen.0 as f32 / 2.0, screen.1 as f32 / 2.0);
                let (fx, fy) = (cx - mw * scale / 2.0, cy - (mh / 2.0 + 6.0) * scale);
                sprites.push(Sprite::new(id, fx, fy, scale));
                let (ox, oy) = (fx + 11.0 * scale, fy + 23.0 * scale);
                let p = &self.hunt.player;
                // CCX = (x / 512) * 2; one map pixel per 4 cells.
                let ccx = ((p.x as i32 / 512) * 2) as f32 / 4.0;
                let ccy = ((p.z as i32 / 512) * 2) as f32 / 4.0;
                let dot = |x: f32, y: f32, c: [f32; 4]| {
                    self.hud
                        .rect(ox + x * scale, oy + y * scale, 2.0 * scale, 2.0 * scale, c)
                };
                let r = self.hunt.opts.view_r as f32 / 4.0;
                for i in 0..48 {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    let (px, py) = (ccx + a.cos() * r, ccy + a.sin() * r);
                    sprites.extend(self.hud.rect(
                        ox + (px + 1.0) * scale,
                        oy + (py + 1.0) * scale,
                        scale,
                        scale,
                        [0.0, 0.12, 0.0, 1.0],
                    ));
                    sprites.extend(self.hud.rect(
                        ox + px * scale,
                        oy + py * scale,
                        scale,
                        scale,
                        [0.0, 0.56, 0.0, 1.0],
                    ));
                }
                sprites.extend(dot(ccx + 1.0, ccy + 1.0, [0.25, 0.0, 0.0, 1.0]));
                sprites.extend(dot(ccx, ccy, [0.97, 0.0, 0.0, 1.0]));
                // Radar: the picked animals green, other huntables that
                // spawned anyway blue.
                if self.hunt.opts.radar {
                    for c in &self.hunt.herd.chars {
                        if c.health <= 0 {
                            continue;
                        }
                        let color = if self.hunt.is_target(c) {
                            [0.0, 0.97, 0.0, 1.0]
                        } else if self.hunt.is_huntable(c) {
                            [0.2, 0.55, 1.0, 1.0]
                        } else {
                            continue;
                        };
                        sprites.extend(dot(c.pos[0] / 1024.0, c.pos[2] / 1024.0, color));
                    }
                }
            }
        }
        // The selected call's icon (top right) after pressing change-call.
        if self.hunt.call_icon_time > 0 {
            let i = (self.hunt.target_call - 10).max(0) as usize;
            if let Some((id, w)) = self
                .call_imgs
                .get(i)
                .copied()
                .flatten()
                .and_then(|id| self.hud.image(id).map(|im| (id, im.w as f32)))
            {
                sprites.push(Sprite::new(
                    id,
                    screen.0 as f32 - (10.0 + w) * scale,
                    7.0 * scale,
                    scale,
                ));
            }
        }
        if self.hunt.opts.trophy {
            if let Some((id, w)) = self
                .trophy_exit_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32)))
            {
                let x = (screen.0 as f32 - w * scale) / 2.0;
                sprites.push(Sprite::new(id, x, 2.0 * scale, scale));
            }
        }
        if let Some(slot) = self.hunt.trophy_body {
            if let Some((id, w, h)) = self
                .plaque_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w as f32, im.h as f32)))
            {
                let y = screen.1 as f32 - (h + 12.0) * scale;
                let x = if self.hunt.opts.trophy {
                    screen.0 as f32 - (w + 16.0) * scale
                } else {
                    (screen.0 as f32 - w * scale) / 2.0
                };
                sprites.push(Sprite::new(id, x, y, scale));
                sprites.extend(self.plaque_text(slot, x, y, scale));
            }
        }
        if let Some(row) = self.picker {
            sprites.extend(self.picker_sprites(row, screen, scale));
        }
        if self.hunt.paused {
            if let Some((id, w, h)) = self
                .pause_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
            {
                let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
                let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
                sprites.push(Sprite::new(id, x, y, scale));
            }
        }
        if self.hunt.show_exit() {
            if let Some((id, w, h)) = self
                .exit_img
                .and_then(|i| self.hud.image(i).map(|im| (i, im.w, im.h)))
            {
                let x = (screen.0 as f32 - w as f32 * scale) / 2.0;
                let y = (screen.1 as f32 - h as f32 * scale) / 2.0;
                sprites.push(Sprite::new(id, x, y, scale));
            }
        }
        self.hud.draw(gpu, pass, screen, &sprites);
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

/// Local-ish date and time for trophy records (UTC; no timezone database).
fn local_clock() -> (i32, i32, i32, i32, i32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y as i32,
        m as i32,
        d as i32,
        (rem / 3600) as i32,
        (rem % 3600 / 60) as i32,
    )
}
