// Water running off the screen after surfacing (PC option
// `modern_underwater`, not in the original): a few drops near the screen's
// edges that slide down, leave a trail and dry, and a film that drains in the
// first moment.
// Drawn over the world, under the HUD. Nothing is refracted: a drop is a
// shaded lens (dark rim, light below, a highlight above).

struct Drops {
    // Seconds since surfacing.
    age: f32,
    // Width over height.
    aspect: f32,
    // Picks this surfacing's drops.
    seed: f32,
    // Seconds the drops last.
    life: f32,
};

@group(0) @binding(0) var<uniform> d: Drops;

const COUNT: i32 = 16;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // x: -aspect/2..aspect/2, y: -0.5 (top) to 0.5 (bottom).
    @location(0) p: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vid: u32) -> VOut {
    let c = vec2<f32>(f32((vid << 1u) & 2u), f32(vid & 2u)) * 2.0 - 1.0;
    var o: VOut;
    o.clip = vec4<f32>(c, 0.0, 1.0);
    o.p = vec2<f32>(c.x * 0.5 * d.aspect, -c.y * 0.5);
    return o;
}

fn hash(n: f32) -> f32 {
    return fract(sin(n * 12.9898) * 43758.547);
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let half_w = 0.5 * d.aspect;
    // The film: thickest at the edges, gone in the first fifth of the time.
    let edge = max(abs(i.p.x) / half_w, abs(i.p.y) * 2.0);
    let film = smoothstep(0.55, 1.0, edge) * 0.22 * (1.0 - smoothstep(0.0, d.life * 0.2, d.age));
    var rgb = vec3<f32>(0.62, 0.78, 0.92) * film;
    var a = film;
    for (var k = 0; k < COUNT; k++) {
        let s = d.seed + f32(k) * 7.31;
        // Two in five down each side, one in five along the top.
        let side = f32(k % 5) * 0.2;
        let r = mix(0.018, 0.05, hash(s + 1.0));
        // Left, right or along the top, within the outer sixth.
        var c = vec2<f32>((hash(s + 2.0) - 0.5) * d.aspect, -0.5 + hash(s + 3.0) * 0.16);
        if (side < 0.4) {
            c = vec2<f32>(-half_w * (1.0 - hash(s + 2.0) * 0.3), hash(s + 3.0) - 0.5);
        } else if (side < 0.8) {
            c = vec2<f32>(half_w * (1.0 - hash(s + 2.0) * 0.3), hash(s + 3.0) - 0.5);
        }
        // Each dries at its own time; the heavier slide further.
        let life = d.life * mix(0.55, 1.0, hash(s + 4.0));
        let fade = 1.0 - smoothstep(life * 0.6, life, d.age);
        let top = c.y;
        c.y += (d.age * 0.05 + d.age * d.age * 0.03) * (r / 0.05) * mix(0.4, 1.0, hash(s + 5.0));
        // The trail it leaves: a thin wet line from where it started.
        let across = abs(i.p.x - c.x) / (r * 0.3);
        if (across < 1.0 && i.p.y > top && i.p.y < c.y) {
            let trail = fade * 0.13 * (1.0 - across) * smoothstep(top, c.y, i.p.y);
            rgb = rgb * (1.0 - trail) + vec3<f32>(0.75, 0.87, 1.0) * trail;
            a = a * (1.0 - trail) + trail;
        }
        var q = (i.p - c) / r;
        // A tail above: the drop is longer where it came from.
        if (q.y < 0.0) {
            q.y *= 0.5;
        }
        let rho = length(q);
        if (rho >= 1.0 || fade <= 0.0) {
            continue;
        }
        let rim = smoothstep(0.7, 1.0, rho);
        let shine = 1.0 - smoothstep(0.0, 0.32, length(q - vec2<f32>(-0.3, -0.38)));
        // A lens turns the picture over: the sky's light sits at the bottom.
        let lens = vec3<f32>(0.70, 0.84, 1.0) * (0.45 + 0.4 * q.y) * (1.0 - 0.6 * rim);
        let colour = mix(lens, vec3<f32>(1.0), shine);
        let cover = fade * (1.0 - smoothstep(0.92, 1.0, rho)) * clamp(0.26 + 0.42 * rim + 0.7 * shine, 0.0, 1.0);
        rgb = rgb * (1.0 - cover) + colour * cover;
        a = a * (1.0 - cover) + cover;
    }
    return vec4<f32>(rgb / max(a, 0.0001), a);
}
