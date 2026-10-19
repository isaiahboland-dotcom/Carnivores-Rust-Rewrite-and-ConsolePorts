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
use c2_formats::cooked::parse_texture;
use c2_vfs::Vfs;

use crate::gpu::{FlatVertex, Gpu, LinearBuf, Lit, Texture, Vertex, BOTTOM_H, TOP_W};
use crate::hunt::Game;

const LEFT_W: f32 = 72.0;
const RIGHT_X: f32 = 248.0;
const MAP: [f32; 4] = [LEFT_W, 0.0, RIGHT_X - LEFT_W, 176.0];
const WPN_SLOT: f32 = 58.0;
const CALL_SLOT: f32 = 60.0;
/// Buttons under the map: (x, y, w, h).
const BINOC: [f32; 4] = [76.0, 212.0, 82.0, 24.0];
const PAUSE: [f32; 4] = [166.0, 212.0, 78.0, 24.0];
/// Map pixels shown across the map panel when zoomed in (1 pixel = 4 cells).
const LOCAL_SPAN: f32 = 64.0;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const GREEN: [u8; 4] = [0, 176, 0, 255];
const GREY: [u8; 4] = [150, 150, 150, 255];
const PANEL: [u8; 4] = [24, 32, 24, 255];
const HILITE: [u8; 4] = [60, 110, 60, 255];

/// What a tap on the bottom screen asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    Weapon(usize),
    /// Select this call (AI); calls if it was already selected.
    Call(i32),
    MapZoom,
    Binoculars,
    Pause,
}

