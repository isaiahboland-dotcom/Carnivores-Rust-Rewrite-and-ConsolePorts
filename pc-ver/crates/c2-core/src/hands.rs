//! The hunter's hands and arms in first-person weapon models: which faces
//! a view that shows the gun alone hides (the DHC OS armoury file's model,
//! a gun held in VR). A weapon `.CAR` is arms and gun in one mesh; the
//! mask is worked out beside it and never changes the model, so the hunt's
//! first-person weapon and its animations are untouched.
//!
//! - **Recognised:** the original game's hand rig ([`VANILLA_RIG`]), found
//!   by its complete signature: 23 bones with consecutive numbers whose
//!   vertex counts are exactly the rig's, in order. The pistol has two.
//!   Mods that reuse the original hands keep the signature.
//! - **Overrides** (`HUNTDAT/_HANDS.TXT`, `c2_formats::hands`), per model
//!   by the `sha256:` of its file, add or remove bones and pieces.
//! - **Anything else** keeps the whole model: nothing is hidden unless it
//!   was recognised or an override says so.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

use c2_formats::hands::{Hands, Part, Rule};
use c2_formats::model::Mesh;

/// The original hand rig: vertices per bone, in bone order (the sleeve,
/// then the palm and finger pieces). Every original weapon with hands has
/// it (`RIFLE`, `SHOTGUN`, `DBSGUN`; the pistol twice); the crossbow and
/// the scope overlay have none.
pub const VANILLA_RIG: [usize; 23] = [
    29, 6, 16, 1, 5, 6, 11, 1, 3, 3, 7, 1, 1, 1, 6, 1, 2, 3, 7, 2, 6, 11, 3,
];

/// What decided a model's mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The model's override rules.
    Override,
    /// This many copies of the original hand rig.
    VanillaRig(usize),
    /// Not recognised: nothing is hidden.
    Unrecognised,
}

/// The first bone of each copy of the original hand rig in `mesh`
/// (ascending). A copy is 23 bones numbered `o..o+23`, each with exactly
/// the rig's vertex count; a bone with more or fewer vertices (gun parts
/// rigged to a hand bone, a remodelled hand) breaks the match, and then
/// nothing is assumed.
pub fn vanilla_hands(mesh: &Mesh) -> Vec<i16> {
    let mut count: BTreeMap<i16, usize> = BTreeMap::new();
    for v in &mesh.vertices {
        *count.entry(v.owner).or_default() += 1;
    }
    let mut out = Vec::new();
    let mut next = i16::MIN;
    for &o in count.keys() {
        if o < next {
            continue;
        }
        let whole = VANILLA_RIG.iter().enumerate().all(|(k, &n)| {
            i16::try_from(k)
                .ok()
                .and_then(|k| o.checked_add(k))
                .is_some_and(|b| count.get(&b) == Some(&n))
        });
        if whole {
            out.push(o);
            next = o.saturating_add(VANILLA_RIG.len() as i16);
        }
    }
    out
}

/// Each face's piece: faces joined by a shared vertex, or by vertices at
/// exactly the same place (a texture seam's split vertices), are one
/// piece. Pieces are numbered from 0 in the order of their first face, so
/// the numbers are the same every time the model is read.
pub fn pieces(mesh: &Mesh) -> Vec<u32> {
    let n = mesh.vertices.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let join = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (a, b) = (root(p, a), root(p, b));
        if a != b {
            p[a.max(b)] = a.min(b);
        }
    };
    let mut at: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    for (i, v) in mesh.vertices.iter().enumerate() {
        let key = [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        match at.get(&key) {
            Some(&j) => join(&mut parent, i, j),
            None => {
                at.insert(key, i);
            }
        }
    }
    let valid = |f: &c2_formats::model::Face| -> Option<[usize; 3]> {
        let mut out = [0; 3];
        for (o, &v) in out.iter_mut().zip(&f.v) {
            *o = usize::try_from(v).ok().filter(|&v| v < n)?;
        }
        Some(out)
    };
    for f in &mesh.faces {
        if let Some([a, b, c]) = valid(f) {
            join(&mut parent, a, b);
            join(&mut parent, a, c);
        }
    }
    // Faces with a vertex out of range are pieces of their own.
    let mut number: BTreeMap<usize, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(mesh.faces.len());
    for (k, f) in mesh.faces.iter().enumerate() {
        let key = match valid(f) {
            Some([a, ..]) => root(&mut parent, a),
            None => n + k,
        };
        let next = number.len() as u32;
        out.push(*number.entry(key).or_insert(next));
    }
    out
}

