//! The field manual's area surveys (DHC OS on PC and Android): what the
//! terminal says about one area of the catalog, from the island rules, the
//! game's secret animals and the hunter's record. Its animals and weapons
//! are links to their own files, in [`links`]'s order.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use super::specimen::{issued_on, Line};
use super::{Catalog, Rules, Tab};
use crate::record::Record;

const TIMES: [&str; 3] = ["DAWN", "DAY", "NIGHT"];

/// The files area `area`'s survey links to: the animals, then the weapons
/// its island issues, each only when the island restricts them.
pub fn links(cat: &Catalog, rules: &Rules, area: usize) -> Vec<(Tab, usize)> {
    [Tab::Targets, Tab::Arms]
        .into_iter()
        .flat_map(|tab| {
            issued_on(cat, rules, area, tab)
                .unwrap_or_default()
                .into_iter()
                .map(move |i| (tab, i))
        })
        .collect()
}

/// How many species may turn up without a TARGETS entry, of those named
/// in `appear` (the animals that may turn up on an area: its secret
/// animals and roaming add-ons the menu doesn't list). Each name counts
/// once; one the catalog lists is known.
pub fn unknown(cat: &Catalog, appear: &[String]) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    for n in appear.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        let listed = cat
            .animals
            .iter()
            .any(|a| a.name.trim().eq_ignore_ascii_case(n));
        if !listed && !seen.iter().any(|s| s.eq_ignore_ascii_case(n)) {
            seen.push(n);
        }
    }
    seen.len()
}

/// Area `area`'s survey: its permit and times, the animals and weapons
/// issued there (each a link, or ALL), the unknown species (`unknown`)
/// and the hunts the hunter completed there (`rec`; none: no hunter
/// signed in).
pub fn lines(
    cat: &Catalog,
    rules: &Rules,
    area: usize,
    unknown: usize,
    rec: Option<&Record>,
) -> Vec<Line> {
    let Some(it) = cat.areas.get(area) else {
        return Vec::new();
    };
    let mut v = vec![
        Line::Head("SURVEY"),
        Line::Fact("PERMIT", format!("{} PTS", it.price)),
    ];
    let times: Vec<&str> = (0..3)
        .filter(|&t| rules.allows_time(cat, area, t))
        .map(|t| TIMES[t])
        .collect();
    if times.len() != TIMES.len() {
        let s = if times.is_empty() {
            "NONE".to_string()
        } else {
            times.join(", ")
        };
        v.push(Line::Fact("TIMES", s));
    }
    let links = links(cat, rules, area);
    for (tab, head) in [
        (Tab::Targets, "LICENSED SPECIMENS"),
        (Tab::Arms, "AUTHORISED ARMS"),
    ] {
        v.push(Line::Blank);
        v.push(Line::Head(head));
        match issued_on(cat, rules, area, tab) {
            None => v.push(Line::Item("ALL".into())),
            Some(l) if l.is_empty() => v.push(Line::Item("NONE AUTHORISED".into())),
            Some(_) => v.extend(links.iter().enumerate().filter(|(_, l)| l.0 == tab).map(
                |(k, &(t, i))| {
                    let name = cat.list(t).get(i).map_or("", |x| x.name.as_str());
                    Line::Link(k, name.trim().to_uppercase())
                },
            )),
        }
        if tab == Tab::Targets {
            v.push(Line::Item(format!("TOTAL DINOSAURS UNKNOWN: {unknown}")));
        }
    }
    v.push(Line::Blank);
    v.push(Line::Head("HUNTER RECORDS"));
    match rec {
        None => v.push(Line::Item("NO HUNTER SIGNED IN".into())),
        Some(r) => v.push(Line::Fact(
            "HUNTS",
            format!("{} COMPLETED", r.hunts_on(&it.stem)),
        )),
    }
    v
}
