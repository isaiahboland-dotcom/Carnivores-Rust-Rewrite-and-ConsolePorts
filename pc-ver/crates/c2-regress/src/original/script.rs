//! The hook's input script for a scenario (`original/script.c` reads it):
//! the same stages as `c2_core::scenario`, as key presses at fixed frames,
//! and the hunt's command line (the menu's: `prj=%s reg=%d din=%d wep=%d
//! dtm=%d`, seen in carn2.exe and IceAge.exe).

use std::fmt::Write as _;

use anyhow::{bail, Result};
use c2_core::scenario::{Scenario, SpeciesRef};

/// Frames are 20 ms (the hook's default `C2H_FRAME_MS`).
pub const FRAME_MS: u32 = 20;

/// One screenshot the script takes, and the stage it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    pub stage: String,
    /// File name without `.bmp`: a checkpoint name, or one with a suffix
    /// (`_ready`, `_after`, `_closed`) used by the checks.
    pub name: String,
    pub frame: u32,
}

pub struct Plan {
    pub text: String,
    pub shots: Vec<Shot>,
    pub stages: Vec<String>,
    pub exit_frame: u32,
}

/// `huntdat\\areas\\area2 reg=0 din=... wep=... dtm=1` for `area` (e.g.
/// `AREA2`). `din` is a bit per menu slot: AI - 10 in the stock games; in
/// a Modder's Edition game (the same command line, `CommandLine.cpp` in the
/// modernized fork) the slot `menu_slot` gives for a character index.
pub fn command_line(
    area: &str,
    s: &Scenario,
    weapons: usize,
    menu_slot: &dyn Fn(usize) -> Option<usize>,
) -> Result<String> {
    let mut din = 0u32;
    for r in &s.species {
        match r {
            SpeciesRef::Ai(a) if (10..42).contains(a) => din |= 1 << (a - 10),
            SpeciesRef::Index(i) => match menu_slot(*i) {
                Some(k) if k < 10 => din |= 1 << k,
                k => bail!("character {i} has no menu slot the command line can pick ({k:?})"),
            },
            other => bail!("the original's menu takes hunt animals by AI 10..41, not {other:?}"),
        }
    }
    let wep = if weapons >= 32 {
        u32::MAX
    } else {
        (1u32 << weapons) - 1
    };
    let area = area.to_ascii_lowercase();
    Ok(format!(
        "prj=huntdat\\areas\\{area} reg=0 din={din} wep={wep} dtm=1"
    ))
}

/// Each character's menu slot in a Modder's Edition game, for `area` by
/// day with nothing picked; empty for a stock game (it picks by AI).
pub fn menu_slots(game_dir: &std::path::Path, area: &str) -> Vec<Option<usize>> {
    use c2_formats::{mee, res_txt::ResTxt};
    let Some(res) = super::wine::find_ci(game_dir, "HUNTDAT")
        .and_then(|h| super::wine::find_ci(&h, "_RES.TXT"))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|d| ResTxt::parse(&String::from_utf8_lossy(&d)).ok())
        .filter(mee::is_dialect)
    else {
        return Vec::new();
    };
    let h = mee::Hunt {
        area: mee::area_of(area),
        time: 1,
        picked: Default::default(),
    };
    let t = mee::Tables::read(&res, &h);
    let raw = res
        .section("characters")
        .map(|s| s.blocks.as_slice())
        .unwrap_or(&[]);
    t.characters
        .iter()
        .enumerate()
        .map(|(i, c)| mee::menu_slot(raw.get(i), &c.block))
        .collect()
}

/// A virtual-key code as the hook's script writes it: a single letter or
/// digit is that key, so the mouse buttons and others go by name.
pub fn vk(code: i32) -> String {
    match code {
        1 => "LBUTTON".into(),
        2 => "RBUTTON".into(),
        9 => "TAB".into(),
        13 => "RETURN".into(),
        16 => "SHIFT".into(),
        17 => "CONTROL".into(),
        18 => "MENU".into(),
        32 => "SPACE".into(),
        0x30..=0x39 | 0x41..=0x5a => char::from(code as u8).to_string(),
        n => n.to_string(),
    }
}

/// Lower-case letters and digits, `_` between words (as the director's
/// stage names).
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// The script: warm-up, each weapon drawn (its number key), fired (the
/// fire key held two frames, shorter than any fire animation, so one
/// shot) and put away; binoculars, map and the call switch opened and
/// closed. Keys are the original's defaults (`profile::default_keys`).
/// A weapon for [`plan`]: its name and its view model's draw, fire and
/// put-away animation lengths (ms; 0 when unknown).
#[derive(Debug, Clone, Default)]
pub struct WeaponTiming {
    pub name: String,
    pub draw_ms: u32,
    pub fire_ms: u32,
    pub hide_ms: u32,
}

/// Frames to wait for an animation of `ms`: at least `floor` (the stock
/// games' waits), else its length plus 10 frames.
fn wait(floor: u32, ms: u32) -> u32 {
    floor.max(ms.div_ceil(FRAME_MS) + 10)
}

