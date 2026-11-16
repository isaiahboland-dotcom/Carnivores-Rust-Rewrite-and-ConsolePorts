// Terrain and water, generated in the vertex shader from per-cell data.
// Mirrors renderd3d.cpp: ProcessMap / DrawTPlane (triangulation, texture
// rotation) and ProcessMapW (water quads).

struct Params {
    // Map cell of the finest level's top-left corner.
    origin: vec2<i32>,
    // Cells per side of each level (each level's cells are `1 << level`
    // map cells across).
    width: u32,
    layers: u32,
    // RealTime in ms, for cloud shadows.
    time_ms: u32,
    clouds: u32,
    // Detail levels: 1 is the original's full-detail window. Past the
    // first, each level is a square ring twice the size of the one inside
    // it, its cells twice as large (PC high-end view only).
    levels: u32,
    // Cells drawn from the camera's cell `eye` along each axis: the
    // coarsest level reaches past it and stops at the first cell beyond.
    reach: i32,
    eye: vec2<i32>,
    // Animated waves on open water (PC option, `wave_offset`).
    waves: u32,
    _pad: u32,
};

// cells[i].x = height | light << 8 | water << 16 | water phase << 24
// cells[i].y = texture1 | flags << 16
// cells[i].z = dawn light | day light << 8 | night light << 16
// cells[i].w = RandomMap here | RandomMap at (x + 7, y + 11) << 16 (0..1023)
@group(1) @binding(0) var<storage, read> cells: array<vec4<u32>>;
// waters[i] = (level * 64, texture index, RSC transparency coefficient, 0)
@group(1) @binding(1) var<storage, read> waters: array<vec4<f32>>;
@group(1) @binding(2) var<uniform> params: Params;
@group(1) @binding(3) var tex: texture_2d_array<f32>;
@group(1) @binding(4) var samp: sampler;
// The RSC's 128x128 cloud map, 4 bytes per u32.
@group(1) @binding(5) var<storage, read> sky_map: array<u32>;
// The world mirrored in the water (render/reflection.rs), read by fs_water
// only.
@group(2) @binding(0) var mirror_tex: texture_2d<f32>;
@group(2) @binding(1) var mirror_samp: sampler;

const MAP: i32 = 1024;
const FM_REVERSE: u32 = 0x10u;
const FM_WATER: u32 = 0x80u;
const FM_WATER_A: u32 = 0x8000u;
// TCMIN/TCMAX from Hunt.h: half-texel inset to avoid bleeding.
const TC_MIN: f32 = 0.0073947906;
const TC_MAX: f32 = 0.9926052;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) dist: f32,
    @location(4) alpha: f32,
    @location(5) world: vec3<f32>,
    // Zone fog (fog_zones.wgsl: zone_fog), the ground only.
    @location(6) zone: vec4<f32>,
};

fn degenerate() -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return o;
}

fn cell_at(x: i32, y: i32) -> vec4<u32> {
    return cells[u32(y * MAP + x)];
}

// Corner (dx, dy) of each of the 6 vertices, for the two triangulations.
fn corner(k: u32, reverse: bool) -> vec2<i32> {
    var n = array<vec2<i32>, 6>(vec2(0, 0), vec2(1, 0), vec2(1, 1), vec2(0, 0), vec2(1, 1), vec2(0, 1));
    var r = array<vec2<i32>, 6>(vec2(0, 0), vec2(1, 0), vec2(0, 1), vec2(0, 1), vec2(1, 0), vec2(1, 1));
    if (reverse) {
        return r[k];
    }
    return n[k];
}

// DrawTPlane's 4 TDirection tables reduce to rotating the corner.
fn rotate_uv(d: vec2<f32>, dir: u32) -> vec2<f32> {
    switch dir {
        case 1u: { return vec2<f32>(d.y, 1.0 - d.x); }
        case 2u: { return vec2<f32>(1.0 - d.x, 1.0 - d.y); }
        case 3u: { return vec2<f32>(1.0 - d.y, d.x); }
        default: { return d; }
    }
}

