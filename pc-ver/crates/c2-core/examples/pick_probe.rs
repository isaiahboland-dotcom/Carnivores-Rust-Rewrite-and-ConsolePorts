//! A picked menu slot on an area, as a hunt resolves it: its spawn links,
//! the groups' sizes and its radar flag. `pick_probe _RES.TXT AREA SLOT`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let raw = c2_formats::res_txt::ResTxt::parse(&std::fs::read_to_string(&a[1])?)?;
    let area: u32 = a[2].parse()?;
    let slot: usize = a[3].parse()?;
    let h = c2_formats::mee::Hunt {
        area: Some(area),
        time: 1,
        picked: c2_formats::bits::Bits::bit(slot),
    };
    let m = c2_core::mee::read_picked_anywhere(&raw, &h, &[]);
    let t = &m.tables;
    for (i, c) in t.characters.iter().enumerate() {
        let call = c.block.get(&["callNo"]).map(c2_formats::res_txt::c_atoi);
        let hit = std::env::var("PROBE_NAME").map_or(call == Some(slot as i32), |n| {
            c.block.get(&["name"]).is_some_and(|v| v.contains(&n))
        });
        if hit {
            println!(
                "  file {:?} (loaded: {:?}) callNo {:?}",
                c.block.get(&["file"]),
                m.res.characters[i].file,
                call
            );
            println!(
                "char {i} {:?}: spawn {:?} packs {:?} radar {:?}",
                c.block.get(&["name"]),
                c.spawn,
                c.packs,
                c.block.get(&["radar"])
            );
            for l in &c.spawn {
                let g = &t.spawn_groups[l.group];
                println!(
                    "  group {}: min {} max {} rate {} regions {}",
                    l.group,
                    g.min,
                    g.max,
                    g.rate,
                    g.regions.len()
                );
            }
        }
    }
    // Who spawns here at all, by group.
    let mut used = std::collections::BTreeMap::new();
    for c in &t.characters {
        for l in &c.spawn {
            *used.entry(l.group).or_insert(0) += 1;
        }
    }
    println!("groups with members here: {used:?}");
    Ok(())
}
