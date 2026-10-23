//! The experimental mobile layout of the hunt setup (`--mobile-menu`), after
//! the phone games: three pages of hexagons (maps, animals, weapons) like
//! the 3DS menu, with gritty green buttons whose red lamp lights when on
//! (time of day and observer on the maps page; tranquilizer, radar, cover
//! scent and camouflage with the weapons). A quick first version: the art
//! is drawn here (no mobile artwork), on the menus' dark olive.

use super::*;

/// The menus' dark olive, which the hexagon frames are filled with.
const BG: [u8; 3] = [18, 24, 16];
const BG_F: [f32; 4] = [18.0 / 255.0, 24.0 / 255.0, 16.0 / 255.0, 1.0];
/// Hexagon images: flat-topped, `HEX_W` across the corners.
const HEX_W: usize = 128;
const HEX_H: usize = 111;

/// Where things go on the 800 x 600 canvas.
const GRID: Rect = (16.0, 56.0, 580.0, 478.0);
const SIDE_X: f32 = 612.0;
const SIDE_W: f32 = 176.0;
const BTN_H: f32 = 44.0;
const BACK_BTN: Rect = (16.0, 548.0, 150.0, BTN_H);
const NEXT_BTN: Rect = (634.0, 548.0, 150.0, BTN_H);
const PAGES: [&str; 3] = ["MAPS", "ANIMALS", "WEAPONS & GEAR"];

pub(super) struct Art {
    /// The olive outside a hexagon (its inside clear): drawn over a picture.
    frame: usize,
    /// A hexagon's rim, white (tinted).
    ring: usize,
    /// A gritty green button.
    button: usize,
    /// A lamp, white (tinted red).
    led: usize,
}

/// How far (pixels) a point is inside a flat-topped hexagon `w` x `h`
/// centred at the origin; negative outside.
fn hex_depth(dx: f32, dy: f32, w: f32, h: f32) -> f32 {
    let (ax, ay) = (dx.abs(), dy.abs());
    let top = h / 2.0 - ay;
    // The slanted sides: from (w/2, 0) to (w/4, h/2).
    let slope = (w / 2.0 - ax) - ay * (w / 4.0) / (h / 2.0);
    top.min(slope * 0.866)
}

impl Art {
    pub(super) fn new(hud: &mut Hud, gpu: &Gpu) -> Self {
        let (w, h) = (HEX_W, HEX_H);
        let mut frame = vec![0u8; w * h * 4];
        let mut ring = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let d = hex_depth(
                    x as f32 + 0.5 - w as f32 / 2.0,
                    y as f32 + 0.5 - h as f32 / 2.0,
                    w as f32 - 2.0,
                    h as f32 - 2.0,
                );
                let i = (y * w + x) * 4;
                // Outside: olive, inside clear, a one-pixel blend between.
                let a = (1.0 - (d + 0.5).clamp(0.0, 1.0)) * 255.0;
                frame[i..i + 4].copy_from_slice(&[BG[0], BG[1], BG[2], a as u8]);
                // The rim: the band 0..4 px in, soft at both edges.
                let r = if d < 0.0 {
                    (1.0 + d).clamp(0.0, 1.0)
                } else {
                    (5.0 - d).clamp(0.0, 1.0)
                };
                ring[i..i + 4].copy_from_slice(&[255, 255, 255, (r * 255.0) as u8]);
            }
        }
        // The button: green with grain, lighter at the top, a dark edge.
        let (bw, bh) = (176usize, 44usize);
        let mut button = vec![0u8; bw * bh * 4];
        let mut seed = 0x2545_F491u32;
        for y in 0..bh {
            for x in 0..bw {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let grain = (seed % 25) as i32 - 12;
                let shade = 14 - (y as i32 * 28 / bh as i32);
                let edge = x < 2 || y < 2 || x + 2 >= bw || y + 2 >= bh;
                let c = |base: i32| {
                    if edge {
                        (base / 3) as u8
                    } else {
                        (base + grain + shade).clamp(0, 255) as u8
                    }
                };
                let i = (y * bw + x) * 4;
                button[i..i + 4].copy_from_slice(&[c(46), c(78), c(38), 255]);
            }
        }
        let n = 32usize;
        let mut led = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let d = ((x as f32 + 0.5 - 16.0).hypot(y as f32 + 0.5 - 16.0)) / 16.0;
                let core = (1.0 - d).clamp(0.0, 1.0);
                let i = (y * n + x) * 4;
                led[i..i + 4].copy_from_slice(&[255, 255, 255, (core.sqrt() * 255.0) as u8]);
            }
        }
        Self {
            frame: hud.add_image_filtered(gpu, w as u32, h as u32, &frame, true),
            ring: hud.add_image_filtered(gpu, w as u32, h as u32, &ring, true),
            button: hud.add_image_filtered(gpu, bw as u32, bh as u32, &button, true),
            led: hud.add_image_filtered(gpu, n as u32, n as u32, &led, true),
        }
    }
}

