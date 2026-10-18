//! Synthetic TGA / WAV tests. Fixtures are generated here; no game data.

use c2_formats::{tga, wav};

fn tga_header(image_type: u8, w: u16, h: u16, bpp: u8, desc: u8, id: &[u8]) -> Vec<u8> {
    let mut v = vec![id.len() as u8, 0, image_type, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(&[bpp, desc]);
    v.extend_from_slice(id);
    v
}

#[test]
fn tga_16bit_roundtrip_flips_rows() {
    let pic = tga::Picture {
        width: 2,
        height: 2,
        pixels: vec![1, 2, 0x8003, 4],
    };
    let bytes = tga::write(&pic);
    // Bottom-up on disk: the second row comes first.
    assert_eq!(&bytes[18..], &[0x03, 0x80, 4, 0, 1, 0, 2, 0]);
    assert_eq!(tga::parse(&bytes).ok(), Some(pic));
}

#[test]
fn tga_24bit_top_left_with_id() {
    let mut d = tga_header(2, 1, 2, 24, 0x20, b"hi");
    d.extend_from_slice(&[0xff, 0, 0, /* */ 0, 0, 0xff]); // blue, red (BGR)
    let p = tga::parse(&d).expect("parse");
    assert_eq!(p.pixels, vec![0x001f, 0x7c00]);
}

#[test]
fn tga_rle_32bit() {
    let mut d = tga_header(10, 3, 1, 32, 0x20, &[]);
    d.extend_from_slice(&[0x82, 0, 0xff, 0, 0xff]); // run of 3 green, alpha 255
    let p = tga::parse(&d).expect("parse");
    assert_eq!(p.pixels, vec![0x83e0; 3]);
}

#[test]
fn tga_bad_input_errors() {
    let full = tga::write(&tga::Picture {
        width: 4,
        height: 4,
        pixels: vec![0; 16],
    });
    for cut in [0, 5, 17, 20] {
        assert!(tga::parse(&full[..cut]).is_err());
    }
    let mut cmap = tga_header(1, 1, 1, 8, 0, &[]);
    cmap[1] = 1;
    assert!(tga::parse(&cmap).is_err());
}

fn wav_bytes(channels: u16, rate: u32, pcm: &[u8], data_len: u32) -> Vec<u8> {
    let mut v = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * 2 * u32::from(channels)).to_le_bytes());
    v.extend_from_slice(&(2 * channels).to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v.extend_from_slice(pcm);
    v
}

#[test]
fn wav_vanilla_format() {
    let w = wav::parse(&wav_bytes(1, 22050, &[1, 2, 3, 4], 4)).expect("parse");
    assert_eq!(w.format, Some(wav::WavFormat::VANILLA));
    assert_eq!(w.pcm, vec![1, 2, 3, 4]);
}

#[test]
fn wav_overlong_length_is_clamped_and_format_reported() {
    let w = wav::parse(&wav_bytes(2, 16000, &[1, 2], 1000)).expect("parse");
    assert_eq!(w.pcm, vec![1, 2]);
    assert_ne!(w.format, Some(wav::WavFormat::VANILLA));
}

#[test]
fn wav_without_data_chunk_errors() {
    assert!(wav::parse(b"RIFF").is_err());
    assert!(wav::parse(&[0u8; 64]).is_err());
}
