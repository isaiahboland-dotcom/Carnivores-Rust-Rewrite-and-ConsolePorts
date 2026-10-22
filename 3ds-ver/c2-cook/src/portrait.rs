//! Menu pictures for animals that have none (roaming add-ons): the model in
//! its rest pose, seen from the side and a little in front, textured and
//! shaded, drawn in software on a transparent background.

use c2_core::model::{PreparedModel, MODEL_TEXTURE_SIZE};

/// The model drawn into a `w` x `h` RGBA8 picture, filling the middle
/// `fill` of its width (the 3DS menu shows the middle 2/3 of a picture).
/// `None` for a model with no faces.
pub fn render(m: &PreparedModel, w: usize, h: usize, fill: f32) -> Option<Vec<u8>> {
    // Side view (screen x along the body, y up), turned 30 degrees.
    let (s, c) = 0.52f32.sin_cos();
    let view: Vec<[f32; 3]> = m
        .positions
        .iter()
        .map(|p| [p[2] * c + p[0] * s, -p[1], p[0] * c - p[2] * s])
        .collect();
    let used = || {
        m.faces
            .iter()
            .flat_map(|f| f.v)
            .filter_map(|i| view.get(i as usize))
    };
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for v in used() {
        for k in 0..2 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if lo[0] > hi[0] {
        return None;
    }
    let (fw, fh) = (w as f32 * fill, h as f32 * 0.9);
    let scale = (fw / (hi[0] - lo[0]).max(1.0)).min(fh / (hi[1] - lo[1]).max(1.0));
    let off = [
        w as f32 / 2.0 - (lo[0] + hi[0]) / 2.0 * scale,
        h as f32 / 2.0 - (lo[1] + hi[1]) / 2.0 * scale,
    ];
    let screen: Vec<[f32; 3]> = view
        .iter()
        .map(|v| [v[0] * scale + off[0], v[1] * scale + off[1], v[2]])
        .collect();
    let tex = &m.texture_rgba;
    let ts = MODEL_TEXTURE_SIZE;
    let mut out = vec![0u8; w * h * 4];
    let mut depth = vec![f32::MAX; w * h];
    // Light from the upper front.
    let light = normalize([-0.4, -0.7, -0.6]);
    for f in &m.faces {
        let [Some(a), Some(b), Some(cc)] = f.v.map(|i| screen.get(i as usize)) else {
            continue;
        };
        let [Some(va), Some(vb), Some(vc)] = f.v.map(|i| view.get(i as usize)) else {
            continue;
        };
        let n = normalize(cross(sub(*vb, *va), sub(*vc, *va)));
        let shade = 0.45 + 0.55 * dot(n, light).abs();
        let area = edge(a, b, cc[0], cc[1]);
        if area.abs() < 1e-6 {
            continue;
        }
        let x0 = a[0].min(b[0]).min(cc[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(cc[0]).ceil() as usize).min(w);
        let y0 = a[1].min(b[1]).min(cc[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(cc[1]).ceil() as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = edge(b, cc, px, py) / area;
                let w1 = edge(cc, a, px, py) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * cc[2];
                let d = y * w + x;
                if z >= depth[d] {
                    continue;
                }
                let u = w0 * f.uv[0][0] + w1 * f.uv[1][0] + w2 * f.uv[2][0];
                let v = w0 * f.uv[0][1] + w1 * f.uv[1][1] + w2 * f.uv[2][1];
                let tx = ((u * ts as f32) as usize).min(ts - 1);
                let ty = ((v * ts as f32) as usize).min(ts - 1);
                let Some(t) = tex.get((ty * ts + tx) * 4..(ty * ts + tx) * 4 + 4) else {
                    continue;
                };
                if t[3] == 0 {
                    continue;
                }
                depth[d] = z;
                for k in 0..3 {
                    out[d * 4 + k] = (f32::from(t[k]) * shade).min(255.0) as u8;
                }
                out[d * 4 + 3] = 255;
            }
        }
    }
    Some(out)
}

fn edge(a: &[f32; 3], b: &[f32; 3], x: f32, y: f32) -> f32 {
    (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

#[cfg(test)]
mod tests {
    use super::*;
    use c2_core::model::PreparedFace;

    #[test]
    fn a_triangle_fills_the_middle() {
        let mut tex = vec![0u8; MODEL_TEXTURE_SIZE * MODEL_TEXTURE_SIZE * 4];
        tex.chunks_mut(4)
            .for_each(|p| p.copy_from_slice(&[200, 100, 50, 255]));
        let m = PreparedModel {
            positions: vec![[0.0, 0.0, -100.0], [0.0, 0.0, 100.0], [0.0, 80.0, 0.0]],
            faces: vec![PreparedFace {
                v: [0, 1, 2],
                uv: [[0.1, 0.1], [0.9, 0.1], [0.5, 0.9]],
                flags: 0,
            }],
            texture_rgba: tex,
        };
        let px = render(&m, 64, 32, 0.5).unwrap_or_default();
        let at = |x: usize, y: usize| px[(y * 64 + x) * 4 + 3];
        // Filled in the middle; nothing at the sides.
        assert_eq!(at(32, 20), 255);
        assert_eq!(at(2, 16), 0);
        assert_eq!(at(61, 16), 0);
    }
}
