//! Menu drawing at the display's own resolution: a logical 1280x720 layout
//! scaled to the TV (1920x1080) or the GamePad (854x480). Independent of GX2,
//! so the host tests can render it.
use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _};
use alloc::{collections::BTreeMap, vec::Vec};
use core::cell::RefCell;

pub const WIDTH: f32 = 1280.0;
pub const HEIGHT: f32 = 720.0;

pub type Rgba = [u8; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
pub const fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}
impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
    pub fn inset(&self, d: f32) -> Rect {
        rect(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
}

/// An RGBA picture at its own size.
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}
impl Image {
    /// A decoded original menu picture (16-bit TGA), opaque.
    pub fn from_tga(bytes: &[u8]) -> Option<Image> {
        let pic = c2_formats::tga::parse(bytes).ok()?;
        if pic.width == 0 || pic.height == 0 {
            return None;
        }
        let px = pic.pixels.iter().map(|&p| c2_core::color::rgb555_to_rgba8(p, 255)).collect();
        Some(Image { w: pic.width, h: pic.height, px })
    }
    // Bilinear sample at source coordinates (pixel centres at +0.5).
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let x = (x - 0.5).clamp(0.0, (self.w - 1) as f32);
        let y = (y - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |x: usize, y: usize| self.px[y * self.w + x];
        let (a, b, c, d) = (p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y1));
        core::array::from_fn(|k| {
            let top = a[k] as f32 + (b[k] as f32 - a[k] as f32) * fx;
            let bottom = c[k] as f32 + (d[k] as f32 - c[k] as f32) * fx;
            top + (bottom - top) * fy
        })
    }
}

