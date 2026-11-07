//! The light of an area's objects (`renderd3d.cpp: _RenderObject`), split
//! into what an object keeps all day and what the time of day changes, so a
//! dynamic-time hunt can light every object each frame cheaply.

use c2_formats::rsc::object_flags;

use super::{idx, Area, Placement};

/// An object's light before the time of day is known; [`ObjectLight::at`]
/// gives [`Area::object_light`] for given time weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ObjectLight {
    /// The same at any time (`ofDEFLIGHT`, or a missing object).
    Fixed(f32),
    /// From the cell's dawn, day and night light: `l / 2 + base`, with `l`
    /// the cell's light at the time ([`Area::light_at`]).
    Cell { lights: [u8; 3], base: i32 },
}

impl ObjectLight {
    /// The light 0..1 at time weights `w` ([`Area::time_weights`]).
    pub fn at(self, w: [f32; 3]) -> f32 {
        match self {
            Self::Fixed(v) => v,
            Self::Cell { lights, base } => {
                // As `Area::light_at`, then truncated as `object_light` does.
                let l: f32 = (0..3).map(|i| f32::from(lights[i]) * w[i]).sum();
                (i32::from(l as u8 >> 1) + base).clamp(64, 192) as f32 / 255.0
            }
        }
    }
}

impl Area {
    /// [`Area::object_light`] without the time of day; `None` for ground-lit
    /// objects (lit per vertex from the ground, `ground_light`).
    pub fn object_light_terms(&self, p: &Placement) -> Option<ObjectLight> {
        let Some(o) = self.rsc.objects.get(usize::from(p.model)) else {
            return Some(ObjectLight::Fixed(0.5));
        };
        if o.info.flags & object_flags::GROUND_LIGHT != 0 {
            return None;
        }
        if o.info.flags & object_flags::DEF_LIGHT != 0 {
            return Some(ObjectLight::Fixed(
                o.info.default_light.clamp(64, 192) as f32 / 255.0,
            ));
        }
        let (x, y) = (usize::from(p.cell.0), usize::from(p.cell.1));
        let cell = idx(x, y);
        let r = self
            .random_map
            .get((y & 31) * 32 + (x & 31))
            .copied()
            .unwrap_or(0);
        Some(ObjectLight::Cell {
            lights: core::array::from_fn(|t| self.map.light[t].get(cell).copied().unwrap_or(128)),
            base: 96 - (r >> 5),
        })
    }

    /// Object brightness 0..1, `renderd3d.cpp: _RenderObject`.
    pub fn object_light(&self, p: &Placement) -> f32 {
        match self.object_light_terms(p) {
            Some(t) => t.at(self.time_weights()),
            // Ground-lit objects take the light of the ground under each
            // vertex (CalcModelGroundLight: 128 + LandLt - 128); here, under
            // the object (the PC lights them per vertex, `ground_light`).
            None => self.ground_light(p.pos[0], p.pos[2]) / 255.0,
        }
    }

    /// The light of a far object's sprite: its light less 16
    /// (`RenderBMPModel(.., mlight-16)`; ground-lit ones from 128).
    pub fn sprite_light(&self, p: &Placement) -> f32 {
        sprite_light(self.object_light_terms(p), self.time_weights())
    }
}

/// [`Area::sprite_light`] from an object's [`Area::object_light_terms`].
pub fn sprite_light(terms: Option<ObjectLight>, w: [f32; 3]) -> f32 {
    let l = terms.map_or(128.0 / 255.0, |t| t.at(w));
    (l - 16.0 / 255.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_light_blends_then_truncates_like_light_at() {
        let t = ObjectLight::Cell {
            lights: [100, 200, 40],
            base: 90,
        };
        // Day only: 200 / 2 + 90 = 190.
        assert_eq!(t.at([0.0, 1.0, 0.0]), 190.0 / 255.0);
        // Half dawn, half day: 150 / 2 + 90 = 165.
        assert_eq!(t.at([0.5, 0.5, 0.0]), 165.0 / 255.0);
        // Night: 40 / 2 + 90 = 110; clamped at 64..192 either side.
        assert_eq!(t.at([0.0, 0.0, 1.0]), 110.0 / 255.0);
        let dark = ObjectLight::Cell {
            lights: [0; 3],
            base: 0,
        };
        assert_eq!(dark.at([0.0, 1.0, 0.0]), 64.0 / 255.0);
    }

    #[test]
    fn sprites_are_sixteen_darker_and_ground_lit_ones_start_at_128() {
        let w = [0.0, 1.0, 0.0];
        assert!((sprite_light(None, w) - 112.0 / 255.0).abs() < 1e-6);
        let f = sprite_light(Some(ObjectLight::Fixed(100.0 / 255.0)), w);
        assert!((f - 84.0 / 255.0).abs() < 1e-6);
    }
}
