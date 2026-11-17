//! Synthetic weapon models laid out as the original ones are (see
//! `VANILLA_RIG`): a hand rig's bones joined by its faces, a gun on bones
//! of its own.

use alloc::vec;
use alloc::vec::Vec;

use super::*;
use c2_formats::hands::Hands;
use c2_formats::model::{Face, Mesh, Vertex};

const ID: &str = "sha256:aa";

/// Adds bones `first..` with `counts` vertices each, as one strip of faces
/// over all of them (so faces cross bones, as a hand's do), at `x`.
fn part(m: &mut Mesh, first: i16, counts: &[usize], x: f32) -> core::ops::Range<usize> {
    let start_face = m.faces.len();
    let start = m.vertices.len();
    for (k, &n) in counts.iter().enumerate() {
        for _ in 0..n {
            let i = m.vertices.len() as f32;
            m.vertices.push(Vertex {
                x,
                y: i,
                z: i * 0.5,
                owner: first + k as i16,
                hide: 0,
            });
        }
    }
    for i in start..m.vertices.len().saturating_sub(2) {
        m.faces.push(Face {
            v: [i as i32, i as i32 + 1, i as i32 + 2],
            tu: [0; 3],
            tv: [0; 3],
            flags: 0,
            dmask: 0,
            distant: 0,
            next: 0,
            group: 0,
            reserved: [0; 12],
        });
    }
    start_face..m.faces.len()
}

fn hidden(mask: &HandMask, m: &Mesh) -> Vec<usize> {
    (0..m.faces.len()).filter(|&k| mask.hides(k)).collect()
}

/// The rifle's layout: one hand on bones 1-23, the gun on 24-27.
fn rifle() -> (Mesh, core::ops::Range<usize>, core::ops::Range<usize>) {
    let mut m = Mesh::default();
    let hand = part(&mut m, 1, &VANILLA_RIG, -20.0);
    let gun = part(&mut m, 24, &[80, 67, 12, 8], 0.0);
    (m, hand, gun)
}

#[test]
fn the_original_hand_rig_is_hidden_and_the_gun_kept() {
    let (m, hand, gun) = rifle();
    let mask = HandMask::new(&m, ID, &Hands::default());
    assert_eq!(mask.source, Source::VanillaRig(1));
    assert_eq!(mask.hands, vec![1]);
    assert_eq!(hidden(&mask, &m), hand.collect::<Vec<_>>());
    let shown = mask.apply(&m);
    assert_eq!(shown.faces.len(), gun.len());
    assert_eq!(shown.vertices, m.vertices, "poses still fit");
    assert_eq!(mask.hand_centres(&m).len(), 1);
    assert_eq!(mask.hand_centres(&m)[0][0], -20.0);
}

#[test]
fn the_pistol_has_two_hands() {
    let mut m = Mesh::default();
    let left = part(&mut m, 0, &VANILLA_RIG, -30.0);
    let gun = part(&mut m, 23, &[136, 52, 8], 0.0);
    let right = part(&mut m, 26, &VANILLA_RIG, 30.0);
    let mask = HandMask::new(&m, ID, &Hands::default());
    assert_eq!(mask.source, Source::VanillaRig(2));
    assert_eq!(mask.hands, vec![0, 26]);
    let want: Vec<usize> = left.chain(right).collect();
    assert_eq!(hidden(&mask, &m), want);
    assert_eq!(mask.apply(&m).faces.len(), gun.len());
    let c = mask.hand_centres(&m);
    assert_eq!((c[0][0], c[1][0]), (-30.0, 30.0));
}

#[test]
fn a_mod_gun_reusing_the_original_hands_is_recognised() {
    // A new gun first, on bones 0-5, the original hand after it.
    let mut m = Mesh::default();
    let gun = part(&mut m, 0, &[200, 41, 39, 8, 5, 90], 0.0);
    let hand = part(&mut m, 27, &VANILLA_RIG, -20.0);
    let mask = HandMask::new(&m, ID, &Hands::default());
    assert_eq!(mask.hands, vec![27]);
    assert_eq!(hidden(&mask, &m), hand.collect::<Vec<_>>());
    assert_eq!(mask.apply(&m).faces.len(), gun.len());
}

