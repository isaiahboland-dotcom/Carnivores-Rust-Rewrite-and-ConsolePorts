// Terrain and water, generated in the vertex shader from per-cell data.
// Mirrors renderd3d.cpp: ProcessMap / DrawTPlane (triangulation, texture
// rotation) and ProcessMapW (water quads).

struct Params {
    // Map cell of the window's top-left corner.
    origin: vec2<i32>,
    // Window width in cells.
    width: u32,
    layers: u32,
};

// cells[i].x = height | light << 8 | water << 16
// cells[i].y = texture1 | flags << 16
@group(1) @binding(0) var<storage, read> cells: array<vec2<u32>>;
// waters[i] = (level * 64, texture index, transparency, 0)
@group(1) @binding(1) var<storage, read> waters: array<vec4<f32>>;
@group(1) @binding(2) var<uniform> params: Params;
@group(1) @binding(3) var tex: texture_2d_array<f32>;
@group(1) @binding(4) var samp: sampler;

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
};

fn degenerate() -> VOut {
    var o: VOut;
    o.clip = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return o;
}

fn cell_at(x: i32, y: i32) -> vec2<u32> {
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

fn window_cell(vid: u32) -> vec2<i32> {
    let ci = vid / 6u;
    return params.origin + vec2<i32>(i32(ci % params.width), i32(ci / params.width));
}

fn in_map(c: vec2<i32>) -> bool {
    return c.x >= 0 && c.y >= 0 && c.x < MAP - 1 && c.y < MAP - 1;
}

@vertex
fn vs_ground(@builtin(vertex_index) vid: u32) -> VOut {
    let c = window_cell(vid);
    if (!in_map(c)) {
        return degenerate();
    }
    let here = cell_at(c.x, c.y);
    let flags = here.y >> 16u;
    let d = corner(vid % 6u, (flags & FM_REVERSE) != 0u);
    let v = c + d;
    let vc = cell_at(v.x, v.y);
    let p = vec3<f32>(f32(v.x) * 256.0, f32(vc.x & 0xffu) * 64.0, f32(v.y) * 256.0);
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = mix(vec2<f32>(TC_MIN), vec2<f32>(TC_MAX), rotate_uv(vec2<f32>(d), flags & 3u));
    o.light = f32((vc.x >> 8u) & 0xffu) / 255.0;
    o.layer = min(here.y & 0xffffu, params.layers - 1u);
    o.dist = distance(p, g.cam_pos.xyz);
    o.alpha = 1.0;
    return o;
}

@vertex
fn vs_water(@builtin(vertex_index) vid: u32) -> VOut {
    let c = window_cell(vid);
    if (!in_map(c)) {
        return degenerate();
    }
    // ProcessMapW: all four corners must be water cells.
    let w = (cell_at(c.x, c.y).x >> 16u) & 0xffu;
    if (w == 255u
        || ((cell_at(c.x + 1, c.y).x >> 16u) & 0xffu) == 255u
        || ((cell_at(c.x, c.y + 1).x >> 16u) & 0xffu) == 255u
        || ((cell_at(c.x + 1, c.y + 1).x >> 16u) & 0xffu) == 255u
        || w >= arrayLength(&waters)) {
        return degenerate();
    }
    let wt = waters[w];
    let d = corner(vid % 6u, false);
    let v = c + d;
    let p = vec3<f32>(f32(v.x) * 256.0, wt.x, f32(v.y) * 256.0);
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = mix(vec2<f32>(TC_MIN), vec2<f32>(TC_MAX), vec2<f32>(d));
    o.light = 1.0;
    o.layer = min(u32(wt.y), params.layers - 1u);
    o.dist = distance(p, g.cam_pos.xyz);
    o.alpha = wt.z;
    return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(fogged(c.rgb * i.light, i.dist), i.alpha);
}
