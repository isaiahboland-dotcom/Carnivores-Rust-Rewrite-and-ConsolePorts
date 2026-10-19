//! Screenshots (L+SELECT, for bug reports) and the test script
//! (`sdmc:/3ds/c2rs/autotest.txt`, used to check builds in an emulator).
//!
//! Script lines, each taking effect after the previous one's frames:
//! ```text
//! wait 120            # do nothing for 120 frames
//! hold up+r 60        # hold keys: a b x y l r zl zr start select
//!                     #   up down left right (D-Pad), press = first frame only
//! press a             # one frame
//! shot NAME           # save shots/NAME.tga
//! cam X Z ALPHA BETA   # put the hunter there (world units, radians), paused
//! tap X Y             # tap the bottom screen at pixel (X, Y)
//! quit
//! ```

use std::io::Write;

use ctru::prelude::KeyPad;

/// Writes the top screen as a 400x240 TGA.
pub fn save_top(path: &str) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (mut w, mut h) = (0u16, 0u16);
    let fb = unsafe {
        ctru_sys::gspWaitForEvent(ctru_sys::GSPGPU_EVENT_VBlank0, true);
        ctru_sys::gfxGetFramebuffer(ctru_sys::GFX_TOP, ctru_sys::GFX_LEFT, &mut w, &mut h)
    };
    // The framebuffer is the screen turned sideways: 240 rows of 400 pixels
    // become 400 columns of 240, bottom to top, 3 bytes (B, G, R) each.
    let (w, h) = (usize::from(w), usize::from(h));
    if fb.is_null() || w != 240 || h < 400 {
        return Err(std::io::Error::other("no top framebuffer"));
    }
    let px = unsafe { std::slice::from_raw_parts(fb, w * h * 3) };
    let mut out = Vec::with_capacity(18 + 400 * 240 * 3);
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&400u16.to_le_bytes());
    out.extend_from_slice(&240u16.to_le_bytes());
    out.extend_from_slice(&[24, 0]);
    // TGA rows go bottom to top by default.
    for y in (0..240).rev() {
        for x in 0..400 {
            let i = (x * 240 + (239 - y)) * 3;
            out.extend_from_slice(&px[i..i + 3]);
        }
    }
    std::fs::File::create(path)?.write_all(&out)
}

enum Step {
    Wait,
    Hold(KeyPad, u32),
    Press(KeyPad),
    Shot(String),
    /// Put the hunter at world (x, z) looking (alpha, beta) and pause.
    Cam([f32; 4]),
    Tap(f32, f32),
    Quit,
}

pub struct Script {
    /// Each step with the frame it starts on.
    steps: Vec<(u32, Step)>,
}

fn keys(s: &str) -> KeyPad {
    s.split('+').fold(KeyPad::empty(), |k, n| {
        k | match n.trim().to_ascii_lowercase().as_str() {
            "a" => KeyPad::A,
            "b" => KeyPad::B,
            "x" => KeyPad::X,
            "y" => KeyPad::Y,
            "l" => KeyPad::L,
            "r" => KeyPad::R,
            "zl" => KeyPad::ZL,
            "zr" => KeyPad::ZR,
            "start" => KeyPad::START,
            "select" => KeyPad::SELECT,
            "up" => KeyPad::DPAD_UP,
            "down" => KeyPad::DPAD_DOWN,
            "left" => KeyPad::DPAD_LEFT,
            "right" => KeyPad::DPAD_RIGHT,
            _ => KeyPad::empty(),
        }
    })
}

impl Script {
    pub fn load(path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut steps = Vec::new();
        let mut at = 0u32;
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let mut p = line.split_whitespace();
            let (Some(cmd), a, b) = (p.next(), p.next(), p.next()) else {
                continue;
            };
            let n = |s: Option<&str>| s.and_then(|v| v.parse().ok()).unwrap_or(1u32);
            let (step, len) = match cmd {
                "wait" => (Step::Wait, n(a)),
                "hold" => (Step::Hold(keys(a.unwrap_or("")), n(b)), n(b)),
                "press" => (Step::Press(keys(a.unwrap_or(""))), 2),
                "shot" => (Step::Shot(a.unwrap_or("shot").to_string()), 1),
                "quit" => (Step::Quit, 1),
                "tap" => {
                    let n = |s: Option<&str>| s.and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    (Step::Tap(n(a), n(b)), 2)
                }
                "cam" => {
                    let v: Vec<f32> = line
                        .split_whitespace()
                        .skip(1)
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if v.len() < 4 {
                        continue;
                    }
                    (Step::Cam([v[0], v[1], v[2], v[3]]), 1)
                }
                _ => continue,
            };
            steps.push((at, step));
            at += len;
        }
        Some(Self { steps })
    }

    fn current(&self, frame: u32) -> Option<&(u32, Step)> {
        self.steps.iter().rev().find(|(at, _)| *at <= frame)
    }

    /// (held, pressed this frame).
    pub fn keys(&self, frame: u32) -> (KeyPad, KeyPad) {
        match self.current(frame) {
            Some((at, Step::Hold(k, n))) if frame < at + n => {
                (*k, if frame == *at { *k } else { KeyPad::empty() })
            }
            Some((at, Step::Press(k))) if frame == *at => (*k, *k),
            _ => (KeyPad::empty(), KeyPad::empty()),
        }
    }

    pub fn shot(&self, frame: u32) -> Option<String> {
        match self.current(frame) {
            Some((at, Step::Shot(n))) if *at == frame => Some(n.clone()),
            _ => None,
        }
    }

    pub fn cam(&self, frame: u32) -> Option<[f32; 4]> {
        match self.current(frame) {
            Some((at, Step::Cam(c))) if *at == frame => Some(*c),
            _ => None,
        }
    }

    pub fn tap(&self, frame: u32) -> Option<(f32, f32)> {
        match self.current(frame) {
            Some((at, Step::Tap(x, y))) if *at == frame => Some((*x, *y)),
            _ => None,
        }
    }

    pub fn done(&self, frame: u32) -> bool {
        matches!(self.current(frame), Some((_, Step::Quit)))
    }
}
