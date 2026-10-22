//! The 2D parts: the bottom screen during a hunt (weapon picker | map |
//! call picker, all touch, with status lines and buttons under the map) and
//! the top-screen overlays (pause, the leave prompt, binocular read-out).
//!
//! ```text
//! 0      72                      248     320
//! +------+------------------------+-------+ 0
//! | WPN  |                        | CALL  |
//! | WPN  |          map           | CALL  |
//! | ...  |   (tap: zoom in/out)   | ...   |
//! |      +------------------------+       | 176
//! |      | status, message        |       |
//! |      | [Binoculars] [ Pause ] |       | 240
//! +------+------------------------+-------+
//! ```

use c2_core::game::Hunt;
use c2_core::CELL;
use c2_vfs::Vfs;

use crate::draw2d::{inside, Canvas, Rgba, WHITE};
use crate::gpu::{Gpu, Texture, BOTTOM_H, TOP_W};
use crate::hunt::Game;

const LEFT_W: f32 = 72.0;
const RIGHT_X: f32 = 248.0;
const MAP: [f32; 4] = [LEFT_W, 0.0, RIGHT_X - LEFT_W, 176.0];
const WPN_SLOT: f32 = 58.0;
const CALL_SLOT: f32 = 60.0;
/// Buttons under the map: (x, y, w, h).
const AMMO: [f32; 4] = [74.0, 212.0, 52.0, 24.0];
const BINOC: [f32; 4] = [130.0, 212.0, 60.0, 24.0];
const PAUSE: [f32; 4] = [194.0, 212.0, 52.0, 24.0];
const RED: Rgba = [230, 40, 30, 255];
/// Map pixels shown across the map panel when zoomed in (1 pixel = 4 cells).
const LOCAL_SPAN: f32 = 64.0;

const GREEN: Rgba = [0, 176, 0, 255];
const GREY: Rgba = [150, 150, 150, 255];
const PANEL: Rgba = [24, 32, 24, 255];
const HILITE: Rgba = [60, 110, 60, 255];

/// What a tap on the bottom screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    Weapon(usize),
    /// Select this call (AI); calls if it was already selected.
    Call(i32),
    MapZoom,
    Binoculars,
    Pause,
    /// Use an extra ammo refill.
    Resupply,
}

pub struct Ui {
    c: Canvas,
    map: Option<Texture>,
    /// Carried weapons: (index, picture).
    weapons: Vec<(usize, Option<Texture>)>,
    /// Calls of the hunted animals: (call id = 10 + menu slot, icon).
    calls: Vec<(i32, Option<Texture>)>,
    pub map_local: bool,
}

pub fn load(vfs: &Vfs, path: &str) -> Option<Texture> {
    Texture::new(&c2_formats::cooked::parse_texture(&vfs.read(path).ok()?).ok()?)
}

impl Ui {
    pub fn new(vfs: &Vfs, game: &Game) -> Self {
        let weapons = if game.hunt.opts.observer || game.setup.trophy() {
            Vec::new()
        } else {
            game.setup
                .carried()
                .into_iter()
                .map(|i| (i, load(vfs, &format!("C2RS/WPNPIC{i}.TX"))))
                .collect()
        };
        let calls = (10..128)
            .filter(|ai| game.hunt.opts.targets & (1 << ai) != 0)
            .filter(|&ai| game.setup.callable(ai))
            .map(|ai| (ai, load(vfs, &format!("C2RS/CALL{ai}.TX"))))
            .collect();
        Self {
            c: Canvas::new(load(vfs, "C2RS/FONT.TX")),
            map: load(vfs, &format!("C2RS/{}/MAP.TX", game.setup.area)),
            weapons,
            calls,
            map_local: true,
        }
    }

