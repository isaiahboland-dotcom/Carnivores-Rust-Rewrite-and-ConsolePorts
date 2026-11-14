//! The memory figures of `C2RS/INFO.TXT`: what each piece of a hunt costs
//! on the 3DS, for its menu and its memory plan (`c2-3ds`'s `budget.rs`).
//! All in KiB.

use std::collections::BTreeMap;

/// Bytes of one vertex in the 3DS's buffers (`c2-3ds`'s `gpu::Vertex`).
const VERTEX_BYTES: usize = 24;

#[derive(Default)]
pub struct Figures {
    /// Linear memory by scope: `hunt`, `AREAn`, `m:{model}`, `wpn{i}`,
    /// `call{id}`.
    pub linear: BTreeMap<String, usize>,
    /// Main memory by scope: `m:{model}`, `wpn{i}`.
    pub heap: BTreeMap<String, usize>,
    /// Each area's loading peak with the hunter and its ambient animals.
    pub area_heap: BTreeMap<String, usize>,
    /// The same with the hunter alone.
    pub area_heap_bare: BTreeMap<String, usize>,
    /// The ambient models in each area's `area_heap`.
    pub ambients: BTreeMap<String, Vec<String>>,
    /// Face corners of each model, by scope (`m:{model}`).
    pub corners: BTreeMap<String, usize>,
    /// Areas with water (aquatic animals are planned for these only).
    pub water: Vec<String>,
}

/// The lines: `cost.*` (linear), `heap.*` (main), and for the memory plan
/// `heapbase.AREAn`, `ambients.AREAn`, `inst.m:{model}` (the vertex
/// buffer one drawn animal of the model takes, linear) and `water.AREAn`.
pub fn lines(f: &Figures) -> String {
    let mut out = String::new();
    for (scope, kb) in &f.linear {
        out += &format!("cost.{scope} = {kb}\n");
    }
    for (scope, kb) in f.heap.iter().chain(&f.area_heap) {
        out += &format!("heap.{scope} = {kb}\n");
    }
    for (area, kb) in &f.area_heap_bare {
        out += &format!("heapbase.{area} = {kb}\n");
    }
    for (area, models) in &f.ambients {
        out += &format!("ambients.{area} = {}\n", models.join(","));
    }
    for (scope, corners) in f.corners.iter().filter(|(s, _)| s.starts_with("m:")) {
        out += &format!(
            "inst.{scope} = {}\n",
            (corners * VERTEX_BYTES).div_ceil(1024)
        );
    }
    for area in &f.water {
        out += &format!("water.{area} = 1\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_info::PackInfo;

    fn map<V: Clone>(v: &[(&str, V)]) -> BTreeMap<String, V> {
        v.iter().map(|(k, x)| (k.to_string(), x.clone())).collect()
    }

    fn figures() -> Figures {
        Figures {
            linear: map(&[
                ("hunt", 900),
                ("AREA1", 4000),
                ("m:MOSH", 120),
                ("call10", 60),
            ]),
            heap: map(&[("m:MOSH", 300), ("m:TREX", 900), ("wpn0", 80)]),
            area_heap: map(&[("AREA1", 24000), ("TROPHY", 20000)]),
            area_heap_bare: map(&[("AREA1", 23400), ("TROPHY", 20000)]),
            ambients: map(&[
                ("AREA1", vec!["MOSH".to_string(), "GALL".to_string()]),
                ("TROPHY", vec![]),
            ]),
            corners: map(&[
                ("m:MOSH", 300),
                ("m:TREX", 4096),
                ("wpn0", 900),
                ("hunt", 50),
            ]),
            water: vec!["AREA1".to_string()],
        }
    }

    #[test]
    fn the_3ds_reads_the_plan_figures_back() {
        let info = PackInfo::parse(&format!("pack = 5\n{}", lines(&figures())));
        assert_eq!(info.cost("m:MOSH"), 120);
        assert_eq!(info.cost("call10"), 60);
        assert_eq!(info.heap("AREA1"), 24000);
        assert_eq!(info.heap("m:TREX"), 900);
        assert_eq!(info.heap_bases.get("AREA1"), Some(&23400));
        assert_eq!(info.ambients.get("AREA1").map(Vec::len), Some(2));
        assert!(info.planned("AREA1"));
        // An area without ambient animals is planned too (an empty list).
        assert!(info.planned("TROPHY"));
        assert!(!info.planned("AREA2"));
        // 300 corners of 24 bytes round up to 8 KiB; 4096 are 96 KiB.
        assert_eq!(info.insts.get("m:MOSH"), Some(&8));
        assert_eq!(info.insts.get("m:TREX"), Some(&96));
        assert!(info.has_water("AREA1") && !info.has_water("TROPHY"));
    }

    #[test]
    fn only_animal_models_get_an_instance_figure() {
        let text = lines(&figures());
        assert!(!text.contains("inst.wpn0"));
        assert!(!text.contains("inst.hunt"));
    }

    #[test]
    fn a_pack_from_before_the_plan_has_no_plan_figures() {
        let info = PackInfo::parse("pack = 4\nheap.AREA1 = 24000\ncost.AREA1 = 4000\n");
        assert!(!info.planned("AREA1"));
        assert!(info.insts.is_empty());
        assert_eq!(info.heap("AREA1"), 24000);
    }
}
