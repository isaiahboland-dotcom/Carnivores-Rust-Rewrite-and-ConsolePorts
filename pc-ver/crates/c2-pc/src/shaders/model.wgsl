// Textured models (characters, weapons, RSC objects), instanced.
// Light is a 0..1 multiplier, as D3DTBLEND_MODULATE with a grey vertex colour.

@group(1) @binding(0) var tex: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    // CalcLights: per-vertex sun term for each quarter turn (0..255 units).
    @location(2) vlight: vec4<f32>,
    @location(3) offset: vec3<f32>,
    @location(4) turns: f32,
    @location(5) light: f32,
    @location(6) layer: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) @interpolate(flat) layer: u32,
    @location(3) dist: f32,
};

fn transform(v: VIn) -> VOut {
    let a = v.turns * 1.5707964;
    let c = cos(a);
    let s = sin(a);
    let p = vec3<f32>(v.pos.x * c + v.pos.z * s, v.pos.y, -v.pos.x * s + v.pos.z * c) + v.offset;
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.uv = v.uv;
    // RenderModel: vertex colour = object light + VLight[VT][v].
    let vt = u32(v.turns) & 3u;
    o.light = clamp(v.light + v.vlight[vt] / 255.0, 0.0, 1.0);
    o.layer = v.layer;
    o.dist = distance(p, g.cam_pos.xyz);
    return o;
}

@vertex
fn vs(v: VIn) -> VOut {
    return transform(v);
}

// First-person weapon: squeeze depth toward the near plane so it always draws
// in front of the world (the original renders it with the z-buffer off) while
// still sorting its own faces correctly.
@vertex
fn vs_near(v: VIn) -> VOut {
    var o = transform(v);
    o.clip.z = o.clip.z * 0.02;
    return o;
}

@fragment
fn fs_opaque(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    if (c.a < 0.5) {
        discard;
    }
    return vec4<f32>(fogged(texel(c.rgb) * i.light, i.dist), 1.0);
}

// sfTransparent faces: blended. The exact original factor is still to be
// confirmed against the retail build; 50% matches it by eye.
@fragment
fn fs_blend(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(fogged(texel(c.rgb) * i.light, i.dist), c.a * 0.5);
}

// Water rings (`RenderWCircles`): added on top (`D3DBLEND_ONE`); the fade is
// folded into the light.
@fragment
fn fs_add(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv, i.layer);
    return vec4<f32>(texel(c.rgb) * i.light * c.a, 1.0);
}
