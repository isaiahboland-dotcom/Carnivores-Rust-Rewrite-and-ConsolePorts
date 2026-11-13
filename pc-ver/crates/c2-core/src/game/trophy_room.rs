//! The trophy room's memory plan for small-memory targets (the 3DS).
//!
//! Showing a trophy as a 3D statue needs its animal's model, texture and
//! animation loaded. What that costs depends on the animals (a few big
//! add-ons outweigh many small ones), not on how many species were hunted,
//! so no fixed species count is a safe limit. The plan sums the real
//! per-species costs against the target's budget: if every displayed
//! animal fits, the room is the original's; if not, it becomes a
//! low-memory room where each stand is a 2D portrait of the exact animal
//! (its own model and skin) with its figures beneath. Trophy records are
//! never changed by either mode.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use c2_formats::sav::TrophyItem;

/// The original room's stands.
pub const STANDS: usize = 24;

/// Linear memory one portrait texture takes, KiB (128 x 128, 16 bits).
pub const PORTRAIT_KB: usize = 32;

/// Longest line of a stand's plate, in characters (a 128-pixel label in an
/// 8-pixel font).
pub const PLATE_COLS: usize = 16;

/// What the room is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomMode {
    /// 3D statues, as the original.
    Models,
    /// 2D portraits with figures, for a room that doesn't fit as models.
    Portraits,
}

/// Memory, in KiB, the target offers the room and the room already uses
/// before any trophy (area, hunter, ambient animals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub linear_kb: usize,
    pub heap_kb: usize,
    pub base_linear_kb: usize,
    pub base_heap_kb: usize,
}

/// What loading one animal's model takes, KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModelCost {
    pub linear_kb: usize,
    pub heap_kb: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomPlan {
    pub mode: RoomMode,
    /// Displayed species (character indices), each once, in stand order.
    pub species: Vec<usize>,
    /// Species to load as models: all of `species` in [`RoomMode::Models`],
    /// none in [`RoomMode::Portraits`].
    pub load: Vec<usize>,
    /// Species shown as a name plate only: the pack has no portrait.
    pub no_portrait: Vec<usize>,
    /// Estimated linear and main memory of the plan, KiB.
    pub linear_kb: usize,
    pub heap_kb: usize,
}

impl RoomPlan {
    /// Whether the visitor should be told the low-memory room is on.
    pub fn low_memory(&self) -> bool {
        self.mode == RoomMode::Portraits
    }
}

/// The species on display: the stands' animals once each, in stand order.
/// Empty stands (`ctype` 0) and saved records whose animal is missing from
/// the pack (negative, or past `species_count`) are not shown; the record
/// itself stays in the save.
pub fn displayed_species(bodies: &[TrophyItem], species_count: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for b in bodies.iter().take(STANDS) {
        let Some(c) = usize::try_from(b.ctype)
            .ok()
            .filter(|&c| c != 0 && c < species_count)
        else {
            continue;
        };
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Chooses the room's mode. `cost` is a species' model cost, or `None` when
/// the pack never cooked its model (it can't be shown as a statue);
/// `has_portrait` is whether the pack carries its portrait.
pub fn plan(
    bodies: &[TrophyItem],
    species_count: usize,
    budget: Budget,
    cost: &dyn Fn(usize) -> Option<ModelCost>,
    has_portrait: &dyn Fn(usize) -> bool,
) -> RoomPlan {
    let species = displayed_species(bodies, species_count);
    let (mut lin, mut heap) = (budget.base_linear_kb, budget.base_heap_kb);
    let mut fits = true;
    for &s in &species {
        match cost(s) {
            Some(c) => {
                lin += c.linear_kb;
                heap += c.heap_kb;
            }
            None => fits = false,
        }
    }
    fits &= lin <= budget.linear_kb && heap <= budget.heap_kb;
    if fits {
        return RoomPlan {
            mode: RoomMode::Models,
            load: species.clone(),
            species,
            no_portrait: Vec::new(),
            linear_kb: lin,
            heap_kb: heap,
        };
    }
    let no_portrait: Vec<usize> = species
        .iter()
        .copied()
        .filter(|&s| !has_portrait(s))
        .collect();
    let shown = species.len() - no_portrait.len();
    RoomPlan {
        mode: RoomMode::Portraits,
        load: Vec::new(),
        linear_kb: budget.base_linear_kb + shown * PORTRAIT_KB,
        heap_kb: budget.base_heap_kb,
        species,
        no_portrait,
    }
}

/// The three lines of a stand's plate, each at most [`PLATE_COLS`] long:
/// the animal, its score, and its weight and length (`mass`, `length` are
/// the species' figures, scaled by the trophy's `scale`; `us` = US units).
pub fn plate_lines(name: &str, mass: f32, length: f32, b: &TrophyItem, us: bool) -> [String; 3] {
    let sc = b.scale;
    let figures = if us {
        format!("{:.1}t {:.0}ft", mass * sc * sc / 0.907, length * sc / 0.3)
    } else {
        format!("{:.1}T {:.1}m", mass * sc * sc, length * sc)
    };
    let cut = |s: &str| s.chars().take(PLATE_COLS).collect::<String>();
    [cut(name), cut(&format!("Score {}", b.score)), cut(&figures)]
}
