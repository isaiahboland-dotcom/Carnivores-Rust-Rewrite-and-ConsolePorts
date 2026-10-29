// The menu panel (`panel.rs`): one of the game's 800x600 menu screens on a
// quad in the world. The pictures are sRGB textures (read as linear, written
// to the sRGB swapchain as they are): layer 0 the screen, layer 1 its lit
// version (`*_ON.TGA`). The hit map (`*_MAP.RAW`, 400x300 region ids) says
// which region a texel is in: the selected one shows lit inside a gold box,
// ones not in VR yet dimmed; the pointer is a gold dot.

struct Push {
    mvp: mat4x4<f32>,
    // The selection's box, canvas pixels: x0, y0, x1, y1 (x1 <= x0: none).
    sel_box: vec4<f32>,
    // The pointer, canvas pixels; z > 0 shown.
    pointer: vec4<f32>,
    // x: the selected region (0 none), y: a bit per region offered.
    sel: vec4<u32>,
}

var<immediate> p: Push;
@group(0) @binding(0) var pics: texture_2d_array<f32>;
@group(0) @binding(1) var hit: texture_2d_array<u32>;
@group(0) @binding(2) var samp: sampler;

struct PanelOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_panel(@builtin(vertex_index) i: u32) -> PanelOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let uv = corners[i];
    var o: PanelOut;
    o.pos = p.mvp * vec4<f32>(uv.x - 0.5, 0.5 - uv.y, 0.0, 1.0);
    o.uv = uv;
    return o;
}

fn linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_panel(i: PanelOut) -> @location(0) vec4<f32> {
    let c = i.uv * vec2<f32>(800.0, 600.0);
    let h = vec2<i32>(clamp(i.uv * vec2<f32>(400.0, 300.0), vec2<f32>(0.0), vec2<f32>(399.0, 299.0)));
    let region = textureLoad(hit, h, 0, 0).r;
    let lit = region != 0u && region == p.sel.x;
    let base = textureSample(pics, samp, i.uv, 0).rgb;
    let on = textureSample(pics, samp, i.uv, 1).rgb;
    var col = select(base, on, lit);
    if region != 0u && ((p.sel.y >> region) & 1u) == 0u {
        col = col * 0.35;
    }
    let gold = linear(vec3<f32>(240.0, 200.0, 70.0) / 255.0);
    let b = p.sel_box;
    let t = 5.0;
    let outer = c.x >= b.x - t && c.x <= b.z + t && c.y >= b.y - t && c.y <= b.w + t;
    let inner = c.x > b.x && c.x < b.z && c.y > b.y && c.y < b.w;
    if b.z > b.x && outer && !inner {
        col = gold;
    }
    if p.pointer.z > 0.0 {
        let d = distance(c, p.pointer.xy);
        if d < 7.0 {
            col = gold;
        } else if d < 9.5 {
            col = vec3<f32>(0.0);
        }
    }
    return vec4<f32>(col, 1.0);
}
