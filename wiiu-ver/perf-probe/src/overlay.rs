//! What the hunt says over the world, as on the PC: the last kill's notice
//! (bottom right), the trophy plaque (`TROPHY_G.TGA`, or `TROPHY.TGA` in the
//! trophy room) with the trophy's record, messages ("Trophy!", survival
//! waves), a survival's wave and kills, and the questions waiting for A / B.
//! Drawn into regions of one small texture, each only when its text
//! changes; C shows the visible ones as quads over the TV and GamePad view.
use crate::menu::{Menu, AREAS, PIC_PLAQUE_HUNT, PIC_PLAQUE_ROOM, WEAPONS};
use crate::ui::{measure, rect, Canvas, Rect, Rgba, HEIGHT, WIDTH};
use alloc::{format, string::String, vec::Vec};
use c2_core::game::Hunt;

/// The overlay texture (pixels) and its scale: the TV's pixels per logical
/// unit, so the text is as sharp on the TV as the menus.
pub const TEX_W: usize = 1024;
pub const TEX_H: usize = 512;
const SCALE: f32 = 1.5;
/// Quads C may be asked to draw (`REGIONS`), 6 vertices of 4 floats each for
/// positions and as many for texture coordinates.
pub const MAX_QUADS: usize = 5;
pub const QUAD_FLOATS: usize = 24;

const PLAQUE: usize = 0;
const KILL: usize = 1;
const MESSAGE: usize = 2;
const PROMPT: usize = 3;
const STATUS: usize = 4;
/// Each region's place in the texture (logical units at `SCALE`).
const REGIONS: [Rect; MAX_QUADS] = [
    rect(0.0, 0.0, 280.0, 142.0),
    rect(290.0, 0.0, 392.0, 142.0),
    rect(0.0, 148.0, 682.0, 64.0),
    rect(0.0, 216.0, 682.0, 64.0),
    rect(0.0, 284.0, 260.0, 64.0),
];

const BOX: Rgba = [0, 0, 0, 178];
const GOLD: Rgba = [242, 217, 51, 255];
const LIGHT: Rgba = [230, 230, 230, 255];
const GREY: Rgba = [191, 191, 191, 255];
const YELLOW: Rgba = [191, 191, 0, 255];
const SHADOW: Rgba = [0, 0, 0, 255];

/// The last kill (6 s) and the last message (2 s), from the hunt's events.
#[derive(Default)]
pub struct Notice {
    kill: Option<(String, i32, f32, bool)>,
    kill_ms: i32,
    message: Option<String>,
    message_ms: i32,
    /// Leaving after the exit was confirmed (the original's 4 s).
    pub leaving: bool,
}

impl Notice {
    pub fn kill(&mut self, name: String, score: i32, range: f32, tranq: bool) {
        self.kill = Some((name, score, range, tranq));
        self.kill_ms = 6000;
    }
    pub fn message(&mut self, text: String, ms: i32) {
        self.message = Some(text);
        self.message_ms = ms;
    }
    pub fn tick(&mut self, dt: i32) {
        self.message_ms -= dt;
        if self.message_ms <= 0 {
            self.message = None;
        }
        self.kill_ms -= dt;
        if self.kill_ms <= 0 {
            self.kill = None;
        }
    }
}

/// The trophy room's question.
#[derive(Clone, Copy, PartialEq)]
pub enum Ask {
    Remove,
    Lock,
}

/// What each region shows now (`None`: hidden), to redraw only changes.
#[derive(Default)]
pub struct Overlay {
    shown: [Option<String>; MAX_QUADS],
}

/// The hunt state the overlay reads.
pub struct View<'a> {
    pub hunt: &'a Hunt,
    pub notice: &'a Notice,
    pub ask: Option<Ask>,
    pub units_us: bool,
    pub night: bool,
}

