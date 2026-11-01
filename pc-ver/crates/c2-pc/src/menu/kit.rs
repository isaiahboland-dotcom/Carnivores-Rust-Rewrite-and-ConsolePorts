//! Menus drawn as the phone game's, for a game built from the player's APK
//! (`c2-pc mobile-base`): its interface art and fonts are kept in
//! `HUNTDAT/MOBILE` (atlases, `SPRITES.CFG`, BMFont `.FNT`s). Each board
//! (background, lit buttons, hit map) is composed from that art where the
//! PC's frames would be read, so the menus' logic is the same; the buttons'
//! words are drawn over them in the game's own lettering.
//!
//! Where the phone puts things lives in its code: these layouts are by eye,
//! on the PC boards' button places (the menus' texts are drawn there).

use super::*;
use c2_formats::mobile_ui::Sprite as KitSprite;
use std::collections::BTreeMap;

/// An RGBA picture.
#[derive(Clone)]
struct Pic {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

fn decode_png(data: &[u8]) -> Option<Pic> {
    let mut d = png::Decoder::new(std::io::Cursor::new(data));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = info.color_type.samples();
    let mut px = Vec::with_capacity(w * h * 4);
    for p in buf[..w * h * ch].chunks(ch) {
        px.extend_from_slice(&match ch {
            4 => [p[0], p[1], p[2], p[3]],
            3 => [p[0], p[1], p[2], 255],
            2 => [p[0], p[0], p[0], p[1]],
            _ => [p[0], p[0], p[0], 255],
        });
    }
    Some(Pic { w, h, px })
}

/// A font: its glyphs, its atlas (a HUD image) and atlas pixels per unit.
pub(crate) struct KitFont {
    font: c2_formats::bmfont::Font,
    image: usize,
    k: f32,
    aw: f32,
    ah: f32,
}

pub(crate) struct Kit {
    dir: PathBuf,
    sprites: BTreeMap<String, KitSprite>,
    atlases: std::cell::RefCell<BTreeMap<String, Option<Pic>>>,
    /// Headings and buttons (Lithos), body text, and the setup pages' red
    /// headlines (FieryTurk).
    pub(crate) title: Option<KitFont>,
    pub(crate) body: Option<KitFont>,
    pub(crate) heading: Option<KitFont>,
}

/// The menus' gold, as the phone game's buttons.
pub(crate) const GOLD: [f32; 4] = [0.96, 0.82, 0.45, 1.0];
pub(crate) const LIGHT: [f32; 4] = [1.0, 0.95, 0.8, 1.0];
/// The phone's button words, its descriptions, and its headlines.
pub(crate) const STONE: [f32; 4] = [0.8, 0.79, 0.76, 1.0];
pub(crate) const PALE: [f32; 4] = [0.86, 0.85, 0.82, 1.0];
pub(crate) const RED: [f32; 4] = [0.8, 0.05, 0.08, 1.0];

impl Kit {
    pub(crate) fn load(huntdat: &Path, hud: &mut Hud, gpu: &Gpu) -> Option<Self> {
        let dir = find_file(huntdat, "MOBILE")?;
        let cfg = std::fs::read(find_file(&dir, "SPRITES.CFG")?).ok()?;
        let mut sprites = BTreeMap::new();
        c2_formats::mobile_ui::parse(&String::from_utf8_lossy(&cfg), &mut sprites);
        let mut font = |name: &str| -> Option<KitFont> {
            let text = std::fs::read(find_file(&dir, name)?).ok()?;
            let font = c2_formats::bmfont::parse(&String::from_utf8_lossy(&text));
            let pic = decode_png(&std::fs::read(find_file(&dir, &font.page)?).ok()?)?;
            let image = hud.add_image_filtered(gpu, pic.w as u32, pic.h as u32, &pic.px, true);
            Some(KitFont {
                k: pic.w as f32 / font.scale_w.max(1) as f32,
                aw: pic.w as f32,
                ah: pic.h as f32,
                font,
                image,
            })
        };
        let title = font("LITH18.FNT");
        // The smaller has the pips of the descriptions' ratings (`=`, `{`,
        // `^`: full, half, empty), as the phone draws them.
        let body = font("OFS13.FNT").or_else(|| font("OFS15.FNT"));
        let heading = font("CCRA14.FNT");
        (!sprites.is_empty()).then(|| Self {
            dir,
            sprites,
            atlases: Default::default(),
            title,
            body,
            heading,
        })
    }

