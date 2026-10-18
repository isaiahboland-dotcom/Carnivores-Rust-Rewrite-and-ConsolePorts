// Particles and blood drops (`RenderCircle`): camera-facing discs fading
// from a centre colour to an edge colour.

struct PIn {
    @builtin(vertex_index) vid: u32,
    @location(0) center: vec3<f32>,
    @location(1) radius: f32,
    @location(2) c0: vec4<f32>,
    @location(3) c1: vec4<f32>,
};

struct POut {
    @builtin(position) clip: vec4<f32>,
    @location(0) corner: vec2<f32>,
    @location(1) c0: vec4<f32>,
    @location(2) c1: vec4<f32>,
    @location(3) dist: f32,
};

@vertex
fn vs(v: PIn) -> POut {
    var k = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));
    let c = k[v.vid];
    var o: POut;
    o.clip = g.view_proj * vec4<f32>(v.center, 1.0);
    // Offset in view space, i.e. scaled by the projection.
    o.clip.x += c.x * v.radius * g.proj[0][0];
    o.clip.y += c.y * v.radius * g.proj[1][1];
    o.corner = c;
    o.c0 = v.c0;
    o.c1 = v.c1;
    o.dist = distance(v.center, g.cam_pos.xyz);
    return o;
}

@fragment
fn fs(i: POut) -> @location(0) vec4<f32> {
    let d = length(i.corner);
    if (d > 1.0) {
        discard;
    }
    let c = mix(i.c0, i.c1, d);
    return vec4<f32>(fogged(texel(c.rgb), i.dist), c.a);
}