#[test]
fn an_unrecognised_model_keeps_everything() {
    // A gun part rigged to a hand bone: the signature breaks, so nothing
    // is assumed.
    let mut odd = VANILLA_RIG;
    odd[2] += 1;
    let mut m = Mesh::default();
    part(&mut m, 1, &odd, -20.0);
    part(&mut m, 24, &[80, 67], 0.0);
    // A gun and a hand-like piece on one bone (most mod guns).
    let mut one = Mesh::default();
    part(&mut one, 0, &[300], 0.0);
    part(&mut one, 0, &[120], -20.0);
    // No hands at all (the crossbow), and nothing at all.
    let mut bow = Mesh::default();
    part(&mut bow, 1, &[14, 14, 14, 14, 50, 16], 0.0);
    // Bones that only match part of the rig (it runs off the end).
    let mut short = Mesh::default();
    part(&mut short, 1, &VANILLA_RIG[..22], 0.0);
    for m in [m, one, bow, short, Mesh::default()] {
        let mask = HandMask::new(&m, ID, &Hands::default());
        assert_eq!(mask.source, Source::Unrecognised);
        assert_eq!(mask.hidden_faces(), 0);
        assert_eq!(mask.apply(&m), m, "the whole model");
        assert!(mask.hand_centres(&m).is_empty());
    }
}

#[test]
fn the_model_itself_is_never_changed() {
    let (m, ..) = rifle();
    let before = m.clone();
    let mask = HandMask::new(&m, ID, &Hands::default());
    let _ = mask.apply(&m);
    assert_eq!(m, before, "the first-person weapon draws the model as read");
}

#[test]
fn pieces_are_numbered_by_first_face_and_seams_join() {
    let mut m = Mesh::default();
    let a = part(&mut m, 0, &[5], 0.0);
    let b = part(&mut m, 0, &[4], 50.0);
    // A seam: a third strip whose first vertex sits exactly on one of
    // the first strip's.
    let first = m.vertices[0];
    let seam = part(&mut m, 0, &[3], 99.0);
    let at = m.vertices.len() - 3;
    m.vertices[at] = Vertex { owner: 0, ..first };
    // A broken face is a piece of its own.
    let mut broken = m.faces[0];
    broken.v = [0, 1, 999];
    m.faces.push(broken);
    let p = pieces(&m);
    assert!(a.clone().all(|k| p[k] == 0));
    assert!(b.clone().all(|k| p[k] == 1));
    assert!(seam.clone().all(|k| p[k] == 0), "joined at the seam");
    assert_eq!(p.last(), Some(&2));
    assert_eq!(pieces(&m), p, "the same every time");
}

#[test]
fn overrides_by_content_add_and_remove_parts() {
    let (m, hand, gun) = rifle();
    let parse = |t: &str| Hands::parse(t.as_bytes());
    // Keep all: the recognised hand shows again.
    let h = parse("model sha256:AA\nkeep all\n");
    let mask = HandMask::new(&m, ID, &h);
    assert_eq!((mask.source, mask.hidden_faces()), (Source::Override, 0));
    // Rules for another file (a different mod's model of the same name)
    // don't apply.
    let mask = HandMask::new(&m, "sha256:bb", &h);
    assert_eq!(mask.source, Source::VanillaRig(1));
    // Drop a gun bone on top of the hand; keep the sleeve (bone 1): faces
    // touching it show.
    let h = parse("model sha256:aa\ndrop owner 27\nkeep owner 1\n");
    let mask = HandMask::new(&m, ID, &h);
    let gun_tail = m
        .faces
        .iter()
        .enumerate()
        .filter(|(_, f)| f.v.iter().all(|&v| m.vertices[v as usize].owner == 27))
        .map(|(k, _)| k);
    let sleeve = |k: &usize| {
        m.faces[*k]
            .v
            .iter()
            .any(|&v| m.vertices[v as usize].owner == 1)
    };
    let mut want: Vec<usize> = hand.clone().filter(|k| !sleeve(k)).collect();
    want.extend(gun_tail);
    assert_eq!(hidden(&mask, &m), want);
    // Pieces: the hand strip is piece 0, the gun piece 1. Keeping a piece
    // wins over everything.
    let h = parse("model sha256:aa\nkeep all\ndrop piece 1\n");
    let mask = HandMask::new(&m, ID, &h);
    assert_eq!(hidden(&mask, &m), gun.clone().collect::<Vec<_>>());
    let h = parse("model sha256:aa\nkeep piece 0\n");
    assert_eq!(HandMask::new(&m, ID, &h).hidden_faces(), 0);
    // A one-bone mod gun, its hand piece named by number.
    let mut one = Mesh::default();
    let g = part(&mut one, 0, &[300], 0.0);
    let arm = part(&mut one, 0, &[120], -20.0);
    let h = parse("model sha256:aa\ndrop piece 1\n");
    let mask = HandMask::new(&one, ID, &h);
    assert_eq!(hidden(&mask, &one), arm.collect::<Vec<_>>());
    assert_eq!(mask.apply(&one).faces.len(), g.len());
}
