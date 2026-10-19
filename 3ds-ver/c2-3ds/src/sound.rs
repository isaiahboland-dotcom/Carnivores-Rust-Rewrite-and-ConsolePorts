//! Sound on the 3DS DSP (`Audio.cpp`): the pack's sounds in linear memory,
//! one DSP channel per voice. Which sound each event plays and the left /
//! right levels come from `c2_core::sound`, as on PC.
//!
//! Channels: 0..16 one-shot voices (`MAX_CHANNEL`), 16 and 17 the ambience
//! (two, to crossfade), 18 the ship's engine loop.

use std::collections::HashMap;
use std::ffi::c_void;

use c2_core::game::{GameEvent, Hunt};
use c2_core::ship::ship_sound;
use c2_core::sound::{ambient_gain, commands, lr_gains, SoundCmd, SoundId, MAX_VOICES};
use c2_vfs::Vfs;
use ctru_sys as sys;

const AMBIENT_CH: [i32; 2] = [16, 17];
const ENGINE_CH: i32 = 18;
/// `SetAmbient` ramps over ~16 mixing chunks of 1024 samples: 0.74 s.
const FADE_S: f32 = 0.74;

/// 16-bit mono samples in linear memory (the DSP reads them directly).
struct Clip {
    ptr: *mut i16,
    len: usize,
}

impl Clip {
    fn new(pcm: &[u8]) -> Option<Self> {
        let len = pcm.len() / 2;
        if len == 0 {
            return None;
        }
        let ptr = unsafe { sys::linearAlloc(len * 2) } as *mut i16;
        if ptr.is_null() {
            return None;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(pcm.as_ptr(), ptr as *mut u8, len * 2);
            let _ = sys::DSP_FlushDataCache(ptr as *const c_void, (len * 2) as u32);
        }
        Some(Self { ptr, len })
    }
}

impl Drop for Clip {
    fn drop(&mut self) {
        unsafe { sys::linearFree(self.ptr as *mut c_void) };
    }
}

/// A playing channel. The DSP keeps a pointer to the wave buffer, so it is
/// boxed and kept until the channel is stopped.
struct Voice {
    buf: Box<sys::ndspWaveBuf>,
    pos: Option<[f32; 3]>,
    volume: f32,
}

impl Voice {
    fn done(&self) -> bool {
        self.buf.status == sys::NDSP_WBUF_DONE as u8
    }
}

fn set_gains(ch: i32, (l, r): (f32, f32)) {
    let mut mix = [0.0f32; 12];
    mix[0] = l;
    mix[1] = r;
    unsafe { sys::ndspChnSetMix(ch, mix.as_mut_ptr()) };
}

fn start(
    ch: i32,
    clip: &Clip,
    rate: f32,
    looping: bool,
    gains: (f32, f32),
) -> Box<sys::ndspWaveBuf> {
    let mut buf: Box<sys::ndspWaveBuf> = Box::new(unsafe { std::mem::zeroed() });
    buf.__bindgen_anon_1.data_pcm16 = clip.ptr;
    buf.nsamples = clip.len as u32;
    buf.looping = looping;
    unsafe {
        sys::ndspChnReset(ch);
        sys::ndspChnSetInterp(ch, sys::NDSP_INTERP_LINEAR);
        sys::ndspChnSetRate(ch, rate);
        sys::ndspChnSetFormat(ch, sys::NDSP_FORMAT_MONO_PCM16 as u16);
        set_gains(ch, gains);
        sys::ndspChnWaveBufAdd(ch, &mut *buf);
    }
    buf
}

fn stop(ch: i32) {
    unsafe { sys::ndspChnWaveBufClear(ch) };
}

struct Ambient {
    zone: Option<usize>,
    /// Kept while the DSP reads it.
    _buf: Box<sys::ndspWaveBuf>,
    /// Current and target level.
    level: f32,
    target: f32,
}

pub struct Sound {
    rate: f32,
    clips: HashMap<SoundId, Clip>,
    ambients: Vec<Option<Clip>>,
    underwater: Option<Clip>,
    voices: Vec<Option<Voice>>,
    /// Per ambience channel.
    ambient: [Option<Ambient>; 2],
    engine: Option<(Box<sys::ndspWaveBuf>, [f32; 3])>,
    cam: [f32; 3],
    alpha: f32,
}

