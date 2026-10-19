//! 2D drawing in screen pixels for the overlays and menus: flat-coloured
//! shapes, pictures (also clipped to hexagons) and 8x8 text.
//!
//! Vertices for a frame are appended to two buffers and drawn from there;
//! the GPU reads them when the frame is submitted, so [`Canvas::new_frame`]
//! is called once per frame, before any drawing.

use crate::gpu::{FlatVertex, Gpu, LinearBuf, Lit, Texture, Vertex};

pub type Rgba = [u8; 4];

pub const WHITE: Rgba = [255, 255, 255, 255];

pub fn inside(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
}

/// Corners of a flat-topped hexagon of circumradius `r` around (cx, cy).
pub fn hex_corners(cx: f32, cy: f32, r: f32) -> [[f32; 2]; 6] {
    std::array::from_fn(|k| {
        let a = k as f32 * std::f32::consts::FRAC_PI_3;
        [cx + r * a.cos(), cy + r * a.sin()]
    })
}

/// Whether (x, y) is inside that hexagon.
pub fn in_hex(cx: f32, cy: f32, r: f32, x: f32, y: f32) -> bool {
    let (dx, dy) = ((x - cx).abs(), (y - cy).abs());
    let h = r * 3f32.sqrt() / 2.0;
    dy <= h && dx <= r - dy / 3f32.sqrt()
}

pub struct Canvas {
    font: Option<Texture>,
    quads: Option<LinearBuf<Vertex>>,
    flats: Option<LinearBuf<FlatVertex>>,
    /// Flat vertices already drawn.
    flushed: usize,
}

fn fv(p: [f32; 2], c: Rgba) -> FlatVertex {
    FlatVertex {
        pos: [p[0], p[1], 0.5],
        rgba: c,
    }
}

fn tv(p: [f32; 2], uv: [f32; 2]) -> Vertex {
    Vertex {
        pos: [p[0], p[1], 0.5],
        uv,
        lit: [0; 4],
    }
}

impl Canvas {
    pub fn new(font: Option<Texture>) -> Self {
        Self {
            font,
            quads: LinearBuf::new(8192),
            flats: LinearBuf::new(8192),
            flushed: 0,
        }
    }

    pub fn new_frame(&mut self) {
        if let Some(b) = self.quads.as_mut() {
            b.clear();
        }
        if let Some(b) = self.flats.as_mut() {
            b.clear();
        }
        self.flushed = 0;
    }

    fn flat(&mut self, v: &[FlatVertex]) {
        if let Some(b) = self.flats.as_mut() {
            let _ = b.append(v);
        }
    }

    /// Queues a rectangle (x, y, w, h); drawn at the next [`flush`].
    ///
    /// [`flush`]: Self::flush
    pub fn rect(&mut self, r: [f32; 4], c: Rgba) {
        let [x, y, w, h] = r;
        let (a, b, cc, d) = ([x, y], [x + w, y], [x + w, y + h], [x, y + h]);
        self.flat(&[fv(a, c), fv(b, c), fv(cc, c), fv(a, c), fv(cc, c), fv(d, c)]);
    }

    /// Queues a filled polygon (convex, as a fan) with a centre colour and
    /// a rim colour.
    pub fn fan(&mut self, centre: [f32; 2], rim: &[[f32; 2]], c0: Rgba, c1: Rgba) {
        let mut v = Vec::with_capacity(rim.len() * 3);
        for k in 0..rim.len() {
            v.extend_from_slice(&[
                fv(centre, c0),
                fv(rim[k], c1),
                fv(rim[(k + 1) % rim.len()], c1),
            ]);
        }
        self.flat(&v);
    }

