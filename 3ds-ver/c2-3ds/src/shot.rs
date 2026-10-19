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

/// Writes the top screen (400x240) as a TGA, and the bottom one (320x240)
/// next to it as `<name>_b.tga`.
pub fn save_top(path: &str) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    unsafe { ctru_sys::gspWaitForEvent(ctru_sys::GSPGPU_EVENT_VBlank0, true) };
    save(ctru_sys::GFX_TOP, 400, path)?;
    save(ctru_sys::GFX_BOTTOM, 320, &path.replace(".tga", "_b.tga"))
}

/// One screen, `width` x 240. Framebuffers are the screen turned sideways:
/// 240 rows of `width` pixels become `width` columns of 240, bottom to top;
/// the top screen is RGB8 (B, G, R bytes), the bottom RGB565.
fn save(screen: ctru_sys::gfxScreen_t, width: usize, path: &str) -> std::io::Result<()> {
    let (mut w, mut h) = (0u16, 0u16);
    let fb = unsafe { ctru_sys::gfxGetFramebuffer(screen, ctru_sys::GFX_LEFT, &mut w, &mut h) };
    let (w, h) = (usize::from(w), usize::from(h));
    if fb.is_null() || w != 240 || h < width {
        return Err(std::io::Error::other("no framebuffer"));
    }
    let bpp = if screen == ctru_sys::GFX_TOP { 3 } else { 2 };
    let px = unsafe { std::slice::from_raw_parts(fb, w * h * bpp) };
    let mut out = Vec::with_capacity(18 + width * 240 * 3);
    out.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    out.extend_from_slice(&(width as u16).to_le_bytes());
    out.extend_from_slice(&240u16.to_le_bytes());
    out.extend_from_slice(&[24, 0]);
    // TGA rows go bottom to top by default.
    for y in (0..240).rev() {
        for x in 0..width {
            let i = (x * 240 + (239 - y)) * bpp;
            if bpp == 3 {
                out.extend_from_slice(&px[i..i + 3]);
            } else {
                let v = u16::from_le_bytes([px[i], px[i + 1]]);
                let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                out.extend_from_slice(&[(b << 3) as u8, (g << 2) as u8, (r << 3) as u8]);
            }
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
