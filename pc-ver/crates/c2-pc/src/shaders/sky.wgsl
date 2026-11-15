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

// A halo around the sun or moon at direction `body`: a Gaussian of the
// angle from it, `radius` wide (radians), its colour running from `core` to
// `rim` over that radius.
fn halo(direction: vec3<f32>, body: vec3<f32>, radius: f32, core: vec3<f32>, rim: vec3<f32>) -> vec3<f32> {
    let d = distance(direction, body) / radius;
    return mix(core, rim, clamp(d, 0.0, 1.0)) * exp(-d * d);
}

// Sun and moon glow (PC option, not in the original): the modernized
// Modder's Edition's sky.frag. `clear`: how much of the sky shows through the
// horizon's fade (their sky fog). Returns the light to add to the sky.
// Their halos are 80 (sun) and 110 (moon) pixels wide around the body's
// place on the screen; here the same angle at a 1080-line screen, around its
// direction, so they keep their size at any resolution and grow with the
// binoculars. Nothing is traced in front of the sun (their SkyTraceK = 1,
// as for our sun): the ground drawn over the sky hides the halo.
fn sky_glow(direction: vec3<f32>, clear: f32) -> vec3<f32> {
    let day = 1.0 - g.daylight.z;
    // A warm band along the horizon, by day only.
    let up = max(direction.y, 0.0);
    let horizon = vec3<f32>(1.0, 0.85, 0.6) * exp(-up * up * 20.0) * 0.15 * clear * day;
    let through = 1.0 - (1.0 - clear) * 0.5;
    let sun = halo(direction, g.sun.xyz, 0.089, vec3<f32>(1.0, 0.95, 0.8), vec3<f32>(0.9, 0.85, 1.0))
        * 0.18 * day;
    let moon = halo(direction, g.moon.xyz, 0.122, vec3<f32>(0.85, 0.90, 1.0), vec3<f32>(0.72, 0.80, 1.0))
        * 0.10 * g.daylight.z;
    return horizon + (sun + moon) * through;
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
    var sky = texel(c.rgb) + grade_texel(vec3(0.78, 0.85, 1.0)) * stars;
    if (g.sun.w > 0.5) {
        // The zenith a little darker than the horizon (1 to 0.92), then the
        // glow: light, like the stars, so the night's dimming leaves it.
        sky = sky * (1.0 - 0.08 * max(direction.y, 0.0)) + grade_texel(sky_glow(direction, a));
    }
    return vec4<f32>(sky, a);
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
