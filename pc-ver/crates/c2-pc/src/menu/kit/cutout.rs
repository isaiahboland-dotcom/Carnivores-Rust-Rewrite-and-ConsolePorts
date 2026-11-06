//! Hunt setup items the APK has no art for (an add-on's map, animal or
//! weapon): its menu picture (`AREAn`/`DINOn`/`WEAPONn.TGA`, or one drawn
//! from its model), cut to the shape of the phone's honeycomb cell.

use super::Pic;

/// `rgba` (`w` x `h`) scaled to cover the cell and cropped to the middle,
/// at the cell's size, its alpha times the cell's.
pub(super) fn hex_cut(cell: &Pic, w: usize, h: usize, rgba: &[u8]) -> Option<Pic> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 || cell.w == 0 || cell.h == 0 {
        return None;
    }
    let s = (cell.w as f32 / w as f32).max(cell.h as f32 / h as f32);
    // The source pixels under the cell's top left, after cropping.
    let ox = (w as f32 - cell.w as f32 / s) / 2.0;
    let oy = (h as f32 - cell.h as f32 / s) / 2.0;
    let mut px = vec![0u8; cell.w * cell.h * 4];
    for y in 0..cell.h {
        for x in 0..cell.w {
            let sx = (ox + (x as f32 + 0.5) / s) as usize;
            let sy = (oy + (y as f32 + 0.5) / s) as usize;
            let src = (sy.min(h - 1) * w + sx.min(w - 1)) * 4;
            let dst = (y * cell.w + x) * 4;
            px[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
            let a = u16::from(px[dst + 3]) * u16::from(cell.px[dst + 3]) / 255;
            px[dst + 3] = a as u8;
        }
    }
    Some(Pic {
        w: cell.w,
        h: cell.h,
        px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_takes_the_cells_size_and_shape() {
        // A cell clear in its left half; a wide red picture.
        let cell = Pic {
            w: 4,
            h: 2,
            px: (0..8)
                .flat_map(|i| [0, 0, 0, if i % 4 < 2 { 0 } else { 255 }])
                .collect(),
        };
        let red: Vec<u8> = (0..16 * 4).flat_map(|_| [200, 0, 0, 255]).collect();
        let p = hex_cut(&cell, 16, 4, &red).unwrap_or_else(|| panic!("no cut"));
        assert_eq!((p.w, p.h), (4, 2));
        assert_eq!(&p.px[0..4], [200, 0, 0, 0]);
        assert_eq!(&p.px[12..16], [200, 0, 0, 255]);
        assert!(hex_cut(&cell, 16, 4, &red[..8]).is_none());
    }
}
