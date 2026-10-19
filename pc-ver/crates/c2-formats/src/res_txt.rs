//! `HUNTDAT/_RES.TXT`: the text script defining weapons, characters and prices.
//!
//! Parsing happens in two layers:
//! - [`ResTxt`] is a lossless-enough generic tree (sections → blocks →
//!   `key = value` entries, and nested `name { ... }` sub-blocks at any
//!   depth, as the Modder's Edition Engine dialect uses) that keeps unknown
//!   keys, so mods that add their own keys survive a parse/merge/write
//!   cycle.
//! - [`ResTxt::to_data`] builds typed [`ResData`] for the engine.
//!
//! Matching the original (`Resources.cpp: LoadResourcesScript`,
//! `ReadWeapons`, `ReadCharacters`), numbers use C `atoi`/`atof` prefix
//! semantics and the file ends at a line beginning with `.`. Deviations
//! (exact key matching instead of `strstr`) are recorded in
//! `docs/DECISIONS.md`.

use crate::error::FormatError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    /// Raw value text with surrounding whitespace and trailing `//` comment removed.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    pub entries: Vec<Entry>,
    /// Nested `name { ... }` blocks (`spawninfo{`, `overwrite area2 {`, an
    /// anonymous `{` has an empty name), in file order.
    pub subs: Vec<Sub>,
}

/// A nested block inside a [`Block`] or [`Section`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sub {
    /// The text before `{`, trimmed (`spawninfo`, `overwrite char0 area1`).
    pub name: String,
    /// How many of the parent's entries come before it (for rewriting in
    /// place).
    pub at: usize,
    pub block: Block,
}

impl Block {
    /// Last entry with this key (case-insensitive) or one of its aliases.
    pub fn get(&self, keys: &[&str]) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|e| keys.iter().any(|k| e.key.eq_ignore_ascii_case(k)))
            .map(|e| e.value.as_str())
    }

    /// Every entry with this key (case-insensitive), in order (keys that
    /// may repeat, such as `fearCall`).
    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.entries
            .iter()
            .filter(move |e| e.key.eq_ignore_ascii_case(key))
            .map(|e| e.value.as_str())
    }

    /// Sub-blocks named `name` (case-insensitive), in order.
    pub fn subs_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.subs
            .iter()
            .filter(move |s| s.name.eq_ignore_ascii_case(name))
            .map(|s| &s.block)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// Anonymous `{ ... }` blocks (weapons, characters).
    pub blocks: Vec<Block>,
    /// Bare `key = value` lines directly in the section (prices).
    pub entries: Vec<Entry>,
    /// Named blocks directly in the section (`spawngroup{` in a spawn
    /// table), in order.
    pub subs: Vec<Sub>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResTxt {
    pub sections: Vec<Section>,
    /// Non-fatal oddities (stray lines, unterminated blocks), with line numbers.
    pub warnings: Vec<String>,
}