impl Overlay {
    /// Redraw the regions that changed into `c` (the whole texture) and
    /// write a quad per visible region into `pos` / `uv` (NDC, texture
    /// coordinates). Returns (quads, whether any pixel changed).
    pub fn draw(&mut self, menu: &Menu, v: &View, c: &mut Canvas, pos: &mut [f32], uv: &mut [f32]) -> (usize, bool) {
        let mut changed = false;
        let mut quads = 0;
        for region in 0..MAX_QUADS {
            let content = self.content(v, region);
            let key = content.as_ref().map(|(k, _)| k.clone());
            if key != self.shown[region] {
                let area = REGIONS[region];
                c.set_clip(None);
                c.clear(area);
                if let Some((_, lines)) = &content {
                    self.paint(menu, v, c, region, lines);
                }
                self.shown[region] = key;
                changed = true;
            }
            let Some((_, lines)) = &content else { continue };
            let (src, dst) = self.place(menu, v, region, lines);
            quad(&mut pos[quads * QUAD_FLOATS..], &mut uv[quads * QUAD_FLOATS..], src, dst);
            quads += 1;
        }
        (quads, changed)
    }

    /// A region's text: its key (everything drawn) and lines.
    fn content(&self, v: &View, region: usize) -> Option<(String, Vec<String>)> {
        let h = v.hunt;
        let lines: Vec<String> = match region {
            PLAQUE => plaque_lines(v, h.trophy_body?)?,
            // The plaque says it already: one notice a kill.
            KILL if h.trophy_body.is_some() && !h.opts.trophy => return None,
            KILL => {
                let (name, score, range, tranq) = v.notice.kill.as_ref()?;
                let dist = if v.units_us { format!("Range: {:.0} ft", range / 0.3) } else { format!("Range: {range:.0} m") };
                let points = if v.night { format!("+{score} points (night +10%)") } else { format!("+{score} points") };
                let how = if *tranq { "Tranquilized (+25%, no trophy)" } else { "Killed" };
                alloc::vec![name.clone(), points, dist, String::from(how)]
            }
            MESSAGE => alloc::vec![v.notice.message.clone()?],
            PROMPT => alloc::vec![prompt(v)?],
            STATUS => {
                let s = h.survival.as_ref()?;
                let wave = if s.wave == 0 { String::from("Get ready") } else { format!("Wave {}", s.wave) };
                alloc::vec![wave, format!("Kills: {}", h.kills.len())]
            }
            _ => return None,
        };
        let mut key = format!("{region}");
        for l in &lines {
            key.push('\u{1f}');
            key.push_str(l);
        }
        Some((key, lines))
    }

    fn paint(&self, menu: &Menu, v: &View, c: &mut Canvas, region: usize, lines: &[String]) {
        let r = REGIONS[region];
        let font = menu.font();
        match region {
            PLAQUE => {
                let pic = if v.hunt.opts.trophy { PIC_PLAQUE_ROOM } else { PIC_PLAQUE_HUNT };
                let (w, h) = plaque_size(menu, v);
                let k = w / 210.0;
                match menu.picture(pic) {
                    Some(img) => {
                        let src = rect(0.0, 0.0, img.w as f32, img.h as f32);
                        c.image_cached(&mut menu.tiles().borrow_mut(), 1000 + pic as u32, img, src, rect(r.x, r.y, w, h), crate::ui::Clip::None, 1.0)
                    }
                    None => {
                        c.fill(rect(r.x, r.y, w, h), [40, 32, 20, 255]);
                        c.frame(rect(r.x, r.y, w, h), 2.0, [120, 100, 60, 255]);
                    }
                }
                // Label grey, value yellow, a black shadow (`DrawTrophyText`).
                let avail = w - 28.0 * k;
                for (i, line) in lines.iter().enumerate() {
                    let base = 9.5 * k;
                    let plain: String = line.chars().filter(|&ch| ch != '\u{1}').collect();
                    let size = base.min(base * avail / measure(font, base, &plain).max(1.0));
                    let mut x = r.x + 14.0 * k;
                    let y = r.y + (11.0 + 16.0 * i as f32) * k;
                    // Parts alternate label / value, split by \u{1}.
                    for (n, part) in line.split('\u{1}').enumerate() {
                        let col = if n % 2 == 0 { GREY } else { YELLOW };
                        c.text(font, x + 1.0, y + 1.0, size, SHADOW, part);
                        x += c.text(font, x, y, size, col, part);
                    }
                }
            }
            KILL | STATUS => {
                let size = 20.0;
                let (bw, bh) = box_size(menu, lines, size);
                // Right-aligned in its region (the kill notice hugs the right
                // edge of the screen).
                let b = rect(r.x + r.w - bw, r.y, bw, bh);
                c.fill(b, BOX);
                for (i, l) in lines.iter().enumerate() {
                    let col = if region == STATUS || i == 1 { GOLD } else { LIGHT };
                    c.text(font, b.x + 12.0, b.y + 8.0 + i as f32 * 26.0, size, col, l);
                }
            }
            _ => {
                let size = if region == MESSAGE { 34.0 } else { 24.0 };
                let w = measure(font, size, &lines[0]).min(r.w - 24.0) + 32.0;
                let b = rect(r.x + (r.w - w) * 0.5, r.y + 4.0, w, r.h - 8.0);
                c.fill(b, BOX);
                let col = if region == MESSAGE || v.ask.is_some() || v.hunt.exit_prompt || v.hunt.paused { GOLD } else { LIGHT };
                c.text_centered(font, b, size, col, &lines[0]);
            }
        }
    }

