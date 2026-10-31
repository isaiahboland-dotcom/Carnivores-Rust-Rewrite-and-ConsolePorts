//! The mobile games' interface sprites (`ui/sprites_*.cfg` in the user's
//! APK): each a named rectangle of a texture atlas
//! (`sprite button_dino_active_1 { texture = "gui_02" tc_x = 0 ... }`). The
//! rectangles are in the atlas's base size; the APK's atlases are drawn at
//! a power of two larger (`menu_01` is 2048 wide, its sprites laid out in
//! 1024), so [`scale`] puts them in the atlas's own pixels.

use alloc::collections::BTreeMap;
use alloc::{format, string::String, string::ToString};

/// One sprite: its atlas (by name, no extension) and rectangle (some
/// configs give halves: `tc_x = 238.5`).
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub texture: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Every `sprite name { ... }` in a `.cfg` (later ones replace earlier).
pub fn parse(text: &str, into: &mut BTreeMap<String, Sprite>) {
    let mut rest = text;
    while let Some(i) = rest.find("sprite ") {
        rest = &rest[i + 7..];
        let Some(open) = rest.find('{') else { break };
        let name = rest[..open].trim().to_string();
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let body = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let get = |k: &str| {
            body.lines().find_map(|l| {
                let (key, v) = l.split_once('=')?;
                (key.trim() == k).then(|| v.trim().trim_matches('"').to_string())
            })
        };
        let num = |k: &str| {
            get(k)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v >= 0.0)
        };
        if let (Some(texture), Some(x), Some(y), Some(w), Some(h)) = (
            get("texture"),
            num("tc_x"),
            num("tc_y"),
            num("tc_width"),
            num("tc_height"),
        ) {
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                into.insert(
                    name,
                    Sprite {
                        texture,
                        x,
                        y,
                        w,
                        h,
                    },
                );
            }
        }
    }
}

/// The sprites in their atlases' own pixels: each atlas's rectangles grown
/// by the largest power of two that still fits them in its width and its
/// height (`size` gives an atlas's; sprites of an unknown atlas are
/// dropped). Width alone misleads on a sparse atlas (`gui_04`).
pub fn scale(
    sprites: &BTreeMap<String, Sprite>,
    size: &dyn Fn(&str) -> Option<(u32, u32)>,
) -> BTreeMap<String, Sprite> {
    let mut extent: BTreeMap<&str, (f32, f32)> = BTreeMap::new();
    for s in sprites.values() {
        let e = extent.entry(&s.texture).or_insert((0.0, 0.0));
        *e = (e.0.max(s.x + s.w), e.1.max(s.y + s.h));
    }
    let mut k_of: BTreeMap<&str, f32> = BTreeMap::new();
    for (&t, &(ex, ey)) in &extent {
        let Some((w, h)) = size(t) else { continue };
        let mut k = 1.0;
        while ex > 0.0 && ex * k * 2.0 <= w as f32 && ey * k * 2.0 <= h as f32 {
            k *= 2.0;
        }
        k_of.insert(t, k);
    }
    sprites
        .iter()
        .filter_map(|(n, s)| {
            let k = *k_of.get(s.texture.as_str())?;
            Some((
                n.clone(),
                Sprite {
                    texture: s.texture.clone(),
                    x: s.x * k,
                    y: s.y * k,
                    w: s.w * k,
                    h: s.h * k,
                },
            ))
        })
        .collect()
}

/// Sprites as a `.cfg` again.
pub fn write(sprites: &BTreeMap<String, Sprite>) -> String {
    let mut out = String::new();
    for (n, s) in sprites {
        out += &format!(
            "sprite {n} {{\n\ttexture = \"{}\"\n\ttc_x = {}\n\ttc_y = {}\n\ttc_width = {}\n\ttc_height = {}\n}}\n\n",
            s.texture, s.x, s.y, s.w, s.h
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprites_scale_to_their_atlas() {
        let mut m = BTreeMap::new();
        parse(
            "\nsprite a {\n\ttexture = \"menu_01\"\n\ttc_x = 512\n\ttc_y = 0\n\ttc_width = 512\n\t\
             tc_height = 384\n}\n\nsprite b {\n\ttexture = \"gui\"\n\ttc_x = 10\n\ttc_y = 2\n\t\
             tc_width = 20\n\ttc_height = 4\n}\n",
            &mut m,
        );
        let s = scale(&m, &|t| match t {
            "menu_01" => Some((2048, 2048)),
            "gui" => Some((32, 8)),
            _ => None,
        });
        assert_eq!(
            s["a"],
            Sprite {
                texture: "menu_01".into(),
                x: 1024.0,
                y: 0.0,
                w: 1024.0,
                h: 768.0
            }
        );
        assert_eq!((s["b"].x, s["b"].w), (10.0, 20.0));
        let mut back = BTreeMap::new();
        parse(&write(&s), &mut back);
        assert_eq!(back, s);
    }
}