/// `n` hexagons laid out in `GRID`, as big as fit (at most 170 across):
/// centre and width.
fn hex_grid(n: usize) -> Vec<(f32, f32, f32)> {
    if n == 0 {
        return Vec::new();
    }
    // Columns a hexagon apart (their boxes mustn't overlap: each frame is
    // drawn over its box), every other one half a row down.
    let (gx, gy, gw, gh) = GRID;
    let gap = 6.0;
    let mut best = (1, 0.0f32);
    for cols in 1..=n {
        let rows = n.div_ceil(cols) as f32;
        let w = ((gw - gap * (cols as f32 - 1.0)) / cols as f32)
            .min((gh - gap * rows) / (rows + 0.5) / 0.866);
        if w > best.1 {
            best = (cols, w);
        }
    }
    let (cols, w) = (best.0, best.1.min(170.0));
    let h = w * 0.866;
    let used = (w + gap) * cols as f32 - gap;
    let x0 = gx + (gw - used) / 2.0 + w / 2.0;
    (0..n)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            let y = gy + h / 2.0 + r as f32 * (h + gap) + if c % 2 == 1 { h / 2.0 } else { 0.0 };
            (x0 + c as f32 * (w + gap), y, w)
        })
        .collect()
}

/// The side buttons' rects, top to bottom.
fn side(n: usize) -> Vec<Rect> {
    (0..n)
        .map(|i| (SIDE_X, 96.0 + i as f32 * 60.0, SIDE_W, BTN_H))
        .collect()
}

impl Menu {
    /// The list a page shows (`columns` index).
    fn mobile_col(&self) -> usize {
        self.mobile_page.min(2)
    }

    /// The page's hexagons: (item, centre x, y, width), cheapest first.
    fn mobile_cells(&self) -> Vec<(usize, f32, f32, f32)> {
        let col = &self.columns[self.mobile_col()];
        hex_grid(col.order.len())
            .into_iter()
            .zip(&col.order)
            .map(|((x, y, w), &i)| (i, x, y, w))
            .collect()
    }

