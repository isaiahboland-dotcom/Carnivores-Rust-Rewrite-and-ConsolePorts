//! The game in the headset: the menu, then where it leads.
//!
//! The menu (`menus`, on the `panel`) stands in the first area's abandoned
//! military camp (the satellite dish's base), in front of wherever the
//! player faces when it shows; walking is off there. From it:
//!
//! - HUNT, the setup, then the hunt: the chosen area at the chosen time,
//!   from one of its landing spots, among the chosen animals (`hunt`; an
//!   observer's ignore you). No weapons yet.
//! - TROPHY: the trophy room (empty: trophies aren't drawn yet).
//! - QUIT.
//!
//! Away from the menu, + or - returns to it, as does dying (after the
//! original's 12 s). - in the menu quits. The right stick click recentres
//! (`xr`: standing height from the head; here the menu in front again).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use ash::vk;
use c2_core::CELL;
use glam::Vec3;

use crate::canvas::Canvas;
use crate::gpu::Gpu;
use crate::hunt::{HuntWorld, Roster};
use crate::menus::{Action, Launch, Menus};
use crate::panel::{self, Panel, Placement};
use crate::scene::{Frame, MakeScene, Next, Scene};
use crate::terrain::{self, TerrainScene};

/// Where the menu stands: AREA1's camp, south of the bunker, looking north
/// to it and the dish (cells; heading 0 looks to -z).
const CAMP: (f32, f32, f32) = (645.5, 302.7, 0.0);
/// The trophy room's start (`c2-pc hunt_view.rs`: `Player::new(area, (76,
/// 70))`).
const TROPHY_START: (f32, f32, f32) = (76.5, 70.5, 0.0);

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

    fn area(&self, name: &str) -> Result<PathBuf> {
        self.huntdat
            .iter()
            .find_map(|h| panel::find_ci(&panel::find_ci(h, "AREAS")?, name))
            .ok_or_else(|| anyhow!("no HUNTDAT/AREAS/{name}"))
    }
}

fn clock_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.subsec_nanos() | 1)
}

/// The menu at the camp.
pub fn menu(game: Game) -> Result<MakeScene> {
    let map = game.area("AREA1.MAP")?;
    let area = terrain::load(&map)?;
    let sun = crate::sky::load_sun(&map, area.time);
    let menus = Menus::load(&game.huntdat)?;
    Ok(Box::new(move |gpu, format| {
        let mut terrain = TerrainScene::new(gpu, format, area, sun)?;
        terrain.walking = false;
        let panel = Panel::new(gpu, terrain.render_pass())?;
        let mut scene = GameScene::new(terrain, game, CAMP);
        scene.menu = Some(Menu {
            panel,
            menus,
            canvas: Canvas::default(),
        });
        Ok(Box::new(scene) as _)
    }))
}

/// The hunt `launch` asks for, from one of the area's landing spots.
fn hunt(game: Game, launch: Launch) -> Result<MakeScene> {
    let area = terrain::load_at(&launch.map, launch.time)?;
    let sun = crate::sky::load_sun(&launch.map, area.time);
    let roster = Roster::load(&game.huntdat)?;
    let seed = clock_seed();
    let (lx, ly) = area
        .landings
        .get(seed as usize % area.landings.len().max(1))
        .copied()
        .unwrap_or((512, 512));
    println!(
        "Hunt: {} at {:?}{}",
        launch.map.display(),
        launch.time,
        if launch.observer { ", observer" } else { "" }
    );
    Ok(Box::new(move |gpu, format| {
        let mut terrain = TerrainScene::new(gpu, format, area, sun)?;
        terrain.add_animals(gpu, &roster.models())?;
        let start = (f32::from(lx) + 0.5, f32::from(ly) + 0.5, 0.0);
        let mut scene = GameScene::new(terrain, game, start);
        scene.roster = Some(roster);
        scene.launch = Some((launch, seed));
        Ok(Box::new(scene) as _)
    }))
}

