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
    /// The phone game's own pieces, for a game built from its APK.
    pub(super) kit: Option<super::kit::KitArt>,
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
            kit: None,
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

    /// The page's hexagons: (item, centre x, y, width, height), cheapest
    /// first.
    fn mobile_cells(&self) -> Vec<(usize, f32, f32, f32, f32)> {
        let col = &self.columns[self.mobile_col()];
        if self.kit.is_some() {
            let comb = Comb::new(col.order.len(), self.mobile_page);
            return comb
                .cells()
                .into_iter()
                .zip(&col.order)
                .map(|((x, y), &i)| (i, x + comb.w / 2.0, y + comb.h / 2.0, comb.w, comb.h))
                .collect();
        }
        hex_grid(col.order.len())
            .into_iter()
            .zip(&col.order)
            .map(|((x, y, w), &i)| (i, x, y, w, w * 0.866))
            .collect()
    }

    /// The side buttons' rects, in `mobile_side`'s order.
    fn side_rects(&self) -> Vec<Rect> {
        if self.kit.is_none() {
            return side(self.mobile_side().len());
        }
        if self.mobile_page == 2 && self.pc.dynamic_time != 0 {
            return [2, 3, 1, 0, 4, 5]
                .iter()
                .map(|&k| {
                    (
                        510.0 + (k % 2) as f32 * 144.0,
                        352.0 + (k / 2) as f32 * 48.0,
                        124.0,
                        40.0,
                    )
                })
                .collect();
        }
        // Two rows of two under the words; the weapons page's as the
        // phone's: camouflage, cover scent / tranquilizer, radar.
        let order = if self.mobile_page == 2 {
            [2, 3, 1, 0]
        } else {
            [0, 1, 2, 3]
        };
        order
            .iter()
            .map(|&k| {
                let (x, y) = ((k % 2) as f32, (k / 2) as f32);
                (510.0 + x * 144.0, 364.0 + y * 72.0, 124.0, 52.0)
            })
            .collect()
    }

    /// Back and next (or hunt).
    fn bottom_rects(&self) -> (Rect, Rect) {
        if self.kit.is_some() {
            (KIT_BACK, KIT_NEXT)
        } else {
            (BACK_BTN, NEXT_BTN)
        }
    }

    /// Whether next (or hunt) can be pressed.
    fn next_ready(&self) -> bool {
        let c = &self.choice;
        match self.mobile_page {
            0 => true,
            1 => self.kit.is_none() || !c.dinos.is_empty() || c.observer || c.survival.is_some(),
            _ => self.can_hunt(),
        }
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
                let mut buttons = vec![
                    ("Tranquilizer".into(), self.tranq()),
                    (named(1, "Radar"), c.equip & 2 != 0),
                    (named(2, "Cover Scent"), c.equip & 4 != 0),
                    (named(0, "Camouflage"), c.equip & 1 != 0),
                ];
                if self.pc.dynamic_time != 0 {
                    buttons.push(("Night vision".into(), c.equip & 16 != 0));
                    buttons.push(("Flash light".into(), c.equip & 32 != 0));
                }
                buttons
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn update_mobile(&mut self, input: &Input) {
        let p = self.cursor;
        let (back, next) = self.bottom_rects();
        if input.pressed(KeyCode::Escape) || (input.clicked && inside(p, back)) {
            if self.mobile_page == 0 {
                self.save_equipment();
                self.screen = Screen::Main;
            } else {
                self.mobile_page -= 1;
            }
            return;
        }
        let col = self.mobile_col();
        let kit = self.kit.is_some();
        let hovered = p.and_then(|(x, y)| {
            self.mobile_cells().into_iter().find(|&(_, cx, cy, w, h)| {
                if kit {
                    // The phone's stand on a corner.
                    hex_depth(y - cy, x - cx, h * 0.96, w * 0.94) >= 0.0
                } else {
                    hex_depth(x - cx, y - cy, w, h) >= 0.0
                }
            })
        });
        if let Some((i, ..)) = hovered {
            self.choice.focus = Some((col, i));
        }
        if !input.clicked {
            return;
        }
        if inside(p, next) {
            if !self.next_ready() {
            } else if self.mobile_page < 2 {
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
        let hit = self.side_rects().into_iter().position(|r| inside(p, r));
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
            (2, Some(b @ 4..=5)) if self.pc.dynamic_time != 0 => self.pick(3, b),
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
        if let Some(k) = &art.kit {
            self.draw_kit_page(out, k);
            return;
        }
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
        for (i, cx, cy, w, h) in self.mobile_cells() {
            let r = (cx - w / 2.0, cy - h / 2.0, w, h);
            let on = mask.has(i);
            let afford = on || c.prices.get(i).is_some_and(|&p| p <= left) || self.choice.observer;
            let dim = if afford {
                [1.0; 4]
            } else {
                [0.4, 0.4, 0.4, 1.0]
            };
            let hover = self.choice.focus == Some((col, i));
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
        for (r, (label, on)) in self.side_rects().into_iter().zip(self.mobile_side()) {
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

/// The phone's page (an APK-built game), after the owner's screenshots:
/// the honeycomb at the left, the item's name, cost and words set against
/// the right with two rows of toggles under them, back and next along the
/// bottom with the account between.
const KIT_GRID: Rect = (9.0, 55.0, 620.0, 440.0);
const KIT_BACK: Rect = (11.0, 505.0, 178.0, 52.0);
const KIT_NEXT: Rect = (611.0, 505.0, 178.0, 52.0);
const KIT_RIGHT: f32 = 787.0;

/// The honeycomb: corner-up cells `w` x `h`, `sx` apart in a row and `sy`
/// between rows, every other row half a cell in; the first cell's top
/// left, and each row's length (the longer rows last, as the phone's).
struct Comb {
    w: f32,
    h: f32,
    sx: f32,
    sy: f32,
    x0: f32,
    y0: f32,
    rows: Vec<usize>,
}

impl Comb {
    /// `n` cells: the phone's rows (three; four of animals), more and
    /// smaller when they wouldn't fit across.
    fn new(n: usize, page: usize) -> Self {
        let (gx, gy, gw, gh) = KIT_GRID;
        let mut r: usize = if page == 1 { 4 } else { 3 };
        loop {
            let h = (420.0 / (1.0 + 0.843 * (r - 1) as f32)).min(163.0);
            let (w, sy) = (h * 0.9, h * 0.843);
            let sx = w * 1.06;
            let long = n.div_ceil(r).max(1);
            if (long - 1) as f32 * sx + w + sx / 2.0 <= gw || r >= 12 {
                let (base, extra) = (n / r, n % r);
                let rows = (0..r).map(|i| base + usize::from(i >= r - extra)).collect();
                let tall = h + sy * (r - 1) as f32;
                return Self {
                    w,
                    h,
                    sx,
                    sy,
                    x0: gx,
                    y0: gy + ((gh - tall) / 2.0).max(0.0),
                    rows,
                };
            }
            r += 1;
        }
    }

    /// Cell `row`, `col`'s top left (beyond the rows too: the empty
    /// cells around).
    fn at(&self, row: i32, col: i32) -> (f32, f32) {
        let odd = if row.rem_euclid(2) == 1 {
            self.sx / 2.0
        } else {
            0.0
        };
        (
            self.x0 + col as f32 * self.sx + odd,
            self.y0 + row as f32 * self.sy,
        )
    }

    fn cells(&self) -> Vec<(f32, f32)> {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(r, &n)| (0..n).map(move |c| (r, c)))
            .map(|(r, c)| self.at(r as i32, c as i32))
            .collect()
    }
}

/// The kit's faces.
#[derive(Clone, Copy)]
enum Face {
    Heading,
    Title,
    Body,
}

impl Menu {
    /// A line in the kit's `face` at `x` (its left, middle or right by
    /// `align` 0, 0.5, 1), top at `y`; its width.
    #[allow(clippy::too_many_arguments)]
    fn kit_line(
        &self,
        out: &mut Vec<Sprite>,
        face: Face,
        (x, y): (f32, f32),
        px: f32,
        color: [f32; 4],
        text: &str,
        align: f32,
    ) -> f32 {
        let Some(k) = &self.kit else { return 0.0 };
        let f = match face {
            Face::Heading => k.heading.as_ref(),
            Face::Title => k.title.as_ref(),
            Face::Body => k.body.as_ref(),
        };
        let Some(f) = f.or(k.body.as_ref()) else {
            return 0.0;
        };
        let w = f.width(px, text);
        let x = x - w * align;
        let c = self.canvas();
        let shadow = [0.0, 0.0, 0.0, color[3]];
        f.sprites(out, (c.0, c.1 + c.0, c.2 + c.0), (x, y), px, shadow, text);
        f.sprites(out, c, (x, y), px, color, text);
        w
    }

    fn draw_kit_page(&self, out: &mut Vec<Sprite>, k: &super::kit::KitArt) {
        use super::kit::{PALE, RED, STONE};
        let page = self.mobile_page.min(2);
        let col = self.mobile_col();
        let c = &self.columns[col];
        let full = [0.0, 0.0, 1.0, 1.0];
        let cover = self.cover();
        self.fill(out, cover, BG_F);
        if let Some(bg) = k.bg {
            self.tinted(out, bg, cover, full, [1.0; 4]);
        }
        // The empty cells of the honeycomb, all over.
        let comb = Comb::new(c.order.len(), page);
        if let Some(id) = k.empty {
            let r0 = ((cover.1 - comb.y0) / comb.sy).floor() as i32 - 1;
            let r1 = ((cover.1 + cover.3 - comb.y0) / comb.sy).ceil() as i32;
            let c0 = ((cover.0 - comb.x0) / comb.sx).floor() as i32 - 1;
            let c1 = ((cover.0 + cover.2 - comb.x0) / comb.sx).ceil() as i32;
            for r in r0..=r1 {
                for cc in c0..=c1 {
                    let used = usize::try_from(r)
                        .ok()
                        .and_then(|r| comb.rows.get(r))
                        .is_some_and(|&n| (0..n as i32).contains(&cc));
                    if !used {
                        let (x, y) = comb.at(r, cc);
                        let a = 0.25;
                        self.tinted(out, id, (x, y, comb.w, comb.h), full, [1.0, 1.0, 1.0, a]);
                    }
                }
            }
        }
        // The items: the plate, the item (a silhouette when out of reach),
        // the red ring when picked.
        let mask = self.picked_mask(col);
        let left = self.remaining();
        for (i, cx, cy, w, h) in self.mobile_cells() {
            let r = (cx - w / 2.0, cy - h / 2.0, w, h);
            let on = mask.has(i);
            let afford = on || c.prices.get(i).is_some_and(|&p| p <= left) || self.choice.observer;
            let lit = if self.choice.focus == Some((col, i)) {
                [1.2, 1.2, 1.15, 1.0]
            } else {
                [1.0; 4]
            };
            if col != 0 {
                if let Some(id) = k.cell {
                    self.tinted(out, id, r, full, lit);
                }
            }
            let (on_art, off_art) = k
                .items
                .get(col)
                .and_then(|v| v.get(i))
                .copied()
                .unwrap_or((None, None));
            let item = if afford { on_art } else { off_art.or(on_art) };
            if let Some(id) = item {
                let (iw, ih) = self
                    .hud
                    .image(id)
                    .map_or((1.0, 1.0), |im| (im.w as f32, im.h as f32));
                let f = if col == 0 { 1.0 } else { 0.84 };
                let s = (w * f / iw).min(h * f / ih);
                let (dw, dh) = (iw * s, ih * s);
                let tint = if afford { lit } else { [0.4, 0.4, 0.4, 1.0] };
                self.tinted(out, id, (cx - dw / 2.0, cy - dh / 2.0, dw, dh), full, tint);
            } else {
                let name: String = c.names[i].chars().take(12).collect();
                self.kit_line(out, Face::Body, (cx, cy - 9.0), 17.0, PALE, &name, 0.5);
            }
            if on {
                if let Some(id) = k.selector {
                    self.tinted(out, id, r, full, [1.0; 4]);
                }
            }
        }
        // The item shown at the right: the one pointed at, else the one
        // picked.
        let gear = (page == 2)
            .then(|| {
                self.side_rects()
                    .iter()
                    .position(|&r| inside(self.cursor, r))
                    .map(|i| [3, 1, 2, 0, 4, 5][i])
            })
            .flatten();
        let c = if gear.is_some() { &self.columns[3] } else { c };
        let shown = gear.or_else(|| {
            self.choice
                .focus
                .filter(|f| f.0 == col)
                .map(|f| f.1)
                .or_else(|| (0..c.names.len()).find(|&i| mask.has(i)))
        });
        let us = self.me().is_some_and(|s| s.options.sys != 0);
        let (heading, cost, words): (Vec<String>, Option<i32>, Vec<String>) = match shown {
            Some(3) if gear == Some(3) => (
                vec!["TRANQUILIZER".into()],
                None,
                vec!["Tranquilize instead of collecting trophies. Adds 25% to the score.".into()],
            ),
            Some(i) => {
                let t = if us { &c.texts_us } else { &c.texts };
                // The first line repeats the name ("Weapon : Pistol").
                let mut words = t.get(i).map_or_else(Vec::new, |v| {
                    v.iter()
                        .skip(1)
                        .skip_while(|l| l.trim().is_empty())
                        .map(|l| l.trim().to_string())
                        .collect()
                });
                if gear.is_some() {
                    let impact = match i {
                        0 => Some("Camouflage reduces the score by 15%."),
                        1 => Some("Radar reduces the score by 30%."),
                        2 => Some("Cover scent reduces the score by 20%."),
                        _ => None,
                    };
                    if let Some(impact) = impact {
                        words.insert(0, impact.into());
                    }
                }
                let cost = c.prices.get(i).copied().filter(|&p| p > 0);
                (vec![c.names[i].to_uppercase()], cost, words)
            }
            None if page == 1 => (
                vec!["PLEASE SELECT".into(), "A DINOSAUR".into()],
                None,
                vec![
                    "To start exploring you need to".into(),
                    "select at least one dinosaur.".into(),
                ],
            ),
            None => (Vec::new(), None, Vec::new()),
        };
        let mut y = 44.0;
        for l in &heading {
            // Narrower when long.
            let px = 30.0;
            let w = self
                .kit
                .as_ref()
                .and_then(|k| k.heading.as_ref())
                .map_or(0.0, |f| f.width(px, l));
            let px = if w > 440.0 { px * 440.0 / w } else { px };
            self.kit_line(out, Face::Heading, (KIT_RIGHT, y), px, RED, l, 1.0);
            y += 34.0;
        }
        if let Some(p) = cost {
            self.kit_line(
                out,
                Face::Body,
                (KIT_RIGHT, y - 4.0),
                24.0,
                PALE,
                &format!("cost: {p}"),
                1.0,
            );
        }
        let bottom = if page == 1 { 495.0 } else { 350.0 };
        // The files' own lines; a long one (ours, the score) wrapped to the
        // room beside the honeycomb's rows at its height.
        let width = |t: &str| {
            self.kit
                .as_ref()
                .and_then(|k| k.body.as_ref())
                .map_or(0.0, |f| f.width(20.0, t))
        };
        let room = |y: f32| {
            let edge = comb
                .rows
                .iter()
                .enumerate()
                .filter(|&(r, &n)| {
                    let top = comb.at(r as i32, 0).1;
                    n > 0 && top < y + 22.0 && top + comb.h > y
                })
                .map(|(r, &n)| comb.at(r as i32, n as i32 - 1).0 + comb.w)
                .fold(0.0, f32::max);
            KIT_RIGHT - edge.max(KIT_RIGHT - 480.0) - 12.0
        };
        let mut y = 122.0;
        let put = |out: &mut Vec<Sprite>, y: &mut f32, t: &str| {
            if *y + 20.0 <= bottom && !t.is_empty() {
                self.kit_line(out, Face::Body, (KIT_RIGHT, *y), 20.0, PALE, t, 1.0);
            }
            *y += 22.5;
        };
        for l in &words {
            let mut cur = String::new();
            for w in l.split(' ') {
                let next = if cur.is_empty() {
                    w.to_string()
                } else {
                    format!("{cur} {w}")
                };
                if width(&next) > room(y) && !cur.is_empty() {
                    put(out, &mut y, &cur);
                    cur = w.to_string();
                } else {
                    cur = next;
                }
            }
            put(out, &mut y, &cur);
        }
        // The toggles: plank, lamp, picture.
        let icons = if page == 2 {
            &k.icons[4..]
        } else {
            &k.icons[..4]
        };
        for (index, (r, (label, on))) in self
            .side_rects()
            .into_iter()
            .zip(self.mobile_side())
            .enumerate()
        {
            let icon = icons.get(index).and_then(|i| i.as_ref());
            let tint = if inside(self.cursor, r) {
                [1.3, 1.3, 1.25, 1.0]
            } else {
                [1.0; 4]
            };
            if let Some(id) = k.button {
                self.tinted(out, id, r, full, tint);
            }
            let lamp = if on { k.lamp_on } else { k.lamp_off };
            if let Some(id) = lamp {
                let s = r.3 * 0.62;
                self.tinted(
                    out,
                    id,
                    (r.0 + r.2 * 0.1, r.1 + (r.3 - s) / 2.0, s, s),
                    full,
                    [1.0; 4],
                );
            }
            match icon {
                Some(id) => {
                    let ih = r.3 * 0.7;
                    let iw = ih * 60.0 / 48.0;
                    let r = (r.0 + r.2 * 0.72 - iw / 2.0, r.1 + (r.3 - ih) / 2.0, iw, ih);
                    self.tinted(out, *id, r, full, [1.0; 4]);
                }
                None => {
                    let at = (r.0 + r.2 * 0.66, r.1 + r.3 * 0.3);
                    self.kit_line(
                        out,
                        Face::Body,
                        (r.0 + r.2 * 0.66, at.1),
                        if index >= 4 { 12.0 } else { 16.0 },
                        PALE,
                        &label,
                        0.5,
                    );
                }
            }
        }
        // Back, the account, next (dim until it can be pressed).
        let (back, next) = self.bottom_rects();
        let ready = self.next_ready();
        let word = if page < 2 { "NEXT" } else { "HUNT" };
        for (r, w, live) in [(back, "BACK", true), (next, word, ready)] {
            let tint = match (live, inside(self.cursor, r)) {
                (false, _) => [0.6, 0.6, 0.6, 0.7],
                (true, true) => [1.3, 1.3, 1.25, 1.0],
                _ => [1.0; 4],
            };
            if let Some(id) = k.plank {
                self.tinted(out, id, r, full, tint);
            }
            let color = if live { STONE } else { [0.45, 0.45, 0.43, 0.8] };
            let px = 28.0;
            self.kit_line(
                out,
                Face::Title,
                (r.0 + r.2 / 2.0, r.1 + (r.3 - px) / 2.0),
                px,
                color,
                w,
                0.5,
            );
        }
        let acc = format!("Your Account: {left}");
        let color = if left < 0 { RED } else { PALE };
        self.kit_line(out, Face::Body, (CW / 2.0, 517.0), 26.0, color, &acc, 0.5);
    }
}

/// The phone's options list: labels at the left, values centred at the
/// right, the dotted sliders between brackets; ours in three tabs (the
/// phone's has 11 rows, ours some 30).
const OPT_TABS: [&str; 3] = ["GAME", "CONTROLS", "VIDEO"];
const OPT_LABEL_X: f32 = 76.0;
const OPT_VALUE_X: f32 = 574.0;
/// The slider's brackets, outer edges.
const OPT_SLIDER: (f32, f32) = (433.0, 715.0);
const OPT_RESET: Rect = (520.0, 505.0, 270.0, 40.0);

/// A row of the phone-style options.
#[derive(Clone, Copy)]
enum KitRow {
    Opt(Opt),
    /// A key binding (`ACTIONS`).
    Key(usize),
}

impl Menu {
    /// The tab's rows with their rectangles, and the words' size.
    fn kit_rows(&self) -> (Vec<(KitRow, Rect)>, f32) {
        let rows = self.kit_all_rows();
        // The phone's spacing; the keys in two columns.
        self.kit_layout(rows)
    }

    fn kit_all_rows(&self) -> Vec<KitRow> {
        match self.opt_tab {
            0 => GAME_OPTS.iter().map(|&o| KitRow::Opt(o)).collect(),
            1 => MOUSE_OPTS
                .iter()
                .map(|&o| KitRow::Opt(o))
                .chain((0..ACTIONS.len()).map(KitRow::Key))
                .collect(),
            _ => VIDEO_OPTS.iter().map(|&o| KitRow::Opt(o)).collect(),
        }
    }

    fn kit_layout(&self, rows: Vec<KitRow>) -> (Vec<(KitRow, Rect)>, f32) {
        // The phone's spacing; the keys in two columns.
        let per = if rows.len() <= 11 {
            11
        } else {
            rows.len().div_ceil(2)
        };
        let (y0, step) = (60.0, 42.0);
        let out = rows
            .into_iter()
            .skip(if self.opt_tab == 0 {
                self.game_scroll.first
            } else {
                0
            })
            .take(if self.opt_tab == 0 { 9 } else { usize::MAX })
            .enumerate()
            .map(|(i, r)| {
                let rect = if per == 11 {
                    (40.0, y0 + i as f32 * step, 720.0, step)
                } else {
                    let (c, k) = ((i / per) as f32, (i % per) as f32);
                    (30.0 + c * 380.0, y0 + k * step, 360.0, step)
                };
                (r, rect)
            })
            .collect();
        (out, 30.0)
    }

    /// The tab headings' rectangles.
    fn kit_tabs() -> [Rect; 3] {
        [0.0, 1.0, 2.0].map(|i| (120.0 + i * 200.0, 6.0, 160.0, 40.0))
    }

    /// Where a row's value goes (its middle), and its slider's brackets.
    fn kit_value_at(r: Rect) -> (f32, (f32, f32)) {
        if r.2 > 500.0 {
            (OPT_VALUE_X, OPT_SLIDER)
        } else {
            (r.0 + 280.0, (r.0 + 180.0, r.0 + r.2))
        }
    }

    /// A slider's value (0..255) for canvas `x` on row `r`.
    fn kit_slider_value(&self, r: Rect) -> Option<i32> {
        let (x, _) = self.cursor?;
        let span = Self::kit_value_at(r).1;
        let (a, b) = (span.0 + 18.0, span.1 - 18.0);
        Some((((x - a) / (b - a)) * 255.0).clamp(0.0, 255.0) as i32)
    }

    pub(super) fn update_kit_options(&mut self, input: &Input) {
        if input.pressed(KeyCode::Escape) || (input.clicked && inside(self.cursor, KIT_BACK)) {
            self.store_me();
            self.pc.store();
            self.screen = Screen::Main;
            return;
        }
        if !input.mouse_down {
            self.drag = None;
        }
        self.scroll_game_options(input, true);
        let (rows, _) = self.kit_rows();
        if input.clicked {
            if let Some(t) = Self::kit_tabs()
                .iter()
                .position(|&r| inside(self.cursor, r))
            {
                self.opt_tab = t;
                self.clear_armed = false;
                return;
            }
            if inside(self.cursor, OPT_RESET) {
                let all = self
                    .kit_all_rows()
                    .into_iter()
                    .map(|r| (r, (0.0, 0.0, 0.0, 0.0)))
                    .collect::<Vec<_>>();
                self.kit_reset(&all);
                return;
            }
            let on_clear = rows.iter().any(|(o, r)| {
                matches!(o, KitRow::Opt(Opt::ClearTrophies)) && inside(self.cursor, *r)
            });
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
            for (o, r) in &rows {
                if !inside(self.cursor, *r) {
                    continue;
                }
                match *o {
                    KitRow::Key(i) => self.binding = Some(i),
                    KitRow::Opt(o @ (Opt::Slider(..) | Opt::PcSlider(..))) => self.drag = Some(o),
                    KitRow::Opt(Opt::Toggle(_, _, f)) => {
                        if let Some(s) = self.me_mut() {
                            let v = f(s);
                            *v = i32::from(*v == 0);
                        }
                    }
                    KitRow::Opt(Opt::PcToggle(_, f)) => {
                        let v = f(&mut self.pc);
                        *v = i32::from(*v == 0);
                        self.offer_animals();
                    }
                    KitRow::Opt(Opt::ClearTrophies) => {}
                }
            }
        }
        // The dragged slider's row.
        let name = match self.drag {
            Some(Opt::PcSlider(n, _) | Opt::Slider(n, _)) => Some(n),
            _ => None,
        };
        let r = rows.iter().find_map(|(o, r)| match o {
            KitRow::Opt(Opt::PcSlider(n, _) | Opt::Slider(n, _)) if Some(*n) == name => Some(*r),
            _ => None,
        });
        let v = r.and_then(|r| self.kit_slider_value(r));
        match (self.drag, v) {
            (Some(Opt::PcSlider(_, f)), Some(v)) => *f(&mut self.pc) = v,
            (Some(Opt::Slider(_, f)), Some(v)) => {
                if let Some(s) = self.me_mut() {
                    *f(s) = v;
                }
            }
            _ => {}
        }
    }

    /// The tab's settings back to a new hunter's (keys too).
    fn kit_reset(&mut self, rows: &[(KitRow, Rect)]) {
        let mut pc_default = PcSettings::default();
        if let Some(s) = self.me_mut() {
            let mut d = s.clone();
            d.options = c2_core::profile::default_options();
            for (r, _) in rows {
                match *r {
                    KitRow::Opt(Opt::Slider(_, f) | Opt::Toggle(_, _, f)) => *f(s) = *f(&mut d),
                    KitRow::Key(i) => {
                        let f = ACTIONS[i].1;
                        *f(&mut s.options.keys) = *f(&mut d.options.keys);
                    }
                    _ => {}
                }
            }
        }
        for (r, _) in rows {
            if let KitRow::Opt(Opt::PcSlider(_, f) | Opt::PcToggle(_, f)) = *r {
                *f(&mut self.pc) = *f(&mut pc_default);
            }
        }
        self.offer_animals();
    }

    pub(super) fn draw_kit_options(&self, out: &mut Vec<Sprite>) {
        self.draw_game_scroll(out, true);
        use super::kit::{PALE, RED, STONE};
        let Some(s) = self.me() else { return };
        let mut copy = s.clone();
        let mut pc = self.pc;
        let dim = [0.55, 0.55, 0.53, 1.0];
        for (i, (r, name)) in Self::kit_tabs().iter().zip(OPT_TABS).enumerate() {
            let on = i == self.opt_tab;
            let c = if on || inside(self.cursor, *r) {
                PALE
            } else {
                dim
            };
            let mid = r.0 + r.2 / 2.0;
            let w = self.kit_line(out, Face::Body, (mid, r.1 + 6.0), 26.0, c, name, 0.5);
            if on {
                self.fill(out, (mid - w / 2.0, r.1 + 34.0, w, 3.0), RED);
            }
        }
        let (rows, px) = self.kit_rows();
        for (row, r) in rows {
            let hover = inside(self.cursor, r);
            let lc = if hover { PALE } else { STONE };
            let ty = r.1 + (r.3 - px) / 2.0;
            let (vx, span) = Self::kit_value_at(r);
            let lx = if r.2 > 500.0 { OPT_LABEL_X } else { r.0 + 10.0 };
            // Smaller when it would run into its value (a narrow column).
            let room = span.0 - lx - 10.0;
            let label = |out: &mut Vec<Sprite>, t: &str| {
                let t = t.to_uppercase();
                let w = self
                    .kit
                    .as_ref()
                    .and_then(|k| k.body.as_ref())
                    .map_or(0.0, |f| f.width(px, &t));
                let lp = if w > room { px * room / w } else { px };
                let y = ty + (px - lp) / 2.0;
                self.kit_line(out, Face::Body, (lx, y), lp, lc, &t, 0.0);
            };
            let value = |out: &mut Vec<Sprite>, t: &str, c: [f32; 4]| {
                self.kit_line(out, Face::Body, (vx, ty), px, c, &t.to_uppercase(), 0.5);
            };
            let slider = |out: &mut Vec<Sprite>, v: i32| {
                let (a, b) = span;
                self.kit_line(
                    out,
                    Face::Body,
                    (a, ty - px * 0.1),
                    px * 1.1,
                    STONE,
                    "[",
                    0.0,
                );
                self.kit_line(
                    out,
                    Face::Body,
                    (b, ty - px * 0.1),
                    px * 1.1,
                    STONE,
                    "]",
                    1.0,
                );
                let (a, b) = (a + 18.0, b - 18.0);
                let cy = r.1 + r.3 / 2.0;
                let d = (px * 0.11).max(2.0);
                let n = if b - a > 200.0 { 18 } else { 9 };
                for k in 0..n {
                    let x = a + (b - a) * k as f32 / (n - 1) as f32;
                    self.fill(out, (x - d / 2.0, cy - d / 2.0, d, d), STONE);
                }
                // The knob: grey in a red frame.
                let x = a + (b - a) * v.clamp(0, 255) as f32 / 255.0;
                let (kw, kh) = (px * 0.6, px * 1.15);
                self.fill(out, (x - kw / 2.0, cy - kh / 2.0, kw, kh), RED);
                let t = 2.0;
                self.fill(
                    out,
                    (
                        x - kw / 2.0 + t,
                        cy - kh / 2.0 + t,
                        kw - 2.0 * t,
                        kh - 2.0 * t,
                    ),
                    [0.12, 0.12, 0.12, 1.0],
                );
                self.fill(
                    out,
                    (x - 1.5, cy - kh / 2.0 + t + 1.0, 3.0, kh - 2.0 * t - 2.0),
                    STONE,
                );
            };
            match row {
                KitRow::Opt(Opt::Slider(n, f)) => {
                    label(out, n);
                    slider(out, *f(&mut copy));
                }
                KitRow::Opt(Opt::PcSlider(n, f)) => {
                    let v = *f(&mut pc);
                    if n == "Total number of secret dinos on map" {
                        let count = if v == 0 {
                            "Off (selected dinos only)".into()
                        } else {
                            v.to_string()
                        };
                        label(out, &format!("{n}: {count}"));
                    } else {
                        label(out, n);
                    }
                    slider(out, v);
                }
                KitRow::Opt(Opt::Toggle(n, labels, f)) => {
                    label(out, n);
                    value(out, labels[usize::from(*f(&mut copy) != 0)], PALE);
                }
                KitRow::Opt(Opt::PcToggle(n, f)) => {
                    label(out, n);
                    value(out, ["Off", "On"][usize::from(*f(&mut pc) != 0)], PALE);
                }
                KitRow::Opt(Opt::ClearTrophies) => {
                    label(out, "Remove unlocked trophies");
                    if self.clear_armed {
                        let n = copy
                            .bodies
                            .iter()
                            .filter(|b| b.ctype != 0 && !b.locked())
                            .count();
                        value(out, &format!("Click again: remove {n}"), RED);
                    }
                }
                KitRow::Key(i) => {
                    let (n, f) = ACTIONS[i];
                    label(out, n);
                    if self.binding == Some(i) {
                        value(out, "Press a key...", RED);
                    } else {
                        value(out, &keys::name(*f(&mut copy.options.keys)), PALE);
                    }
                }
            }
        }
        // Back, and the tab's defaults.
        let hover = inside(self.cursor, KIT_BACK);
        if let Some(id) = self
            .mobile
            .as_ref()
            .and_then(|m| m.kit.as_ref())
            .and_then(|k| k.plank)
        {
            let tint = if hover {
                [1.3, 1.3, 1.25, 1.0]
            } else {
                [1.0; 4]
            };
            self.tinted(out, id, KIT_BACK, [0.0, 0.0, 1.0, 1.0], tint);
        }
        let r = KIT_BACK;
        self.kit_line(
            out,
            Face::Title,
            (r.0 + r.2 / 2.0, r.1 + (r.3 - 28.0) / 2.0),
            28.0,
            STONE,
            "BACK",
            0.5,
        );
        let c = if inside(self.cursor, OPT_RESET) {
            PALE
        } else {
            STONE
        };
        self.kit_line(
            out,
            Face::Body,
            (KIT_RIGHT, OPT_RESET.1 + 6.0),
            26.0,
            c,
            "[RESET TO DEFAULTS]",
            1.0,
        );
    }
}
