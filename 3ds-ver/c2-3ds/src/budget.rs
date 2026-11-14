//! The memory plan of a hunt: which animal models are loaded and how many
//! animals of each may be placed, by `c2-cook`'s figures (`pack_info`).
//! No console code here: `c2-cook`'s tests build this file too.
//!
//! Memory is spent in this order, each step with what the ones before left:
//!
//! 1. The hunt itself: the area, the hunter, the carried weapons, the
//!    picked animals' calls.
//! 2. One animal of each picked species. A hunt whose picks don't fit is
//!    refused ([`Plan::fits`]).
//! 3. One secret animal (an instance rule's, else an unpicked roamer the
//!    map's tables place), unless secrets are set to none. It never costs
//!    a pick its place.
//! 4. More of each pick, in turn, up to the population the hunt aims for.
//! 5. The other secret animals.
//! 6. Ambient animals: the first thing to go when memory is short.
//! 7. Aquatic animals, the cheapest species first.
//! 8. What is left raises the picks' limits.
//!
//! A pack without the plan's figures (`PackInfo::planned`), a survival and
//! the trophy room load as they did before the plan ([`Plan::planned`] is
//! false): every ambient species when ambient animals are on, no limits.

use c2_core::dino::DinoInfo;
use c2_formats::bits::Bits;

use crate::pack_info::PackInfo;
use crate::rules::{HuntRules, Role};

/// Main memory one placed animal takes, KiB: its `Character`, its share of
/// the herd's lists and the allocator's overhead. Not measured on hardware;
/// several times the structure's size, to err on the high side.
pub const INST_HEAP_KB: usize = 4;

/// Linear memory one placed animal takes when the pack has no figure for
/// its model, KiB: the vertex buffer of a 1,365-face model.
pub const INST_LINEAR_KB: usize = 96;

/// Brachiosaurs the original places when it has one (`PlaceCharacters`).
const BRACHIOSAURS: usize = 3;

/// The most aquatic animals of one species, and in all.
const AQUATIC_EACH: usize = 2;
pub const AQUATIC_MAX: usize = 4;

/// The most picked animals a plan allows (the Modder's Edition Engine's
/// `kCharacterCapacity`).
const HUNTABLE_LIMIT: usize = 256;

/// Linear and main memory, KiB.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kb {
    pub linear: usize,
    pub heap: usize,
}

impl Kb {
    fn plus(self, o: Kb) -> Kb {
        Kb {
            linear: self.linear + o.linear,
            heap: self.heap + o.heap,
        }
    }

    fn times(self, n: usize) -> Kb {
        Kb {
            linear: self.linear * n,
            heap: self.heap * n,
        }
    }

    pub fn within(self, o: Kb) -> bool {
        self.linear <= o.linear && self.heap <= o.heap
    }

    /// What is left of `self` after `used`.
    pub fn left(self, used: Kb) -> Kb {
        Kb {
            linear: self.linear.saturating_sub(used.linear),
            heap: self.heap.saturating_sub(used.heap),
        }
    }
}

/// What the menu picked, as far as memory goes.
#[derive(Debug, Clone, Default)]
pub struct Request {
    /// `AREA3`.
    pub area: String,
    /// A hunt: not a survival, not the trophy room.
    pub hunting: bool,
    /// Bit 10 + menu slot per target.
    pub targets: Bits,
    /// Picked roaming add-ons (character indices).
    pub roamers: Vec<usize>,
    /// Carried weapons.
    pub weapons: Vec<usize>,
    /// Small ambient animals at most (0: none).
    pub ambient_max: usize,
    /// The only ambient species to consider ("Random ambients"), in order.
    pub ambient_only: Option<Vec<usize>>,
    /// Secret animals the map's own tables may bring at most (0: none).
    pub secret_max: usize,
    /// The instance's secret animals for this area: character and count.
    pub rule_secrets: Vec<(usize, usize)>,
    /// The picked animals the hunt aims for in all (the original's main
    /// population at the hunt's density).
    pub huntable_want: usize,
    /// Whether the area has water an aquatic animal can use.
    pub water: bool,
}

