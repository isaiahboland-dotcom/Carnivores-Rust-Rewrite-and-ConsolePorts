//! The original Triassic executable uses Ice Age's layouts through AI 18,
//! plus a T-Rex at 19. It is not the later, explicitly scripted MEE edition.
//! Keep this recognition narrow: AI numbers alone cannot identify an engine.

use c2_formats::res_txt::{c_atoi, Entry, ResTxt};
use c2_vfs::{MemBackend, Vfs};

use crate::ModError;

const GOJIRA: &str = "runspd = 2.49\nwlkspd = 0.76\nswmspd = 0.70\n\
killdist = 380\ncanswim = TRUE\nrunAnim = 0\nwalkAnim = 1\nroarAnim = 2\n\
swimAnim = 3\nlookAnim = 4\nlookAnim = 5\nsmellAnim = 6\nsmellAnim = 7\n\
deathtype {\ndieAnim = 8\nnoSleep = TRUE\n}\nwaterLevel = 560\n\
killtype {\nhunterAnim = 12\neatAnim = 9\nhunterOffset = 0\nhunterSync = TRUE\nscream = TRUE\n}\n\
CamYLand = 768\nCamYWater = 256\nCamBaseLand = 1424\nCamBaseWater = 824\n";

fn convert(res: &mut ResTxt) -> Result<bool, ModError> {
    if c2_formats::mee::is_dialect(res) {
        return Ok(false);
    }
    let Some(chars) = res
        .sections
        .iter_mut()
        .find(|s| s.name.eq_ignore_ascii_case("characters"))
    else {
        return Ok(false);
    };
    let matches = |name: &str, ai: i32| {
        chars.blocks.iter().any(|b| {
            b.get(&["name"])
                .is_some_and(|n| n.trim_matches('\'').eq_ignore_ascii_case(name))
                && b.get(&["ai"]).map(c_atoi) == Some(ai)
        })
    };
    if !(matches("Lystrosaurus", 1)
        && matches("Peteinosaurus", 2)
        && matches("Megalosaurus", 18)
        && matches("Gojirasaurus", 19))
    {
        return Ok(false);
    }
    for b in &mut chars.blocks {
        let ai = b.get(&["ai"]).map_or(0, c_atoi);
        let Some((mapped, lines)) = (if ai == 19 {
            Some((18, GOJIRA))
        } else {
            crate::ice_age::ai_rule(ai)
        }) else {
            continue;
        };
        for e in &mut b.entries {
            match e.key.to_ascii_lowercase().as_str() {
                "ai" => e.value = mapped.to_string(),
                "smell" => e.key = "smellK".into(),
                "hear" => e.key = "hearK".into(),
                "look" => e.key = "lookK".into(),
                _ => {}
            }
        }
        if ai >= 10 {
            b.entries.push(Entry {
                key: "callNo".into(),
                value: (ai - 10).to_string(),
            });
        }
        let rules = ResTxt::parse(&format!("characters {{\n{{\n{lines}}}\n}}\n"))?;
        if let Some(extra) = rules.section("characters").and_then(|s| s.blocks.first()) {
            let at = b.entries.len();
            b.entries.extend(extra.entries.iter().cloned());
            b.subs.extend(extra.subs.iter().cloned().map(|mut s| {
                s.at += at;
                s
            }));
        }
    }
    Ok(true)
}

pub(crate) fn layer(vfs: &Vfs) -> Result<Option<MemBackend>, ModError> {
    let Ok(data) = vfs.read("HUNTDAT/_RES.TXT") else {
        return Ok(None);
    };
    let mut res = ResTxt::parse(&String::from_utf8_lossy(&data))?;
    if !convert(&mut res)? {
        return Ok(None);
    }
    let mut layer = MemBackend::new();
    layer.insert("HUNTDAT/_RES.TXT", res.write().into_bytes());
    Ok(Some(layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patched_layouts_are_converted_once_without_changing_models() -> Result<(), ModError> {
        let mut res = ResTxt::parse("characters {\n{\nname = 'Lystrosaurus'\nai = 1\n}\n{\nname = 'Peteinosaurus'\nai = 2\n}\n{\nname = 'Megalosaurus'\nai = 18\nfile = 'synthetic.car'\n}\n{\nname = 'Gojirasaurus'\nai = 19\n}\n}\n")?;
        assert!(convert(&mut res)?);
        let chars = &res.sections[0].blocks;
        assert_eq!(chars[0].get(&["ai"]), Some("6"));
        assert_eq!(chars[1].get(&["ai"]), Some("3"));
        assert_eq!(chars[2].get(&["ai"]), Some("27"));
        assert_eq!(chars[2].get(&["file"]), Some("'synthetic.car'"));
        assert_eq!(chars[2].get(&["runspd"]), Some("1.792"));
        assert_eq!(chars[3].get(&["ai"]), Some("18"));
        assert_eq!(chars[3].get(&["callNo"]), Some("9"));
        assert!(!convert(&mut res)?);
        Ok(())
    }

    #[test]
    fn ai_numbers_alone_do_not_trigger_conversion() -> Result<(), ModError> {
        let mut res = ResTxt::parse("characters {\n{\nname = 'Other'\nai = 18\n}\n}\n")?;
        assert!(!convert(&mut res)?);
        Ok(())
    }
}