    /// What a tap at bottom-screen pixel (x, y) means.
    pub fn tap(&self, x: f32, y: f32) -> Option<Tap> {
        if x < LEFT_W {
            let i = (y / self.wpn_slot()) as usize;
            return self.weapons.get(i).map(|w| Tap::Weapon(w.0));
        }
        if x >= RIGHT_X {
            let i = (y / CALL_SLOT) as usize;
            return self.calls.get(i).map(|c| Tap::Call(c.0));
        }
        if inside(MAP, x, y) {
            return Some(Tap::MapZoom);
        }
        if inside(AMMO, x, y) {
            return Some(Tap::Resupply);
        }
        if inside(BINOC, x, y) {
            return Some(Tap::Binoculars);
        }
        if inside(PAUSE, x, y) {
            return Some(Tap::Pause);
        }
        None
    }

    /// Height of a weapon card: 58, less when more than four are carried.
    fn wpn_slot(&self) -> f32 {
        WPN_SLOT.min(BOTTOM_H as f32 / self.weapons.len().max(1) as f32)
    }

    /// Call once per frame before drawing.
    pub fn new_frame(&mut self) {
        self.c.new_frame();
    }

    /// The bottom screen for this frame.
    pub fn draw_bottom(&mut self, gpu: &mut Gpu, game: &Game, message: Option<&str>) {
        let h = &game.hunt;
        gpu.begin_2d(Some([0, 0, 0]));
        let c = &mut self.c;

        // Panels and highlights.
        c.rect([0.0, 0.0, LEFT_W, BOTTOM_H as f32], PANEL);
        c.rect([RIGHT_X, 0.0, 320.0 - RIGHT_X, BOTTOM_H as f32], PANEL);
        let w = &h.weapons;
        let drawn = w.pose().is_some();
        let slot = WPN_SLOT.min(BOTTOM_H as f32 / self.weapons.len().max(1) as f32);
        for (k, (i, _)) in self.weapons.iter().enumerate() {
            if *i == w.current && drawn {
                c.rect(
                    [2.0, k as f32 * slot + 1.0, LEFT_W - 4.0, slot - 2.0],
                    HILITE,
                );
            }
        }
        for (k, (ai, _)) in self.calls.iter().enumerate() {
            if *ai == h.target_call {
                let r = [
                    RIGHT_X + 2.0,
                    k as f32 * CALL_SLOT + 1.0,
                    320.0 - RIGHT_X - 4.0,
                    CALL_SLOT - 2.0,
                ];
                c.rect(r, HILITE);
            }
        }
        for (r, on) in [(AMMO, false), (BINOC, h.bin_mode), (PAUSE, h.paused)] {
            c.rect(r, if on { HILITE } else { PANEL });
        }
        c.flush(gpu);

        // Weapon pictures, names and shots left; with more than four
        // weapons, smaller pictures and no names.
        let pic_h = (slot - 21.0).min(37.0);
        let pic_w = pic_h * 64.0 / 37.0;
        for (k, (i, t)) in self.weapons.iter().enumerate() {
            let y = k as f32 * slot;
            if let Some(t) = t {
                c.image(
                    gpu,
                    t,
                    [(LEFT_W - pic_w) / 2.0, y + 3.0, pic_w, pic_h],
                    [0.0, 0.0, 1.0, 37.0 / 64.0],
                    WHITE,
                );
            }
            let y = y + pic_h - 37.0;
            let name: String = w
                .info
                .get(*i)
                .map_or("?", |x| x.name.as_str())
                .chars()
                .take(9)
                .collect();
            c.text(gpu, 4.0, y + 41.0, 1.0, WHITE, &name);
            let shots = w.shots_left.get(*i).copied().unwrap_or(0);
            let col = if *i == w.current && drawn {
                WHITE
            } else {
                GREY
            };
            c.text(gpu, 4.0, y + 49.0, 1.0, col, &format!("x{shots}"));
        }

        // Call icons.
        for (k, (_, t)) in self.calls.iter().enumerate() {
            if let Some(t) = t {
                let y = k as f32 * CALL_SLOT;
                c.image(
                    gpu,
                    t,
                    [RIGHT_X + 4.0, y + 4.0, 64.0, 52.0],
                    [0.0, 0.0, 1.0, 52.0 / 64.0],
                    WHITE,
                );
            }
        }

        draw_map(c, gpu, game, self.map.as_ref(), self.map_local);

        // Status under the map.
        let cx = MAP[0] + MAP[2] / 2.0;
        let state = if h.exit_prompt {
            "Leave? A yes, B no, Y restart".to_string()
        } else if h.death.is_some() {
            "You are dead...".to_string()
        } else if h.player.underwater {
            "Underwater".to_string()
        } else if game.run {
            "Running".to_string()
        } else {
            format!("Bagged: {}", h.successes)
        };
        c.text_c(gpu, cx, 180.0, 1.0, WHITE, &state);
        if let Some(m) = message {
            let m: String = m.chars().take(22).collect();
            c.text_c(gpu, cx, 194.0, 1.0, GREEN, &m);
        }
        let left = game.resupplies;
        let col = if left > 0 { WHITE } else { GREY };
        c.text_c(
            gpu,
            AMMO[0] + AMMO[2] / 2.0 - 4.0,
            AMMO[1] + 8.0,
            1.0,
            col,
            "Ammo",
        );
        if left > 0 {
            // The refills left, as a red badge.
            c.disc(AMMO[0] + AMMO[2] - 6.0, AMMO[1] + 4.0, 7.0, RED);
            c.flush(gpu);
            c.text_c(
                gpu,
                AMMO[0] + AMMO[2] - 6.0,
                AMMO[1],
                1.0,
                WHITE,
                &left.to_string(),
            );
        }
        c.text_c(
            gpu,
            BINOC[0] + BINOC[2] / 2.0,
            BINOC[1] + 8.0,
            1.0,
            WHITE,
            "Binocs",
        );
        c.text_c(
            gpu,
            PAUSE[0] + PAUSE[2] / 2.0,
            PAUSE[1] + 8.0,
            1.0,
            WHITE,
            "Pause",
        );
    }

