//! The low-memory trophy room: each trophy is a 2D portrait of its animal
//! (the picture the cooker drew from that animal's own model and skin) on
//! an upright card that turns to the visitor, with a plate of figures
//! beneath. Used when the room's animals don't fit in memory as 3D statues
//! (`c2_core::game::trophy_room`); the stands themselves are the same
//! characters the 3D room places, so walking, plaques, removing, locking
//! and moving trophies work as before.

use std::collections::HashMap;

use c2_core::game::trophy_room::{plate_lines, PLATE_COLS};
use c2_formats::cooked::{tile, CookedTexture, TexFormat};
use c2_formats::sav::TrophyItem;
use c2_vfs::Vfs;
use font8x8::{UnicodeFonts, BASIC_FONTS};
use glam::{Mat4, Vec3};

use crate::gpu::{Gpu, LinearBuf, Lit, Texture, Vertex};
use crate::hunt::Game;
use c2_core::game::Hunt;

/// The plate texture: `PLATE_COLS` 8-pixel characters wide, three lines.
const PLATE_W: usize = PLATE_COLS * 8;
const PLATE_H: usize = 32;
/// Card sizes in world units (a cell is 256): the portrait is square.
const CARD: f32 = 220.0;
const PLATE_QUAD_H: f32 = CARD * PLATE_H as f32 / PLATE_W as f32;
/// Gap between the ground and the plate's bottom edge.
const LIFT: f32 = 24.0;

/// An RGBA8 plate: white 8x8 text on a dark card.
pub fn plate_pixels(lines: &[String; 3]) -> Vec<u8> {
    let mut px = vec![0u8; PLATE_W * PLATE_H * 4];
    for p in px.chunks_exact_mut(4) {
        p.copy_from_slice(&[24, 22, 20, 255]);
    }
    for (row, line) in lines.iter().enumerate() {
        let y0 = 1 + row * 10;
        for (col, ch) in line.chars().take(PLATE_COLS).enumerate() {
            let Some(glyph) = BASIC_FONTS.get(ch) else {
                continue;
            };
            for (gy, bits) in glyph.iter().enumerate() {
                for gx in 0..8 {
                    if bits & (1 << gx) != 0 {
                        let i = ((y0 + gy) * PLATE_W + col * 8 + gx) * 4;
                        px[i..i + 4].copy_from_slice(&[240, 220, 150, 255]);
                    }
                }
            }
        }
    }
    px
}

fn plate_texture(lines: &[String; 3]) -> Option<Texture> {
    let t: CookedTexture = tile(&plate_pixels(lines), PLATE_W, PLATE_H, TexFormat::Rgba5551);
    Texture::new(&t)
}

/// One upright card at `at`, `CARD` wide, from height `y0` to `y1`, turned by
/// `rot` to face the visitor: two triangles.
fn push_quad(verts: &mut Vec<Vertex>, at: Vec3, rot: &Mat4, y0: f32, y1: f32) {
    let h = CARD / 2.0;
    let corners = [(-h, y1), (h, y1), (h, y0), (-h, y1), (h, y0), (-h, y0)];
    const UV: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 1.0],
        [0.0, 0.0],
        [1.0, 1.0],
        [0.0, 1.0],
    ];
    for (i, (x, y)) in corners.into_iter().enumerate() {
        let p = at + rot.transform_vector3(Vec3::new(x, y, 0.0));
        verts.push(Vertex {
            pos: p.to_array(),
            uv: UV[i],
            lit: [255, 0, 0, 0],
        });
    }
}

struct Plate {
    body: TrophyItem,
    tex: Option<Texture>,
}

pub struct Stands {
    /// Portrait by character index; `None`: the pack has none (plate only).
    portraits: HashMap<usize, Option<Texture>>,
    /// The plate of each stand, rebuilt when the trophy in it changes.
    plates: HashMap<usize, Plate>,
    buf: Option<LinearBuf<Vertex>>,
}

impl Stands {
    /// The stands for `game`'s trophy room, when it is the low-memory one.
    pub fn new(vfs: &Vfs, game: &Game) -> Option<Self> {
        let plan = game.trophy_plan.as_ref().filter(|p| p.low_memory())?;
        let portraits =
            plan.species
                .iter()
                .map(|&s| {
                    let tex = game.keys.get(s).and_then(|k| {
                        crate::world::load_texture(vfs, &format!("C2RS/MENU/R/{k}.TX"))
                    });
                    (s, tex)
                })
                .collect();
        Some(Self {
            portraits,
            plates: HashMap::new(),
            buf: LinearBuf::new(24 * 12),
        })
    }

    /// Cards for every trophy standing, turned to face `fwd`'s opposite.
    pub fn draw(&mut self, gpu: &mut Gpu, hunt: &Hunt, view: &Mat4, fwd: Vec3) {
        let rot = Mat4::from_rotation_y(f32::atan2(-fwd.x, -fwd.z));
        let us = hunt.room.options.sys != 0;
        gpu.set_model_view(view);
        gpu.set_light(0.0, Some(0));
        // The quads of every stand in one buffer: (stand, portrait, first).
        let mut verts: Vec<Vertex> = Vec::new();
        let mut runs: Vec<(usize, Option<usize>, usize)> = Vec::new();
        for c in &hunt.herd.chars {
            let slot = c.state as usize;
            let Some(body) = hunt.room.bodies.get(slot).filter(|b| b.ctype != 0) else {
                continue;
            };
            let stale = self.plates.get(&slot).is_none_or(|p| p.body != *body);
            if stale {
                let info = hunt.herd.species.get(c.ctype).map(|s| &s.info);
                let name = info.map_or("", |i| i.name.as_str());
                let (mass, len) = info.map_or((0.0, 0.0), |i| (i.mass, i.length));
                let lines = plate_lines(name, mass, len, body, us);
                self.plates.insert(
                    slot,
                    Plate {
                        body: *body,
                        tex: plate_texture(&lines),
                    },
                );
            }
            let at = Vec3::from(c.pos);
            let plate_top = LIFT + PLATE_QUAD_H;
            // Plate first, then the portrait above it (when the pack has one).
            let first = verts.len();
            push_quad(&mut verts, at, &rot, LIFT, plate_top);
            runs.push((slot, None, first));
            if self.portraits.get(&c.ctype).is_some_and(Option::is_some) {
                let first = verts.len();
                push_quad(&mut verts, at, &rot, plate_top, plate_top + CARD);
                runs.push((slot, Some(c.ctype), first));
            }
        }
        let Some(buf) = self.buf.as_mut() else { return };
        if buf.cap() < verts.len() {
            self.buf = LinearBuf::new(verts.len().next_power_of_two());
            return;
        }
        buf.write(&verts);
        for (slot, portrait, first) in runs {
            let tex = match portrait {
                Some(ctype) => self.portraits.get(&ctype).and_then(Option::as_ref),
                None => self.plates.get(&slot).and_then(|p| p.tex.as_ref()),
            };
            if let Some(tex) = tex {
                tex.bind();
                gpu.draw(buf, Lit::Unsigned, first, 6);
            }
        }
    }
}