/// `C2RS/SND/{name}.PCM` names (see `c2-cook`) to what they are.
enum Name {
    Id(SoundId),
    Ambient(usize),
    Underwater,
}

fn parse_name(n: &str) -> Option<Name> {
    let pair = |s: &str| -> Option<(usize, usize)> {
        let (a, b) = s.split_once('_')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let num = |s: &str| s.parse::<usize>().ok();
    Some(Name::Id(if let Some(r) = n.strip_prefix("SHIP_") {
        SoundId::Ship(num(r)?)
    } else if let Some(r) = n.strip_prefix("FW") {
        SoundId::StepWater(num(r)?)
    } else if let Some(r) = n.strip_prefix('C') {
        let (ctype, sound) = pair(r)?;
        SoundId::Species { ctype, sound }
    } else if let Some(r) = n.strip_prefix('W') {
        let (weapon, slot) = pair(r)?;
        SoundId::Weapon { weapon, slot }
    } else if let Some(r) = n.strip_prefix('K') {
        let (ai, variant) = pair(r)?;
        SoundId::Call {
            ai: ai as i32,
            variant,
        }
    } else if let Some(r) = n.strip_prefix('R') {
        SoundId::Random(num(r)?)
    } else if let Some(r) = n.strip_prefix('F') {
        SoundId::Step(num(r)?)
    } else if let Some(r) = n.strip_prefix('H') {
        SoundId::Scream(num(r)?)
    } else if let Some(r) = n.strip_prefix('A') {
        return Some(Name::Ambient(num(r)?));
    } else if n == "U" {
        return Some(Name::Underwater);
    } else {
        return None;
    }))
}

impl Sound {
    /// Starts the DSP and loads the pack's sounds. `Err` says why there is
    /// no sound (usually: no DSP firmware dump on the SD card).
    pub fn new(vfs: &Vfs, rate: u32) -> Result<Self, String> {
        if unsafe { sys::ndspInit() } < 0 {
            return Err("no sound: DSP firmware missing (sdmc:/3ds/dspfirm.cdc)".into());
        }
        unsafe { sys::ndspSetOutputMode(sys::NDSP_OUTPUT_STEREO) };
        let mut s = Self {
            rate: rate as f32,
            clips: HashMap::new(),
            ambients: Vec::new(),
            underwater: None,
            voices: (0..MAX_VOICES).map(|_| None).collect(),
            ambient: [None, None],
            engine: None,
            cam: [0.0; 3],
            alpha: 0.0,
        };
        let names: Vec<String> = vfs
            .files()
            // Keys are lower case; the layer's own spelling is upper.
            .filter_map(|(_, r)| {
                r.path
                    .strip_prefix("C2RS/SND/")?
                    .strip_suffix(".PCM")
                    .map(str::to_string)
            })
            .collect();
        for n in names {
            let Some(clip) = vfs
                .read(&format!("C2RS/SND/{n}.PCM"))
                .ok()
                .and_then(|d| Clip::new(&d))
            else {
                continue;
            };
            match parse_name(&n) {
                Some(Name::Id(id)) => {
                    s.clips.insert(id, clip);
                }
                Some(Name::Ambient(k)) => {
                    if s.ambients.len() <= k {
                        s.ambients.resize_with(k + 1, || None);
                    }
                    s.ambients[k] = Some(clip);
                }
                Some(Name::Underwater) => s.underwater = Some(clip),
                None => {}
            }
        }
        Ok(s)
    }

    /// Plays what this tick's events start, follows the camera, fades the
    /// ambience and runs the ship's engine loop.
    pub fn update(&mut self, events: &[GameEvent], hunt: &Hunt, dt: f32) {
        let (cam, alpha, _) = hunt.camera();
        self.cam = cam;
        self.alpha = alpha;
        for c in commands(events, hunt) {
            match c {
                SoundCmd::Play { id, pos, volume } => self.play(id, pos, volume),
                SoundCmd::Ambient { zone, volume } => self.set_ambient(zone, volume),
            }
        }
        // Re-pan positioned voices (`AudioSetCameraPos`); free finished ones.
        for (ch, slot) in self.voices.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(Voice::done) {
                *slot = None;
            }
            if let Some(v) = slot {
                if v.pos.is_some() {
                    set_gains(ch as i32, lr_gains(v.volume, v.pos, cam, alpha));
                }
            }
        }
        // Ambience crossfade.
        let step = dt / FADE_S;
        for (i, a) in self.ambient.iter_mut().enumerate() {
            let Some(amb) = a else { continue };
            let full = amb.target.max(amb.level).max(1e-6);
            let d = full * step;
            amb.level = if amb.level < amb.target {
                (amb.level + d).min(amb.target)
            } else {
                (amb.level - d).max(amb.target)
            };
            set_gains(AMBIENT_CH[i], (amb.level, amb.level));
            if amb.level <= 0.0 && amb.target <= 0.0 {
                stop(AMBIENT_CH[i]);
                *a = None;
            }
        }
        // The ship's engine (`SetAmbient3d`) while it flies.
        let ship = &hunt.ship;
        if ship.state == -1 {
            if self.engine.take().is_some() {
                stop(ENGINE_CH);
            }
        } else if let Some((_, at)) = &mut self.engine {
            *at = ship.pos;
            set_gains(ENGINE_CH, lr_gains(256.0, Some(ship.pos), cam, alpha));
        } else if let Some(clip) = self.clips.get(&SoundId::Ship(ship_sound::ENGINE)) {
            let g = lr_gains(256.0, Some(ship.pos), cam, alpha);
            self.engine = Some((start(ENGINE_CH, clip, self.rate, true, g), ship.pos));
        }
    }

    /// `AddVoice3dv`: dropped if all voices are busy, as in the original.
    fn play(&mut self, id: SoundId, pos: Option<[f32; 3]>, volume: i32) {
        let Some(clip) = self.clips.get(&id) else {
            return;
        };
        let Some(ch) = self.voices.iter().position(Option::is_none) else {
            return;
        };
        let volume = volume as f32;
        let g = lr_gains(volume, pos, self.cam, self.alpha);
        let buf = start(ch as i32, clip, self.rate, false, g);
        self.voices[ch] = Some(Voice { buf, pos, volume });
    }

    /// `SetAmbient`: fade the old ambience out and the new one in.
    fn set_ambient(&mut self, zone: Option<usize>, volume: i32) {
        let target = ambient_gain(volume);
        for a in self.ambient.iter_mut().flatten() {
            if a.zone == zone {
                a.target = target;
            } else {
                a.target = 0.0;
            }
        }
        if self.ambient.iter().flatten().any(|a| a.zone == zone) {
            return;
        }
        let clip = match zone {
            Some(z) => self.ambients.get(z).and_then(Option::as_ref),
            None => self.underwater.as_ref(),
        };
        let Some(clip) = clip else { return };
        // A free channel, or the quieter one.
        let i = match &self.ambient {
            [None, _] => 0,
            [_, None] => 1,
            [Some(a), Some(b)] => usize::from(b.level < a.level),
        };
        stop(AMBIENT_CH[i]);
        let buf = start(AMBIENT_CH[i], clip, self.rate, true, (0.0, 0.0));
        self.ambient[i] = Some(Ambient {
            zone,
            _buf: buf,
            level: 0.0,
            target,
        });
    }

    /// For the pause screen: DSP channels playing (voices + ambience +
    /// engine) and clips loaded.
    pub fn status(&self) -> String {
        let playing = (0..=ENGINE_CH)
            .filter(|&c| unsafe { sys::ndspChnIsPlaying(c) })
            .count();
        format!(
            "snd {playing}ch {}clips",
            self.clips.len() + self.ambients.len()
        )
    }

    /// Silence (a new hunt).
    pub fn stop_all(&mut self) {
        for ch in 0..=ENGINE_CH {
            stop(ch);
        }
        for v in &mut self.voices {
            *v = None;
        }
        self.ambient = [None, None];
        self.engine = None;
    }
}

impl Drop for Sound {
    fn drop(&mut self) {
        self.stop_all();
        unsafe { sys::ndspExit() };
    }
}
