// VR's fragment entry for the PC terrain (appended to common.wgsl and
// c2-pc's terrain.wgsl): the same colour as `fs`, worked out in gamma space
// as the PC game does, then the sRGB transfer undone, since the headset's
// images are sRGB and the hardware applies it again on writing.

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_vr(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(to_linear(fogged(texel(c.rgb) * i.light, i.dist)), i.alpha);
}

// The PC's local fog volumes (`c2-pc/src/shaders/fog_zones.wgsl`) need
// bindings the headset's sets do not have; not drawn here yet.
fn zone_fog(p: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn zone_fogged(rgb: vec3<f32>, f: vec4<f32>) -> vec3<f32> {
    return rgb;
}

fn zone_glow(world: vec3<f32>, a: f32) -> vec3<f32> {
    return vec3<f32>(0.0);
}

fn zone_fog_under_water() -> bool {
    return false;
}