    /// The side buttons of the page: label and whether it is on.
    fn mobile_side(&self) -> Vec<(String, bool)> {
        let c = &self.choice;
        match self.mobile_page {
            0 => vec![
                ("Dawn".into(), c.time == 0),
                ("Day".into(), c.time == 1),
                ("Night".into(), c.time == 2),
                ("Observer".into(), c.observer),
            ],
            2 => {
                let eq = &self.columns[3];
                let price = |i: usize| eq.prices.get(i).copied().unwrap_or(0);
                let named = |i: usize, n: &str| {
                    if price(i) > 0 {
                        format!("{n} {}", price(i))
                    } else {
                        n.to_string()
                    }
                };
                vec![
                    ("Tranquilizer".into(), self.tranq()),
                    (named(1, "Radar"), c.equip & 2 != 0),
                    (named(2, "Cover Scent"), c.equip & 4 != 0),
                    (named(0, "Camouflage"), c.equip & 1 != 0),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn update_mobile(&mut self, input: &Input) {
        let p = self.cursor;
        if input.pressed(KeyCode::Escape) || (input.clicked && inside(p, BACK_BTN)) {
            if self.mobile_page == 0 {
                self.save_equipment();
                self.screen = Screen::Main;
            } else {
                self.mobile_page -= 1;
            }
            return;
        }
        let col = self.mobile_col();
        let hovered = p.and_then(|(x, y)| {
            self.mobile_cells()
                .into_iter()
                .find(|&(_, cx, cy, w)| hex_depth(x - cx, y - cy, w, w * 0.866) >= 0.0)
        });
        if let Some((i, ..)) = hovered {
            self.choice.focus = Some((col, i));
        }
        if !input.clicked {
            return;
        }
        if inside(p, NEXT_BTN) {
            if self.mobile_page < 2 {
                self.mobile_page += 1;
            } else if self.can_hunt() {
                self.launch_hunt();
            }
            return;
        }
        if let Some((i, ..)) = hovered {
            self.pick(col, i);
            return;
        }
        let hit = side(4).into_iter().position(|r| inside(p, r));
        match (self.mobile_page, hit) {
            (0, Some(t @ 0..=2)) => self.choice.time = t,
            (0, Some(3)) => self.toggle_observer(),
            (2, Some(0)) => {
                if let Some(s) = self.me_mut() {
                    s.options.tranq = i32::from(s.options.tranq == 0);
                }
            }
            // Radar, cover scent, camouflage: equipment bits 1, 2, 0.
            (2, Some(b @ 1..=3)) => self.pick(3, [1, 2, 0][b - 1]),
            _ => {}
        }
    }

    /// A filled rectangle on the canvas.
    pub(super) fn fill(&self, out: &mut Vec<Sprite>, r: Rect, color: [f32; 4]) {
        let (s, ox, oy) = self.canvas();
        out.extend(
            self.hud
                .rect(ox + r.0 * s, oy + r.1 * s, r.2 * s, r.3 * s, color),
        );
    }

    /// An image on the canvas, tinted.
    fn tinted(&self, out: &mut Vec<Sprite>, id: usize, r: Rect, uv: [f32; 4], color: [f32; 4]) {
        let (s, ox, oy) = self.canvas();
        out.push(Sprite {
            size: Some([r.2 * s, r.3 * s]),
            uv,
            color,
            ..Sprite::new(id, ox + r.0 * s, oy + r.1 * s, 1.0)
        });
    }

    /// A green button with its label, and a lamp when it has one.
    fn mobile_button(&self, out: &mut Vec<Sprite>, r: Rect, label: &str, lamp: Option<bool>) {
        let Some(art) = &self.mobile else { return };
        let hover = inside(self.cursor, r);
        let tint = if hover {
            [1.15, 1.2, 1.1, 1.0]
        } else {
            [1.0; 4]
        };
        self.tinted(out, art.button, r, [0.0, 0.0, 1.0, 1.0], tint);
        let mut x = r.0 + 14.0;
        if let Some(on) = lamp {
            let (lx, ly) = (r.0 + 12.0, r.1 + r.3 / 2.0 - 8.0);
            if on {
                // The glow, then the lit lamp.
                let g = (lx - 10.0, ly - 10.0, 36.0, 36.0);
                self.tinted(
                    out,
                    art.led,
                    g,
                    [0.0, 0.0, 1.0, 1.0],
                    [1.0, 0.1, 0.05, 0.45],
                );
            }
            let col = if on {
                [1.0, 0.2, 0.12, 1.0]
            } else {
                [0.25, 0.06, 0.05, 1.0]
            };
            self.tinted(
                out,
                art.led,
                (lx, ly, 16.0, 16.0),
                [0.0, 0.0, 1.0, 1.0],
                col,
            );
            x = r.0 + 38.0;
        }
        // Smaller when it wouldn't fit.
        let room = r.0 + r.2 - 6.0 - x;
        let px = if self.text_width(14.0, label) > room {
            11.0
        } else {
            14.0
        };
        self.text(out, x, r.1 + r.3 / 2.0 - px / 2.0, px, SAND, label);
    }

    pub(super) fn draw_mobile(&self, out: &mut Vec<Sprite>) {
        let Some(art) = &self.mobile else { return };
        self.fill(out, (0.0, 0.0, CW, CH), BG_F);
        // The page and the account.
        let page = self.mobile_page.min(2);
        self.text(out, 20.0, 18.0, 18.0, PICKED, PAGES[page]);
        let score = self.me().map_or(0, |s| s.score);
        let acc = format!("{} / {score}", self.remaining());
        let w = self.text_width(16.0, &acc);
        self.text(
            out,
            CW - 24.0 - w,
            20.0,
            16.0,
            if self.remaining() < 0 { DIM } else { PICKED },
            &acc,
        );
        // The hexagons: picture, the olive frame over it, the rim.
        let col = self.mobile_col();
        let c = &self.columns[col];
        let mask = self.picked_mask(col);
        let left = self.remaining();
        for (i, cx, cy, w) in self.mobile_cells() {
            let h = w * 0.866;
            let r = (cx - w / 2.0, cy - h / 2.0, w, h);
            let on = mask.has(i);
            let afford = on || c.prices.get(i).is_some_and(|&p| p <= left) || self.choice.observer;
            let dim = if afford {
                [1.0; 4]
            } else {
                [0.4, 0.4, 0.4, 1.0]
            };
            match c.pics.get(i).copied().flatten() {
                Some(id) => {
                    // The middle of the picture, at the hexagon's shape.
                    let (iw, ih) = self
                        .hud
                        .image(id)
                        .map_or((1.0, 1.0), |im| (im.w as f32, im.h as f32));
                    let k = (w / h) / (iw / ih);
                    let uv = if k < 1.0 {
                        [(1.0 - k) / 2.0, 0.0, (1.0 + k) / 2.0, 1.0]
                    } else {
                        [0.0, (1.0 - 1.0 / k) / 2.0, 1.0, (1.0 + 1.0 / k) / 2.0]
                    };
                    self.tinted(out, id, r, uv, dim);
                }
                None => self.fill(out, r, [0.12, 0.17, 0.11, 1.0]),
            }
            self.tinted(out, art.frame, r, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
            let hover = self.choice.focus == Some((col, i));
            let rim = if on {
                [1.0, 0.85, 0.3, 1.0]
            } else if hover {
                [0.9, 1.0, 0.75, 1.0]
            } else {
                [0.3, 0.42, 0.27, 1.0]
            };
            self.tinted(out, art.ring, r, [0.0, 0.0, 1.0, 1.0], rim);
            if pics_missing(c, i) {
                let name: String = c.names[i].chars().take(12).collect();
                let tw = self.text_width(12.0, &name);
                self.text(out, cx - tw / 2.0, cy - 6.0, 12.0, TEXT, &name);
            }
            if let Some(&p) = c.prices.get(i).filter(|&&p| p > 0) {
                let t = p.to_string();
                let tw = self.text_width(12.0, &t);
                self.fill(
                    out,
                    (cx - tw / 2.0 - 4.0, cy + h / 2.0 - 20.0, tw + 8.0, 15.0),
                    [0.0, 0.0, 0.0, 0.6],
                );
                let col = if afford { PICKED } else { DIM };
                self.text(out, cx - tw / 2.0, cy + h / 2.0 - 19.0, 12.0, col, &t);
            }
        }
        for (r, (label, on)) in side(4).into_iter().zip(self.mobile_side()) {
            self.mobile_button(out, r, &label, Some(on));
        }
        // What the cursor is on, and the way on.
        if let Some((fc, fi)) = self.choice.focus.filter(|f| f.0 == col) {
            let c = &self.columns[fc];
            if let Some(name) = c.names.get(fi) {
                let t = format!("{name}  {} pts", c.prices.get(fi).copied().unwrap_or(0));
                let tw = self.text_width(16.0, &t);
                self.text(out, CW / 2.0 - tw / 2.0, 560.0, 16.0, TEXT, &t);
            }
        }
        self.mobile_button(out, BACK_BTN, "Back", None);
        let next = if page < 2 { "Next" } else { "HUNT" };
        let ready = page < 2 || self.can_hunt();
        self.mobile_button(out, NEXT_BTN, next, (page == 2).then_some(ready));
    }
}

/// A list item with no picture (its name is written in the hexagon).
fn pics_missing(c: &Column, i: usize) -> bool {
    c.pics.get(i).copied().flatten().is_none()
}
