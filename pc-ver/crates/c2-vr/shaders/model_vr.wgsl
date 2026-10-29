// VR's fragment entries for the PC's models (appended to common.wgsl and
// c2-pc's model.wgsl): `fs_opaque` and `fs_blend` with the sRGB transfer
// undone, as in terrain_vr.wgsl.

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_opaque_vr(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    if (c.a < 0.5) {
        discard;
    }
    return vec4<f32>(to_linear(fogged(texel(c.rgb) * i.light, i.dist)), 1.0);
}

@fragment
fn fs_blend_vr(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(to_linear(fogged(texel(c.rgb) * i.light, i.dist)), c.a * 0.5);
}