/// The trophy room; leaving it is the menu again.
fn trophy_room(game: Game) -> Result<MakeScene> {
    let map = game.area("TROPHY.MAP")?;
    let area = terrain::load(&map)?;
    let sun = crate::sky::load_sun(&map, area.time);
    Ok(Box::new(move |gpu, format| {
        let terrain = TerrainScene::new(gpu, format, area, sun)?;
        Ok(Box::new(GameScene::new(terrain, game, TROPHY_START)) as _)
    }))
}

/// The menu's pieces (the camp's scene only).
struct Menu {
    panel: Panel,
    menus: Menus,
    canvas: Canvas,
}

pub struct GameScene {
    terrain: TerrainScene,
    menu: Option<Menu>,
    game: Game,
    /// Cells and heading the head is put at.
    start: (f32, f32, f32),
    /// Where the panel stands; `None`: put it in front on the next frame
    /// (and the head at `start`).
    placed: Option<Placement>,
    /// The game's characters, and the hunt among them.
    roster: Option<Roster>,
    world: Option<HuntWorld>,
    /// The hunt to start once the head is placed (and its seed).
    launch: Option<(Launch, u32)>,
    next: Option<Next>,
}

impl GameScene {
    fn new(terrain: TerrainScene, game: Game, start: (f32, f32, f32)) -> Self {
        Self {
            terrain,
            menu: None,
            game,
            start,
            placed: None,
            roster: None,
            world: None,
            launch: None,
            next: None,
        }
    }

    /// Loads `make` (or says why not and stays).
    fn load(&mut self, what: &str, make: Result<MakeScene>) {
        self.next = Some(match make {
            Ok(m) => Next::Load(m),
            Err(e) => {
                println!("The {what} didn't load: {e:#}");
                Next::Stay
            }
        });
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
        if self.menu.is_some() && f.input.recenter {
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
        match self.menu.as_mut() {
            Some(m) => match m.menus.update(&f.input, |r| place.pick(r)) {
                Action::None => {}
                Action::Hunt(l) => {
                    let make = hunt(self.game.clone(), l);
                    self.load("hunt", make);
                }
                Action::Trophy => {
                    let make = trophy_room(self.game.clone());
                    self.load("trophy room", make);
                }
                Action::Quit => self.next = Some(Next::Quit),
            },
            None => {
                let died = self.world.as_ref().and_then(|w| w.hunt.outcome()).is_some();
                if died {
                    println!("You died: back to the menu.");
                }
                if died || f.input.back || f.input.pad.pressed(c2_pad::Action::Pause) {
                    let make = menu(self.game.clone());
                    self.load("menu", make);
                }
            }
        }
        self.terrain.move_player(f);
        let (head, heading) = self.terrain.head(f);
        if let (Some((l, seed)), Some(r)) = (self.launch.take(), &self.roster) {
            self.world = Some(HuntWorld::new(
                self.terrain.area(),
                r,
                head.x,
                head.z,
                seed,
                l.targets,
                l.observer,
            ));
        }
        let posed = match (&mut self.world, &self.roster) {
            (Some(w), Some(r)) => {
                w.step(self.terrain.area(), r, head, heading, f.dt);
                w.posed(r, head, self.terrain.view_range())
            }
            _ => Vec::new(),
        };
        match &mut self.menu {
            Some(m) => {
                if m.menus.take_dirty() {
                    m.menus.compose(&mut m.canvas);
                    m.panel.upload(gpu, f.cmd, f.slot, &m.canvas);
                }
                let model = {
                    let t = &self.terrain;
                    place.model(|p| t.world(p, f.floor))
                };
                let pointer = m.menus.pointer();
                let panel = &m.panel;
                let mut overlay = |gpu: &Gpu, cmd, _eye, view_proj: glam::Mat4| {
                    panel.draw(gpu, cmd, view_proj * model, pointer);
                };
                self.terrain.draw(gpu, f, &posed, Some(&mut overlay))
            }
            None => self.terrain.draw(gpu, f, &posed, None),
        }
    }

    fn destroy(&mut self, gpu: &Gpu) {
        if let Some(m) = self.menu.as_mut() {
            m.panel.destroy(gpu);
        }
        self.terrain.destroy(gpu);
    }

    fn handles_back(&self) -> bool {
        self.menu.is_none()
    }

    fn next(&mut self) -> Next {
        self.next.take().unwrap_or(Next::Stay)
    }
}
