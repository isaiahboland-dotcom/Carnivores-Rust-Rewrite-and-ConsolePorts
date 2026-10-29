// The menu panel (`panel.rs`): the menu's canvas (`canvas.rs`, drawn on
// the CPU) on a quad in the world, as an sRGB texture (read as linear,
// written to the sRGB swapchain as it is); the pointer is a gold dot.

struct Push {
    mvp: mat4x4<f32>,
    // The pointer, canvas units (800x600); z > 0 shown.
    pointer: vec4<f32>,
}

var<immediate> p: Push;
@group(0) @binding(0) var pic: texture_2d_array<f32>;
@group(0) @binding(1) var samp: sampler;

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
    var col = textureSample(pic, samp, i.uv, 0).rgb;
    if p.pointer.z > 0.0 {
        let d = distance(i.uv * vec2<f32>(800.0, 600.0), p.pointer.xy);
        if d < 7.0 {
            col = linear(vec3<f32>(240.0, 200.0, 70.0) / 255.0);
        } else if d < 9.5 {
            col = vec3<f32>(0.0);
        }
    }
    return vec4<f32>(col, 1.0);
}
