struct Globals {
    view_proj: mat4x4<f32>,
    proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start, y: fog end (world units), z: desaturated night 0..1
    // (PC option `desaturated_night`).
    fog: vec4<f32>,
    // x: night vision blend (0..1), y: time in seconds, z: brightness factor, w: ambient light
    misc: vec4<f32>,
    // xyz: dawn/day/night blend, w: dynamic-time stars enabled.
    daylight: vec4<f32>,
    // xyz: camera forward, w: flashlight mode (0 off, 1 broad, 2 narrow).
    torch: vec4<f32>,
    // xyz: what the water over the camera lets through of the ground's colour
    // (PC option `modern_water_tint`; ones otherwise).
    water: vec4<f32>,
    // xyz: direction to the sun (world), w: sky glow on (PC option
    // `modern_sky_glow`).
    sun: vec4<f32>,
    // xyz: direction to the moon.
    moon: vec4<f32>,
    // The water's mirror (PC option `modern_reflections`). x: the water
    // level mirrored, y: on, zw: one over the frame's size in pixels.
    reflect: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

// The world's colour faded into the distance fog. Desaturated night (PC
// option, the modernized Modder's Edition's night without night vision,
// terrain.frag / model.frag): 60% towards grey and half as bright, on the
// world only, before the fade (theirs after it), so the far ground still
// meets the sky.
fn fogged(rgb: vec3<f32>, dist: f32) -> vec3<f32> {
    var c = rgb;
    let night = g.fog.z;
    if (night > 0.0) {
        let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
        c = mix(c, vec3<f32>(grey), 0.6 * night) * mix(1.0, 0.5, night);
    }
    let f = clamp((dist - g.fog.x) / max(g.fog.y - g.fog.x, 1.0), 0.0, 1.0);
    return mix(c, g.fog_color.rgb, f);
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

// A camera-mounted light: broad 20-cell (80m) or narrow 40-cell (160m) beam.
// Depth testing still occludes geometry; no additional shadow-map pass.
fn surface_light(c: vec3<f32>, light: f32, world: vec3<f32>) -> vec3<f32> {
    let offset = world - g.cam_pos.xyz;
    let dist = length(offset);
    let facing = dot(offset / max(dist, 0.001), g.torch.xyz);
    let narrow = g.torch.w > 1.5;
    let cone = smoothstep(select(0.88, 0.97, narrow), select(0.98, 0.995, narrow), facing);
    let falloff = 1.0 - smoothstep(select(1024.0, 2048.0, narrow), select(5120.0, 10240.0, narrow), dist);
    let beam = min(g.torch.w, 1.0) * cone * falloff;
    return grade_texel(c) * max(light * g.misc.w, beam);
}