    /// Where a visible region's pixels are (texture, logical units at
    /// `SCALE`) and where they go on the screen (logical 1280x720).
    fn place(&self, menu: &Menu, v: &View, region: usize, lines: &[String]) -> (Rect, Rect) {
        let r = REGIONS[region];
        match region {
            PLAQUE => {
                let (w, h) = plaque_size(menu, v);
                let src = rect(r.x, r.y, w, h);
                let x = if v.hunt.opts.trophy { WIDTH - w - 16.0 } else { (WIDTH - w) * 0.5 };
                (src, rect(x, HEIGHT - h - 12.0, w, h))
            }
            KILL | STATUS => {
                let (bw, bh) = box_size(menu, lines, 20.0);
                let src = rect(r.x + r.w - bw, r.y, bw, bh);
                // The notice above the corner HUD (weapon and ammo); the
                // survival's wave top right.
                let y = if region == KILL { HEIGHT * 0.87 - bh - 8.0 } else { 16.0 };
                (src, rect(WIDTH - bw - 16.0, y, bw, bh))
            }
            _ => {
                let y = if region == MESSAGE { HEIGHT * 0.25 - r.h * 0.5 } else { HEIGHT * 0.40 };
                (r, rect((WIDTH - r.w) * 0.5, y, r.w, r.h))
            }
        }
    }
}

fn box_size(menu: &Menu, lines: &[String], size: f32) -> (f32, f32) {
    let w = lines.iter().map(|l| measure(menu.font(), size, l)).fold(0.0, f32::max) + 24.0;
    (w.min(REGIONS[KILL].w), lines.len() as f32 * 26.0 + 14.0)
}

/// The plaque picture's size on the screen: the original's 210 wide
/// (`TROPHY_G.TGA` 210x106, `TROPHY.TGA` 210x124), a third bigger.
fn plaque_size(menu: &Menu, v: &View) -> (f32, f32) {
    let pic = if v.hunt.opts.trophy { PIC_PLAQUE_ROOM } else { PIC_PLAQUE_HUNT };
    let (w, h) = menu.picture(pic).map_or((210.0, 106.0), |i| (i.w as f32, i.h as f32));
    let k = 280.0 / w;
    (280.0, (h * k).min(REGIONS[PLAQUE].h))
}

