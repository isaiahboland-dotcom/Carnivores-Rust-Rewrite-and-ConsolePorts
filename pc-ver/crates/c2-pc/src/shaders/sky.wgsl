// renderd3d.cpp: RenderSkyPlane (scrolling cloud plane above the camera) and
// RenderSun / RenderModelSun (camera-facing sun or moon, blended additively).

// Layers 0..2: dawn/day/night skies; layers 3..4: sun/moon model textures.
@group(1) @binding(0) var sky_tex: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

// The plane sits 4*512*16 units above the camera and follows it.
const SKY_H: f32 = 32768.0;
const SKY_R: f32 = 1000000.0;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vid: u32) -> SkyOut {
    var c = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let p = vec3<f32>(g.cam_pos.x + c[vid].x * SKY_R, g.cam_pos.y + SKY_H, g.cam_pos.z + c[vid].y * SKY_R);
    var o: SkyOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    // Always behind everything; depth testing is off for this pass anyway.
    o.clip.z = 0.0;
    o.world = p;
    return o;
}

// Stereographic coordinates are seamless over the upper hemisphere. Direction
// relative to the camera keeps the stars fixed as the player walks; unlike the
// cloud UVs, these coordinates never scroll.
fn star_field(direction: vec3<f32>) -> f32 {
    let p = direction.xz / (1.0 + direction.y) * 80.0;
    let cell = floor(p);
    var h = fract(vec3<f32>(cell.x, cell.y, cell.x) * vec3(0.1031, 0.1030, 0.0973));
    h += dot(h, h.yxz + 33.33);
    let random = fract((h.xxy + h.yzz) * h.zyx);
    let center = vec2(0.2) + random.xy * 0.6;
    let radius = mix(0.045, 0.095, random.z);
    let distance_to_star = length(fract(p) - center);
    let pixel = max(length(fwidth(p)), 0.001);
    let edge = max(radius, pixel * 0.5);
    let coverage = 1.0 - smoothstep(0.0, edge, distance_to_star);
    let energy = min(1.0, radius * radius / (edge * edge));
    return coverage * energy * step(0.88, random.z)
        * smoothstep(0.04, 0.25, direction.y);
}

@fragment
fn fs_sky(i: SkyOut) -> @location(0) vec4<f32> {
    // Texture space: 0.002 texels per world unit, scrolled by
    // (RealTime & 0x1FFFF) / 512 texels (wraps after exactly one texture).
    let ms = g.misc.y * 1000.0;
    let dtt = (ms - floor(ms / 131072.0) * 131072.0) / 512.0;
    let uv = vec2<f32>(i.world.x * 0.002 + dtt, i.world.z * 0.002 - dtt) / 256.0;
    let c = textureSample(sky_tex, samp, uv, 0) * g.daylight.x
        + textureSample(sky_tex, samp, uv, 1) * g.daylight.y
        + textureSample(sky_tex, samp, uv, 2) * g.daylight.z;
    // Fade with distance: alpha = 40240 / (40240 + max(0, z - 100200)).
    let d = max(distance(i.world, g.cam_pos.xyz) - 100200.0, 0.0);
    let a = 40240.0 / (40240.0 + d);
    let direction = normalize(i.world - g.cam_pos.xyz);
    let stars = star_field(direction) * g.daylight.w * smoothstep(0.05, 0.85, g.daylight.z);
    // Stars emit light: night ambient darkening must not extinguish them.
    // Keep night-vision grading, cloud texture and the existing horizon fade.
    return vec4<f32>(texel(c.rgb) + grade_texel(vec3(0.78, 0.85, 1.0)) * stars, a);
}

struct SunIn {
    // Already in view space.
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: f32,
    @location(3) alpha: f32,
};

struct SunOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) alpha: f32,
};

@vertex
fn vs_sun(v: SunIn) -> SunOut {
    var o: SunOut;
    o.clip = g.proj * vec4<f32>(v.pos, 1.0);
    o.clip.z = 0.0;
    o.uv = v.uv;
    o.layer = u32(v.layer);
    o.alpha = v.alpha;
    return o;
}

@fragment
fn fs_sun(i: SunOut) -> @location(0) vec4<f32> {
    let c = textureSample(sky_tex, samp, i.uv, i.layer);
    // Alpha 200 (SkyTraceK = 1: nothing in front of the sun yet).
    return vec4<f32>(grade_texel(c.rgb) * (200.0 / 255.0) * i.alpha, 1.0);
}
