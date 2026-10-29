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
