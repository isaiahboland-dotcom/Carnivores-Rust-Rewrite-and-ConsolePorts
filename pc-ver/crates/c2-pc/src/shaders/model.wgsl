// Textured models (characters, weapons, RSC objects), instanced.
// Light is a 0..1 multiplier, as D3DTBLEND_MODULATE with a grey vertex colour.

@group(1) @binding(0) var tex: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) offset: vec3<f32>,
    @location(3) turns: f32,
    @location(4) light: f32,
    @location(5) layer: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) dist: f32,
};

@vertex
fn vs(v: VIn) -> VOut {
    let a = v.turns * 1.5707964;
    let c = cos(a);
    let s = sin(a);
    let p = vec3<f32>(v.pos.x * c + v.pos.z * s, v.pos.y, -v.pos.x * s + v.pos.z * c) + v.offset;
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = v.uv;
    o.light = v.light;
    o.layer = v.layer;
    o.dist = distance(p, g.cam_pos.xyz);
    return o;
}

@fragment
fn fs_opaque(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    if (c.a < 0.5) {
        discard;
    }
    return vec4<f32>(fogged(c.rgb * i.light, i.dist), 1.0);
}

// sfTransparent faces: blended. The exact original factor is still to be
// confirmed against the retail build; 50% matches it by eye.
@fragment
fn fs_blend(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(fogged(c.rgb * i.light, i.dist), c.a * 0.5);
}
