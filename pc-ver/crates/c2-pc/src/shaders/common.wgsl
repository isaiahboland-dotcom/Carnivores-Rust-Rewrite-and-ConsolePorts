struct Globals {
    view_proj: mat4x4<f32>,
    proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start, y: fog end (world units)
    fog: vec4<f32>,
    // x: night vision (0/1), y: time in seconds
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

fn fogged(rgb: vec3<f32>, dist: f32) -> vec3<f32> {
    let f = clamp((dist - g.fog.x) / max(g.fog.y - g.fog.x, 1.0), 0.0, 1.0);
    return mix(rgb, g.fog_color.rgb, f);
}

// Resources.cpp: BrightenTexture at night: B = G >> 3, R = G >> 3 (5-bit).
fn texel(c: vec3<f32>) -> vec3<f32> {
    if (g.misc.x < 0.5) {
        return c;
    }
    let lo = floor(c.g * 31.0 / 8.0) / 31.0;
    return vec3<f32>(lo, c.g, lo);
}
