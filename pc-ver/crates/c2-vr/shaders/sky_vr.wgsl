// VR's fragment entries for the PC's sky (appended to common.wgsl and
// c2-pc's sky.wgsl): `fs_sky` and `fs_sun` with the sRGB transfer undone,
// as in terrain_vr.wgsl. WGSL can't call an entry point, so the two bodies
// are copies: keep them in step with sky.wgsl.

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_sky_vr(i: SkyOut) -> @location(0) vec4<f32> {
    let ms = g.misc.y * 1000.0;
    let dtt = (ms - floor(ms / 131072.0) * 131072.0) / 512.0;
    let uv = vec2<f32>(i.world.x * 0.002 + dtt, i.world.z * 0.002 - dtt) / 256.0;
    let c = textureSample(sky_tex, samp, uv, 0);
    let d = max(distance(i.world, g.cam_pos.xyz) - 100200.0, 0.0);
    let a = 40240.0 / (40240.0 + d);
    return vec4<f32>(to_linear(texel(c.rgb)), a);
}

@fragment
fn fs_sun_vr(i: SunOut) -> @location(0) vec4<f32> {
    let c = textureSample(sky_tex, samp, i.uv, 1);
    return vec4<f32>(to_linear(texel(c.rgb) * (200.0 / 255.0)), 1.0);
}
