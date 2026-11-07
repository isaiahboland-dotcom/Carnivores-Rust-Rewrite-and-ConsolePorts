use super::support::setup;
use crate::dino::ai;
use crate::game::species_named;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use c2_formats::bits::Bits;
use c2_formats::rules::Rules;

fn of_kind(h: &crate::game::Hunt, ctype: usize) -> usize {
    h.herd.chars.iter().filter(|c| c.ctype == ctype).count()
}

#[test]
fn secrets_are_placed_on_their_areas_only() {
    let (area, mut h, _) = setup(false);
    h.herd.species[1].info.name = "Stego".into();
    let rules = Rules::parse(b"secret 3 area1,area4 stego\n");
    assert_eq!(h.apply_spawn_rules(&area, &rules, "AREA2"), 0);
    assert_eq!(h.apply_spawn_rules(&area, &rules, "AREA1"), 3);
    assert_eq!(of_kind(&h, 1), 4);
    let (px, pz) = (h.player.x, h.player.z);
    for c in &h.herd.chars[1..] {
        assert!((c.pos[0] - px).abs() + (c.pos[2] - pz).abs() >= 256.0 * 40.0);
    }
}

#[test]
fn picked_animals_are_capped_per_area() {
    let (area, mut h, _) = setup(false);
    h.opts.targets = Bits::bit(ai::STEGO as usize);
    let c = h.herd.chars[0].clone();
    h.herd.chars.extend((0..4).map(|_| c.clone()));
    let rules = Rules::parse(b"area area3 huntable 2\n");
    h.apply_spawn_rules(&area, &rules, "area1");
    assert_eq!(of_kind(&h, 1), 5, "another area's cap");
    h.apply_spawn_rules(&area, &rules, "area3");
    assert_eq!(of_kind(&h, 1), 2);
}

#[test]
fn repeated_names_are_numbered() {
    let (_, mut h, _) = setup(false);
    for s in &mut h.herd.species {
        s.info.name = "Raptor".into();
    }
    let names: Vec<Option<usize>> = ["raptor", "Raptor #2", "Raptor #3", "Rex"]
        .iter()
        .map(|n| species_named(&h.herd.species, n))
        .collect();
    assert_eq!(names, [Some(0), Some(1), None, None]);
}
