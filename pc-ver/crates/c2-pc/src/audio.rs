//! Sound output (`Audio.cpp`) on top of kira.
//!
//! The original mixes up to 16 voices in software. A voice positioned in the
//! world keeps that position (it doesn't follow its source) and its left/right
//! gains are recomputed from the camera as it moves (`CalcLRVolumes`). We keep
//! those exact gain curves and drive kira's per-sound volume and panning with
//! them, rather than using kira's own spatial model.

use std::sync::Arc;

use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::sound::PlaybackState;
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Panning, Tween};

/// `MAX_CHANNEL`.
const MAX_VOICES: usize = 16;
/// `MIN_RADIUS` in `Audio.h`.
const MIN_RADIUS: f32 = 512.0;
/// Every vanilla sound is 16-bit mono at this rate.
pub const SAMPLE_RATE: u32 = 22050;

/// A sound ready to play.
#[derive(Clone)]
pub struct Clip(StaticSoundData);

impl Clip {
    /// From raw 16-bit little-endian mono PCM (as stored in `.CAR`/`.RSC`).
    pub fn from_pcm(pcm: &[u8], sample_rate: u32, channels: u16) -> Option<Self> {
        let samples: Vec<f32> = pcm
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
            .collect();
        let frames: Arc<[Frame]> = if channels == 2 {
            samples
                .chunks_exact(2)
                .map(|s| Frame::new(s[0], s[1]))
                .collect()
        } else {
            samples.iter().map(|&s| Frame::from_mono(s)).collect()
        };
        if frames.is_empty() {
            return None;
        }
        Some(Self(StaticSoundData {
            sample_rate: sample_rate.max(1),
            frames,
            settings: StaticSoundSettings::default(),
            slice: None,
        }))
    }

    /// Loads a `.WAV` file (honouring its format header).
    pub fn from_wav(path: &std::path::Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        let w = c2_formats::wav::parse(&data).ok()?;
        let (rate, ch) = w
            .format
            .map_or((SAMPLE_RATE, 1), |f| (f.sample_rate, f.channels));
        Self::from_pcm(&w.pcm, rate, ch)
    }
}

/// `CalcLRVolumes` as linear (left, right) amplitudes for volume `v0`
/// (0..=256). `pos = None` is a non-positional voice.
fn lr_gains(v0: f32, pos: Option<[f32; 3]>, cam: [f32; 3], alpha: f32) -> (f32, f32) {
    let Some(p) = pos else {
        let g = v0 * 180.0 / 65536.0;
        return (g, g);
    };
    let v0 = v0 * 200.0;
    let (x, y, z) = (p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]);
    let (sa, ca) = alpha.sin_cos();
    let xx = x * ca + z * sa;
    let zz = (z * ca - x * sa).abs();
    let d = (xx * xx + y * y + zz * zz).sqrt() - MIN_RADIUS;
    let mut k = if d <= 0.0 { 1.0 } else { 1224.0 / (1224.0 + d) };
    if d > 6000.0 {
        k *= (4000.0 - (d - 6000.0)) / 4000.0;
    }
    let k = k.max(0.0);
    let fi = xx.abs().atan2(zz);
    let r = 0.7 + 0.3 * fi / (std::f32::consts::PI / 2.0);
    let l = 0.7 - 0.6 * fi / (std::f32::consts::PI / 2.0);
    let (near, far) = (v0 * r * k / 65536.0, v0 * l * k / 65536.0);
    if xx > 0.0 {
        (far, near)
    } else {
        (near, far)
    }
}

/// kira volume + constant-power panning that reproduce `(gl, gr)` exactly.
fn volume_pan(gl: f32, gr: f32) -> (Decibels, Panning) {
    let p2 = gl * gl + gr * gr;
    if p2 <= 1e-12 {
        return (Decibels::SILENCE, Panning::CENTER);
    }
    let amp = (p2 / 2.0).sqrt();
    let m = gr * gr / p2;
    (Decibels(20.0 * amp.log10()), Panning(2.0 * m - 1.0))
}

struct Voice {
    handle: StaticSoundHandle,
    pos: Option<[f32; 3]>,
    volume: f32,
}

pub struct Audio {
    manager: Option<AudioManager<DefaultBackend>>,
    voices: Vec<Voice>,
    ambient: Option<StaticSoundHandle>,
    ambient_volume: f32,
    /// The ship's engine loop (`SetAmbient3d`).
    engine: Option<(StaticSoundHandle, [f32; 3])>,
    cam: [f32; 3],
    alpha: f32,
}