fn inside(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

pub struct Ui {
    font: Option<Texture>,
    map: Option<Texture>,
    /// Carried weapons: (index, picture).
    weapons: Vec<(usize, Option<Texture>)>,
    /// Calls of the hunted animals: (AI, icon).
    calls: Vec<(i32, Option<Texture>)>,
    pub map_local: bool,
    quads: Option<LinearBuf<Vertex>>,
    flats: Option<LinearBuf<FlatVertex>>,
}

fn load(vfs: &Vfs, path: &str) -> Option<Texture> {
    Texture::new(&parse_texture(&vfs.read(path).ok()?).ok()?)
}

impl Ui {
    pub fn new(vfs: &Vfs, game: &Game) -> Self {
        let weapons = game
            .info
            .weapons
            .iter()
            .map(|&i| (i, load(vfs, &format!("C2RS/WPNPIC{i}.TX"))))
            .collect();
        let mut calls: Vec<(i32, Option<Texture>)> = Vec::new();
        for &d in &game.info.dinos {
            let Some(ai) = game.species.get(d).map(|s| s.info.ai) else {
                continue;
            };
            if ai >= 10 && !calls.iter().any(|c| c.0 == ai) {
                calls.push((ai, load(vfs, &format!("C2RS/CALL{ai}.TX"))));
            }
        }
        Self {
            font: load(vfs, "C2RS/FONT.TX"),
            map: load(vfs, "C2RS/MAP.TX"),
            weapons,
            calls,
            map_local: true,
            quads: LinearBuf::new(4096),
            flats: LinearBuf::new(4096),
        }
    }

    /// What a tap at bottom-screen pixel (x, y) means.
    pub fn tap(&self, x: f32, y: f32) -> Option<Tap> {
        if x < LEFT_W {
            let i = (y / WPN_SLOT) as usize;
            return self.weapons.get(i).map(|w| Tap::Weapon(w.0));
        }
        if x >= RIGHT_X {
            let i = (y / CALL_SLOT) as usize;
            return self.calls.get(i).map(|c| Tap::Call(c.0));
        }
        if inside(MAP, x, y) {
            return Some(Tap::MapZoom);
        }
        if inside(BINOC, x, y) {
            return Some(Tap::Binoculars);
        }
        if inside(PAUSE, x, y) {
            return Some(Tap::Pause);
        }
        None
    }

    fn rect(&mut self, r: [f32; 4], c: [u8; 4]) {
        let [x, y, w, h] = r;
        let v = |x: f32, y: f32| FlatVertex {
            pos: [x, y, 0.5],
            rgba: c,
        };
        let q = [
            v(x, y),
            v(x + w, y),
            v(x + w, y + h),
            v(x, y),
            v(x + w, y + h),
            v(x, y + h),
        ];
        if let Some(b) = self.flats.as_mut() {
            let _ = b.append(&q);
        }
    }

    /// Flushes the rectangles queued since the last flush.
    fn flush_rects(&mut self, gpu: &mut Gpu, from: &mut usize) {
        if let Some(b) = &self.flats {
            gpu.draw_flat(b, *from, b.len() - *from);
            *from = b.len();
        }
    }

    /// A textured rectangle; `uv` = (u0, v0, u1, v1).
    fn image(
        gpu: &mut Gpu,
        buf: &mut LinearBuf<Vertex>,
        t: &Texture,
        r: [f32; 4],
        uv: [f32; 4],
        tint: [u8; 4],
    ) {
        let [x, y, w, h] = r;
        let v = |x: f32, y: f32, u: f32, t: f32| Vertex {
            pos: [x, y, 0.5],
            uv: [u, t],
            lit: [0; 4],
        };
        let q = [
            v(x, y, uv[0], uv[1]),
            v(x + w, y, uv[2], uv[1]),
            v(x + w, y + h, uv[2], uv[3]),
            v(x, y, uv[0], uv[1]),
            v(x + w, y + h, uv[2], uv[3]),
            v(x, y + h, uv[0], uv[3]),
        ];
        if let Some(at) = buf.append(&q) {
            gpu.texenv_tint(tint);
            t.bind();
            gpu.draw(buf, Lit::Unsigned, at, 6);
        }
    }

    /// 8x8 text, `scale` 1 or 2, at the top left (x, y).
    fn text(&mut self, gpu: &mut Gpu, x: f32, y: f32, scale: f32, c: [u8; 4], s: &str) {
        let (Some(font), Some(buf)) = (&self.font, self.quads.as_mut()) else {
            return;
        };
        let mut v = Vec::with_capacity(s.len() * 6);
        let k = 8.0 * scale;
        for (i, ch) in s.chars().enumerate() {
            let code = u32::from(ch);
            if !(33..128).contains(&code) {
                continue;
            }
            let g = (code - 32) as f32;
            let (u0, v0) = (
                (g % 16.0).floor() * 8.0 / 128.0,
                (g / 16.0).floor() * 8.0 / 64.0,
            );
            let (u1, v1) = (u0 + 8.0 / 128.0, v0 + 8.0 / 64.0);
            let x0 = x + i as f32 * k;
            let p = |x: f32, y: f32, u: f32, t: f32| Vertex {
                pos: [x, y, 0.5],
                uv: [u, t],
                lit: [0; 4],
            };
            v.extend_from_slice(&[
                p(x0, y, u0, v0),
                p(x0 + k, y, u1, v0),
                p(x0 + k, y + k, u1, v1),
                p(x0, y, u0, v0),
                p(x0 + k, y + k, u1, v1),
                p(x0, y + k, u0, v1),
            ]);
        }
        if let Some(at) = buf.append(&v) {
            gpu.texenv_tint(c);
            font.bind();
            gpu.draw(buf, Lit::Unsigned, at, v.len());
        }
    }

    /// Text centred on `cx`.
    fn text_c(&mut self, gpu: &mut Gpu, cx: f32, y: f32, scale: f32, c: [u8; 4], s: &str) {
        let w = s.chars().count() as f32 * 8.0 * scale;
        self.text(gpu, cx - w / 2.0, y, scale, c, s);
    }

    /// Call once per frame before drawing: the GPU reads this frame's
    /// vertices when the frame is submitted, so they are only reused then.
    pub fn new_frame(&mut self) {
        if let Some(b) = self.quads.as_mut() {
            b.clear();
        }
        if let Some(b) = self.flats.as_mut() {
            b.clear();
        }
    }

    /// The bottom screen for this frame.
    pub fn draw_bottom(&mut self, gpu: &mut Gpu, game: &Game, message: Option<&str>) {
        let h = &game.hunt;
        gpu.begin_2d(Some([0, 0, 0]));
        let mut flushed = self.flats.as_ref().map_or(0, LinearBuf::len);

        // Panels and highlights.
        self.rect([0.0, 0.0, LEFT_W, BOTTOM_H as f32], PANEL);
        self.rect([RIGHT_X, 0.0, 320.0 - RIGHT_X, BOTTOM_H as f32], PANEL);
        let w = &h.weapons;
        let drawn = w.pose().is_some();
        let carried: Vec<usize> = self.weapons.iter().map(|w| w.0).collect();
        for (k, i) in carried.iter().enumerate() {
            if *i == w.current && drawn {
                let r = [2.0, k as f32 * WPN_SLOT + 1.0, LEFT_W - 4.0, WPN_SLOT - 2.0];
                self.rect(r, HILITE);
            }
        }
        let calls: Vec<i32> = self.calls.iter().map(|c| c.0).collect();
        for (k, ai) in calls.iter().enumerate() {
            if *ai == h.target_call {
                let r = [
                    RIGHT_X + 2.0,
                    k as f32 * CALL_SLOT + 1.0,
                    320.0 - RIGHT_X - 4.0,
                    CALL_SLOT - 2.0,
                ];
                self.rect(r, HILITE);
            }
        }
        for (r, on) in [(BINOC, h.bin_mode), (PAUSE, h.paused)] {
            self.rect(r, if on { HILITE } else { PANEL });
        }
        self.flush_rects(gpu, &mut flushed);

        // Weapon pictures, names and shots left.
        let weapons: Vec<(usize, String, i32)> = self
            .weapons
            .iter()
            .map(|(i, _)| {
                let name = w.info.get(*i).map_or("?", |x| x.name.as_str());
                (
                    *i,
                    name.chars().take(9).collect(),
                    w.shots_left.get(*i).copied().unwrap_or(0),
                )
            })
            .collect();
        for (k, (i, name, _)) in weapons.iter().enumerate() {
            let y = k as f32 * WPN_SLOT;
            if let (Some((_, Some(t))), Some(buf)) =
                (self.weapons.iter().find(|w| w.0 == *i), self.quads.as_mut())
            {
                Self::image(
                    gpu,
                    buf,
                    t,
                    [4.0, y + 3.0, 64.0, 37.0],
                    [0.0, 0.0, 1.0, 37.0 / 64.0],
                    WHITE,
                );
            }
            self.text(gpu, 4.0, y + 41.0, 1.0, WHITE, name);
        }
        for (k, (i, _, shots)) in weapons.iter().enumerate() {
            let y = k as f32 * WPN_SLOT;
            let c = if *i == w.current && drawn {
                WHITE
            } else {
                GREY
            };
            self.text(gpu, 4.0, y + 49.0, 1.0, c, &format!("x{shots}"));
        }

        // Call icons.
        for (k, (_, t)) in self.calls.iter().enumerate() {
            let y = k as f32 * CALL_SLOT;
            if let (Some(t), Some(buf)) = (t, self.quads.as_mut()) {
                Self::image(
                    gpu,
                    buf,
                    t,
                    [RIGHT_X + 4.0, y + 4.0, 64.0, 52.0],
                    [0.0, 0.0, 1.0, 52.0 / 64.0],
                    WHITE,
                );
            }
        }

        self.draw_map(gpu, game, &mut flushed);

        // Status under the map.
        let cx = MAP[0] + MAP[2] / 2.0;
        let state = if h.exit_prompt {
            "Leave? A yes, B no".to_string()
        } else if h.death.is_some() {
            "You are dead...".to_string()
        } else if h.player.underwater {
            "Underwater".to_string()
        } else if game.run {
            "Running".to_string()
        } else {
            format!("Bagged: {}", h.successes)
        };
        self.text_c(gpu, cx, 180.0, 1.0, WHITE, &state);
        if let Some(m) = message {
            let m: String = m.chars().take(22).collect();
            self.text_c(gpu, cx, 194.0, 1.0, GREEN, &m);
        }
        self.text_c(
            gpu,
            BINOC[0] + BINOC[2] / 2.0,
            BINOC[1] + 8.0,
            1.0,
            WHITE,
            "Binocs",
        );
        self.text_c(
            gpu,
            PAUSE[0] + PAUSE[2] / 2.0,
            PAUSE[1] + 8.0,
            1.0,
            WHITE,
            "Pause",
        );
    }

    /// The area map (`DrawHMap`) with the view-range circle, the hunter and
    /// the animals in range. Zoomed in, it follows the hunter.
    fn draw_map(&mut self, gpu: &mut Gpu, game: &Game, flushed: &mut usize) {
        let h = &game.hunt;
        let p = &h.player;
        // World units per map pixel: 4 cells.
        let px = |v: f32| v / (4.0 * CELL);
        let span = if self.map_local { LOCAL_SPAN } else { 256.0 };
        let (ox, oy) = if self.map_local {
            let c = |v: f32| (px(v) - span / 2.0).clamp(0.0, 256.0 - span);
            (c(p.x), c(p.z))
        } else {
            (0.0, 0.0)
        };
        let k = MAP[2] / span;
        // Map pixel -> screen; the panel is square-ish (176 x 176).
        let to_screen = |mx: f32, my: f32| (MAP[0] + (mx - ox) * k, MAP[1] + (my - oy) * k);
        if let (Some(t), Some(buf)) = (&self.map, self.quads.as_mut()) {
            let uv = [
                ox / 256.0,
                oy / 256.0,
                (ox + span) / 256.0,
                (oy + span) / 256.0,
            ];
            Self::image(gpu, buf, t, [MAP[0], MAP[1], MAP[2], MAP[3]], uv, WHITE);
        }
        let dot = |ui: &mut Self, x: f32, y: f32, s: f32, c: [u8; 4]| {
            if inside(MAP, x, y) {
                ui.rect([x - s / 2.0, y - s / 2.0, s, s], c);
            }
        };
        // View range circle.
        let (hx, hy) = to_screen(px(p.x), px(p.z));
        let r = px(h.opts.view_r as f32 * CELL) * k;
        for i in 0..48 {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            dot(
                self,
                hx + a.cos() * r,
                hy + a.sin() * r,
                1.5,
                [0, 144, 0, 255],
            );
        }
        // Animals within view range: huntable bright, others dim.
        let range = h.opts.view_r as f32 * CELL;
        for c in &h.herd.chars {
            let (dx, dz) = (c.pos[0] - p.x, c.pos[2] - p.z);
            if c.health <= 0 || dx * dx + dz * dz > range * range {
                continue;
            }
            let (x, y) = to_screen(px(c.pos[0]), px(c.pos[2]));
            let huntable = c.ai >= 10 && h.opts.targets & (1 << c.ai) != 0;
            let (s, col) = if huntable {
                (4.0, [0, 248, 0, 255])
            } else {
                (3.0, [190, 190, 120, 255])
            };
            dot(self, x, y, s, col);
        }
        // The hunter, with a tick showing where he looks.
        dot(self, hx, hy, 4.0, [248, 0, 0, 255]);
        let (s, c) = p.alpha.sin_cos();
        for i in 1..4 {
            let d = i as f32 * 2.5;
            dot(self, hx + s * d, hy - c * d, 2.0, [248, 80, 80, 255]);
        }
        self.flush_rects(gpu, flushed);
    }

    /// Overlays on the top screen, after the world.
    /// `debug` (fps, memory) shows while paused.
    pub fn draw_top(&mut self, gpu: &mut Gpu, game: &Game, hunt: &Hunt, debug: &str) {
        gpu.begin_2d(None);
        let cx = TOP_W as f32 / 2.0;
        if hunt.paused {
            self.text_c(gpu, cx, 104.0, 2.0, WHITE, "PAUSED");
            self.text_c(gpu, cx, 128.0, 1.0, WHITE, "SELECT to resume");
            self.text(gpu, 4.0, 4.0, 1.0, WHITE, debug);
        }
        if hunt.show_exit() {
            self.text_c(gpu, cx, 104.0, 2.0, WHITE, "Leave the hunt?");
            self.text_c(gpu, cx, 128.0, 1.0, WHITE, "A = yes   B = no");
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
                self.text(gpu, x, 160.0 + 10.0 * i as f32, 1.0, GREEN, l);
            }
        }
    }
}
