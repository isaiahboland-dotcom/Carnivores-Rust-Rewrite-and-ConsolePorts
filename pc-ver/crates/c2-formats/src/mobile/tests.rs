use super::*;

/// A `.pkm` with the given dimensions and `blocks` bytes of ETC1 data.
fn pkm(bw: u16, bh: u16, w: u16, h: u16, blocks: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(b"PKM 10");
    d.extend_from_slice(&0u16.to_be_bytes()); // format
    d.extend_from_slice(&bw.to_be_bytes());
    d.extend_from_slice(&bh.to_be_bytes());
    d.extend_from_slice(&w.to_be_bytes());
    d.extend_from_slice(&h.to_be_bytes());
    d.extend_from_slice(blocks);
    d
}

#[test]
fn pkm_allocates_from_the_input_not_the_declared_dimensions() {
    // Audit F3: the header may claim any dimensions; the buffer is
    // bounded by the bytes actually present (tga.rs's rule).
    let img = decode_pkm(&pkm(8, 8, 8, 8, &[0u8; 32])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((img.width, img.height), (8, 8));
    assert_eq!(img.pixels.len(), 8 * 8 * 4);

    // 65535 x 65535 claims ~17 GB of pixels from ~48 bytes of input:
    // rejected with a count error, never allocated.
    let hostile = pkm(8, 8, 65535, 65535, &[0u8; 32]);
    let Err(err) = decode_pkm(&hostile) else {
        panic!("a pkm larger than its input must be rejected");
    };
    assert!(matches!(err.kind, FormatErrorKind::BadCount(_)), "{err}");
}

#[test]
fn pkm_shorter_than_its_blocks_is_an_error_not_a_panic() {
    let short = pkm(8, 8, 8, 8, &[0u8; 7]);
    assert!(decode_pkm(&short).is_err());
}

#[test]
fn pkm_validates_padding_format_and_full_encoded_payload() {
    let image = decode_pkm(&pkm(8, 4, 5, 3, &[0u8; 16])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((image.width, image.height, image.pixels.len()), (5, 3, 60));
    assert!(image.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    for (bw, bh, w, h, payload) in [
        (0, 0, 0, 0, 0),
        (4, 4, 8, 8, 8),
        (6, 4, 5, 3, 16),
        (12, 4, 5, 3, 24),
        // Enough bytes for the cropped pixels, but not the encoded blocks.
        (4, 8, 1, 5, 8),
    ] {
        assert!(decode_pkm(&pkm(bw, bh, w, h, &alloc::vec![0; payload])).is_err());
    }
    let mut unsupported = pkm(4, 4, 4, 4, &[0; 8]);
    unsupported[7] = 1;
    assert!(decode_pkm(&unsupported).is_err());
}

#[test]
fn cat_lists_animations_and_sounds() {
    let t = "carnivores_character\r\n\r\n[general]\r\ntitle = \"Rex\"\r\nmodel = \"rex.3dn\"\r\n\
[animations]\r\ncount = 2\r\nanimation_id 0\r\n\tfile = \"rex_walk\"\r\n\tsound_index = 1\r\n\
animation_id 1\r\n\tfile = \"rex_die\"\r\n[sounds]\r\ncount = 2\r\nsound_id 0\r\n\tfile = \"a\"\r\n\
sound_id 1\r\n\tfile = \"b\"\r\n";
    let c = parse_cat(t).unwrap_or_default();
    assert_eq!((c.title.as_str(), c.model.as_str()), ("Rex", "rex.3dn"));
    assert_eq!(
        c.animations,
        vec![("rex_walk".into(), Some(1)), ("rex_die".into(), None)]
    );
    assert_eq!(c.sounds, vec!["a".to_string(), "b".to_string()]);
    assert!(parse_cat("hello").is_err());
}

fn model() -> Vec<u8> {
    let mut d = Vec::new();
    for v in [3u32, 1, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    let mut name = [0u8; 32];
    name[..3].copy_from_slice(b"rex");
    d.extend_from_slice(&name);
    d.extend_from_slice(&0u32.to_le_bytes());
    for i in 0..3 {
        for f in [i as f32, 1.0, 2.0] {
            d.extend_from_slice(&f.to_le_bytes());
        }
        d.extend_from_slice(&[7, 0, 0, 0]);
    }
    for v in [0i32, 1, 2] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for (u, v) in [(10u16, 20u16), (30, 40), (50, 60)] {
        d.extend_from_slice(&u.to_le_bytes());
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&3u16.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    for v in [-1i32, 5, 0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&[0; 12]);
    d
}

#[test]
fn model_reads_like_a_car_mesh() {
    let (name, m) = parse_3dn(&model()).unwrap_or_default();
    assert_eq!(name, "rex");
    assert_eq!(m.vertices.len(), 3);
    assert_eq!(m.vertices[2].x, 2.0);
    assert_eq!(m.vertices[0].owner, 7);
    let f = &m.faces[0];
    assert_eq!(
        (f.tu, f.tv, f.flags, f.next),
        ([10, 30, 50], [20, 40, 60], 3, 5)
    );
    // Truncated: an error, not a panic.
    assert!(parse_3dn(&model()[..60]).is_err());
}

#[test]
fn animation_needs_the_models_vertex_count() {
    let mut d = Vec::new();
    for v in [20u32, 2, 3] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend((0..18i16).flat_map(|v| v.to_le_bytes()));
    let a = parse_ani(&d, "rex_walk", 3).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        (a.kps, a.frames, a.data.len(), a.name()),
        (20, 2, 18, "rex_walk".into())
    );
    assert!(parse_ani(&d, "x", 4).is_err());
}

#[test]
fn etc1_individual_and_differential_blocks() {
    // Individual mode, both halves grey 0x88, table 0: index 0 adds 2.
    let px = etc1_block(&[0x88, 0x88, 0x88, 0x00, 0, 0, 0, 0]);
    assert!(px.iter().all(|p| *p == [138, 138, 138]));
    // Differential, red 31 / delta -1 (left / right halves), msb set on
    // every pixel (index 2: subtract the small modifier, 2).
    let px = etc1_block(&[0xFF, 0, 0, 0x02, 0xFF, 0xFF, 0, 0]);
    assert_eq!(px[0], [253, 0, 0]);
    assert_eq!(px[3], [245, 0, 0]);
}

#[test]
fn ima_adpcm_decodes_a_block() {
    let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    d.extend_from_slice(&20u32.to_le_bytes());
    for v in [0x11u16, 1] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&22050u32.to_le_bytes());
    d.extend_from_slice(&11025u32.to_le_bytes());
    d.extend_from_slice(&8u16.to_le_bytes()); // block align
    d.extend_from_slice(&4u16.to_le_bytes());
    d.extend_from_slice(&[2, 0, 13, 0]);
    d.extend_from_slice(b"data");
    d.extend_from_slice(&8u32.to_le_bytes());
    d.extend_from_slice(&[100, 0, 0, 0, 0x77, 0x77, 0x77, 0x77]);
    let (pcm, rate) = decode_wav(&d).unwrap_or_default();
    assert_eq!(rate, 22050);
    // Header sample, then 8 nibbles, rising.
    assert_eq!(pcm.len(), 9 * 2);
    let s: Vec<i16> = pcm
        .chunks(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(s[0], 100);
    assert!(s.windows(2).all(|w| w[1] > w[0]));
}

#[test]
fn stereo_ima_adpcm_is_mixed_to_mono() {
    let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    d.extend_from_slice(&20u32.to_le_bytes());
    for v in [0x11u16, 2] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&22050u32.to_le_bytes());
    d.extend_from_slice(&22050u32.to_le_bytes());
    d.extend_from_slice(&16u16.to_le_bytes()); // block align
    d.extend_from_slice(&4u16.to_le_bytes());
    d.extend_from_slice(&[2, 0, 13, 0]);
    d.extend_from_slice(b"data");
    d.extend_from_slice(&16u32.to_le_bytes());
    // Left starts at 100, right at 300; then four bytes each: the left
    // stays (nibble 0 adds a little), the right too.
    d.extend_from_slice(&[100, 0, 0, 0, 44, 1, 0, 0]);
    d.extend_from_slice(&[0; 8]);
    let (pcm, _) = decode_wav(&d).unwrap_or_default();
    let s: Vec<i16> = pcm
        .chunks(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_eq!(s.len(), 9);
    assert_eq!(s[0], 200);
}