/// What a hunt loads and how many animals it may place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Whether the hunt fits: the area and one of each pick.
    pub fits: bool,
    /// False: loaded as before the plan (no figures, survival, trophy
    /// room); the limits below are then not set.
    pub planned: bool,
    /// Per character: `None` not loaded, else the most of it placed
    /// (`usize::MAX`: no limit).
    pub max: Vec<Option<usize>>,
    /// Small ambient animals at most, over every ambient species.
    pub ambients_max: usize,
    /// The ambient species loaded.
    pub ambient_models: Vec<usize>,
    /// The picked species and their limits.
    pub huntables: Vec<(usize, usize)>,
    /// Secret animals: the instance rules' (character, count as planned),
    /// then the map's own (character, limit).
    pub rule_secrets: Vec<(usize, usize)>,
    pub native_secrets: Vec<(usize, usize)>,
    /// Aquatic animals: character and count.
    pub aquatic: Vec<(usize, usize)>,
    /// When the picks don't fit: the costliest pick, to drop first.
    pub drop: Option<usize>,
    /// Memory planned for, and the console's.
    pub used: Kb,
    pub budget: Kb,
}

impl Plan {
    /// Whether character `i`'s model is loaded.
    pub fn loads(&self, i: usize) -> bool {
        self.max.get(i).is_some_and(Option::is_some)
    }

    /// The most picked animals in all.
    pub fn huntables_max(&self) -> usize {
        self.huntables
            .iter()
            .fold(0usize, |n, h| n.saturating_add(h.1))
    }

    /// Secret animals planned in all.
    pub fn secrets(&self) -> usize {
        self.rule_secrets
            .iter()
            .chain(&self.native_secrets)
            .map(|s| s.1)
            .sum()
    }
}

/// The plan's working state: memory spent and models loaded so far.
struct Spend<'a> {
    info: &'a PackInfo,
    rules: &'a HuntRules,
    budget: Kb,
    used: Kb,
    /// Models loaded (by key): characters can share one.
    models: Vec<&'a str>,
}

impl<'a> Spend<'a> {
    fn key(&self, c: usize) -> Option<&'a str> {
        self.rules.keys.get(c).map(String::as_str)
    }

    /// Whether the pack has character `c`'s model.
    fn packed(&self, c: usize) -> bool {
        self.key(c)
            .is_some_and(|k| self.info.costs.contains_key(&format!("m:{k}")))
    }

    /// What loading `c`'s model takes (nothing when it is loaded).
    fn model(&self, c: usize) -> Kb {
        match self.key(c) {
            Some(k) if !self.models.contains(&k) => model_kb(self.info, k),
            _ => Kb::default(),
        }
    }

    /// What one placed animal of `c` takes.
    fn inst(&self, c: usize) -> Kb {
        let linear = self
            .key(c)
            .and_then(|k| self.info.insts.get(&format!("m:{k}")).copied())
            .unwrap_or(INST_LINEAR_KB);
        Kb {
            linear,
            heap: INST_HEAP_KB,
        }
    }

    fn fits(&self, add: Kb) -> bool {
        self.used.plus(add).within(self.budget)
    }

    /// Loads `c`'s model (if it isn't) and spends `more` besides, if both
    /// fit and the pack has the model.
    fn take(&mut self, c: usize, more: Kb) -> bool {
        let cost = self.model(c).plus(more);
        if !self.packed(c) || !self.fits(cost) {
            return false;
        }
        self.used = self.used.plus(cost);
        if let Some(k) = self.key(c) {
            if !self.models.contains(&k) {
                self.models.push(k);
            }
        }
        true
    }

    /// One more animal of `c` (its model too, if not loaded), if it fits.
    fn add(&mut self, c: usize) -> bool {
        self.take(c, self.inst(c))
    }

    /// Animals of `cands` (character, most of it) in turn, one each a
    /// round, until each has its most, `total` are placed among them or
    /// nothing more fits. `tally` counts them per candidate.
    fn grow(&mut self, cands: &[(usize, usize)], tally: &mut [usize], total: usize) {
        loop {
            let mut any = false;
            for (i, &(c, most)) in cands.iter().enumerate() {
                if tally.iter().sum::<usize>() >= total {
                    return;
                }
                if tally[i] < most && self.add(c) {
                    tally[i] += 1;
                    any = true;
                }
            }
            if !any {
                return;
            }
        }
    }
}