/// Strips a `//` comment that is not inside a `'quoted'` string.
fn strip_comment(line: &str) -> &str {
    let mut in_quote = false;
    let b = line.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'\'' => in_quote = !in_quote,
            b'/' if !in_quote && b.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_entry(line: &str) -> Option<Entry> {
    let (k, v) = line.split_once('=')?;
    let key = k.trim();
    if key.is_empty() {
        return None;
    }
    Some(Entry {
        key: key.to_string(),
        value: v.trim().to_string(),
    })
}

/// One `{ ... }` body: entries and child bodies, read up to its `}`.
#[derive(Default)]
struct Node {
    entries: Vec<Entry>,
    children: Vec<(String, usize, Node)>,
}

impl Node {
    fn into_block(self) -> Block {
        Block {
            entries: self.entries,
            subs: self
                .children
                .into_iter()
                .map(|(name, at, n)| Sub {
                    name,
                    at,
                    block: n.into_block(),
                })
                .collect(),
        }
    }
}

/// Reads a body after its `{`. Returns it and whether its `}` was found
/// (`false`: the file ended first, or a `.` line ended it).
fn parse_node<'a>(
    lines: &mut impl Iterator<Item = (usize, &'a str)>,
    warnings: &mut Vec<String>,
) -> (Node, bool) {
    let mut node = Node::default();
    while let Some((ln, raw)) = lines.next() {
        if raw.starts_with('.') {
            return (node, false);
        }
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('}') {
            return (node, true);
        }
        if let Some(name) = line.strip_suffix('{').map(str::trim) {
            let at = node.entries.len();
            let (child, closed) = parse_node(lines, warnings);
            node.children.push((name.to_string(), at, child));
            if !closed {
                return (node, false);
            }
        } else if let Some(e) = parse_entry(line) {
            node.entries.push(e);
        } else {
            warnings.push(format!(
                "line {}: not a `key = value` line: {line:?}",
                ln + 1
            ));
        }
    }
    (node, false)
}

impl ResTxt {
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        let mut out = ResTxt::default();
        let mut lines = text.lines().enumerate();
        while let Some((ln, raw)) = lines.next() {
            if raw.starts_with('.') {
                break;
            }
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            let Some(name) = line.strip_suffix('{').map(str::trim) else {
                out.warnings.push(format!(
                    "line {}: ignored outside any section: {line:?}",
                    ln + 1
                ));
                continue;
            };
            if name.is_empty() {
                // Some modded engines add blocks of their own here. The
                // original (`LoadResourcesScript`) only looks for lines
                // containing "weapons" / "characters" and skips the rest,
                // so this is kept (for rewriting) as an unnamed section that
                // `to_data` ignores.
                out.warnings.push(format!(
                    "line {}: block outside a section (ignored, as the original does)",
                    ln + 1
                ));
            }
            let (node, closed) = parse_node(&mut lines, &mut out.warnings);
            let mut section = Section {
                name: name.to_string(),
                blocks: Vec::new(),
                entries: node.entries,
                subs: Vec::new(),
            };
            for (n, at, child) in node.children {
                if n.is_empty() {
                    section.blocks.push(child.into_block());
                } else {
                    section.subs.push(Sub {
                        name: n,
                        at,
                        block: child.into_block(),
                    });
                }
            }
            if !closed {
                out.warnings.push(format!(
                    "section {:?} not closed before end of file",
                    section.name
                ));
            }
            out.sections.push(section);
        }
        Ok(out)
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Serialises back to the original style (CRLF, one entry per line,
    /// terminated by `.`).
    pub fn write(&self) -> String {
        let mut s = String::new();
        for sec in &self.sections {
            if sec.name.is_empty() {
                s.push_str("{\r\n");
            } else {
                s.push_str(&format!("{} {{\r\n", sec.name));
            }
            for b in &sec.blocks {
                s.push_str("{\r\n");
                write_body(&mut s, b, 1);
                s.push_str("}\r\n\r\n");
            }
            write_body(
                &mut s,
                &Block {
                    entries: sec.entries.clone(),
                    subs: sec.subs.clone(),
                },
                1,
            );
            s.push_str("}\r\n\r\n");
        }
        s.push_str(".\r\n");
        s
    }

    pub fn to_data(&self) -> ResData {
        let blocks = |name| {
            self.section(name)
                .map(|s| s.blocks.as_slice())
                .unwrap_or_default()
        };
        let weapons = blocks("weapons")
            .iter()
            .map(WeaponDef::from_block)
            .collect();
        let characters = blocks("characters")
            .iter()
            .map(CharacterDef::from_block)
            .collect();
        let prices = self.section("prices").map(Prices::from_section);
        ResData {
            weapons,
            characters,
            prices,
        }
    }
}

/// A block's entries with its sub-blocks in their places, indented.
fn write_body(s: &mut String, b: &Block, depth: usize) {
    let pad = " ".repeat(depth);
    let mut subs = b.subs.iter().peekable();
    for i in 0..=b.entries.len() {
        while let Some(sub) = subs.next_if(|x| x.at <= i) {
            s.push_str(&format!("{pad}{} {{\r\n", sub.name));
            write_body(s, &sub.block, depth + 1);
            s.push_str(&format!("{pad}}}\r\n"));
        }
        if let Some(e) = b.entries.get(i) {
            s.push_str(&format!("{pad}{} = {}\r\n", e.key, e.value));
        }
    }
}

/// C `atoi`: optional whitespace and sign, then leading digits; 0 if none.
pub fn c_atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut v: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + i64::from(b - b'0')).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// C `atof`: the longest leading decimal float; 0.0 if none.
pub fn c_atof(s: &str) -> f32 {
    let s = s.trim_start();
    let b = s.as_bytes();
    let mut end = 0;
    if matches!(b.first(), Some(b'-' | b'+')) {
        end = 1;
    }
    let mut seen_dot = false;
    let mut seen_digit = false;
    while let Some(&c) = b.get(end) {
        match c {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            _ => break,
        }
        end += 1;
    }
    if seen_digit && matches!(b.get(end), Some(b'e' | b'E')) {
        let mut e = end + 1;
        if matches!(b.get(e), Some(b'-' | b'+')) {
            e += 1;
        }
        if b.get(e).is_some_and(u8::is_ascii_digit) {
            end = e + b[e..].iter().take_while(|c| c.is_ascii_digit()).count();
        }
    }
    s[..end].parse().unwrap_or(0.0)
}

/// A `'quoted'` string value; unquoted text is accepted as-is.
fn quoted(v: &str) -> String {
    let v = v.trim();
    match (v.find('\''), v.rfind('\'')) {
        (Some(a), Some(b)) if b > a => v[a + 1..b].to_string(),
        (Some(a), _) => v[a + 1..].to_string(),
        _ => v.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeaponDef {
    pub name: Option<String>,
    pub file: Option<String>,
    pub pic: Option<String>,
    pub power: Option<f32>,
    pub prec: Option<f32>,
    pub loud: Option<f32>,
    pub rate: Option<f32>,
    pub shots: Option<i32>,
    pub reload: Option<i32>,
    /// Pellets per shot as written; the original stores `trace - 1`.
    pub trace: Option<i32>,
    pub optic: Option<i32>,
    pub fall: Option<i32>,
    /// Keys the original engine does not read (kept for mods).
    pub extra: Vec<Entry>,
}

const WEAPON_KEYS: &[&str] = &[
    "name", "file", "pic", "power", "prec", "loud", "rate", "shots", "reload", "trace", "optic",
    "fall",
];

impl WeaponDef {
    pub fn from_block(b: &Block) -> Self {
        Self {
            name: b.get(&["name"]).map(quoted),
            file: b.get(&["file"]).map(quoted),
            pic: b.get(&["pic"]).map(quoted),
            power: b.get(&["power"]).map(c_atof),
            prec: b.get(&["prec"]).map(c_atof),
            loud: b.get(&["loud"]).map(c_atof),
            rate: b.get(&["rate"]).map(c_atof),
            shots: b.get(&["shots"]).map(c_atoi),
            reload: b.get(&["reload"]).map(c_atoi),
            trace: b.get(&["trace"]).map(c_atoi),
            optic: b.get(&["optic"]).map(c_atoi),
            fall: b.get(&["fall"]).map(c_atoi),
            extra: extra(b, WEAPON_KEYS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharacterDef {
    pub name: Option<String>,
    pub file: Option<String>,
    pub pic: Option<String>,
    pub ai: Option<i32>,
    pub health: Option<i32>,
    pub mass: Option<f32>,
    pub length: Option<f32>,
    pub radius: Option<f32>,
    pub basescore: Option<i32>,
    pub smell: Option<f32>,
    /// Also read from `hearK` (the original's `strstr("hear")` matches it).
    pub hear: Option<f32>,
    /// Also read from `lookK`.
    pub look: Option<f32>,
    pub shipdelta: Option<f32>,
    pub scale0: Option<i32>,
    pub scale_a: Option<i32>,
    /// The original sets this whenever a `danger` line exists, whatever its value.
    pub danger: bool,
    pub extra: Vec<Entry>,
}

const CHARACTER_KEYS: &[&str] = &[
    "name",
    "file",
    "pic",
    "ai",
    "health",
    "mass",
    "length",
    "radius",
    "basescore",
    "smell",
    "hear",
    "hearK",
    "look",
    "lookK",
    "shipdelta",
    "scale0",
    "scaleA",
    "danger",
];

impl CharacterDef {
    pub fn from_block(b: &Block) -> Self {
        Self {
            name: b.get(&["name"]).map(quoted),
            file: b.get(&["file"]).map(quoted),
            pic: b.get(&["pic"]).map(quoted),
            ai: b.get(&["ai"]).map(c_atoi),
            health: b.get(&["health"]).map(c_atoi),
            mass: b.get(&["mass"]).map(c_atof),
            length: b.get(&["length"]).map(c_atof),
            radius: b.get(&["radius"]).map(c_atof),
            basescore: b.get(&["basescore"]).map(c_atoi),
            smell: b.get(&["smell"]).map(c_atof),
            hear: b.get(&["hear", "hearK"]).map(c_atof),
            look: b.get(&["look", "lookK"]).map(c_atof),
            shipdelta: b.get(&["shipdelta"]).map(c_atof),
            scale0: b.get(&["scale0"]).map(c_atoi),
            scale_a: b.get(&["scaleA"]).map(c_atoi),
            danger: b.get(&["danger"]).is_some(),
            extra: extra(b, CHARACTER_KEYS),
        }
    }
}

/// The `prices { }` section. Not read by the released source (its menu code
/// is missing), but present in every retail and mod `_RES.TXT`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prices {
    pub start: Option<i32>,
    pub area: Vec<i32>,
    pub dino: Vec<i32>,
    pub weapon: Vec<i32>,
    /// Spelled `acces` in the data files.
    pub accessory: Vec<i32>,
    pub extra: Vec<Entry>,
}

impl Prices {
    fn from_section(s: &Section) -> Self {
        let mut p = Prices::default();
        for e in &s.entries {
            let v = c_atoi(&e.value);
            match e.key.to_ascii_lowercase().as_str() {
                "start" => p.start = Some(v),
                "area" => p.area.push(v),
                "dino" => p.dino.push(v),
                "weapon" => p.weapon.push(v),
                "acces" => p.accessory.push(v),
                _ => p.extra.push(e.clone()),
            }
        }
        p
    }
}

fn extra(b: &Block, known: &[&str]) -> Vec<Entry> {
    b.entries
        .iter()
        .filter(|e| !known.iter().any(|k| e.key.eq_ignore_ascii_case(k)))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResData {
    pub weapons: Vec<WeaponDef>,
    pub characters: Vec<CharacterDef>,
    pub prices: Option<Prices>,
}
