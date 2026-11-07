//! View-frustum culling on the CPU: whole ground chunks and object chunks
//! the camera can't see are not sent to the GPU at all. Only work is saved:
//! the picture is the same.

use glam::{Mat4, Vec3, Vec4};

/// The six planes of a view-projection matrix (wgpu's 0..1 depth), each
/// `(n, d)` with `n.dot(p) + d >= 0` inside.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn new(view_proj: Mat4) -> Self {
        let r = [
            view_proj.row(0),
            view_proj.row(1),
            view_proj.row(2),
            view_proj.row(3),
        ];
        Self {
            planes: [
                r[3] + r[0],
                r[3] - r[0],
                r[3] + r[1],
                r[3] - r[1],
                r[2],
                r[3] - r[2],
            ],
        }
    }

    /// Whether any of the box `min..max` may be visible (conservative: a
    /// box near a frustum corner can pass while just outside it).
    pub fn sees_box(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|p| {
            let far = Vec3::new(
                if p.x >= 0.0 { max.x } else { min.x },
                if p.y >= 0.0 { max.y } else { min.y },
                if p.z >= 0.0 { max.z } else { min.z },
            );
            p.truncate().dot(far) + p.w >= 0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looking_down_neg_z() -> Frustum {
        let view = glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let proj = crate::camera::projection(16.0 / 9.0, 10_000.0, 1.0);
        Frustum::new(proj * view)
    }

    #[test]
    fn keeps_what_is_ahead_and_drops_what_is_behind() {
        let f = looking_down_neg_z();
        let unit = Vec3::splat(50.0);
        let ahead = Vec3::new(0.0, 0.0, -2000.0);
        assert!(f.sees_box(ahead - unit, ahead + unit));
        let behind = Vec3::new(0.0, 0.0, 2000.0);
        assert!(!f.sees_box(behind - unit, behind + unit));
        let far_left = Vec3::new(-20_000.0, 0.0, -2000.0);
        assert!(!f.sees_box(far_left - unit, far_left + unit));
        let past_far = Vec3::new(0.0, 0.0, -20_000.0);
        assert!(!f.sees_box(past_far - unit, past_far + unit));
    }

    #[test]
    fn a_box_around_the_camera_is_seen() {
        let f = looking_down_neg_z();
        assert!(f.sees_box(Vec3::splat(-100.0), Vec3::splat(100.0)));
    }
}