/// What model `key` takes loaded.
fn model_kb(info: &PackInfo, key: &str) -> Kb {
    let m = format!("m:{key}");
    Kb {
        linear: info.cost(&m),
        heap: info.heap(&m),
    }
}

/// Whether character `c` is picked.
fn picked(req: &Request, c: usize, role: Role) -> bool {
    match role {
        Role::Slot(k) => req.targets.has(k + 10),
        Role::Roamer => req.roamers.contains(&c),
        _ => false,
    }
}

/// Unpicked roaming add-ons the map's tables place here.
fn natives(rules: &HuntRules, req: &Request) -> Vec<usize> {
    let Some(t) = &rules.tables else {
        return Vec::new();
    };
    (0..rules.roles.len())
        .filter(|&c| rules.roles[c] == Role::Roamer && !picked(req, c, Role::Roamer))
        .filter(|&c| t.characters.get(c).is_some_and(|ch| ch.spawns()))
        .collect()
}

/// The memory every hunt on the area takes whatever is picked, without
/// animal models other than the hunter's: `heap` is the area's figure.
fn fixed(info: &PackInfo, rules: &HuntRules, req: &Request, heap: usize) -> Kb {
    let mut k = Kb {
        linear: info.cost("hunt") + info.cost(&req.area),
        heap,
    };
    for (c, &role) in rules.roles.iter().enumerate() {
        if role == Role::Hunter {
            k.linear += info.cost(&format!("m:{}", rules.keys[c]));
        }
    }
    for w in &req.weapons {
        k.linear += info.cost(&format!("wpn{w}"));
        k.heap += info.heap(&format!("wpn{w}"));
    }
    // The targets' calls (sounds and button picture), loaded per pick.
    k.linear += (10..Bits::LEN)
        .filter(|&id| req.targets.has(id))
        .map(|id| info.cost(&format!("call{id}")))
        .sum::<usize>();
    k
}