    /// Overlays on the top screen, after the world. `debug` (fps, memory)
    /// shows while paused.
    /// `kill`: the last kill's species name and points, shown for a while
    /// in the bottom right.
    pub fn draw_top(
        &mut self,
        gpu: &mut Gpu,
        game: &Game,
        hunt: &Hunt,
        debug: &str,
        kill: Option<(&str, i32)>,
    ) {
        gpu.begin_2d(None);
        let c = &mut self.c;
        let cx = TOP_W as f32 / 2.0;
        if let Some((name, score)) = kill {
            let name: String = name.chars().take(18).collect();
            let w = 8.0 * name.len().max(10) as f32 + 16.0;
            let (x, y) = (TOP_W as f32 - w - 6.0, 240.0 - 42.0);
            c.rect([x, y, w, 36.0], [0, 0, 0, 200]);
            c.flush(gpu);
            c.text(gpu, x + 8.0, y + 6.0, 1.0, WHITE, &name);
            c.text(
                gpu,
                x + 8.0,
                y + 20.0,
                1.0,
                [240, 200, 70, 255],
                &format!("+{score} pts"),
            );
        }
        if hunt.paused {
            c.text_c(gpu, cx, 104.0, 2.0, WHITE, "PAUSED");
            c.text_c(gpu, cx, 128.0, 1.0, WHITE, "SELECT to resume");
            for (i, l) in debug.lines().enumerate() {
                c.text(gpu, 4.0, 4.0 + 10.0 * i as f32, 1.0, WHITE, l);
            }
        }
        if hunt.show_exit() {
            c.text_c(gpu, cx, 104.0, 2.0, WHITE, "Leave the hunt?");
            c.text_c(gpu, cx, 128.0, 1.0, WHITE, "A = yes   B = no");
            c.text_c(
                gpu,
                cx,
                142.0,
                1.0,
                WHITE,
                "Y = restart (this hunt isn't saved)",
            );
        }
        // Binoculars: what's in the cross-hairs (`Render_LifeInfo`).
        if let Some(li) = hunt.life_info(&game.area) {
            let x = cx + 8.0;
            let lines = [
                li.name.clone(),
                format!("Weight: {:.2}T", li.weight),
                format!("Distance: {}m", (li.distance * 3.0 / 64.0) as i32 / 3),
            ];
            for (i, l) in lines.iter().enumerate() {
                c.text(gpu, x, 160.0 + 10.0 * i as f32, 1.0, GREEN, l);
            }
        }
    }
}