fn sky_at(x: i32, y: i32) -> i32 {
    let i = u32((y & 127) * 128 + (x & 127));
    return i32((sky_map[i / 4u] >> ((i % 4u) * 8u)) & 0xffu);
}

// Hunt2.cpp: cloud shadow for map vertex (x, y). The cloud map scrolls one
// cell every 512 ms diagonally, interpolated with kx = ky = SKYDTime & 255.
fn cloud_shadow(x: i32, y: i32) -> i32 {
    let t = i32(params.time_ms >> 1u);
    let k = t & 255;
    let dt = t >> 8;
    let sx = x + dt;
    let sy = y + dt;
    let db = ((sky_at(sx, sy) * (256 - k) + sky_at(sx + 1, sy) * k) * (256 - k)
        + (sky_at(sx, sy + 1) * (256 - k) + sky_at(sx + 1, sy + 1) * k) * k) >> 17u;
    return clamp(db - 40, 0, 48);
}

// One cell of the window: its top-left map cell and its size in map cells.
struct LodCell {
    c: vec2<i32>,
    step: i32,
    level: u32,
    ok: bool,
};

fn half_width() -> i32 {
    return i32(params.width / 2u);
}

fn center() -> vec2<i32> {
    return params.origin + vec2<i32>(half_width());
}

// Cell `vid / 6` of a drawn chunk: `chunk` is its top-left map cell and
// its level (render/terrain.rs culls chunks; cells outside the level's
// ring or the map are dropped here).
fn lod_cell(vid: u32, chunk: vec4<i32>) -> LodCell {
    let ci = vid / 6u;
    var o: LodCell;
    o.level = u32(chunk.z);
    o.step = 1 << o.level;
    o.c = chunk.xy + vec2<i32>(i32(ci % 16u), i32(ci / 16u)) * o.step;
    let r = o.c - center();
    let outer = half_width() << o.level;
    let e = o.c - params.eye;
    o.ok = o.level < params.levels
        && r.x >= -outer && r.x < outer && r.y >= -outer && r.y < outer
        && e.x < params.reach && e.x + o.step > -params.reach
        && e.y < params.reach && e.y + o.step > -params.reach
        && o.c.x >= 0 && o.c.y >= 0 && o.c.x + o.step <= MAP - 1 && o.c.y + o.step <= MAP - 1;
    if (o.level > 0u) {
        // The level inside draws this part.
        let inner = half_width() << (o.level - 1u);
        if (r.x >= -inner && r.x < inner && r.y >= -inner && r.y < inner) {
            o.ok = false;
        }
    }
    return o;
}

fn vertex_height(v: vec2<i32>) -> f32 {
    let c = clamp(v, vec2<i32>(0), vec2<i32>(MAP - 1));
    return f32(cell_at(c.x, c.y).x & 0xffu) * 64.0;
}

fn water_index(v: vec2<i32>) -> u32 {
    return (cell_at(v.x, v.y).x >> 16u) & 0xffu;
}

fn vertex_light(v: vec2<i32>) -> f32 {
    let c = clamp(v, vec2<i32>(0), vec2<i32>(MAP - 1));
    let vc = cell_at(c.x, c.y);
    var light = dot(vec3<f32>(f32(vc.z & 255u), f32((vc.z >> 8u) & 255u), f32((vc.z >> 16u) & 255u)), g.daylight.xyz);
    if (params.clouds != 0u) {
        light = max(64.0, light - f32(cloud_shadow(c.x, c.y)));
    }
    return light;
}

// On a level's outer edge, the next level out has a vertex only every
// other one of this level's: the ones between take the mean of their two
// neighbours along the edge, so the two levels meet without gaps.
fn edge_neighbour(v: vec2<i32>, l: LodCell) -> vec2<i32> {
    if (l.level + 1u >= params.levels) {
        return vec2<i32>(0);
    }
    let r = v - center();
    let outer = half_width() << l.level;
    if (abs(r.x) == outer && ((r.y >> l.level) & 1) != 0) {
        return vec2<i32>(0, l.step);
    }
    if (abs(r.y) == outer && ((r.x >> l.level) & 1) != 0) {
        return vec2<i32>(l.step, 0);
    }
    return vec2<i32>(0);
}