/// The plan for `req` on a pack with `info`'s figures.
pub fn plan(info: &PackInfo, rules: &HuntRules, req: &Request) -> Plan {
    let budget = Kb {
        linear: info.linear_kb,
        heap: info.heap_kb,
    };
    if !req.hunting || !info.planned(&req.area) {
        return unplanned(info, rules, req, budget);
    }
    let n = rules.roles.len();
    let bare = info.heap_bases.get(&req.area).copied().unwrap_or(0);
    let hunters = (0..n).filter(|&c| rules.roles[c] == Role::Hunter);
    let mut s = Spend {
        info,
        rules,
        budget,
        // The hunter's model is in the fixed figures.
        used: fixed(info, rules, req, bare),
        models: hunters.map(|c| rules.keys[c].as_str()).collect(),
    };
    let picks: Vec<usize> = (0..n).filter(|&c| picked(req, c, rules.roles[c])).collect();
    let mut p = Plan {
        planned: true,
        budget,
        max: vec![None; n],
        ..Default::default()
    };
    // The picks' floor: one of each, or the hunt is refused.
    let mut pick_n = vec![0usize; picks.len()];
    for (i, &c) in picks.iter().enumerate() {
        pick_n[i] = usize::from(s.add(c));
    }
    if pick_n.contains(&0) || !s.used.within(budget) {
        let cost = |c: usize| {
            let k = model_kb(info, &rules.keys[c]);
            k.linear + k.heap
        };
        p.drop = picks.iter().copied().max_by_key(|&c| cost(c));
        // What the area and the picks' models take, for the menu.
        let mut seen: Vec<&str> = Vec::new();
        p.used = fixed(info, rules, req, bare);
        for &c in &picks {
            let k = rules.keys[c].as_str();
            if !seen.contains(&k) {
                seen.push(k);
                p.used = p.used.plus(model_kb(info, k));
            }
        }
        return p;
    }
    p.fits = true;
    // One secret animal: the first that fits.
    let natives = if req.secret_max > 0 {
        natives(rules, req)
    } else {
        Vec::new()
    };
    let rule: Vec<(usize, usize)> = req
        .rule_secrets
        .iter()
        .copied()
        .filter(|&(c, count)| c < n && count > 0)
        .collect();
    let native: Vec<(usize, usize)> = natives.iter().map(|&c| (c, req.secret_max)).collect();
    let mut rule_n = vec![0usize; rule.len()];
    let mut native_n = vec![0usize; native.len()];
    if let Some(i) = (0..rule.len()).find(|&i| s.add(rule[i].0)) {
        rule_n[i] = 1;
    } else if let Some(i) = (0..native.len()).find(|&i| s.add(native[i].0)) {
        native_n[i] = 1;
    }
    // The picks up to the population the hunt aims for.
    let each = req.huntable_want.div_ceil(picks.len().max(1)).max(1);
    let aimed: Vec<(usize, usize)> = picks.iter().map(|&c| (c, each)).collect();
    s.grow(&aimed, &mut pick_n, usize::MAX);
    // The other secrets: the instance's, then the map's own.
    s.grow(&rule, &mut rule_n, usize::MAX);
    s.grow(&native, &mut native_n, req.secret_max);
    // Ambient animals: one species after another, then animals up to the
    // setting. The placement shares the animals out among the species, so
    // each is priced as the costliest species'.
    let (ambients, brach) = ambient_species(rules, req);
    let mut ambient_models = Vec::new();
    let mut ambients_max = 0;
    if req.ambient_max > 0 {
        let one = ambients
            .iter()
            .filter(|&&c| s.packed(c))
            .map(|&c| s.inst(c))
            .fold(Kb::default(), |a, b| Kb {
                linear: a.linear.max(b.linear),
                heap: a.heap.max(b.heap),
            });
        for &c in &ambients {
            let animal = ambients_max < req.ambient_max;
            if s.take(c, if animal { one } else { Kb::default() }) {
                ambient_models.push(c);
                ambients_max += usize::from(animal);
            }
        }
        while !ambient_models.is_empty() && ambients_max < req.ambient_max && s.fits(one) {
            s.used = s.used.plus(one);
            ambients_max += 1;
        }
    }
    // The original's brachiosaurs, apart from the small animals.
    let mut brach_n = [0usize];
    if let Some(b) = brach.filter(|_| req.ambient_max > 0) {
        s.grow(&[(b, BRACHIOSAURS)], &mut brach_n, usize::MAX);
    }
    // Aquatic animals: the cheapest species first, where there is water.
    let mut aquatic: Vec<usize> = (0..n)
        .filter(|&c| rules.roles[c] == Role::Aquatic && req.water && s.packed(c))
        .collect();
    aquatic.sort_by_key(|&c| {
        let k = s.model(c).plus(s.inst(c));
        (k.linear + k.heap, c)
    });
    let aquatic: Vec<(usize, usize)> = aquatic.into_iter().map(|c| (c, AQUATIC_EACH)).collect();
    let mut aquatic_n = vec![0usize; aquatic.len()];
    s.grow(&aquatic, &mut aquatic_n, AQUATIC_MAX);
    // What is left: the picks' limits, as far as it goes.
    let free: Vec<(usize, usize)> = picks.iter().map(|&c| (c, usize::MAX)).collect();
    s.grow(&free, &mut pick_n, HUNTABLE_LIMIT);

    let counted = |list: &[(usize, usize)], tally: &[usize]| -> Vec<(usize, usize)> {
        list.iter()
            .zip(tally)
            .map(|(&(c, _), &k)| (c, k))
            .filter(|x| x.1 > 0)
            .collect()
    };
    p.huntables = counted(&aimed, &pick_n);
    p.rule_secrets = counted(&rule, &rule_n);
    p.native_secrets = counted(&native, &native_n);
    p.aquatic = counted(&aquatic, &aquatic_n);
    // The instance's secrets are placed besides the limits (`rule_secrets`
    // has their counts): a model loaded for them alone places no other.
    for &(c, _) in &p.rule_secrets {
        p.max[c] = Some(0);
    }
    for c in (0..n).filter(|&c| rules.roles[c] == Role::Hunter) {
        p.max[c] = Some(usize::MAX);
    }
    // Shared out by the placement, under `ambients_max`.
    for &c in &ambient_models {
        p.max[c] = Some(usize::MAX);
    }
    if let Some(b) = brach.filter(|_| brach_n[0] > 0) {
        p.max[b] = Some(brach_n[0]);
        ambient_models.push(b);
    }
    let limits = [&p.native_secrets, &p.aquatic, &p.huntables];
    for &(c, k) in limits.into_iter().flatten() {
        p.max[c] = Some(k);
    }
    p.ambient_models = ambient_models;
    p.ambients_max = ambients_max;
    p.used = s.used;
    p
}