pub fn plan(s: &Scenario, weapons: &[WeaponTiming]) -> Plan {
    let k = c2_core::profile::default_keys();
    let (fire, show) = (vk(k.fire), vk(k.show));
    let (binoculars, change_call) = (vk(k.binoculars), vk(k.change_call));
    let ticks_per_frame = FRAME_MS / c2_core::scenario::TICK_MS as u32;
    let mut t = (s.warmup_ticks / ticks_per_frame).max(1);
    let mut text = String::from("# generated by c2-regress for the original engine\n");
    let (mut shots, mut stages) = (Vec::new(), vec!["warmup".to_string()]);
    let mut shot = |text: &mut String, stage: &str, name: String, f: u32| {
        let _ = writeln!(text, "{f} shot {name}");
        shots.push(Shot {
            stage: stage.into(),
            name,
            frame: f,
        });
    };
    shot(&mut text, "warmup", "warmup".into(), t);
    for (i, w) in weapons.iter().enumerate().take(9) {
        let st = format!("weapon_{:02}_{}", i + 1, slug(&w.name));
        stages.push(st.clone());
        t += 10;
        let _ = writeln!(text, "{t} mark {st}\n{t} tap {}", i + 1);
        // A slow draw (a Modder's Edition bow) ignores a click before it ends.
        t += wait(75, w.draw_ms);
        shot(&mut text, &st, format!("{st}_ready"), t);
        t += 2;
        let _ = writeln!(text, "{t} key {} down\n{} key {} up", fire, t + 2, fire);
        shot(&mut text, &st, format!("{st}_fire"), t + 6);
        t += wait(90, w.fire_ms);
        shot(&mut text, &st, format!("{st}_after"), t);
        let _ = writeln!(
            text,
            "{} key {} down\n{} key {} up",
            t + 1,
            show,
            t + 3,
            show
        );
        t += wait(80, w.hide_ms);
    }
    for (key, name) in [(binoculars.clone(), "binoculars"), ("TAB".into(), "map")] {
        stages.push(name.into());
        t += 10;
        let _ = writeln!(text, "{t} mark {name}\n{t} tap {key}");
        t += 30;
        shot(&mut text, name, format!("{name}_open"), t);
        let _ = writeln!(text, "{} tap {key}", t + 2);
        t += 30;
        shot(&mut text, name, format!("{name}_closed"), t);
    }
    stages.push("call_icon".into());
    t += 10;
    let cc = &change_call;
    let _ = writeln!(
        text,
        "{t} mark call_icon\n{t} tap {cc}\n{} tap {cc}",
        t + 10
    );
    shot(&mut text, "call_icon", "call_icon_open".into(), t + 20);
    // The icon shows 2048 ms.
    t += 150;
    shot(&mut text, "call_icon", "call_icon_closed".into(), t);
    t += 10;
    stages.push("end".into());
    shot(&mut text, "end", "end".into(), t);
    let _ = writeln!(text, "{} exit", t + 1);
    Plan {
        text,
        shots,
        stages,
        exit_frame: t + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario() -> Scenario {
        let f = crate::scenario_file::ScenarioFile::parse(include_str!(
            "../../scenarios/vanilla_map2_four_dinos.toml"
        ))
        .unwrap();
        f.for_game(f.game("c2").unwrap()).unwrap()
    }

    #[test]
    fn the_menus_command_line() {
        // As carn2.exe's menu passed it for the same choices.
        assert_eq!(
            command_line("AREA2", &scenario(), 6, &|_| None).unwrap(),
            "prj=huntdat\\areas\\area2 reg=0 din=284 wep=63 dtm=1"
        );
        let mut s = scenario();
        s.species[0] = SpeciesRef::Index(3);
        assert!(command_line("AREA2", &s, 6, &|_| None).is_err());
        assert_eq!(
            command_line("AREA2", &s, 6, &|i| (i == 3).then_some(4)).unwrap(),
            // Slot 4 (the index) and AI 13, 12, 14 (slots 3, 2, 4).
            "prj=huntdat\\areas\\area2 reg=0 din=28 wep=63 dtm=1"
        );
    }

    #[test]
    fn every_stage_has_its_shots() {
        let names: Vec<WeaponTiming> = ["Pistol", "Sniper rifle"]
            .map(|n| WeaponTiming {
                name: n.into(),
                ..Default::default()
            })
            .to_vec();
        let p = plan(&scenario(), &names);
        // Unknown lengths keep the stock waits; a 3 s draw waits it out.
        let fast = p
            .shots
            .iter()
            .find(|s| s.name == "weapon_02_sniper_rifle_ready")
            .map(|s| s.frame);
        let mut slow = names.clone();
        slow[0].draw_ms = 3000;
        let q = plan(&scenario(), &slow);
        let late = q
            .shots
            .iter()
            .find(|s| s.name == "weapon_02_sniper_rifle_ready")
            .map(|s| s.frame);
        assert_eq!(late.zip(fast).map(|(l, f)| l - f), Some(160 - 75));
        let shots: Vec<&str> = p.shots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(shots[0], "warmup");
        assert!(shots.contains(&"weapon_02_sniper_rifle_fire"));
        assert!(shots.contains(&"map_closed"));
        assert_eq!(shots.last(), Some(&"end"));
        assert!(p.text.contains("tap 1\n") && p.text.contains(" key LBUTTON down"));
        assert!(p.text.contains(" key E down") && p.text.contains(" tap B\n"));
        assert_eq!(
            (vk(1), vk(69), vk(9), vk(200)),
            ("LBUTTON".into(), "E".into(), "TAB".into(), "200".into())
        );
        assert!(p
            .text
            .trim_end()
            .ends_with(&format!("{} exit", p.exit_frame)));
        // Frames only go forward.
        let frames: Vec<u32> = p.shots.iter().map(|s| s.frame).collect();
        assert!(frames.windows(2).all(|w| w[0] < w[1]));
    }
}