    /// A sprite's pixels.
    fn sprite(&self, name: &str) -> Option<Pic> {
        let s = self.sprites.get(name)?;
        let mut atlases = self.atlases.borrow_mut();
        let atlas = atlases
            .entry(s.texture.clone())
            .or_insert_with(|| {
                let f = find_file(&self.dir, &format!("{}.PNG", s.texture))?;
                decode_png(&std::fs::read(f).ok()?)
            })
            .as_ref()?;
        let (x0, y0) = (s.x.round() as usize, s.y.round() as usize);
        let (w, h) = (s.w.round() as usize, s.h.round() as usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in y0..y0 + h {
            let i = (y.min(atlas.h.saturating_sub(1)) * atlas.w + x0) * 4;
            px.extend_from_slice(atlas.px.get(i..i + w * 4)?);
        }
        Some(Pic { w, h, px })
    }

    /// A picture of its own in `MOBILE` (`MAPS/AREA1_DAY.PNG`,
    /// `COMPAS.PNG`), as a HUD image.
    pub(crate) fn picture(&self, rel: &str, hud: &mut Hud, gpu: &Gpu) -> Option<usize> {
        let mut p = self.dir.clone();
        for part in rel.split('/') {
            p = find_file(&p, part)?;
        }
        let pic = decode_png(&std::fs::read(p).ok()?)?;
        Some(hud.add_image_filtered(gpu, pic.w as u32, pic.h as u32, &pic.px, true))
    }

    /// A sprite as a HUD image.
    pub(crate) fn image(&self, name: &str, hud: &mut Hud, gpu: &Gpu) -> Option<usize> {
        let p = self.sprite(name)?;
        Some(hud.add_image_filtered(gpu, p.w as u32, p.h as u32, &p.px, true))
    }
}

/// An 800 x 600 picture being composed.
struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            px: vec![0; CW as usize * CH as usize * 4],
        }
    }

    /// `p` scaled into `r` (bilinear), over what's there.
    fn blit(&mut self, p: &Pic, r: Rect, gain: f32) {
        let (cw, ch) = (CW as i32, CH as i32);
        let (x0, y0) = (r.0.round() as i32, r.1.round() as i32);
        let (w, h) = (r.2.round().max(1.0) as i32, r.3.round().max(1.0) as i32);
        for y in y0.max(0)..(y0 + h).min(ch) {
            for x in x0.max(0)..(x0 + w).min(cw) {
                let fx = ((x - x0) as f32 + 0.5) * p.w as f32 / w as f32 - 0.5;
                let fy = ((y - y0) as f32 + 0.5) * p.h as f32 / h as f32 - 0.5;
                let (ix, iy) = (fx.floor().max(0.0) as usize, fy.floor().max(0.0) as usize);
                let (tx, ty) = (
                    (fx - ix as f32).clamp(0.0, 1.0),
                    (fy - iy as f32).clamp(0.0, 1.0),
                );
                let at = |x: usize, y: usize| {
                    let i = (y.min(p.h - 1) * p.w + x.min(p.w - 1)) * 4;
                    [p.px[i], p.px[i + 1], p.px[i + 2], p.px[i + 3]].map(f32::from)
                };
                let (a, b, c, d) = (
                    at(ix, iy),
                    at(ix + 1, iy),
                    at(ix, iy + 1),
                    at(ix + 1, iy + 1),
                );
                let mut s = [0.0f32; 4];
                for k in 0..4 {
                    let top = a[k] * (1.0 - tx) + b[k] * tx;
                    let bot = c[k] * (1.0 - tx) + d[k] * tx;
                    s[k] = top * (1.0 - ty) + bot * ty;
                }
                let al = s[3] / 255.0;
                let i = ((y * cw + x) * 4) as usize;
                for (d, &sk) in self.px[i..i + 3].iter_mut().zip(&s[..3]) {
                    let v = (sk * gain).min(255.0);
                    *d = (v * al + f32::from(*d) * (1.0 - al)) as u8;
                }
                self.px[i + 3] = (al * 255.0 + f32::from(self.px[i + 3]) * (1.0 - al)) as u8;
            }
        }
    }

    /// A translucent fill.
    fn shade(&mut self, r: Rect, rgba: [u8; 4]) {
        let p = Pic {
            w: 1,
            h: 1,
            px: rgba.to_vec(),
        };
        self.blit(&p, r, 1.0);
    }

    /// The part of the canvas in `r`, as an image.
    fn cut(&self, r: Rect) -> Pic {
        let (x0, y0, w, h) = (r.0 as usize, r.1 as usize, r.2 as usize, r.3 as usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let i = (y.min(CH as usize - 1) * CW as usize + x.min(CW as usize - 1)) * 4;
                px.extend_from_slice(&self.px[i..i + 4]);
            }
        }
        Pic { w, h, px }
    }
}

