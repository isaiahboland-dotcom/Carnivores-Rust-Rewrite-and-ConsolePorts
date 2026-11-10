use super::*;
use crate::pack::{open_root, ModSource};
use crate::res_merge::{block_id, Change};
use c2_formats::res_txt::{Block, Entry, ResTxt};
use c2_vfs::{key, MemBackend, Vfs};

pub(crate) struct Preserved {
    group: usize,
    member: usize,
    key: String,
    name: String,
    file: String,
    from: usize,
    menu_assets: Vec<(String, Vec<u8>)>,
}

fn set(b: &mut Block, k: &str, value: String) {
    b.entries.retain(|e| !e.key.eq_ignore_ascii_case(k));
    b.entries.push(Entry {
        key: k.into(),
        value,
    });
}

fn source_matches(m: &ModSource, source: &str, child: &Option<String>) -> bool {
    match &m.variant_origin {
        Some((s, c)) => s == source && c == child,
        None => m.manifest.info.id == source && child.is_none(),
    }
}

fn read_asset(m: &ModSource, base: &Vfs, path: &str) -> Result<Vec<u8>, ModError> {
    if let Some((_, disk)) = m.map.iter().find(|(p, _)| key(p) == key(path)) {
        return std::fs::read(disk).map_err(crate::io(disk));
    }
    if m.map.is_empty() && m.files.iter().any(|p| key(p) == key(path)) {
        return Ok(open_root(&m.root)?.read(path)?);
    }
    Ok(base.read(path)?)
}

/// Copy referenced assets from their source before the VFS can shadow them.
/// Resolve only actual file-valued strings, recursively including area/time
/// overwrites. Paths that could escape the private namespace are rejected.
fn assets(
    b: &mut Block,
    m: &ModSource,
    base: &Vfs,
    prefix: &str,
    out: &mut MemBackend,
) -> Result<(), ModError> {
    for e in &mut b.entries {
        let value = e.value.trim().trim_matches('\'');
        if !value.contains('.') {
            continue;
        }
        let path = format!("HUNTDAT/{}", value.replace('\\', "/"));
        let exists = m.files.iter().any(|p| key(p) == key(&path)) || base.resolve(&path).is_some();
        if !exists {
            continue;
        }
        if !c2_vfs::is_safe_relative_path(value) {
            return Err(bad(format!("unsafe asset reference '{value}'")));
        }
        let private = format!("{prefix}/{}", value.replace('\\', "/"));
        out.insert(&format!("HUNTDAT/{private}"), read_asset(m, base, &path)?);
        e.value = format!("'{private}'");
    }
    for sub in &mut b.subs {
        assets(&mut sub.block, m, base, prefix, out)?;
    }
    Ok(())
}

