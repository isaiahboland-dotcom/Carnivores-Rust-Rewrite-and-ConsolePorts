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
    _pad: u32,
};

// cells[i].x = height | light << 8 | water << 16 | water phase << 24
// cells[i].y = texture1 | flags << 16
// cells[i].z = dawn light | day light << 8 | night light << 16
@group(1) @binding(0) var<storage, read> cells: array<vec4<u32>>;
// waters[i] = (level * 64, texture index, transparency, 0)
@group(1) @binding(1) var<storage, read> waters: array<vec4<f32>>;
@group(1) @binding(2) var<uniform> params: Params;
@group(1) @binding(3) var tex: texture_2d_array<f32>;
@group(1) @binding(4) var samp: sampler;
// The RSC's 128x128 cloud map, 4 bytes per u32.
@group(1) @binding(5) var<storage, read> sky_map: array<u32>;

const MAP: i32 = 1024;
const FM_REVERSE: u32 = 0x10u;
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
    o.ok = o.level < params.levels
        && r.x >= -outer && r.x < outer && r.y >= -outer && r.y < outer
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
    var h = vertex_height(v);
    var light = vertex_light(v);
    let e = edge_neighbour(v, l);
    if (e.x != 0 || e.y != 0) {
        h = (vertex_height(v - e) + vertex_height(v + e)) * 0.5;
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
    // ProcessMapW: all four corners must be water cells.
    let w = (cell_at(c.x, c.y).x >> 16u) & 0xffu;
    if (w == 255u
        || ((cell_at(c.x + s, c.y).x >> 16u) & 0xffu) == 255u
        || ((cell_at(c.x, c.y + s).x >> 16u) & 0xffu) == 255u
        || ((cell_at(c.x + s, c.y + s).x >> 16u) & 0xffu) == 255u
        || w >= arrayLength(&waters)) {
        return degenerate();
    }
    let wt = waters[w];
    let d = corner(vid % 6u, false);
    let v = c + d * s;
    let p = vec3<f32>(f32(v.x) * 256.0, wt.x, f32(v.y) * 256.0);
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.world = p;
    o.uv = mix(vec2<f32>(TC_MIN), vec2<f32>(TC_MAX), vec2<f32>(d));
    // Hunt2.cpp: Light = 168 - (int)(wdelta * 24), wdelta = sin(-pi/2 +
    // RandomMap / 128 + RealTime / 200) per map vertex (c2_core::area::water_light).
    let phase = f32(cell_at(v.x, v.y).x >> 24u);
    let wdelta = sin(-1.5707964 + phase + f32(params.time_ms) / 200.0);
    o.light = (168.0 - trunc(wdelta * 24.0)) / 255.0;
    o.layer = min(u32(wt.y), params.layers - 1u);
    o.dist = distance(p, g.cam_pos.xyz);
    o.alpha = wt.z;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(fogged(surface_light(c.rgb, i.light, i.world), i.dist), i.alpha);
}