/// The ambient species to consider, in order, and the original game's
/// brachiosaur (placed apart from them, three at a time).
fn ambient_species(rules: &HuntRules, req: &Request) -> (Vec<usize>, Option<usize>) {
    let ai = |c: usize| {
        rules
            .res
            .characters
            .get(c)
            .map(|d| DinoInfo::from_def(d).ai)
    };
    let brach = (0..rules.roles.len())
        .find(|&c| {
            rules.tables.is_none()
                && rules.roles[c] == Role::Ambient
                && ai(c) == Some(c2_core::dino::ai::BRACH)
        })
        .filter(|b| req.ambient_only.as_ref().is_none_or(|o| o.contains(b)));
    let all = (0..rules.roles.len()).filter(|&c| rules.roles[c] == Role::Ambient);
    let list = match &req.ambient_only {
        Some(only) => only
            .iter()
            .copied()
            .filter(|&c| rules.roles.get(c) == Some(&Role::Ambient))
            .collect(),
        None => all.collect::<Vec<_>>(),
    };
    (
        list.into_iter().filter(|c| Some(*c) != brach).collect(),
        brach,
    )
}

/// The loads of a hunt without a plan, as before it: every ambient species
/// when they are on, the picks, the instance's secrets, and the map's own
/// roamers as far as memory goes after those.
fn unplanned(info: &PackInfo, rules: &HuntRules, req: &Request, budget: Kb) -> Plan {
    let n = rules.roles.len();
    let mut used = fixed(info, rules, req, info.heap(&req.area));
    let mut max = vec![None; n];
    let mut ambient_models = Vec::new();
    let mut huntables = Vec::new();
    for (c, &role) in rules.roles.iter().enumerate() {
        let m = format!("m:{}", rules.keys[c]);
        match role {
            Role::Hunter => max[c] = Some(usize::MAX),
            Role::Ambient if req.ambient_max > 0 => {
                if req.ambient_only.as_ref().is_some_and(|o| !o.contains(&c)) {
                    continue;
                }
                // Its main memory is in the area's figure.
                used.linear += info.cost(&m);
                max[c] = Some(usize::MAX);
                ambient_models.push(c);
            }
            _ if picked(req, c, role) => {
                used.linear += info.cost(&m);
                used.heap += info.heap(&m);
                max[c] = Some(usize::MAX);
                huntables.push((c, usize::MAX));
            }
            _ => {}
        }
    }
    let fits = used.within(budget);
    let mut native_secrets = Vec::new();
    if req.hunting {
        for c in natives(rules, req) {
            let m = format!("m:{}", rules.keys[c]);
            let add = Kb {
                linear: info.cost(&m),
                heap: info.heap(&m),
            };
            if info.costs.contains_key(&m) && used.plus(add).within(budget) {
                used = used.plus(add);
                max[c] = Some(usize::MAX);
                native_secrets.push((c, usize::MAX));
            }
        }
    }
    let rule_secrets: Vec<(usize, usize)> = if req.hunting {
        req.rule_secrets
            .iter()
            .copied()
            .filter(|s| s.0 < n)
            .collect()
    } else {
        Vec::new()
    };
    for &(c, _) in &rule_secrets {
        max[c] = Some(usize::MAX);
    }
    Plan {
        fits,
        planned: false,
        max,
        ambients_max: req.ambient_max,
        ambient_models,
        huntables,
        rule_secrets,
        native_secrets,
        aquatic: Vec::new(),
        drop: None,
        used,
        budget,
    }
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