// The ground's height at vertex `v` of cell `l`, as drawn (see edge_neighbour).
fn ground_height(v: vec2<i32>, l: LodCell) -> f32 {
    let e = edge_neighbour(v, l);
    if (e.x != 0 || e.y != 0) {
        return (vertex_height(v - e) + vertex_height(v + e)) * 0.5;
    }
    return vertex_height(v);
}

@vertex
fn vs_ground(@builtin(vertex_index) vid: u32, @location(0) chunk: vec4<i32>) -> VOut {
    let l = lod_cell(vid, chunk);
    if (!l.ok) {
        return degenerate();
    }
    let c = l.c;
    let here = cell_at(c.x, c.y);
    let flags = here.y >> 16u;
    let d = corner(vid % 6u, (flags & FM_REVERSE) != 0u);
    let v = c + d * l.step;
    let h = ground_height(v, l);
    var light = vertex_light(v);
    let e = edge_neighbour(v, l);
    if (e.x != 0 || e.y != 0) {
        light = (vertex_light(v - e) + vertex_light(v + e)) * 0.5;
    }
    let p = vec3<f32>(f32(v.x) * 256.0, h, f32(v.y) * 256.0);
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.world = p;
    // A larger cell stretches its top-left cell's texture over it.
    o.uv = mix(vec2<f32>(TC_MIN), vec2<f32>(TC_MAX), rotate_uv(vec2<f32>(d), flags & 3u));
    o.light = light / 255.0;
    o.layer = min(here.y & 0xffffu, params.layers - 1u);
    o.dist = distance(p, g.cam_pos.xyz);
    o.alpha = 1.0;
    // Hunt2.cpp: PreCashGroundModel: no zone fog on the ground of water
    // cells, but for the camera under water (the original's underwater fog).
    let vflags = cell_at(clamp(v.x, 0, MAP - 1), clamp(v.y, 0, MAP - 1)).y >> 16u;
    if ((vflags & (FM_WATER | FM_WATER_A)) == 0u || zone_fog_under_water()) {
        o.zone = zone_fog(p);
    }
    return o;
}

@vertex
fn vs_water(@builtin(vertex_index) vid: u32, @location(0) chunk: vec4<i32>) -> VOut {
    let l = lod_cell(vid, chunk);
    if (!l.ok) {
        return degenerate();
    }
    let c = l.c;
    let s = l.step;
    let ends = array<vec2<i32>, 4>(c, c + vec2<i32>(s, 0), c + vec2<i32>(0, s), c + vec2<i32>(s, s));
    // The first water corner's record (top-left first, as ProcessMapW).
    var w = 255u;
    for (var k = 0u; k < 4u; k++) {
        w = water_index(ends[k]);
        if (w != 255u) {
            break;
        }
    }
    if (w >= arrayLength(&waters)) {
        return degenerate();
    }
    let wt = waters[w];
    // ProcessMapW: all four corners must be water cells. A coarser cell
    // (high-end view) spans water narrower than itself, so it also takes
    // dry corners whose drawn ground is above the water: that ground hides
    // the water there by depth, as the bank does at full detail.
    for (var k = 0u; k < 4u; k++) {
        if (water_index(ends[k]) == 255u && (l.level == 0u || ground_height(ends[k], l) <= wt.x)) {
            return degenerate();
        }
    }
    let d = corner(vid % 6u, false);
    let v = c + d * s;
    let p = vec3<f32>(f32(v.x) * 256.0, wt.x, f32(v.y) * 256.0);
    var o: VOut;
    // Only the drawn position sways: the light, alpha, fog and texture are
    // the flat water's.
    o.clip = g.view_proj * vec4<f32>(p + wave_offset(v, l), 1.0);
    o.world = p;
    o.uv = mix(vec2<f32>(TC_MIN), vec2<f32>(TC_MAX), vec2<f32>(d));
    // Hunt2.cpp: Light = 168 - (int)(wdelta * 24), wdelta = sin(-pi/2 +
    // RandomMap / 128 + RealTime / 200) per map vertex (c2_core::area::water_light).
    let phase = f32(cell_at(v.x, v.y).x >> 24u);
    let wdelta = sin(-1.5707964 + phase + f32(params.time_ms) / 200.0);
    o.light = (168.0 - trunc(wdelta * 24.0)) / 255.0;
    o.layer = min(u32(wt.y), params.layers - 1u);
    o.dist = distance(p, g.cam_pos.xyz);
    o.alpha = water_alpha(v, wt, o.dist, wdelta);
    return o;
}

