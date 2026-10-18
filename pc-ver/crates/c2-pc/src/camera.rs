//! Fly and orbit cameras. World is right-handed, Y up, which matches the
//! original's camera space (looks down -Z; `renderd3d.cpp` projection).

use glam::{Mat4, Vec3};

use crate::input::Input;
use winit::keyboard::KeyCode;

/// The original's view: `CameraW = VideoCX * 1.25` with the 4:3 aspect
/// correction gives a vertical half-angle tangent of 0.6 (about 62 degrees);
/// `zoom` divides it (binoculars, scope).
pub fn projection(aspect: f32, far: f32, zoom: f32) -> Mat4 {
    let fov = 2.0 * (0.6 / zoom.max(0.1)).atan();
    // wgpu clip space is D3D-style (depth 0..1).
    glam::camera::rh::proj::directx::perspective(fov, aspect, 16.0, far)
}

pub struct FlyCam {
    pub pos: Vec3,
    /// Radians; 0 looks toward -Z.
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl FlyCam {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            -self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_to_mat4(self.pos, self.forward(), Vec3::Y)
    }

    pub fn update(&mut self, dt: f32, input: &Input) {
        self.yaw -= input.mouse_delta.0 * 0.004;
        self.pitch = (self.pitch - input.mouse_delta.1 * 0.004).clamp(-1.5, 1.5);
        let flat = Vec3::new(-self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = flat.cross(Vec3::Y);
        let mut v = Vec3::ZERO;
        if input.held(KeyCode::KeyW) {
            v += self.forward();
        }
        if input.held(KeyCode::KeyS) {
            v -= self.forward();
        }
        if input.held(KeyCode::KeyD) {
            v += right;
        }
        if input.held(KeyCode::KeyA) {
            v -= right;
        }
        if input.held(KeyCode::KeyE) {
            v += Vec3::Y;
        }
        if input.held(KeyCode::KeyQ) {
            v -= Vec3::Y;
        }
        let boost = if input.held(KeyCode::ShiftLeft) {
            5.0
        } else {
            1.0
        };
        self.speed = (self.speed * 1.2f32.powf(input.scroll)).clamp(100.0, 50_000.0);
        self.pos += v.normalize_or_zero() * self.speed * boost * dt;
    }
}

pub struct OrbitCam {
    pub target: Vec3,
    pub dist: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl OrbitCam {
    pub fn eye(&self) -> Vec3 {
        self.target
            + Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            ) * self.dist
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    pub fn update(&mut self, input: &Input) {
        self.yaw -= input.mouse_delta.0 * 0.01;
        self.pitch = (self.pitch + input.mouse_delta.1 * 0.01).clamp(-1.5, 1.5);
        self.dist = (self.dist * 0.9f32.powf(input.scroll)).clamp(10.0, 100_000.0);
    }
}