pub(crate) fn preserve(
    groups: &[Group],
    base: &Vfs,
    res: &ResTxt,
    mods: &mut [ModSource],
) -> Result<(Vec<Preserved>, MemBackend), ModError> {
    validate(groups)?;
    if !groups.is_empty() && base.resolve(PATH).is_some() {
        return Err(bad(
            "configure variants against the source game, rather than an already built variant pack",
        ));
    }
    let mut kept = Vec::new();
    let mut out = MemBackend::new();
    for (gi, g) in groups.iter().enumerate() {
        for (mi, member) in g.members.iter().enumerate() {
            let matches: Vec<_> = mods
                .iter()
                .enumerate()
                .filter(|(_, m)| source_matches(m, &member.source, &member.child))
                .map(|(i, _)| i)
                .collect();
            if matches.len() != 1 {
                return Err(bad(format!("{}:{}: expected one source, found {} (collection child must use its folder name)", g.id, member.id, matches.len())));
            }
            let m = &mut mods[matches[0]];
            let id = canonical(&member.character);
            // A model-only replacement still has the base character's
            // identity even when diff() produced no text change for it.
            let has_character = m.res_changes.iter().any(|c| matches!(c,
                Change::Add { section, block } | Change::Modify { section, block, .. }
                if section.eq_ignore_ascii_case("characters") && block_id(block).as_deref() == Some(&id)));
            if !has_character {
                if let Some(section) = res.section("characters") {
                    let inherited: Vec<_> = section
                        .blocks
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| block_id(b).as_deref() == Some(&id))
                        .collect();
                    if inherited.len() == 1 {
                        let (index, block) = inherited[0];
                        let path = format!(
                            "HUNTDAT/{}",
                            block.get(&["file"]).unwrap_or("").trim().trim_matches('\'')
                        );
                        if m.files.iter().any(|p| key(p) == key(&path)) {
                            m.res_changes.push(Change::Modify {
                                section: "characters".into(),
                                index,
                                block: block.clone(),
                                was: None,
                            });
                        }
                    }
                }
            }
            let matches: Vec<_> = m
                .res_changes
                .iter()
                .enumerate()
                .filter_map(|(i, c)| match c {
                    Change::Add { section, block } | Change::Modify { section, block, .. }
                        if section.eq_ignore_ascii_case("characters")
                            && block_id(block).as_deref() == Some(&id) =>
                    {
                        Some(i)
                    }
                    _ => None,
                })
                .collect();
            if matches.len() != 1 {
                return Err(bad(format!(
                    "{}:{}: expected one source character '{id}', found {}",
                    g.id,
                    member.id,
                    matches.len()
                )));
            }
            let ci = matches[0];
            let mut block = match &m.res_changes[ci] {
                Change::Add { block, .. } | Change::Modify { block, .. } => block.clone(),
                _ => unreachable!(),
            };
            if !(10..30).contains(&block.get(&["ai"]).map_or(0, c2_formats::res_txt::c_atoi)) {
                return Err(bad(format!(
                    "{}:{}: variants require a huntable character",
                    g.id, member.id
                )));
            }
            let name = block
                .get(&["name"])
                .unwrap_or("")
                .trim()
                .trim_matches('\'')
                .to_string();
            assets(
                &mut block,
                m,
                base,
                &format!("VARIANTS/{}/{}", g.id, member.id),
                &mut out,
            )?;
            let file = block
                .get(&["file"])
                .unwrap_or("")
                .trim()
                .trim_matches('\'')
                .to_string();
            if !file.starts_with(&format!("VARIANTS/{}/{}/", g.id, member.id)) {
                return Err(bad(format!(
                    "{}:{}: model is missing from source and base",
                    g.id, member.id
                )));
            }
            let owner = member.id == g.menu_owner;
            let from = m
                .slots
                .iter()
                .find(|s| s.section == "characters" && s.id == id)
                .map(|s| s.from)
                .unwrap_or_else(|| {
                    block
                        .get(&["callNo"])
                        .map(c2_formats::res_txt::c_atoi)
                        .unwrap_or_else(|| {
                            block.get(&["ai"]).map_or(10, c2_formats::res_txt::c_atoi) - 10
                        })
                        .max(0) as usize
                        + 1
                });
            let mut menu_assets = Vec::new();
            if owner {
                for path in &m.files {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    if crate::loose::numbered(name).is_some_and(|(p, n)| {
                        n == from && ["dino", "call"].contains(&p.to_ascii_lowercase().as_str())
                    }) {
                        menu_assets.push((path.clone(), read_asset(m, base, path)?));
                    }
                }
            }
            let internal = if owner {
                id.clone()
            } else {
                format!("__variant_{}_{}", g.id, member.id)
            };
            if !owner {
                if res.section("characters").is_some_and(|s| {
                    s.blocks
                        .iter()
                        .any(|b| block_id(b).as_deref() == Some(&internal))
                }) {
                    return Err(bad("private character name conflicts with base"));
                }
                set(&mut block, "name", format!("'{internal}'"));
                let removed = [from];
                let shared: Vec<_> = m
                    .slots
                    .iter()
                    .filter(|s| s.section == "characters" && s.id != id)
                    .map(|s| s.from)
                    .collect();
                let keep = |path: &str| {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    !crate::loose::numbered(name).is_some_and(|(prefix, n)| {
                        removed.contains(&n)
                            && !shared.contains(&n)
                            && ["dino", "call"].contains(&prefix.to_ascii_lowercase().as_str())
                    })
                };
                m.files.retain(|p| keep(p));
                m.map.retain(|(p, _)| keep(p));
                m.slots
                    .retain(|s| !(s.section == "characters" && s.id == id));
                m.res_changes[ci] = Change::Add {
                    section: "characters".into(),
                    block,
                };
            } else {
                if matches!(&m.res_changes[ci], Change::Add { .. })
                    && !m
                        .slots
                        .iter()
                        .any(|s| s.section == "characters" && s.id == id)
                {
                    m.slots.push(crate::loose::SlotAdd {
                        section: "characters".into(),
                        id: id.clone(),
                        from,
                        alongside: name.clone(),
                    });
                }
                match &mut m.res_changes[ci] {
                    Change::Add { block: b, .. } | Change::Modify { block: b, .. } => *b = block,
                    _ => unreachable!(),
                }
            }
            kept.push(Preserved {
                group: gi,
                member: mi,
                key: internal,
                name,
                file,
                from,
                menu_assets,
            });
        }
    }
    Ok((kept, out))
}