// Animated waves (PC option, not in the original, whose water is flat): the
// modernized Modder's Edition's swell, Effects.cpp: PreCashGroundModel. Three
// sines per axis (amplitudes WWave1..3Amp, t = RealTime / 200 * WWaveSpeed,
// the height 0.8 of the sideways sway) move each open-water vertex; bank
// vertices (in the water's quads, not fmWater) stay put, so the shore keeps
// its line. That engine reads a 32x32 table per frame, which repeats with a
// seam every 32 cells; here the map position runs on unwrapped.
// The sway fades out over the 8 cells before the last 4 of the full-detail
// window (theirs stops at ctViewR - 4), where the coarser levels, drawn
// flat, take over.
fn wave_offset(v: vec2<i32>, l: LodCell) -> vec3<f32> {
    let cell = cell_at(v.x, v.y);
    if (params.waves == 0u || l.level != 0u || ((cell.y >> 16u) & FM_WATER) == 0u) {
        return vec3<f32>(0.0);
    }
    let c = abs(v - center());
    let e = abs(v - params.eye);
    let edge = min(half_width() - max(c.x, c.y), params.reach - max(e.x, e.y));
    let fade = clamp((f32(edge) - 4.0) / 8.0, 0.0, 1.0);
    let t = f32(params.time_ms) / 200.0 * 0.5;
    let px = f32(v.x) * 0.5;
    let py = f32(v.y) * 0.5;
    let r1 = f32(cell.w & 0xffffu) * 0.01;
    let r2 = f32(cell.w >> 16u) * 0.01;
    let amp = vec3<f32>(14.58, 8.10, 5.00) * fade;
    let x = vec3<f32>(
        sin(px + py + t),
        sin(px * 1.5 - py * 0.7 + t * 1.3 + r1),
        sin(px * 2.3 + py * 1.2 + t * 2.1 + r2),
    );
    let y = vec3<f32>(
        sin(px * 0.8 + py * 0.6 + t * 0.9),
        sin(px * 1.2 - py * 0.9 + t * 1.1 + r1),
        sin(px * 2.0 + py * 1.5 + t * 1.8 + r2),
    );
    let z = vec3<f32>(
        sin(1.5707964 + px + py + t),
        sin(1.0471976 + px * 1.5 - py * 0.7 + t * 1.3 + r2),
        sin(0.7853982 + px * 2.3 + py * 1.2 + t * 2.1 + r1),
    );
    return vec3<f32>(dot(x, amp), dot(y, amp) * 0.8, dot(z, amp));
}