/// The console's system font, or the 8x8 bitmap font when none was given
/// (the emulator may have no system fonts).
pub enum Font {
    Ttf(FontRef<'static>, RefCell<Glyphs>),
    Bitmap,
}
impl Font {
    pub fn new(bytes: Option<&'static [u8]>) -> Font {
        bytes
            .and_then(|b| FontRef::try_from_slice(b).ok())
            .map_or(Font::Bitmap, |f| Font::Ttf(f, RefCell::new(Glyphs::default())))
    }
}

/// Rasterized glyphs, by (glyph, size in quarter pixels, quarter-pixel x
/// offset): outlines are rasterized once, then copied. Emptied past `LIMIT`.
#[derive(Default)]
pub struct Glyphs {
    map: BTreeMap<(u16, u32, u8), Coverage>,
}
impl Glyphs {
    const LIMIT: usize = 4096;
}
/// A glyph's coverage (0..=255) and its offset from the pen on the baseline.
pub struct Coverage {
    x: i32,
    y: i32,
    w: usize,
    cov: Vec<u8>,
}

/// Where a picture is drawn: everywhere in its rectangle, or only inside the
/// hexagon (pointy sides left and right) inscribed in the given rectangle.
#[derive(Clone, Copy, PartialEq)]
pub enum Clip {
    None,
    Hex(Rect),
}

pub fn in_hex(r: &Rect, x: f32, y: f32) -> bool {
    let (cx, cy) = r.center();
    let (dx, dy) = ((x - cx).abs(), (y - cy).abs());
    let (hw, hh) = (r.w * 0.5, r.h * 0.5);
    dy <= hh && dx <= hw - dy * (hw * 0.5) / hh
}

/// A picture already scaled to a canvas: its pixels over a span, alpha =
/// coverage.
pub struct Tile {
    x0: usize,
    y0: usize,
    w: usize,
    px: Vec<Rgba>,
}

/// Scaled pictures kept between redraws (the menu's pictures rarely change
/// while the focus moves). Emptied when it grows past `LIMIT` bytes.
#[derive(Default)]
pub struct TileCache {
    tiles: Vec<([u32; 16], Tile)>,
    bytes: usize,
}
impl TileCache {
    const LIMIT: usize = 48 * 1024 * 1024;
}

// A hexagon in pixels, for coverage: the distance to its nearest edge across
// the pixel (it is convex). The slanted edges' normal is (hh, hw/2); its
// length is computed once (the Wii U CPU has no hardware square root).
struct HexShape {
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    inv_n: f32,
}
impl HexShape {
    fn new(r: &Rect, s: f32) -> Self {
        let (cx, cy) = r.center();
        let (hw, hh) = (r.w * 0.5 * s, r.h * 0.5 * s);
        let n = libm::sqrtf(hh * hh + hw * hw * 0.25);
        HexShape { cx: cx * s, cy: cy * s, hw, hh, inv_n: if n > 0.0 { 1.0 / n } else { 0.0 } }
    }
    fn cover(&self, x: usize, y: usize) -> f32 {
        if self.hw <= 0.0 || self.hh <= 0.0 {
            return 0.0;
        }
        let dx = (x as f32 + 0.5 - self.cx).abs();
        let dy = (y as f32 + 0.5 - self.cy).abs();
        let flat = self.hh - dy;
        let slant = (self.hw * self.hh - dx * self.hh - dy * self.hw * 0.5) * self.inv_n;
        (flat.min(slant) + 0.5).clamp(0.0, 1.0)
    }
}

pub struct Canvas<'a> {
    px: &'a mut [u8],
    /// Row length in pixels.
    pitch: usize,
    pub w: usize,
    pub h: usize,
    /// Pixels per logical unit.
    pub s: f32,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u8], pitch: usize, w: usize, h: usize) -> Self {
        assert!(px.len() >= pitch * h * 4 && pitch >= w);
        Canvas { px, pitch, w, h, s: w as f32 / WIDTH }
    }

    // Logical rectangle to a clamped pixel span.
    fn span(&self, r: &Rect) -> (usize, usize, usize, usize) {
        let x0 = libm::roundf(r.x * self.s).clamp(0.0, self.w as f32) as usize;
        let y0 = libm::roundf(r.y * self.s).clamp(0.0, self.h as f32) as usize;
        let x1 = libm::roundf((r.x + r.w) * self.s).clamp(0.0, self.w as f32) as usize;
        let y1 = libm::roundf((r.y + r.h) * self.s).clamp(0.0, self.h as f32) as usize;
        (x0, y0, x1, y1)
    }

    fn blend(&mut self, x: usize, y: usize, c: Rgba, coverage: f32) {
        let a = (c[3] as f32 * coverage.clamp(0.0, 1.0) + 0.5) as u32;
        self.blend_alpha((y * self.pitch + x) * 4, c, a);
    }

    // Integer blend of `c` over the pixel at byte `i`, `a` in 0..=255.
    fn blend_alpha(&mut self, i: usize, c: Rgba, a: u32) {
        if a >= 255 {
            self.px[i..i + 3].copy_from_slice(&c[..3]);
        } else if a > 0 {
            for k in 0..3 {
                let d = self.px[i + k] as i32;
                self.px[i + k] = (d + ((c[k] as i32 - d) * a as i32 + 127) / 255) as u8;
            }
        } else {
            return;
        }
        self.px[i + 3] = 255;
    }

    pub fn fill(&mut self, r: Rect, c: Rgba) {
        let (x0, y0, x1, y1) = self.span(&r);
        for y in y0..y1 {
            if c[3] == 255 {
                let row = &mut self.px[(y * self.pitch + x0) * 4..(y * self.pitch + x1) * 4];
                for p in row.chunks_exact_mut(4) {
                    p.copy_from_slice(&c);
                }
                continue;
            }
            for x in x0..x1 {
                self.blend_alpha((y * self.pitch + x) * 4, c, c[3] as u32);
            }
        }
    }

    /// A vertical gradient from `top` to `bottom`.
    pub fn gradient(&mut self, r: Rect, top: Rgba, bottom: Rgba) {
        let (x0, y0, x1, y1) = self.span(&r);
        for y in y0..y1 {
            let t = (y - y0) as f32 / (y1 - y0).max(1) as f32;
            let c: Rgba = core::array::from_fn(|k| (top[k] as f32 + (bottom[k] as f32 - top[k] as f32) * t) as u8);
            for x in x0..x1 {
                self.blend(x, y, c, 1.0);
            }
        }
    }

    /// A horizontal fade from `c` at the left edge to transparent.
    pub fn fade_right(&mut self, r: Rect, c: Rgba) {
        let (x0, y0, x1, y1) = self.span(&r);
        for y in y0..y1 {
            for x in x0..x1 {
                let t = 1.0 - (x - x0) as f32 / (x1 - x0).max(1) as f32;
                self.blend(x, y, c, t * t);
            }
        }
    }

    pub fn frame(&mut self, r: Rect, t: f32, c: Rgba) {
        self.fill(rect(r.x, r.y, r.w, t), c);
        self.fill(rect(r.x, r.y + r.h - t, r.w, t), c);
        self.fill(rect(r.x, r.y + t, t, r.h - 2.0 * t), c);
        self.fill(rect(r.x + r.w - t, r.y + t, t, r.h - 2.0 * t), c);
    }

    /// A filled hexagon with an outline of width `t` (0: none), antialiased.
    pub fn hex(&mut self, r: Rect, fill: Option<Rgba>, t: f32, outline: Rgba) {
        let (x0, y0, x1, y1) = self.span(&r.inset(-1.0));
        let inner = r.inset(t);
        let (outer_hex, inner_hex) = (HexShape::new(&r, self.s), HexShape::new(&inner, self.s));
        let (icx, icy) = inner.center();
        let (ihw, ihh) = (inner.w * 0.5 * self.s, inner.h * 0.5 * self.s);
        let n = libm::sqrtf(ihh * ihh + ihw * ihw * 0.25);
        for y in y0..y1 {
            // Without a fill, skip the part of the row fully inside the inset.
            let dy = (y as f32 + 0.5 - icy * self.s).abs();
            let (mut skip0, mut skip1) = (1, 0);
            if fill.is_none() && ihh > 0.0 && ihh - dy >= 0.5 {
                let reach = (ihw * ihh - dy * ihw * 0.5 - 0.5 * n) / ihh - 0.5;
                if reach > 0.0 {
                    skip0 = libm::ceilf(icx * self.s - reach).max(0.0) as usize;
                    skip1 = libm::floorf(icx * self.s + reach).max(0.0) as usize;
                }
            }
            for x in x0..x1 {
                if x >= skip0 && x < skip1 {
                    continue;
                }
                let cov = outer_hex.cover(x, y);
                if cov <= 0.0 {
                    continue;
                }
                let inside = inner_hex.cover(x, y);
                if t > 0.0 && inside < 1.0 {
                    self.blend(x, y, outline, cov * (1.0 - inside));
                }
                if let Some(f) = fill {
                    self.blend(x, y, f, inside.min(cov));
                }
            }
        }
    }

    /// A filled circle, antialiased.
    pub fn disc(&mut self, cx: f32, cy: f32, radius: f32, c: Rgba) {
        let (x0, y0, x1, y1) = self.span(&rect(cx - radius - 1.0, cy - radius - 1.0, radius * 2.0 + 2.0, radius * 2.0 + 2.0));
        let (pcx, pcy, pr) = (cx * self.s, cy * self.s, radius * self.s);
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - pcx, y as f32 + 0.5 - pcy);
                let cov = pr - libm::sqrtf(dx * dx + dy * dy) + 0.5;
                if cov > 0.0 {
                    self.blend(x, y, c, cov.min(1.0));
                }
            }
        }
    }

    /// `src` (a part of `img`, in its pixels) scaled into `dst`, `bright`
    /// scaling the colour. The scaled result is kept in `cache` under `id`
    /// (which names `img`), so an unchanged picture is copied, not resampled.
    pub fn image_cached(&mut self, cache: &mut TileCache, id: u32, img: &Image, src: Rect, dst: Rect, clip: Clip, bright: f32) {
        let (cr, ch) = match clip {
            Clip::None => (0, rect(0.0, 0.0, 0.0, 0.0)),
            Clip::Hex(h) => (1, h),
        };
        let f = |r: Rect| [r.x.to_bits(), r.y.to_bits(), r.w.to_bits(), r.h.to_bits()];
        let mut key = [0u32; 16];
        key[0] = id;
        key[1] = self.w as u32;
        key[2] = bright.to_bits();
        key[3] = cr;
        key[4..8].copy_from_slice(&f(src));
        key[8..12].copy_from_slice(&f(dst));
        key[12..16].copy_from_slice(&f(ch));
        if let Some((_, tile)) = cache.tiles.iter().find(|(k, _)| *k == key) {
            self.put(tile);
            return;
        }
        let tile = self.render_tile(img, src, dst, clip, bright);
        self.put(&tile);
        cache.bytes += tile.px.len() * 4;
        if cache.bytes > TileCache::LIMIT {
            cache.tiles.clear();
            cache.bytes = tile.px.len() * 4;
        }
        cache.tiles.push((key, tile));
    }

    // The scaled picture over the pixels of `dst`, alpha = clip coverage.
    fn render_tile(&self, img: &Image, src: Rect, dst: Rect, clip: Clip, bright: f32) -> Tile {
        let (x0, y0, x1, y1) = self.span(&dst);
        let (kx, ky) = (src.w / (dst.w * self.s), src.h / (dst.h * self.s));
        let ox = dst.x * self.s;
        let oy = dst.y * self.s;
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        let shape = match clip {
            Clip::None => None,
            Clip::Hex(h) => Some(HexShape::new(&h, self.s)),
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = shape.as_ref().map_or(1.0, |h| h.cover(x, y));
                if cov <= 0.0 {
                    px.push([0; 4]);
                    continue;
                }
                let c = img.sample(src.x + (x as f32 + 0.5 - ox) * kx, src.y + (y as f32 + 0.5 - oy) * ky);
                px.push([(c[0] * bright) as u8, (c[1] * bright) as u8, (c[2] * bright) as u8, (cov * 255.0 + 0.5) as u8]);
            }
        }
        Tile { x0, y0, w: x1 - x0, px }
    }

    fn put(&mut self, t: &Tile) {
        for (row, line) in t.px.chunks(t.w.max(1)).enumerate() {
            let base = ((t.y0 + row) * self.pitch + t.x0) * 4;
            for (k, p) in line.iter().enumerate() {
                self.blend_alpha(base + k * 4, *p, p[3] as u32);
            }
        }
    }

    /// The whole of `img` fitted inside `dst`, centred, keeping its shape.
    pub fn image_fit(&mut self, cache: &mut TileCache, id: u32, img: &Image, dst: Rect, bright: f32) -> Rect {
        let k = (dst.w / img.w as f32).min(dst.h / img.h as f32);
        let (w, h) = (img.w as f32 * k, img.h as f32 * k);
        let r = rect(dst.x + (dst.w - w) * 0.5, dst.y + (dst.h - h) * 0.5, w, h);
        self.image_cached(cache, id, img, rect(0.0, 0.0, img.w as f32, img.h as f32), r, Clip::None, bright);
        r
    }

    /// Text with its top at `y`; `size` is the line height in logical units.
    /// Returns the advance width in logical units.
    pub fn text(&mut self, font: &Font, x: f32, y: f32, size: f32, c: Rgba, s: &str) -> f32 {
        match font {
            Font::Ttf(f, glyphs) => {
                let px_size = libm::roundf(size * self.s * 4.0) / 4.0;
                let scaled = f.as_scaled(PxScale::from(px_size));
                let base = libm::roundf(y * self.s + scaled.ascent() + (size * self.s - scaled.height()) * 0.5) as i32;
                let mut pen = x * self.s;
                let mut last = None;
                let mut glyphs = glyphs.borrow_mut();
                if glyphs.map.len() > Glyphs::LIMIT {
                    glyphs.map.clear();
                }
                for ch in s.chars() {
                    let id = scaled.glyph_id(ch);
                    if let Some(prev) = last {
                        pen += scaled.kern(prev, id);
                    }
                    let whole = libm::floorf(pen);
                    let quarter = ((pen - whole) * 4.0) as u8 & 3;
                    let key = (id.0, (px_size * 4.0) as u32, quarter);
                    let g = glyphs.map.entry(key).or_insert_with(|| {
                        let glyph = id.with_scale_and_position(scaled.scale, ab_glyph::point(quarter as f32 / 4.0, 0.0));
                        let Some(outline) = f.outline_glyph(glyph) else {
                            return Coverage { x: 0, y: 0, w: 0, cov: Vec::new() };
                        };
                        let b = outline.px_bounds();
                        let w = b.width() as usize;
                        let mut cov = alloc::vec![0u8; w * b.height() as usize];
                        outline.draw(|gx, gy, v| {
                            if let Some(c) = cov.get_mut(gy as usize * w + gx as usize) {
                                *c = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                            }
                        });
                        Coverage { x: b.min.x as i32, y: b.min.y as i32, w, cov }
                    });
                    pen += scaled.h_advance(id);
                    last = Some(id);
                    let (ox, oy) = (whole as i32 + g.x, base + g.y);
                    for (row, line) in g.cov.chunks(g.w.max(1)).enumerate() {
                        let py = oy + row as i32;
                        if py < 0 || py as usize >= self.h {
                            continue;
                        }
                        for (col, &v) in line.iter().enumerate() {
                            let px = ox + col as i32;
                            if v > 0 && px >= 0 && (px as usize) < self.w {
                                self.blend_alpha((py as usize * self.pitch + px as usize) * 4, c, (v as u32 * c[3] as u32 + 127) / 255);
                            }
                        }
                    }
                }
                pen / self.s - x
            }
            Font::Bitmap => {
                let cell = size / 9.0;
                let mut pen = x;
                for ch in s.chars() {
                    let code = ch as usize;
                    let glyph = if code < 128 { font8x8::legacy::BASIC_LEGACY[code] } else { [0; 8] };
                    self.bitmap_glyph(&glyph, pen, y + cell * 0.5, cell, c);
                    pen += cell * 8.0;
                }
                pen - x
            }
        }
    }

    // An 8x8 glyph, each dot `cell` units square, smoothed by 3x3 sampling.
    fn bitmap_glyph(&mut self, glyph: &[u8; 8], x: f32, y: f32, cell: f32, c: Rgba) {
        let (x0, y0, x1, y1) = self.span(&rect(x, y, cell * 8.0, cell * 8.0));
        for py in y0..y1 {
            for px in x0..x1 {
                let mut n = 0;
                for sy in 0..3 {
                    for sx in 0..3 {
                        let gx = ((px as f32 + (sx as f32 + 0.5) / 3.0) / self.s - x) / cell;
                        let gy = ((py as f32 + (sy as f32 + 0.5) / 3.0) / self.s - y) / cell;
                        if (0.0..8.0).contains(&gx) && (0.0..8.0).contains(&gy) && glyph[gy as usize] & (1 << gx as usize) != 0 {
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    self.blend(px, py, c, n as f32 / 9.0);
                }
            }
        }
    }

    /// Text centred in `r` horizontally (and vertically for one line).
    pub fn text_centered(&mut self, font: &Font, r: Rect, size: f32, c: Rgba, s: &str) {
        let w = measure(font, size, s);
        self.text(font, r.x + (r.w - w) * 0.5, r.y + (r.h - size) * 0.5, size, c, s);
    }

    /// Text wrapped to `w`, from `y`, lines `size * 1.25` apart, stopping
    /// before `max_y`. Blank lines in `s` are kept as paragraph breaks.
    pub fn paragraph(&mut self, font: &Font, x: f32, y: f32, w: f32, max_y: f32, size: f32, c: Rgba, s: &str) -> f32 {
        let mut line_y = y;
        for line in wrap(font, size, w, s) {
            if line_y + size > max_y {
                break;
            }
            self.text(font, x, line_y, size, c, &line);
            line_y += size * 1.25;
        }
        line_y
    }
}

pub fn measure(font: &Font, size: f32, s: &str) -> f32 {
    match font {
        Font::Ttf(f, _) => {
            let scaled = f.as_scaled(PxScale::from(size));
            let mut w = 0.0;
            let mut last = None;
            for ch in s.chars() {
                let id = scaled.glyph_id(ch);
                if let Some(prev) = last {
                    w += scaled.kern(prev, id);
                }
                w += scaled.h_advance(id);
                last = Some(id);
            }
            w
        }
        Font::Bitmap => s.chars().count() as f32 * size / 9.0 * 8.0,
    }
}

/// Word-wrapped lines of `s` no wider than `w`.
pub fn wrap(font: &Font, size: f32, w: f32, s: &str) -> Vec<alloc::string::String> {
    use alloc::string::String;
    let mut out = Vec::new();
    for para in s.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() { String::from(word) } else { alloc::format!("{line} {word}") };
            if !line.is_empty() && measure(font, size, &candidate) > w {
                out.push(core::mem::replace(&mut line, String::from(word)));
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}