/// What a board is made of.
pub(super) enum Part<'a> {
    /// The picture behind it all, filling the window (drawn apart: the
    /// board is the 800 x 600 middle of a wider screen).
    Back(&'a str),
    /// A sprite filling a rectangle.
    Art(&'a str, Rect),
    /// A darkened panel.
    Panel(Rect),
    /// A button: its region, rectangle, sprite and word.
    Button(u8, Rect, &'a str, &'a str),
    /// Words (heading size when `true`), placed at `x`, `y`.
    Words(f32, f32, &'a str, bool),
}

impl Kit {
    /// A board from `parts`, in order.
    pub(super) fn board(&self, hud: &mut Hud, gpu: &Gpu, parts: &[Part]) -> Board {
        let mut c = Canvas::new();
        let mut hit = vec![0u8; 400 * 300];
        let mut buttons = Vec::new();
        let mut labels = Vec::new();
        let (mut back, mut dim) = (None, false);
        let mark = |hit: &mut Vec<u8>, id: u8, r: Rect| {
            for y in (r.1 / 2.0) as usize..((r.1 + r.3) / 2.0).min(300.0) as usize {
                for x in (r.0 / 2.0) as usize..((r.0 + r.2) / 2.0).min(400.0) as usize {
                    hit[y * 400 + x] = id;
                }
            }
        };
        for p in parts {
            match *p {
                Part::Back(name) => back = self.image(name, hud, gpu),
                Part::Art(name, r) => {
                    if let Some(s) = self.sprite(name) {
                        c.blit(&s, r, 1.0);
                    }
                }
                Part::Panel(r) => {
                    // A question over it all: the words under it go.
                    if r == FULL {
                        dim = true;
                        labels.clear();
                    }
                    c.shade(r, [0, 0, 0, 150]);
                }
                Part::Button(id, r, sprite, word) => {
                    if let Some(s) = self.sprite(sprite) {
                        c.blit(&s, r, 1.0);
                    }
                    mark(&mut hit, id, r);
                    buttons.push((id, r, sprite));
                    labels.push(Label {
                        rect: r,
                        text: word.to_string(),
                        title: true,
                        centred: true,
                    });
                }
                Part::Words(x, y, text, title) => labels.push(Label {
                    rect: (x, y, 0.0, 0.0),
                    text: text.to_string(),
                    title,
                    centred: false,
                }),
            }
        }
        let base = hud.add_image_filtered(gpu, CW as u32, CH as u32, &c.px, true);
        // Lit: the button brighter, cut from a canvas with it redrawn.
        let mut lit = vec![None; 256];
        for (id, r, sprite) in buttons {
            let mut on = Canvas { px: c.px.clone() };
            if let Some(s) = self.sprite(sprite) {
                on.blit(&s, r, 1.45);
            }
            let r = (
                r.0.max(0.0).floor(),
                r.1.max(0.0).floor(),
                r.2.ceil(),
                r.3.ceil(),
            );
            let cut = on.cut(r);
            let img = hud.add_image_filtered(gpu, cut.w as u32, cut.h as u32, &cut.px, true);
            lit[usize::from(id)] = Some((img, r));
        }
        Board {
            base: Some(base),
            lit,
            hit,
            labels,
            back,
            dim,
        }
    }
}

/// Words a composed board shows.
#[derive(Debug, Clone)]
pub(super) struct Label {
    pub rect: Rect,
    pub text: String,
    pub title: bool,
    pub centred: bool,
}

impl KitFont {
    /// Width of `text` at `px` tall.
    pub(crate) fn width(&self, px: f32, text: &str) -> f32 {
        self.font.width(text) as f32 * px / self.font.line_height.max(1) as f32
    }

    /// `text` at canvas-space `x`, `y` (its top), `px` tall: screen sprites
    /// for scale `s` and origin `ox`, `oy`.
    pub(crate) fn sprites(
        &self,
        out: &mut Vec<Sprite>,
        (s, ox, oy): (f32, f32, f32),
        (x, y): (f32, f32),
        px: f32,
        color: [f32; 4],
        text: &str,
    ) {
        let f = &self.font;
        let unit = px / f.line_height.max(1) as f32;
        let mut cx = x;
        let mut prev = None;
        for c in text.chars().map(u32::from) {
            if let Some(p) = prev {
                cx += f.kerning.get(&(p, c)).copied().unwrap_or(0) as f32 * unit;
            }
            prev = Some(c);
            let Some(g) = f.glyphs.get(&c) else {
                cx += px / 3.0;
                continue;
            };
            if g.w > 0 && g.h > 0 {
                let (u0, v0) = (g.x as f32 * self.k / self.aw, g.y as f32 * self.k / self.ah);
                let (u1, v1) = (
                    (g.x + g.w) as f32 * self.k / self.aw,
                    (g.y + g.h) as f32 * self.k / self.ah,
                );
                let gx = cx + g.x_offset as f32 * unit;
                let gy = y + g.y_offset as f32 * unit;
                out.push(Sprite {
                    uv: [u0, v0, u1, v1],
                    size: Some([g.w as f32 * unit * s, g.h as f32 * unit * s]),
                    color,
                    ..Sprite::new(self.image, ox + gx * s, oy + gy * s, 1.0)
                });
            }
            cx += g.advance as f32 * unit;
        }
    }
}

/// The menus' boards, composed.
pub(super) struct Boards {
    pub main: Board,
    pub more: Board,
    pub stats: Board,
    pub quit: Board,
    pub reg: Board,
    pub waiver: Board,
    pub delete: Board,
    pub options: Board,
    pub credits: Board,
    pub loading: Board,
}

const FULL: Rect = (0.0, 0.0, CW, CH);

impl Kit {
    pub(super) fn boards(&self, hud: &mut Hud, gpu: &Gpu, title: &str, survive: bool) -> Boards {
        use Part::*;
        // The main menu as the phone's: its picture, the logo, four
        // buttons (hunt, survive when offered, trophy, more); the rest
        // under MORE.
        let mut main_parts: Vec<Part> = vec![
            Back("menu_main_background"),
            Art("menu_main_logo", (10.0, 44.0, 588.0, 156.0)),
            Art("menu_main_pro", (590.0, 168.0, 92.0, 50.0)),
        ];
        let buttons = [(1, "HUNT"), (7, "SURVIVE"), (3, "TROPHY"), (8, "MORE")];
        for (i, &(id, w)) in buttons.iter().filter(|b| survive || b.0 != 7).enumerate() {
            let y = 254.0 + i as f32 * 73.0;
            main_parts.push(Button(id, (205.0, y, 234.0, 58.0), "button_med_01", w));
        }
        let main = self.board(hud, gpu, &main_parts);
        // More: the hunter (name, points, statistics), options, credits,
        // quit; back to the main menu.
        let more_parts = |extra: &mut Vec<Part<'static>>| {
            let mut p: Vec<Part<'static>> = vec![
                Back("menu_more_background"),
                Panel((8.0, 6.0, 300.0, 34.0)),
                Words(16.0, 13.0, "NAME", false),
                Panel((380.0, 6.0, 200.0, 34.0)),
                Words(388.0, 13.0, "POINTS", false),
            ];
            for (i, (id, w)) in [
                (6, "STATISTICS"),
                (2, "OPTIONS"),
                (4, "CREDITS"),
                (5, "QUIT"),
                (9, "BACK"),
            ]
            .into_iter()
            .enumerate()
            {
                let y = 120.0 + i as f32 * 73.0;
                p.push(Button(id, (205.0, y, 234.0, 58.0), "button_med_01", w));
            }
            p.append(extra);
            p
        };
        let more = self.board(hud, gpu, &more_parts(&mut Vec::new()));
        let stats = self.board(
            hud,
            gpu,
            &more_parts(&mut vec![
                Panel((532.0, 50.0, 260.0, 268.0)),
                Words(548.0, 54.0, "LAST HUNT", true),
                Words(548.0, 184.0, "OVERALL", true),
            ]),
        );
        let mut quit_parts = main_parts;
        quit_parts.append(&mut vec![
            Panel(FULL),
            Art("base_popup_background", (250.0, 270.0, 300.0, 124.0)),
            Words(296.0, 290.0, "QUIT GAME?", true),
            Button(1, (270.0, 344.0, 128.0, 38.0), "button_small_01", "YES"),
            Button(2, (402.0, 344.0, 128.0, 38.0), "button_small_01", "NO"),
        ]);
        let quit = self.board(hud, gpu, &quit_parts);
        // Registration: on the phone's second picture, a panel with the
        // name field and the hunters.
        let reg_parts = |extra: &mut Vec<Part<'static>>| {
            let mut p: Vec<Part<'static>> = vec![
                Back("menu_more_background"),
                Art("menu_main_logo_small", (230.0, 40.0, 340.0, 102.0)),
                Art("base_popup_background", (270.0, 240.0, 260.0, 304.0)),
                Words(296.0, 262.0, "PLEASE REGISTER", true),
                Panel((300.0, 316.0, 200.0, 26.0)),
                Panel((300.0, 362.0, 200.0, 126.0)),
                Button(1, (290.0, 496.0, 108.0, 36.0), "button_small_01", "OK"),
                Button(2, (402.0, 496.0, 108.0, 36.0), "button_small_01", "DELETE"),
            ];
            p.append(extra);
            p
        };
        let reg = self.board(hud, gpu, &reg_parts(&mut Vec::new()));
        let delete = self.board(
            hud,
            gpu,
            &reg_parts(&mut vec![
                Panel(FULL),
                Art("base_popup_background", (262.0, 384.0, 278.0, 104.0)),
                Words(300.0, 400.0, "DELETE THIS HUNTER?", false),
                Button(1, (270.0, 444.0, 130.0, 36.0), "button_small_01", "DELETE"),
                Button(2, (402.0, 444.0, 130.0, 36.0), "button_small_01", "CANCEL"),
            ]),
        );
        let waiver = self.board(
            hud,
            gpu,
            &reg_parts(&mut vec![
                Panel(FULL),
                Art("base_popup_background", (180.0, 330.0, 440.0, 210.0)),
                Words(350.0, 344.0, "WAIVER", true),
                Words(
                    204.0,
                    388.0,
                    "Hunting dinosaurs is dangerous. I hunt at my own",
                    false,
                ),
                Words(
                    204.0,
                    408.0,
                    "risk, and I accept the reserve's rules.",
                    false,
                ),
                Button(1, (192.0, 494.0, 206.0, 38.0), "button_small_01", "ACCEPT"),
                Button(2, (402.0, 494.0, 206.0, 38.0), "button_small_01", "DECLINE"),
            ]),
        );
        // Options: the phone's options picture; its list is drawn over it
        // (`mobile::draw_kit_options`).
        let options = self.board(hud, gpu, &[Back("menu_options_background")]);
        let credits = self.board(
            hud,
            gpu,
            &[
                Back("menu_about_background"),
                Art("menu_main_logo", (200.0, 40.0, 400.0, 120.0)),
                Panel((150.0, 210.0, 500.0, 230.0)),
                Words(176.0, 228.0, "Built from your own copy of", false),
                Words(176.0, 252.0, title, true),
                Words(176.0, 300.0, "Carnivores: Tatem Games, Action Forms", false),
                Words(176.0, 324.0, "This port: c2rs", false),
                Words(176.0, 380.0, "Click to go back", false),
            ],
        );
        // While a hunt loads: the phone's picture and logo.
        let loading = self.board(
            hud,
            gpu,
            &[
                Back("menu_main_background"),
                Art("menu_main_logo", (200.0, 120.0, 400.0, 120.0)),
                Words(330.0, 420.0, "LOADING...", true),
            ],
        );
        Boards {
            main,
            more,
            stats,
            quit,
            reg,
            waiver,
            delete,
            options,
            credits,
            loading,
        }
    }
}

