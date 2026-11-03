//! Map navigation in normalized map coordinates, independent of window size.
use crate::hud::{Hud, Sprite};
use crate::input::Input;

pub(super) struct MapViewport {
    pub rect: Option<(f32, f32, f32)>,
    pub origin: [f32; 2],
    pub span: f32,
    pointer: [f32; 2],
    previous_cursor: Option<(f32, f32)>,
}
impl Default for MapViewport {
    fn default() -> Self {
        Self {
            rect: None,
            origin: [0.0; 2],
            span: 1.0,
            pointer: [0.5; 2],
            previous_cursor: None,
        }
    }
}
impl MapViewport {
    pub fn map_point(&self) -> [f32; 2] {
        core::array::from_fn(|i| self.origin[i] + self.pointer[i] * self.span)
    }
    fn clamp(&mut self) {
        for v in &mut self.origin {
            *v = v.clamp(0.0, 1.0 - self.span);
        }
    }
    fn zoom(&mut self, delta: f32) {
        if !delta.is_finite() {
            return;
        }
        let anchor = self.map_point();
        self.span = (self.span * 2.0f32.powf(-delta.clamp(-32.0, 32.0) / 4.0)).clamp(0.125, 1.0);
        self.origin = core::array::from_fn(|i| anchor[i] - self.pointer[i] * self.span);
        self.clamp();
    }
    fn pan(&mut self, delta: [f32; 2]) {
        for (v, d) in self.origin.iter_mut().zip(delta) {
            *v -= d * self.span;
        }
        self.clamp();
    }
    pub fn update(&mut self, input: &Input, dt: f32) {
        let Some((x, y, side)) = self.rect else {
            return;
        };
        if side <= 0.0 {
            return;
        }
        let mut over = true;
        if input.grabbed {
            if input.right_down {
                self.pan([input.mouse_delta.0 / side, input.mouse_delta.1 / side]);
            } else {
                self.pointer[0] += input.mouse_delta.0 / side;
                self.pointer[1] += input.mouse_delta.1 / side;
            }
        } else if let Some((px, py)) = input.cursor {
            over = px >= x && py >= y && px <= x + side && py <= y + side;
            if over {
                self.pointer = [(px - x) / side, (py - y) / side];
                if input.right_down {
                    if let Some((last_x, last_y)) = self.previous_cursor {
                        self.pan([(px - last_x) / side, (py - last_y) / side]);
                    }
                }
            }
        }
        self.previous_cursor = input.cursor;
        let [lx, ly] = input.pad.look;
        let [dx, dy] = input.pad.dpad.map(f32::from);
        self.pointer[0] += (lx + dx) * dt;
        self.pointer[1] -= (ly + dy) * dt;
        for v in &mut self.pointer {
            *v = v.clamp(0.0, 1.0);
        }
        if over {
            self.zoom(input.scroll);
        }
        if input.pressed(winit::keyboard::KeyCode::Home) {
            self.span = 1.0;
            self.origin = [0.0; 2];
        }
    }
    /// Transform a full-map sprite, then clip both geometry and UVs to the viewport.
    pub fn project(&self, mut sprite: Sprite, hud: &Hud) -> Option<Sprite> {
        let (x, y, side) = self.rect?;
        let im = hud.image(sprite.image)?;
        let [u0, v0, u1, v1] = sprite.uv;
        let size = sprite.size.unwrap_or([
            im.w as f32 * (u1 - u0) * sprite.scale,
            im.h as f32 * (v1 - v0) * sprite.scale,
        ]);
        sprite.x = x + (sprite.x - x - self.origin[0] * side) / self.span;
        sprite.y = y + (sprite.y - y - self.origin[1] * side) / self.span;
        sprite.size = Some(size.map(|v| v / self.span));
        clip(sprite, (x, y, side))
    }
    pub fn pointer(&self, hud: &Hud, out: &mut Vec<Sprite>) {
        let Some((x, y, side)) = self.rect else {
            return;
        };
        let (px, py) = (x + self.pointer[0] * side, y + self.pointer[1] * side);
        for (w, h) in [(11.0, 1.0), (1.0, 11.0)] {
            if let Some(s) = hud
                .rect(px - w / 2.0, py - h / 2.0, w, h, [1.0; 4])
                .and_then(|s| clip(s, (x, y, side)))
            {
                out.push(s);
            }
        }
    }
}

fn clip(mut s: Sprite, (x, y, side): (f32, f32, f32)) -> Option<Sprite> {
    let [w, h] = s.size?;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (left, top, right, bottom) = (
        s.x.max(x),
        s.y.max(y),
        (s.x + w).min(x + side),
        (s.y + h).min(y + side),
    );
    if right <= left || bottom <= top {
        return None;
    }
    let [u0, v0, u1, v1] = s.uv;
    s.uv = [
        u0 + (u1 - u0) * (left - s.x) / w,
        v0 + (v1 - v0) * (top - s.y) / h,
        u0 + (u1 - u0) * (right - s.x) / w,
        v0 + (v1 - v0) * (bottom - s.y) / h,
    ];
    s.x = left;
    s.y = top;
    s.size = Some([right - left, bottom - top]);
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zoom_anchors_any_map_point_and_round_trips() {
        for pointer in [[0.0, 0.0], [1.0, 1.0], [0.2, 0.8], [0.8, 0.1]] {
            let mut v = MapViewport {
                pointer,
                ..Default::default()
            };
            v.zoom(8.0);
            assert_eq!(v.span, 0.25);
            for (actual, expected) in v.map_point().into_iter().zip(pointer) {
                assert!((actual - expected).abs() < 0.00001);
            }
            v.zoom(-8.0);
            assert_eq!(v.span, 1.0);
            assert_eq!(v.origin, [0.0; 2]);
        }
    }
    #[test]
    fn panning_reaches_both_edges_and_zoom_is_bounded() {
        let mut v = MapViewport::default();
        v.zoom(100.0);
        assert_eq!(v.span, 0.125);
        v.pan([-100.0, -100.0]);
        assert_eq!(v.origin, [0.875; 2]);
        v.pan([100.0, 100.0]);
        assert_eq!(v.origin, [0.0; 2]);
        v.zoom(-100.0);
        assert_eq!(v.span, 1.0);
        assert_eq!(v.origin, [0.0; 2]);
    }
    #[test]
    fn clipping_crops_texture_instead_of_stretching_it() {
        let s = Sprite {
            size: Some([200.0, 200.0]),
            ..Sprite::new(0, -50.0, -50.0, 1.0)
        };
        let s = clip(s, (0.0, 0.0, 100.0)).unwrap();
        assert_eq!(s.uv, [0.25, 0.25, 0.75, 0.75]);
        assert_eq!(s.size, Some([100.0; 2]));
    }
}
