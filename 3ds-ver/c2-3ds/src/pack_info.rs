//! `C2RS/INFO.TXT`: what the cooker put in the pack, and what each piece of
//! a hunt costs in memory. No console code here: `c2-cook` builds this file
//! too, for its tests.

use std::collections::HashMap;

/// The INFO.TXT `pack` this build writes and reads: 5 = every area, animal
/// model (by file, `C2RS/M/`) and weapon; calls by menu slot; `cost.*`
/// (linear) and `heap.*` (main memory) lines; `dialect` for Modder's Edition
/// rules; and the memory plan's figures (`heapbase.*`, `ambients.*`,
/// `inst.*`).
pub const PACK_VERSION: u32 = 5;
/// The oldest `pack` still read: 4 has no memory-plan figures, so its hunts
/// load as they did before the plan (`budget::plan`).
pub const PACK_OLDEST: u32 = 4;

pub fn kv(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// `C2RS/INFO.TXT`: what the cooker put in the pack.
#[derive(Debug, Clone, Default)]
pub struct PackInfo {
    /// Pack layout version (`pack` in INFO.TXT).
    pub pack: u32,
    pub target: String,
    /// `AREA1`.. and `TROPHY`, as packed.
    pub areas: Vec<String>,
    pub view: u32,
    /// Ground at full detail this far (`view` in packs from before it).
    pub detail: u32,
    pub object_view: u32,
    /// Sample rate of the pack's sounds.
    pub sound_rate: u32,
    /// Whether the pack has sounds (`c2-cook --no-sound` leaves them out).
    pub sound: bool,
    /// Linear memory a hunt may use, and what each piece takes (KiB):
    /// `hunt` (always), `AREAn`, `m:{model}`, `wpn{i}`, `call{id}`.
    pub linear_kb: usize,
    pub costs: HashMap<String, usize>,
    /// Main memory: the budget, each area's loading peak with the hunter
    /// and ambient animals (`AREAn`), and each model and weapon.
    pub heap_kb: usize,
    pub heaps: HashMap<String, usize>,
    /// Each area's loading peak with the hunter alone (`heapbase.AREAn`):
    /// with it, the ambient animals are a budgeted choice. Empty in packs
    /// from before the memory plan.
    pub heap_bases: HashMap<String, usize>,
    /// The ambient models each area's figure in `heaps` includes
    /// (`ambients.AREAn = KEY,KEY`).
    pub ambients: HashMap<String, Vec<String>>,
    /// Linear memory one animal of a model takes while it is drawn
    /// (`inst.m:{model}`, KiB): its own vertex buffer.
    pub insts: HashMap<String, usize>,
    /// Areas where it snows by their `.c2map` (Ice Age's beside C2's).
    pub snow: Vec<String>,
    /// Where each roaming add-on (character index) spawns, as the cooker
    /// worked it out (`roam.N = AREA3,AREA7`); `None` in older packs.
    pub roam_areas: Option<HashMap<usize, Vec<String>>>,
    /// Whether the game has add-ons (`None` in older packs).
    pub addons: Option<bool>,
}

fn list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect()
}

impl PackInfo {
    pub fn parse(text: &str) -> Self {
        let m = kv(text);
        let get = |k: &str| m.get(k).cloned().unwrap_or_default();
        let num = |k: &str, d: u32| get(k).parse().unwrap_or(d);
        Self {
            pack: num("pack", 1),
            target: get("target"),
            areas: list(&get("areas")),
            view: num("view", 42),
            detail: num("detail", num("view", 42)),
            object_view: num("object_view", 28),
            sound_rate: num("sound_rate", c2_core::sound::SAMPLE_RATE),
            sound: num("sound", 1) != 0,
            linear_kb: num("linear_kb", 16 * 1024) as usize,
            costs: Self::prefixed(&m, "cost."),
            heap_kb: num("heap_kb", 34 * 1024) as usize,
            heaps: Self::prefixed(&m, "heap."),
            heap_bases: Self::prefixed(&m, "heapbase."),
            ambients: m
                .iter()
                .filter_map(|(k, v)| Some((k.strip_prefix("ambients.")?.to_string(), list(v))))
                .collect(),
            insts: Self::prefixed(&m, "inst."),
            roam_areas: m.contains_key("roam_map").then(|| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let i = k.strip_prefix("roam.")?.parse().ok()?;
                        Some((i, v.split(',').map(|a| a.trim().to_string()).collect()))
                    })
                    .collect()
            }),
            addons: m.get("addons").map(|v| v.trim() != "0"),
            snow: list(&get("snow")),
        }
    }

    fn prefixed(m: &HashMap<String, String>, prefix: &str) -> HashMap<String, usize> {
        m.iter()
            .filter_map(|(k, v)| Some((k.strip_prefix(prefix)?.to_string(), v.parse().ok()?)))
            .collect()
    }

    pub fn cost(&self, piece: &str) -> usize {
        self.costs.get(piece).copied().unwrap_or(0)
    }

    pub fn heap(&self, piece: &str) -> usize {
        self.heaps.get(piece).copied().unwrap_or(0)
    }

    /// Whether the pack has the memory plan's figures for `area`: its
    /// heap without ambient animals, and which models those are.
    pub fn planned(&self, area: &str) -> bool {
        self.heap_bases.contains_key(area) && self.ambients.contains_key(area)
    }
}
