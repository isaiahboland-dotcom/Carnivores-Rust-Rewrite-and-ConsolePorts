//! `HUNTDAT/_HANDS.TXT`: per weapon model, which of its parts are the
//! hunter's hands and arms (not in the original game). A weapon `.CAR` is
//! a first-person model, arms and gun in one mesh; views that show the gun
//! alone (the DHC OS armoury file, a gun held in VR) hide the parts this
//! file names, on top of what `c2_core::hands` recognises by itself. The
//! hunt's own first-person weapon never reads it.
//!
//! A model is named by its file's content (`sha256:` and the hex digest of
//! the `.CAR`, as recipes name files), so two mods' `RIFLE.CAR`s never
//! share rules. One setting per line; `;` starts a comment. The lines after
//! `model` are that model's rules; a model may appear more than once (its
//! rules add up).
//!
//! ```text
//! model sha256:0123...cdef   ; WEAPONS/RIFLE.CAR of some mod
//! keep all                   ; ignore what was recognised
//! drop owner 1-23 26         ; hide these bones' faces
//! drop piece 4 7             ; hide these mesh pieces
//! keep owner 5               ; show these, whatever else says
//! keep piece 2
//! ```
//!
//! An owner is a vertex's bone (`Vertex::owner`). A piece is a connected
//! part of the mesh, numbered from 0 by its first face
//! (`c2_core::hands::pieces`).

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

/// The path the rules are read from.
pub const PATH: &str = "HUNTDAT/_HANDS.TXT";

/// A part of a model a rule names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A bone: every face whose vertices all belong to it (or to other
    /// hidden bones).
    Owner(i16),
    /// A connected mesh piece.
    Piece(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Start from nothing hidden, ignoring what was recognised.
    KeepAll,
    Drop(Part),
    Keep(Part),
}

/// One model's rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Model {
    /// `sha256:<hex>`, lower case.
    pub id: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hands {
    pub models: Vec<Model>,
}

/// `a`, or `a-b` (inclusive), as numbers; none if malformed or reversed.
fn range(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a <= b).then_some((a, b))
}

impl Hands {
    /// Reads the file. Unknown lines and bad values are skipped, as are
    /// rules before any `model` line.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut h = Self::default();
        for line in text.lines() {
            let words: Vec<&str> = line
                .split(';')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect();
            let lower = |i: usize| words.get(i).map(|w| w.to_ascii_lowercase());
            match (lower(0).as_deref(), lower(1).as_deref()) {
                (Some("model"), Some(id)) => h.models.push(Model {
                    id: id.into(),
                    rules: Vec::new(),
                }),
                (Some("keep"), Some("all")) => {
                    if let Some(m) = h.models.last_mut() {
                        m.rules.push(Rule::KeepAll);
                    }
                }
                (Some(verb @ ("keep" | "drop")), Some(kind @ ("owner" | "piece"))) => {
                    let Some(m) = h.models.last_mut() else {
                        continue;
                    };
                    // At most this many parts from one range (a typo like
                    // `0-65535` stays cheap).
                    const MAX: u32 = 4096;
                    for (a, b) in words[2..].iter().filter_map(|w| range(w)) {
                        for n in a..=b.min(a.saturating_add(MAX)) {
                            let part = match kind {
                                "owner" => match i16::try_from(n) {
                                    Ok(o) => Part::Owner(o),
                                    Err(_) => continue,
                                },
                                _ => Part::Piece(n),
                            };
                            m.rules.push(if verb == "keep" {
                                Rule::Keep(part)
                            } else {
                                Rule::Drop(part)
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        h
    }

    /// Every rule for the model `id` (`sha256:<hex>`, case ignored), in the
    /// file's order.
    pub fn rules_for<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Rule> + 'a {
        let id = id.trim();
        self.models
            .iter()
            .filter(move |m| m.id.eq_ignore_ascii_case(id))
            .flat_map(|m| m.rules.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn rules_are_read_per_model_by_content() {
        let text = b"; header\n\
            drop owner 9          ; before any model: skipped\n\
            model SHA256:AbC\n\
            drop owner 1-3 26\n\
            keep piece 2 ; comment\n\
            model sha256:def\n\
            keep all\n\
            drop piece 0-1 x 5-4\n\
            frobnicate 7\n\
            model sha256:abc\n\
            keep owner 70000 2\n";
        let h = Hands::parse(text);
        assert_eq!(h.models.len(), 3);
        let abc: Vec<Rule> = h.rules_for("sha256:ABC").copied().collect();
        assert_eq!(
            abc,
            vec![
                Rule::Drop(Part::Owner(1)),
                Rule::Drop(Part::Owner(2)),
                Rule::Drop(Part::Owner(3)),
                Rule::Drop(Part::Owner(26)),
                Rule::Keep(Part::Piece(2)),
                Rule::Keep(Part::Owner(2)),
            ],
            "a second block adds up; an owner past i16 is skipped"
        );
        let def: Vec<Rule> = h.rules_for(" sha256:def ").copied().collect();
        assert_eq!(
            def,
            vec![
                Rule::KeepAll,
                Rule::Drop(Part::Piece(0)),
                Rule::Drop(Part::Piece(1)),
            ],
            "bad and reversed ranges skipped"
        );
        assert_eq!(h.rules_for("sha256:000").count(), 0);
        assert_eq!(Hands::parse(b""), Hands::default());
    }

    #[test]
    fn a_huge_range_is_capped() {
        let h = Hands::parse(b"model a\ndrop piece 0-4294967295\n");
        assert_eq!(h.rules_for("a").count(), 4097);
    }
}