impl Audio {
    /// Opens the default output device; without one, every call is a no-op.
    pub fn new() -> Self {
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("no audio output: {e}");
                None
            }
        };
        Self {
            manager,
            voices: Vec::new(),
            ambient: None,
            ambient_volume: 0.0,
            engine: None,
            cam: [0.0; 3],
            alpha: 0.0,
        }
    }

    fn start(&mut self, clip: &Clip, gains: (f32, f32), looped: bool) -> Option<StaticSoundHandle> {
        let m = self.manager.as_mut()?;
        let (vol, pan) = volume_pan(gains.0, gains.1);
        let mut data = clip.0.volume(vol).panning(pan);
        if looped {
            data = data.loop_region(..);
        }
        m.play(data).ok()
    }

    /// `AddVoice3dv` / `AddVoicev`: `pos = None` plays without position.
    /// Dropped if all 16 voices are busy, as in the original.
    pub fn play(&mut self, clip: Option<&Clip>, pos: Option<[f32; 3]>, volume: i32) {
        let Some(clip) = clip else { return };
        self.voices
            .retain(|v| v.handle.state() != PlaybackState::Stopped);
        if self.voices.len() >= MAX_VOICES {
            return;
        }
        let volume = volume as f32;
        let gains = lr_gains(volume, pos, self.cam, self.alpha);
        if let Some(handle) = self.start(clip, gains, false) {
            self.voices.push(Voice {
                handle,
                pos,
                volume,
            });
        }
    }

    /// `SetAmbient`: crossfade to a new looping ambience (`None` = silence).
    pub fn set_ambient(&mut self, clip: Option<&Clip>, avolume: i32) {
        // ~16 mixing chunks of 1024 samples, as the original ramps.
        let fade = Tween {
            duration: std::time::Duration::from_millis(740),
            ..Default::default()
        };
        if let Some(mut old) = self.ambient.take() {
            old.stop(fade);
        }
        self.ambient_volume = 32000.0 * avolume as f32 / 256.0 / 65536.0;
        let Some(clip) = clip else { return };
        if let Some(mut h) = self.start(clip, (0.0, 0.0), true) {
            let (vol, _) = volume_pan(self.ambient_volume, self.ambient_volume);
            h.set_volume(vol, fade);
            self.ambient = Some(h);
        }
    }

    /// `SetAmbient3d`: the ship engine loop at `pos`, or off.
    pub fn set_engine(&mut self, clip: Option<&Clip>, pos: Option<[f32; 3]>) {
        match (pos, &mut self.engine) {
            (None, _) => {
                if let Some((mut h, _)) = self.engine.take() {
                    h.stop(Tween::default());
                }
            }
            (Some(p), Some((_, at))) => *at = p,
            (Some(p), None) => {
                if let Some(clip) = clip {
                    let gains = lr_gains(256.0, Some(p), self.cam, self.alpha);
                    if let Some(h) = self.start(clip, gains, true) {
                        self.engine = Some((h, p));
                    }
                }
            }
        }
    }

    /// `AudioSetCameraPos`: re-pan every positioned voice for the new view.
    pub fn set_camera(&mut self, pos: [f32; 3], alpha: f32) {
        self.cam = pos;
        self.alpha = alpha;
        let tween = Tween {
            duration: std::time::Duration::from_millis(30),
            ..Default::default()
        };
        let (cam, a) = (self.cam, self.alpha);
        for v in &mut self.voices {
            if v.pos.is_some() {
                let (gl, gr) = lr_gains(v.volume, v.pos, cam, a);
                let (vol, pan) = volume_pan(gl, gr);
                v.handle.set_volume(vol, tween);
                v.handle.set_panning(pan, tween);
            }
        }
        if let Some((h, p)) = &mut self.engine {
            let (gl, gr) = lr_gains(256.0, Some(*p), cam, a);
            let (vol, pan) = volume_pan(gl, gr);
            h.set_volume(vol, tween);
            h.set_panning(pan, tween);
        }
    }

    /// Silence everything (new hunt).
    pub fn stop_all(&mut self) {
        for v in &mut self.voices {
            v.handle.stop(Tween::default());
        }
        self.voices.clear();
        self.set_ambient(None, 0);
        self.set_engine(None, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gains_follow_the_original_curve() {
        let cam = [0.0; 3];
        // Non-positional: 180/65536 per unit of volume, both sides.
        assert_eq!(
            lr_gains(256.0, None, cam, 0.0),
            (256.0 * 180.0 / 65536.0, 256.0 * 180.0 / 65536.0)
        );
        // Close and dead ahead (-z): full k, equal gains (l = r = 0.7).
        let (l, r) = lr_gains(256.0, Some([0.0, 0.0, -100.0]), cam, 0.0);
        assert!((l - r).abs() < 1e-6 && (l - 256.0 * 200.0 * 0.7 / 65536.0).abs() < 1e-4);
        // To the right (+x): louder right.
        let (l, r) = lr_gains(256.0, Some([2000.0, 0.0, 0.0]), cam, 0.0);
        assert!(r > l);
        // Beyond 10000 + MIN_RADIUS: silent.
        assert_eq!(
            lr_gains(256.0, Some([0.0, 0.0, -11000.0]), cam, 0.0),
            (0.0, 0.0)
        );
    }

    #[test]
    fn volume_pan_reproduces_gains() {
        for (gl, gr) in [(0.5, 0.5), (0.2, 0.6), (0.7, 0.1)] {
            let (v, p) = volume_pan(gl, gr);
            let f = Frame::from_mono(1.0).panned(p) * v.as_amplitude();
            assert!(
                (f.left - gl).abs() < 1e-4 && (f.right - gr).abs() < 1e-4,
                "{gl} {gr} -> {f:?}"
            );
        }
    }
}