/// The hunt setup's pieces from the kit.
pub(super) struct KitArt {
    pub bg: Option<usize>,
    /// A hexagon's plate, and the ring on a picked one.
    pub cell: Option<usize>,
    pub selector: Option<usize>,
    /// Buttons: with the lamp off and on, and plain.
    pub lamp_off: Option<usize>,
    pub lamp_on: Option<usize>,
    pub button: Option<usize>,
    /// The bottom bar's planks; the honeycomb's empty cells; the toggles'
    /// pictures (dawn, day, night, observer, tranquilizer, radar, cover
    /// scent, camouflage).
    pub plank: Option<usize>,
    pub empty: Option<usize>,
    pub icons: [Option<usize>; 8],
    /// Each page's items: (in colour, as a silhouette).
    pub items: [Vec<(Option<usize>, Option<usize>)>; 3],
}

impl Kit {
    /// A sprite scaled to `w` x `h`, as a HUD image.
    pub(super) fn image_sized(
        &self,
        name: &str,
        w: usize,
        h: usize,
        hud: &mut Hud,
        gpu: &Gpu,
    ) -> Option<usize> {
        let p = self.sprite(name)?;
        let mut c = Canvas {
            px: vec![0; w * h * 4],
        };
        // `blit` works on the 800-wide canvas; scale by hand here.
        for y in 0..h {
            for x in 0..w {
                let sx = (x * p.w / w.max(1)).min(p.w - 1);
                let sy = (y * p.h / h.max(1)).min(p.h - 1);
                let s = (sy * p.w + sx) * 4;
                c.px[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&p.px[s..s + 4]);
            }
        }
        Some(hud.add_image_filtered(gpu, w as u32, h as u32, &c.px, true))
    }

