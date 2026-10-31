//! BMFont text descriptors (`.fnt`; the mobile games' `fonts/*.fnt`,
//! `*.fnthd`): a glyph rectangle per character in an atlas picture, with
//! kerning pairs. Rectangles are in `scale_w` x `scale_h` units; an atlas
//! drawn larger (the mobile games' `_hd` ones) scales them by its size.

use alloc::collections::BTreeMap;
use alloc::{string::String, string::ToString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Glyph {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub advance: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Font {
    pub line_height: i32,
    pub base: i32,
    pub scale_w: i32,
    pub scale_h: i32,
    /// The atlas picture's file (page 0).
    pub page: String,
    pub glyphs: BTreeMap<u32, Glyph>,
    pub kerning: BTreeMap<(u32, u32), i32>,
}

impl Font {
    /// Width of `text` in font units.
    pub fn width(&self, text: &str) -> i32 {
        let mut w = 0;
        let mut prev = None;
        for c in text.chars().map(u32::from) {
            if let Some(p) = prev {
                w += self.kerning.get(&(p, c)).copied().unwrap_or(0);
            }
            w += self
                .glyphs
                .get(&c)
                .map_or(self.line_height / 3, |g| g.advance);
            prev = Some(c);
        }
        w
    }
}

/// `key=value` pairs of one line (values maybe quoted).
fn pairs(line: &str) -> impl Iterator<Item = (&str, &str)> {
    line.split_whitespace()
        .filter_map(|t| t.split_once('='))
        .map(|(k, v)| (k, v.trim_matches('"')))
}

pub fn parse(text: &str) -> Font {
    let mut f = Font::default();
    for line in text.lines() {
        let (kind, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
        let get = |k: &str| {
            pairs(rest)
                .find(|(key, _)| *key == k)
                .and_then(|(_, v)| v.parse::<i32>().ok())
                .unwrap_or(0)
        };
        match kind {
            "common" => {
                f.line_height = get("lineHeight");
                f.base = get("base");
                f.scale_w = get("scaleW");
                f.scale_h = get("scaleH");
            }
            "page" if get("id") == 0 => {
                if let Some((_, v)) = pairs(rest).find(|(k, _)| *k == "file") {
                    f.page = v.to_string();
                }
            }
            "char" => {
                f.glyphs.insert(
                    get("id").max(0) as u32,
                    Glyph {
                        x: get("x"),
                        y: get("y"),
                        w: get("width"),
                        h: get("height"),
                        x_offset: get("xoffset"),
                        y_offset: get("yoffset"),
                        advance: get("xadvance"),
                    },
                );
            }
            "kerning" => {
                let (a, b) = (get("first").max(0) as u32, get("second").max(0) as u32);
                f.kerning.insert((a, b), get("amount"));
            }
            _ => {}
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_glyphs_and_kerning() {
        let f = parse(
            "info face=\"X\" size=-36\r\ncommon lineHeight=48 base=34 scaleW=512 scaleH=512 pages=1\r\n\
             page id=0 file=\"x_hd.tga\"\r\nchars count=2\r\n\
             char id=65   x=74    y=116   width=35    height=33    xoffset=-2    yoffset=5     xadvance=30    page=0  chnl=15\r\n\
             char id=86 x=1 y=2 width=3 height=4 xoffset=0 yoffset=0 xadvance=20 page=0 chnl=15\r\n\
             kerning first=65 second=86 amount=-3\r\n",
        );
        assert_eq!(
            (f.line_height, f.scale_w, f.page.as_str()),
            (48, 512, "x_hd.tga")
        );
        assert_eq!(f.glyphs[&65].x_offset, -2);
        assert_eq!(f.width("AV"), 30 + 20 - 3);
    }
}