    /// Queues a filled circle.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let rim: Vec<[f32; 2]> = (0..20)
            .map(|k| {
                let a = k as f32 / 20.0 * std::f32::consts::TAU;
                [cx + a.cos() * r, cy + a.sin() * r]
            })
            .collect();
        self.fan([cx, cy], &rim, c, c);
    }

    /// Queues a filled hexagon.
    pub fn hex(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        self.fan([cx, cy], &hex_corners(cx, cy, r), c, c);
    }

    /// Draws the queued flat shapes.
    pub fn flush(&mut self, gpu: &mut Gpu) {
        if let Some(b) = &self.flats {
            if b.len() > self.flushed {
                gpu.draw_flat(b, self.flushed, b.len() - self.flushed);
                self.flushed = b.len();
            }
        }
    }

    fn textured(&mut self, gpu: &mut Gpu, t: &Texture, v: &[Vertex], tint: Rgba) {
        let Some(buf) = self.quads.as_mut() else {
            return;
        };
        if let Some(at) = buf.append(v) {
            gpu.texenv_tint(tint);
            t.bind();
            gpu.draw(buf, Lit::Unsigned, at, v.len());
        }
    }

    /// A picture in rectangle `r`; `uv` = (u0, v0, u1, v1).
    pub fn image(&mut self, gpu: &mut Gpu, t: &Texture, r: [f32; 4], uv: [f32; 4], tint: Rgba) {
        self.flush(gpu);
        let [x, y, w, h] = r;
        let v = [
            tv([x, y], [uv[0], uv[1]]),
            tv([x + w, y], [uv[2], uv[1]]),
            tv([x + w, y + h], [uv[2], uv[3]]),
            tv([x, y], [uv[0], uv[1]]),
            tv([x + w, y + h], [uv[2], uv[3]]),
            tv([x, y + h], [uv[0], uv[3]]),
        ];
        self.textured(gpu, t, &v, tint);
    }

    /// A picture clipped to a hexagon: the hexagon's bounding box shows
    /// the `uv` part of the texture.
    pub fn hex_image(
        &mut self,
        gpu: &mut Gpu,
        t: &Texture,
        (cx, cy, r): (f32, f32, f32),
        uv: [f32; 4],
        tint: Rgba,
    ) {
        self.flush(gpu);
        let h = r * 3f32.sqrt() / 2.0;
        let map = |p: [f32; 2]| {
            let u = (p[0] - (cx - r)) / (2.0 * r);
            let v = (p[1] - (cy - h)) / (2.0 * h);
            tv(
                p,
                [uv[0] + u * (uv[2] - uv[0]), uv[1] + v * (uv[3] - uv[1])],
            )
        };
        let c = hex_corners(cx, cy, r);
        let mut v = Vec::with_capacity(18);
        for k in 0..6 {
            v.extend_from_slice(&[map([cx, cy]), map(c[k]), map(c[(k + 1) % 6])]);
        }
        self.textured(gpu, t, &v, tint);
    }

    /// 8x8 text at `scale` (1 or 2), top left at (x, y).
    pub fn text(&mut self, gpu: &mut Gpu, x: f32, y: f32, scale: f32, c: Rgba, s: &str) {
        self.flush(gpu);
        let Some(font) = self.font.take() else {
            return;
        };
        let k = 8.0 * scale;
        let mut v = Vec::with_capacity(s.len() * 6);
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
            v.extend_from_slice(&[
                tv([x0, y], [u0, v0]),
                tv([x0 + k, y], [u1, v0]),
                tv([x0 + k, y + k], [u1, v1]),
                tv([x0, y], [u0, v0]),
                tv([x0 + k, y + k], [u1, v1]),
                tv([x0, y + k], [u0, v1]),
            ]);
        }
        if !v.is_empty() {
            self.textured(gpu, &font, &v, c);
        }
        self.font = Some(font);
    }

    /// Text centred on `cx`.
    pub fn text_c(&mut self, gpu: &mut Gpu, cx: f32, y: f32, scale: f32, c: Rgba, s: &str) {
        let w = s.chars().count() as f32 * 8.0 * scale;
        self.text(gpu, cx - w / 2.0, y, scale, c, s);
    }
}
