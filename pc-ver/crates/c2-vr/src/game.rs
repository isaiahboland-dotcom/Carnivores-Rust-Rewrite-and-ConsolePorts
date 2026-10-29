//! The game in the headset: the main menu, then where it leads.
//!
//! The menu is the PC's main screen (`MENUM.TGA`, `panel`) standing in the
//! first area's abandoned military camp (the satellite dish's base), in
//! front of wherever the player faces when it shows. A controller's ray or
//! the D-pad (and the left stick) picks an item, shown lit in a gold box;
//! the trigger or A (+) takes it. In VR so far:
//!
//! - HUNT: the menu goes and the sticks walk the area from the camp (the
//!   hunt itself isn't in VR yet).
//! - TROPHY: the trophy room (empty: trophies aren't drawn yet).
//! - QUIT.
//!
//! Options, credits and statistics are dimmed. Away from the menu, + or -
//! comes back to it; - in the menu quits. The right stick click recentres
//! (`xr`: standing height from the head; here the menu in front again).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ash::vk;
use c2_core::CELL;
use glam::Vec3;

use crate::gpu::Gpu;
use crate::panel::{self, Art, Look, Panel, Placement};
use crate::scene::{Frame, MakeScene, Next, Scene};
use crate::terrain::{self, TerrainScene};

/// Where the menu stands: AREA1's camp, south of the bunker, looking north
/// to it and the dish (cells; heading 0 looks to -z).
const CAMP: (f32, f32, f32) = (645.5, 302.7, 0.0);
/// The trophy room's start (`c2-pc hunt_view.rs`: `Player::new(area, (76,
/// 70))`).
const TROPHY_START: (f32, f32, f32) = (76.5, 70.5, 0.0);

/// The main screen's regions (`MAIN_MAP.RAW`, `c2-pc menu.rs:
/// update_main`).
const HUNT: u8 = 1;
const TROPHY: u8 = 3;
const QUIT: u8 = 5;
/// Top to bottom; the D-pad steps through these.
const OFFERED: [u8; 3] = [HUNT, TROPHY, QUIT];

/// The game's folders.
#[derive(Clone, Debug)]
pub struct Game {
    /// `HUNTDAT` folders, the base's last (overlays, latest first, before
    /// it): a file is taken from the first that has it.
    pub huntdat: Vec<PathBuf>,
}

impl Game {
    /// The game at `base` with `overlays` laid over it (latest last).
    pub fn new(base: &Path, overlays: &[PathBuf]) -> Result<Self> {
        let huntdat = overlays
            .iter()
            .rev()
            .chain(std::iter::once(&base.to_path_buf()))
            .filter_map(|d| panel::find_ci(d, "HUNTDAT"))
            .collect::<Vec<_>>();
        if huntdat.is_empty() {
            return Err(anyhow!("{}: no HUNTDAT folder", base.display()));
        }
        Ok(Self { huntdat })
    }

    /// `HUNTDAT/<parts..>`, from the first folder that has it.
    fn find(&self, parts: &[&str]) -> Option<PathBuf> {
        self.huntdat.iter().find_map(|h| {
            parts
                .iter()
                .try_fold(h.clone(), |d, p| panel::find_ci(&d, p))
        })
    }

    fn area(&self, name: &str) -> Result<PathBuf> {
        self.find(&["AREAS", name])
            .ok_or_else(|| anyhow!("no HUNTDAT/AREAS/{name}"))
    }

    fn menus(&self) -> Vec<PathBuf> {
        self.huntdat
            .iter()
            .filter_map(|h| panel::find_ci(h, "MENU"))
            .collect()
    }
}

/// The main menu at the camp.
pub fn menu(game: Game) -> Result<MakeScene> {
    let map = game.area("AREA1.MAP")?;
    let area = terrain::load(&map)?;
    let sun = crate::sky::load_sun(&map, area.time);
    let art = Art::load(&game.menus(), "MENUM.TGA", "MENUM_ON.TGA", "MAIN_MAP.RAW")?;
    Ok(Box::new(move |gpu, format| {
        let terrain = TerrainScene::new(gpu, format, area, sun)?;
        let panel = Panel::new(gpu, art, terrain.render_pass())?;
        Ok(Box::new(GameScene::new(terrain, Some(panel), game, CAMP)) as _)
    }))
}

/// The trophy room; leaving it is the menu again.
fn trophy_room(game: Game) -> Result<MakeScene> {
    let map = game.area("TROPHY.MAP")?;
    let area = terrain::load(&map)?;
    let sun = crate::sky::load_sun(&map, area.time);
    Ok(Box::new(move |gpu, format| {
        let terrain = TerrainScene::new(gpu, format, area, sun)?;
        Ok(Box::new(GameScene::new(terrain, None, game, TROPHY_START)) as _)
    }))
}

pub struct GameScene {
    terrain: TerrainScene,
    /// The menu (the camp's scene only).
    panel: Option<Panel>,
    game: Game,
    /// Cells and heading the head is put at.
    start: (f32, f32, f32),
    /// The menu shows (else the sticks walk).
    in_menu: bool,
    /// Where the panel stands; `None`: put it in front on the next frame
    /// (and the head at `start`).
    placed: Option<Placement>,
    selected: u8,
    pointer: Option<(f32, f32)>,
    /// The left stick is pushed (it steps once per push).
    stick_held: bool,
    next: Option<Next>,
}

