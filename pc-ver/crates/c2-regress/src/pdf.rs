//! A minimal PDF writer (text, lines and RGB images) and the side-by-side
//! screenshot report built with it. Images are embedded as their PNG's
//! zlib stream with the PNG predictor (`/DecodeParms /Predictor 15`), so
//! no image or compression code is needed here.

use std::fmt::Write as _;

use anyhow::{bail, Result};

/// Landscape A4, points.
pub const PAGE_W: f32 = 842.0;
pub const PAGE_H: f32 = 595.0;

pub struct Pdf {
    /// Object bodies; object `n` is `objects[n - 1]`. 1: catalog, 2: page
    /// tree, 3: font (filled in by [`Pdf::finish`]).
    objects: Vec<Vec<u8>>,
    pages: Vec<usize>,
}

/// One image object and its size in pixels.
#[derive(Debug, Clone, Copy)]
pub struct Image {
    obj: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
pub struct Page {
    ops: String,
    images: Vec<usize>,
}

/// PDF string: ASCII only, with `(`, `)` and `\` escaped.
fn pdf_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('(');
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                o.push('\\');
                o.push(c);
            }
            ' '..='~' => o.push(c),
            _ => o.push('?'),
        }
    }
    o.push(')');
    o
}

/// The concatenated IDAT data of a non-interlaced 8-bit RGB PNG.
fn idat(png: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    if png.get(..8) != Some(b"\x89PNG\r\n\x1a\n") {
        bail!("not a PNG");
    }
    let (mut at, mut size, mut data) = (8, None, Vec::new());
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let kind = &png[at + 4..at + 8];
        let Some(body) = png.get(at + 8..at + 8 + len) else {
            bail!("truncated PNG chunk");
        };
        match kind {
            b"IHDR" if body.len() >= 13 => {
                if body[8] != 8 || body[9] != 2 || body[12] != 0 {
                    bail!("only 8-bit RGB non-interlaced PNGs are embedded");
                }
                let w = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                let h = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
                size = Some((w, h));
            }
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let Some((w, h)) = size else {
        bail!("PNG without IHDR")
    };
    Ok((w, h, data))
}

impl Default for Pdf {
    fn default() -> Self {
        Self::new()
    }
}

impl Pdf {
    pub fn new() -> Self {
        Self {
            objects: vec![Vec::new(), Vec::new(), Vec::new()],
            pages: Vec::new(),
        }
    }

    fn add(&mut self, body: Vec<u8>) -> usize {
        self.objects.push(body);
        self.objects.len()
    }

    fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
        let mut b = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
        b.extend_from_slice(data);
        b.extend_from_slice(b"\nendstream");
        b
    }

    /// Embeds an RGB PNG (as written by `image_diff::encode_rgb`).
    pub fn image(&mut self, png: &[u8]) -> Result<Image> {
        let (w, h, data) = idat(png)?;
        let dict = format!(
            "/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /FlateDecode \
             /DecodeParms << /Predictor 15 /Colors 3 /BitsPerComponent 8 /Columns {w} >>"
        );
        let obj = self.add(Self::stream(&dict, &data));
        Ok(Image {
            obj,
            width: w,
            height: h,
        })
    }

    pub fn page(&mut self, p: Page) {
        let content = self.add(Self::stream("", p.ops.as_bytes()));
        let mut xobj = String::new();
        for o in &p.images {
            let _ = write!(xobj, "/Im{o} {o} 0 R ");
        }
        let body = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
             /Resources << /Font << /F1 3 0 R >> /XObject << {xobj}>> >> /Contents {content} 0 R >>"
        );
        let page = self.add(body.into_bytes());
        self.pages.push(page);
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.objects[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
        let kids: String = self.pages.iter().map(|p| format!("{p} 0 R ")).collect();
        self.objects[1] = format!(
            "<< /Type /Pages /Kids [{kids}] /Count {} >>",
            self.pages.len()
        )
        .into_bytes();
        self.objects[2] =
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_vec();
        let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in self.objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        }
        let xref = out.len();
        let mut t = format!("xref\n0 {}\n0000000000 65535 f \n", self.objects.len() + 1);
        for o in offsets {
            let _ = writeln!(t, "{o:010} 00000 n ");
        }
        let _ = write!(
            t,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.objects.len() + 1
        );
        out.extend_from_slice(t.as_bytes());
        out
    }
}

impl Page {
    /// Text at (x, y) from the bottom left, `size` points.
    pub fn text(&mut self, x: f32, y: f32, size: f32, s: &str) {
        let _ = writeln!(
            self.ops,
            "BT /F1 {size} Tf {x:.1} {y:.1} Td {} Tj ET",
            pdf_str(s)
        );
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let _ = writeln!(self.ops, "0.5 w {x0:.1} {y0:.1} m {x1:.1} {y1:.1} l S");
    }

    /// `img` scaled to fit `w` x `h` at (x, y), keeping its aspect.
    /// Returns the height used.
    pub fn image(&mut self, img: Image, x: f32, y: f32, w: f32, h: f32) -> f32 {
        let k = (w / img.width as f32).min(h / img.height as f32);
        let (dw, dh) = (img.width as f32 * k, img.height as f32 * k);
        let _ = writeln!(
            self.ops,
            "q {dw:.2} 0 0 {dh:.2} {x:.1} {:.1} cm /Im{} Do Q",
            y + h - dh,
            img.obj
        );
        self.images.push(img.obj);
        dh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_well_formed_document() {
        let rgb = vec![200u8; 4 * 2 * 3];
        let png = crate::image_diff::encode_rgb(4, 2, &rgb).unwrap();
        let mut pdf = Pdf::new();
        let img = pdf.image(&png).unwrap();
        let mut p = Page::default();
        p.text(40.0, 550.0, 14.0, "Checkpoint (map_open) \\ test");
        p.line(40.0, 540.0, 800.0, 540.0);
        p.image(img, 40.0, 100.0, 300.0, 300.0);
        pdf.page(p);
        let out = pdf.finish();
        let s = String::from_utf8_lossy(&out);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.contains("/Count 1"));
        assert!(s.contains("\\(map_open\\) \\\\ test"));
        assert!(s.contains("/Predictor 15"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // Every xref offset points at its object (the xref is ASCII).
        let xref = s.rfind("xref\n").unwrap();
        let tail = &s[xref..];
        let xref_bytes = out.len() - tail.len();
        let table = String::from_utf8(out[xref_bytes..].to_vec()).unwrap();
        for (i, l) in table.lines().skip(3).take(5).enumerate() {
            let off: usize = l[..10].parse().unwrap();
            assert!(out[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()));
        }
        assert!(idat(b"nope").is_err());
    }
}