    /// The hunt setup's art; `counts` the items on each page (maps,
    /// animals, weapons), numbered as the phone's sprites (the weapons'
    /// 7th is the photo camera, so the revolver is 8th).
    pub(super) fn art(&self, hud: &mut Hud, gpu: &Gpu, counts: [usize; 3]) -> KitArt {
        let mut img = |n: &str| self.image(n, hud, gpu);
        let bg = img("menu_hunt_background");
        let cell = img("menu_hunt_cell_button");
        let selector = img("menu_hunt_cell_selector");
        let lamp_off = img("button_checkbox_wide_off");
        let lamp_on = img("button_checkbox_wide_on");
        let button = img("button_small_01");
        let plank = img("button_med_01");
        let empty = img("menu_hunt_cell_empty");
        let icons = [
            "dawn",
            "day",
            "night",
            "observer",
            "tranquilizer",
            "radar",
            "cover_scent",
            "camouflage",
        ]
        .map(|n| img(&format!("icon_{n}")));
        let mut items: [Vec<(Option<usize>, Option<usize>)>; 3] = Default::default();
        for (page, kind) in ["area", "dino", "weapon"].iter().enumerate() {
            for i in 0..counts[page] {
                let n = if page == 2 && i >= 6 { i + 2 } else { i + 1 };
                items[page].push((
                    img(&format!("button_{kind}_active_{n}")),
                    img(&format!("button_{kind}_inactive_{n}")),
                ));
            }
        }
        KitArt {
            bg,
            cell,
            selector,
            lamp_off,
            lamp_on,
            button,
            plank,
            empty,
            icons,
            items,
        }
    }
}