/// The area map (`DrawHMap`) with the view-range circle, the hunter and the
/// animals in range. Zoomed in, it follows the hunter.
fn draw_map(c: &mut Canvas, gpu: &mut Gpu, game: &Game, map: Option<&Texture>, local: bool) {
    let h = &game.hunt;
    let p = &h.player;
    // World units per map pixel: 4 cells.
    let px = |v: f32| v / (4.0 * CELL);
    let span = if local { LOCAL_SPAN } else { 256.0 };
    let (ox, oy) = if local {
        let cl = |v: f32| (px(v) - span / 2.0).clamp(0.0, 256.0 - span);
        (cl(p.x), cl(p.z))
    } else {
        (0.0, 0.0)
    };
    let k = MAP[2] / span;
    let to_screen = |mx: f32, my: f32| (MAP[0] + (mx - ox) * k, MAP[1] + (my - oy) * k);
    if let Some(t) = map {
        let uv = [
            ox / 256.0,
            oy / 256.0,
            (ox + span) / 256.0,
            (oy + span) / 256.0,
        ];
        c.image(gpu, t, MAP, uv, WHITE);
    }
    let mut dot = |x: f32, y: f32, s: f32, col: Rgba| {
        if inside(MAP, x, y) {
            c.rect([x - s / 2.0, y - s / 2.0, s, s], col);
        }
    };
    // View range circle.
    let (hx, hy) = to_screen(px(p.x), px(p.z));
    let r = px(h.opts.view_r as f32 * CELL) * k;
    for i in 0..48 {
        let a = i as f32 / 48.0 * std::f32::consts::TAU;
        dot(hx + a.cos() * r, hy + a.sin() * r, 1.5, [0, 144, 0, 255]);
    }
    // Animals within view range: the hunted ones green (roaming add-ons,
    // which have no call, blue with a gold rim), other huntable ones
    // (spawned anyway) blue, the rest pale. With the radar, the green and
    // blue ones everywhere (`DrawHMap`'s blips).
    let range = h.opts.view_r as f32 * CELL;
    for ch in &h.herd.chars {
        let (dx, dz) = (ch.pos[0] - p.x, ch.pos[2] - p.z);
        let target = h.is_target(ch);
        let huntable = target || h.is_huntable(ch);
        let in_range = dx * dx + dz * dz <= range * range;
        if ch.health <= 0 || !(in_range || huntable && h.opts.radar) {
            continue;
        }
        let (x, y) = to_screen(px(ch.pos[0]), px(ch.pos[2]));
        let no_call = h
            .call_bit(ch)
            .is_some_and(|b| h.opts.no_call_bonus & 1u128.checked_shl(b as u32).unwrap_or(0) != 0);
        if target && no_call {
            dot(x, y, 5.0, [250, 200, 60, 255]);
            dot(x, y, 4.0, [60, 140, 255, 255]);
        } else if target {
            dot(x, y, 4.0, [0, 248, 0, 255]);
        } else if huntable {
            dot(x, y, 4.0, [60, 140, 255, 255]);
        } else {
            dot(x, y, 3.0, [190, 190, 120, 255]);
        }
    }
    // The hunter, with a tick showing where he looks.
    dot(hx, hy, 4.0, [248, 0, 0, 255]);
    let (s, co) = p.alpha.sin_cos();
    for i in 1..4 {
        let d = i as f32 * 2.5;
        dot(hx + s * d, hy - co * d, 2.0, [248, 80, 80, 255]);
    }
    c.flush(gpu);
}