impl GameScene {
    fn new(
        mut terrain: TerrainScene,
        panel: Option<Panel>,
        game: Game,
        start: (f32, f32, f32),
    ) -> Self {
        let in_menu = panel.is_some();
        terrain.walking = !in_menu;
        Self {
            terrain,
            panel,
            game,
            start,
            in_menu,
            placed: None,
            selected: HUNT,
            pointer: None,
            stick_held: false,
            next: None,
        }
    }

    /// The menu again: here at the camp, or by loading it.
    fn back_to_menu(&mut self) {
        if self.panel.is_some() {
            self.in_menu = true;
            self.terrain.walking = false;
            self.placed = None;
        } else {
            self.next = Some(match menu(self.game.clone()) {
                Ok(m) => Next::Load(m),
                Err(e) => {
                    println!("The menu didn't load: {e:#}");
                    Next::Quit
                }
            });
        }
    }

    fn take(&mut self, item: u8) {
        match item {
            HUNT => {
                self.in_menu = false;
                self.terrain.walking = true;
                self.pointer = None;
            }
            TROPHY => {
                self.next = Some(match trophy_room(self.game.clone()) {
                    Ok(m) => Next::Load(m),
                    Err(e) => {
                        println!("The trophy room didn't load: {e:#}");
                        Next::Stay
                    }
                });
            }
            QUIT => self.next = Some(Next::Quit),
            _ => {}
        }
    }

    /// The menu's input: rays, triggers, the D-pad and stick, A / +.
    fn menu_input(&mut self, f: &Frame, place: &Placement) {
        use c2_pad::Action as Pad;
        let Some(panel) = &self.panel else { return };
        let input = &f.input;
        self.pointer = None;
        // The right hand first: it's the one pointing, usually.
        for hand in [1, 0] {
            let Some(c) = input.aim[hand].and_then(|r| place.pick(&r)) else {
                continue;
            };
            self.pointer = Some(c);
            let r = panel.art.region(c);
            if OFFERED.contains(&r) {
                self.selected = r;
                if input.select[hand] {
                    self.take(r);
                    return;
                }
            }
            break;
        }
        let stick = input.pad.walk[1];
        let mut step = 0i32;
        if stick.abs() > 0.6 && !self.stick_held {
            self.stick_held = true;
            step = if stick > 0.0 { -1 } else { 1 };
        } else if stick.abs() < 0.3 {
            self.stick_held = false;
        }
        // D-pad up / down are crouch / next call (`c2_pad`).
        if input.pad.pressed(Pad::Crouch) {
            step = -1;
        }
        if input.pad.pressed(Pad::NextCall) {
            step = 1;
        }
        if step != 0 {
            let i = OFFERED
                .iter()
                .position(|&r| r == self.selected)
                .unwrap_or(0) as i32;
            let n = OFFERED.len() as i32;
            self.selected = OFFERED[(i + step).rem_euclid(n) as usize];
        }
        if input.pad.pressed(Pad::Jump) || input.pad.pressed(Pad::Pause) {
            self.take(self.selected);
        }
    }
}

impl Scene for GameScene {
    fn render_pass(&self) -> vk::RenderPass {
        self.terrain.render_pass()
    }

    fn eye_targets(&mut self, gpu: &Gpu, extent: vk::Extent2D) -> Result<[Vec<vk::ImageView>; 2]> {
        self.terrain.eye_targets(gpu, extent)
    }

    fn record(&mut self, gpu: &Gpu, f: &Frame) -> Result<()> {
        if self.in_menu && f.input.recenter {
            self.placed = None;
        }
        if self.placed.is_none() {
            let (x, z, heading) = self.start;
            self.terrain.place_head(f, x * CELL, z * CELL, heading);
            self.placed = Some(Placement::facing(f.head(), f.eyes[0].rot * Vec3::NEG_Z));
        }
        let place = self
            .placed
            .unwrap_or_else(|| Placement::facing(f.head(), Vec3::NEG_Z));
        if self.in_menu {
            self.menu_input(f, &place);
        } else if f.input.back || f.input.pad.pressed(c2_pad::Action::Pause) {
            self.back_to_menu();
        }
        self.terrain.move_player(f);
        let menu = self.panel.as_ref().filter(|_| self.in_menu);
        match menu {
            Some(panel) => {
                let model = {
                    let t = &self.terrain;
                    place.model(|p| t.world(p, f.floor))
                };
                let look = Look {
                    selected: self.selected,
                    offered: OFFERED.iter().fold(0, |m, &r| m | 1 << r),
                    pointer: self.pointer,
                };
                let mut overlay = |gpu: &Gpu, cmd, _eye, view_proj: glam::Mat4| {
                    panel.draw(gpu, cmd, view_proj * model, &look);
                };
                self.terrain.draw(gpu, f, Some(&mut overlay));
            }
            None => self.terrain.draw(gpu, f, None),
        }
        Ok(())
    }

    fn destroy(&mut self, gpu: &Gpu) {
        if let Some(p) = self.panel.as_mut() {
            p.destroy(gpu);
        }
        self.terrain.destroy(gpu);
    }

    fn handles_back(&self) -> bool {
        !self.in_menu
    }

    fn next(&mut self) -> Next {
        self.next.take().unwrap_or(Next::Stay)
    }
}