fn token(g: &str, m: &str, file: &str, data: &[u8]) -> u64 {
    // Fixed algorithm, independent of std's randomized hashers or load order.
    let mut hash = 0xcbf29ce484222325u64;
    for b in g
        .bytes()
        .chain([0])
        .chain(m.bytes())
        .chain([0])
        .chain(file.bytes())
        .chain([0])
        .chain(data.iter().copied())
    {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    // TOML integer values are signed 64-bit.
    (hash & i64::MAX as u64).max(1)
}

pub(crate) fn finish(
    groups: &[Group],
    kept: &[Preserved],
    res: &mut ResTxt,
    vfs: &mut Vfs,
) -> Result<(), ModError> {
    let menu = crate::detect::read_res(&vfs.read("HUNTDAT/_MENU.TXT")?)?;
    let menu = c2_formats::menu_txt::MenuData::from_res(&menu);
    let chars = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
        .ok_or_else(|| bad("no characters"))?;
    let mut packed = Packed {
        version: 1,
        groups: Vec::new(),
    };
    let mut tokens = BTreeSet::new();
    let mut used = BTreeSet::new();
    let mut layer = MemBackend::new();
    for (gi, group) in groups.iter().enumerate() {
        let owner = kept
            .iter()
            .find(|k| k.group == gi && group.members[k.member].id == group.menu_owner)
            .ok_or_else(|| bad("missing owner"))?;
        let slot = menu
            .huntables
            .iter()
            .position(|h| canonical(&h.name) == owner.key)
            .ok_or_else(|| bad(format!("group '{}': owner has no menu slot", group.id)))?;
        for (path, data) in &owner.menu_assets {
            let (dir, name) = path.rsplit_once('/').unwrap_or(("HUNTDAT", path));
            if let Some(new) = crate::pack::renumber(name, &["dino", "call"], owner.from, slot + 1)
            {
                layer.insert(&format!("{dir}/{new}"), data.clone());
            }
        }
        let mut members = Vec::new();
        let mut owner_index = None;
        for mi in 0..group.members.len() {
            let k = kept
                .iter()
                .find(|k| k.group == gi && k.member == mi)
                .ok_or_else(|| bad("lost member"))?;
            let matching: Vec<_> = chars
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| block_id(b).as_deref() == Some(&k.key))
                .map(|(i, _)| i)
                .collect();
            if matching.len() != 1 {
                return Err(bad(format!(
                    "{}: member '{}' lost or ambiguous after merge",
                    group.id, k.key
                )));
            }
            let index = matching[0];
            if !used.insert(index) {
                return Err(bad("two groups resolved to the same character"));
            }
            let b = &mut chars.blocks[index];
            if b.get(&["file"]).map(|f| f.trim().trim_matches('\'')) != Some(k.file.as_str()) {
                return Err(bad(format!(
                    "{}: another add-on replaced member '{}'",
                    group.id, k.key
                )));
            }
            set(b, "menuSlot", slot.to_string());
            set(b, "callNo", slot.to_string());
            let data = vfs.read(&format!("HUNTDAT/{}", k.file))?;
            let token = token(&group.id, &group.members[mi].id, &k.file, &data);
            if !tokens.insert(token) {
                return Err(bad("variant trophy identity collision"));
            }
            if group.members[mi].id == group.menu_owner {
                owner_index = Some(index);
            }
            members.push(PackedMember {
                index,
                name: k.name.clone(),
                file: k.file.clone(),
                token,
            });
        }
        let mut rule = group.clone();
        // Direct add-ons keep source area file stems. Standalone remapping is
        // deliberately rejected rather than binding a different packed area.
        for member in &mut rule.members {
            let mut maps: Vec<&mut AreaId> = member.maps.iter_mut().flatten().collect();
            maps.extend(member.map_weights.iter_mut().map(|w| &mut w.map));
            for map in maps {
                let path = format!("HUNTDAT/AREAS/{}.MAP", map.area);
                let Some(found) = vfs.resolve(&path) else {
                    return Err(bad(format!("missing map {}:{}", map.source, map.area)));
                };
                if map.source != "base"
                    && !vfs
                        .layers()
                        .iter()
                        .enumerate()
                        .any(|(i, l)| l.name == map.source && found.providers.contains(&i))
                {
                    return Err(bad(format!(
                        "map {}:{} is not provided by that source",
                        map.source, map.area
                    )));
                }
                // `base` must actually provide this map, not just an add-on.
                if map.source == "base"
                    && !found.providers.iter().any(|&i| {
                        vfs.layers()[i].name == "vanilla"
                            || vfs.layers()[i].name.starts_with("base overlay ")
                    })
                {
                    return Err(bad(format!("base does not provide map {}", map.area)));
                }
                map.source = "base".into();
            }
        }
        validate(std::slice::from_ref(&rule))?;
        packed.groups.push(PackedGroup {
            rule,
            owner: owner_index.ok_or_else(|| bad("missing owner index"))?,
            slot,
            members,
        });
    }
    layer.insert(
        PATH,
        toml::to_string_pretty(&packed)
            .map_err(|e| bad(e.to_string()))?
            .into_bytes(),
    );
    vfs.push("(resolved variants)", Box::new(layer));
    Ok(())
}