// Hunt2.cpp (hardware path): the RSC transparency scales the water's depth
// at this vertex; distance, shimmer and a grazing view add opacity, so
// shallow water near the camera is clear and the far sea opaque.
// From under the water (`UNDERWATER`): `160 - dist * 160 / 220 / ctViewR`,
// at least 10, of 255, so the surface overhead is a veil that thins out
// towards the edge of the view.
fn water_alpha(v: vec2<i32>, wt: vec4<f32>, dist: f32, wdelta: f32) -> f32 {
    if (g.cam_pos.y < wt.x) {
        return max(160.0 - dist * 160.0 / 220.0 / f32(max(params.reach, 1)), 10.0) / 255.0;
    }
    let ground = vertex_height(v);
    let to_ground = vec3<f32>(f32(v.x) * 256.0, ground, f32(v.y) * 256.0) - g.cam_pos.xyz;
    let down = max(-normalize(to_ground).y, 0.0);
    let depth = (wt.x - ground) / 64.0;
    let a = max((depth * 2.0 + 4.0) * wt.z + dist / 256.0 + wdelta * 2.0, 0.0);
    return min(a * 6.0 / (down + 0.1), 255.0) / 255.0;
}

// The ground. Under water with the depth tint on (PC option, not in the
// original), the water absorbs its colour by depth, the water's own hue the
// least (the modernized Modder's Edition's terrain.frag); the water's
// surface and the models are left as they are, as there.
@fragment
fn fs_ground(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    var lit = surface_light(c.rgb, i.light, i.world);
    if (any(g.water.xyz < vec3<f32>(1.0))) {
        lit = max(lit * g.water.xyz, vec3<f32>(0.01));
    }
    let zoned = zone_fogged(lit, i.zone) + zone_glow(i.world, i.zone.a);
    return vec4<f32>(fogged(zoned, i.dist), i.alpha);
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(fogged(zone_fogged(surface_light(c.rgb, i.light, i.world), i.zone), i.dist), i.alpha);
}

// The most the water's own colour covers of what is under it with
// reflections on: the bed and the animals in the water stay in sight where
// the original's alpha has closed.
const SEE_THROUGH: f32 = 0.7;

// Where the mirror is read for the water at `world`, `frag` its pixel: the
// pixel itself (a point of the mirror's plane has the same place in both
// pictures), pushed about by a ripple that calms with distance.
fn mirror_uv(frag: vec2<f32>, world: vec3<f32>, dist: f32) -> vec2<f32> {
    let t = g.misc.y;
    let w = world.xz * 0.004;
    let ripple = vec2<f32>(
        sin(w.x * 1.7 + w.y * 0.6 + t * 1.3) + 0.5 * sin(w.x * 3.1 - w.y * 2.3 - t * 1.9),
        sin(w.y * 1.9 - w.x * 0.7 + t * 1.1) + 0.5 * sin(w.y * 2.7 + w.x * 2.9 + t * 2.3),
    );
    let calm = clamp(1536.0 / max(dist, 1.0), 0.25, 1.0);
    return frag * g.reflect.zw + ripple * vec2<f32>(0.003, 0.008) * calm;
}

// The water's surface. With reflections on (PC option, not in the original)
// and the camera over the water: a mirror by Fresnel's rule (Schlick, water's
// 2% head on, everything at a grazing view) over the water's own colour,
// which covers at most SEE_THROUGH of what is under it. The mirror holds one
// water level; water at another level reflects the fog's colour.
@fragment
fn fs_water(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    let own = fogged(surface_light(c.rgb, i.light, i.world), i.dist);
    if (g.reflect.y < 0.5 || g.cam_pos.y <= i.world.y) {
        return vec4<f32>(own, i.alpha);
    }
    let up = clamp((g.cam_pos.y - i.world.y) / max(i.dist, 1.0), 0.0, 1.0);
    let fresnel = 0.02 + 0.98 * pow(1.0 - up, 5.0);
    var seen = g.fog_color.rgb;
    if (abs(i.world.y - g.reflect.x) < 1.0) {
        seen = textureSampleLevel(mirror_tex, mirror_samp, mirror_uv(i.clip.xy, i.world, i.dist), 0.0).rgb;
    }
    let body = min(i.alpha, SEE_THROUGH) * (1.0 - fresnel);
    let a = fresnel + body;
    return vec4<f32>((seen * fresnel + own * body) / max(a, 0.0001), a);
}
