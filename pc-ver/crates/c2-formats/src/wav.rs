//! `.WAV` sounds. Loaded by `Resources.cpp: LoadWav`.
//!
//! The original ignores the `fmt ` chunk: it scans forward from offset 36 for
//! the bytes `data`, reads a length and that many bytes of PCM, which the audio
//! layer plays as 16-bit mono at 22050 Hz. Every vanilla file is exactly that
//! format. We read `fmt ` when present so mismatched mod files can be flagged
//! (or resampled by the cooker), and fall back to the original scan otherwise.

use crate::error::{FormatError, FormatErrorKind};
use crate::reader::Reader;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wav {
    /// `None` when the file had no readable `fmt ` chunk.
    pub format: Option<WavFormat>,
    pub pcm: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavFormat {
    /// 1 = integer PCM.
    pub tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
}

impl WavFormat {
    /// The only format the original engine plays correctly.
    pub const VANILLA: WavFormat = WavFormat {
        tag: 1,
        channels: 1,
        sample_rate: 22050,
        bits: 16,
    };
}

fn parse_fmt(data: &[u8]) -> Option<WavFormat> {
    // RIFF header (12 bytes), then walk chunks.
    if data.get(0..4) != Some(b"RIFF") || data.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let mut pos = 12;
    while let Some(head) = data.get(pos..pos + 8) {
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if &head[..4] == b"fmt " {
            let b = data.get(pos + 8..pos + 24)?;
            return Some(WavFormat {
                tag: u16::from_le_bytes([b[0], b[1]]),
                channels: u16::from_le_bytes([b[2], b[3]]),
                sample_rate: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                bits: u16::from_le_bytes([b[14], b[15]]),
            });
        }
        pos = pos.checked_add(8 + len + (len & 1))?;
    }
    None
}

pub fn parse(data: &[u8]) -> Result<Wav, FormatError> {
    let format = parse_fmt(data);
    let start = data
        .get(36..)
        .and_then(|tail| tail.windows(4).position(|w| w == b"data"))
        .map(|i| 36 + i + 4)
        .ok_or_else(|| FormatError {
            offset: data.len().min(36),
            context: "wav".into(),
            kind: FormatErrorKind::Syntax("no `data` chunk".into()),
        })?;
    let mut r = Reader::new(&data[start..]);
    let len = r.count(usize::MAX, "wav.data_length")?;
    // Some tools write a too-large length; the original's ReadFile just
    // returns what is there, so clamp the same way.
    let pcm = data[start + 4..]
        .get(..len)
        .unwrap_or(&data[start + 4..])
        .to_vec();
    Ok(Wav { format, pcm })
}