/// `DrawTrophyText`: the trophy's record, label and value parts split by
/// `\u{1}`.
fn plaque_lines(v: &View, slot: usize) -> Option<Vec<String>> {
    let b = v.hunt.room.bodies.get(slot)?;
    let info = v.hunt.herd.species.get(usize::try_from(b.ctype).ok()?).map(|s| &s.info)?;
    let weapon = usize::try_from(b.weapon).ok().and_then(|w| WEAPONS.get(w)).copied().unwrap_or("");
    let (sc, us) = (b.scale, v.units_us);
    let (weight, len, range) = if us {
        (format!("{:.2}t", info.mass * sc * sc / 0.907), format!("{:.2}ft", info.length * sc / 0.3), format!("{:.1}ft", b.range / 0.3))
    } else {
        (format!("{:.2}T", info.mass * sc * sc), format!("{:.2}m", info.length * sc), format!("{:.1}m", b.range))
    };
    let (d, m, y) = (b.date & 255, (b.date >> 10) & 255, b.date >> 20);
    let date = if us { format!("{m}.{d}.{y}") } else { format!("{d}.{m}.{y}") };
    let time = format!("{}:{:02}", (b.time >> 10) & 255, b.time & 255);
    let lock = if b.locked() { "  (locked)" } else { "" };
    let area = usize::try_from(b.area() - 1).ok().and_then(|a| AREAS.get(a)).copied().unwrap_or("");
    Some(alloc::vec![
        format!("Name: \u{1}{}{lock}", info.name),
        format!("Weight: \u{1}{weight}  \u{1}Length: \u{1}{len}"),
        format!("Weapon: \u{1}{weapon}  \u{1}Score: \u{1}{}", b.score),
        format!("Range: \u{1}{range}  \u{1}Area: \u{1}{area}"),
        format!("Date: \u{1}{date}  \u{1}Time: \u{1}{time}"),
    ])
}

/// The question waiting for A / B, or the trophy room's controls.
fn prompt(v: &View) -> Option<String> {
    let h = v.hunt;
    if h.health <= 0 {
        return None;
    }
    if v.notice.leaving {
        return Some(String::from("Leaving the hunt..."));
    }
    if h.exit_prompt {
        return Some(String::from(if h.survival.is_some() { "End the survival?   A: yes   B: no   Y: restart" } else { "Leave the hunt?   A: yes   B: no   Y: restart" }));
    }
    if h.paused {
        return Some(String::from("Paused   D-pad Right: resume   +: leave"));
    }
    if !h.opts.trophy {
        return None;
    }
    let locked = h.trophy_body.and_then(|s| h.room.bodies.get(s)).is_some_and(|b| b.locked());
    Some(String::from(match v.ask {
        Some(Ask::Remove) => "Remove this trophy?   A: yes   B: no",
        Some(Ask::Lock) if locked => "Unlock this trophy?   A: yes   B: no",
        Some(Ask::Lock) => "Lock this trophy (it can't be removed)?   A: yes   B: no",
        None if h.trophy_body.is_some() => "ZL / ZR: move   X: remove   Y: lock   +: leave",
        None => "+: leave the trophy room",
    }))
}

// Six vertices: `src` (texture, logical units at SCALE) shown at `dst`
// (screen, logical 1280x720).
fn quad(pos: &mut [f32], uv: &mut [f32], src: Rect, dst: Rect) {
    let (x0, x1) = (dst.x / WIDTH * 2.0 - 1.0, (dst.x + dst.w) / WIDTH * 2.0 - 1.0);
    let (y0, y1) = (1.0 - dst.y / HEIGHT * 2.0, 1.0 - (dst.y + dst.h) / HEIGHT * 2.0);
    let (u0, u1) = (src.x * SCALE / TEX_W as f32, (src.x + src.w) * SCALE / TEX_W as f32);
    let (v0, v1) = (src.y * SCALE / TEX_H as f32, (src.y + src.h) * SCALE / TEX_H as f32);
    let corners = [(x0, y0, u0, v0), (x1, y0, u1, v0), (x1, y1, u1, v1), (x0, y0, u0, v0), (x1, y1, u1, v1), (x0, y1, u0, v1)];
    for (i, (x, y, u, v)) in corners.into_iter().enumerate() {
        pos[i * 4..i * 4 + 4].copy_from_slice(&[x, y, 0.0, 1.0]);
        uv[i * 4..i * 4 + 4].copy_from_slice(&[u, v, 1.0, 0.0]);
    }
}

/// A canvas over the overlay texture.
pub fn canvas(px: &mut [u8], pitch: usize) -> Canvas<'_> {
    Canvas::with_scale(px, pitch, TEX_W, TEX_H, SCALE)
}
