struct Globals {
    view_proj: mat4x4<f32>,
    proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start, y: fog end (world units)
    fog: vec4<f32>,
    // x: night vision blend (0..1), y: time in seconds, z: brightness factor, w: ambient light
    misc: vec4<f32>,
    // xyz: dawn/day/night blend, w: dynamic-time stars enabled.
    daylight: vec4<f32>,
    // xyz: camera forward, w: flashlight enabled.
    torch: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

fn fogged(rgb: vec3<f32>, dist: f32) -> vec3<f32> {
    let f = clamp((dist - g.fog.x) / max(g.fog.y - g.fog.x, 1.0), 0.0, 1.0);
    return mix(rgb, g.fog_color.rgb, f);
}

// Resources.cpp: BrightenTexture: each 5-bit channel * (OptBrightness + 128)
// >> 8, capped; at night B = G >> 3, R = G >> 3.
fn grade_texel(c0: vec3<f32>) -> vec3<f32> {
    let c = min(floor(round(c0 * 31.0) * g.misc.z + 0.001) / 31.0, vec3<f32>(1.0));
    let lo = floor(c.g * 31.0 / 8.0) / 31.0;
    return mix(c, vec3<f32>(lo, c.g, lo), g.misc.x);
}

fn texel(c: vec3<f32>) -> vec3<f32> {
    return grade_texel(c) * g.misc.w;
}

// A camera-mounted light with a soft cone and a 20-cell (80m) range.
// Depth testing still occludes geometry; no additional shadow-map pass.
fn surface_light(c: vec3<f32>, light: f32, world: vec3<f32>) -> vec3<f32> {
    let offset = world - g.cam_pos.xyz;
    let dist = length(offset);
    let facing = dot(offset / max(dist, 0.001), g.torch.xyz);
    let cone = smoothstep(0.88, 0.98, facing);
    let falloff = 1.0 - smoothstep(1024.0, 5120.0, dist);
    let beam = g.torch.w * cone * falloff;
    return grade_texel(c) * max(light * g.misc.w, beam);
}