/// Which faces of a weapon model a gun-only view hides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandMask {
    /// Per face of the mesh: hidden.
    hidden: Vec<bool>,
    /// The first bone of each recognised hand rig.
    pub hands: Vec<i16>,
    pub source: Source,
}

impl HandMask {
    /// The mask of `mesh`, the model whose file is `id` (`sha256:<hex>`),
    /// under `overrides`. Without rules for it, the recognised hand rigs
    /// are hidden, or nothing.
    ///
    /// With rules: `keep all` starts from nothing hidden, else from the
    /// recognised rigs. Hidden bones are those plus `drop owner` less
    /// `keep owner`; a face is hidden when all its vertices belong to
    /// hidden bones, or its piece is dropped, unless its piece is kept.
    pub fn new(mesh: &Mesh, id: &str, overrides: &Hands) -> Self {
        let hands = vanilla_hands(mesh);
        let rules: Vec<Rule> = overrides.rules_for(id).copied().collect();
        let keep_all = rules.contains(&Rule::KeepAll);
        let mut owners: Vec<i16> = if keep_all {
            Vec::new()
        } else {
            hands
                .iter()
                .flat_map(|&o| (0..VANILLA_RIG.len() as i16).map(move |k| o.saturating_add(k)))
                .collect()
        };
        let (mut drop_pieces, mut keep_pieces) = (Vec::new(), Vec::new());
        for r in &rules {
            match *r {
                Rule::Drop(Part::Owner(o)) => owners.push(o),
                Rule::Drop(Part::Piece(p)) => drop_pieces.push(p),
                Rule::Keep(Part::Piece(p)) => keep_pieces.push(p),
                Rule::Keep(Part::Owner(_)) | Rule::KeepAll => {}
            }
        }
        owners.retain(|o| !rules.contains(&Rule::Keep(Part::Owner(*o))));
        let piece_of = if drop_pieces.is_empty() && keep_pieces.is_empty() {
            Vec::new()
        } else {
            pieces(mesh)
        };
        let owner_of = |v: i32| {
            usize::try_from(v)
                .ok()
                .and_then(|v| mesh.vertices.get(v))
                .map(|v| v.owner)
        };
        let hidden = mesh
            .faces
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let by_bone =
                    f.v.iter()
                        .all(|&v| owner_of(v).is_some_and(|o| owners.contains(&o)));
                let piece = piece_of.get(k);
                let dropped = piece.is_some_and(|p| drop_pieces.contains(p));
                let kept = piece.is_some_and(|p| keep_pieces.contains(p));
                (by_bone || dropped) && !kept
            })
            .collect();
        let source = match (rules.is_empty(), hands.len()) {
            (false, _) => Source::Override,
            (true, 0) => Source::Unrecognised,
            (true, n) => Source::VanillaRig(n),
        };
        HandMask {
            hidden,
            hands,
            source,
        }
    }

    /// Whether face `k` is hidden.
    pub fn hides(&self, k: usize) -> bool {
        self.hidden.get(k).copied().unwrap_or(false)
    }

    /// How many faces are hidden.
    pub fn hidden_faces(&self) -> usize {
        self.hidden.iter().filter(|h| **h).count()
    }

    /// `mesh` without the hidden faces: a copy for drawing the gun alone.
    /// Its vertices, texture and their order are the model's, so the
    /// model's animations pose it unchanged.
    pub fn apply(&self, mesh: &Mesh) -> Mesh {
        Mesh {
            vertices: mesh.vertices.clone(),
            faces: mesh
                .faces
                .iter()
                .enumerate()
                .filter(|(k, _)| !self.hides(*k))
                .map(|(_, f)| *f)
                .collect(),
            texture: mesh.texture.clone(),
        }
    }

    /// The rest position of each recognised hand (the middle of its
    /// vertices, in the model's own units): where a held gun's grip is.
    pub fn hand_centres(&self, mesh: &Mesh) -> Vec<[f32; 3]> {
        self.hands
            .iter()
            .filter_map(|&o| {
                let end = o.saturating_add(VANILLA_RIG.len() as i16);
                let mut sum = [0.0f32; 3];
                let mut n = 0.0f32;
                for v in mesh.vertices.iter().filter(|v| (o..end).contains(&v.owner)) {
                    sum[0] += v.x;
                    sum[1] += v.y;
                    sum[2] += v.z;
                    n += 1.0;
                }
                (n > 0.0).then(|| sum.map(|s| s / n))
            })
            .collect()
    }
}

impl Default for HandMask {
    /// Nothing hidden.
    fn default() -> Self {
        HandMask {
            hidden: vec![],
            hands: vec![],
            source: Source::Unrecognised,
        }
    }
}

#[cfg(test)]
mod tests;
