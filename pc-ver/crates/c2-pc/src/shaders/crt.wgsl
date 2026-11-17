// The DHC OS terminal's CRT layers over the menu: scanlines, a vignette and
// noise, each on its own bit of `layers`. A full-screen triangle that
// darkens what is under it (alpha blending, black).

struct Crt {
    // The window in pixels.
    size: vec2<f32>,
    // Window pixels per canvas unit (the scanlines follow the canvas).
    scale: f32,
    // Seconds, for the noise.
    time: f32,
    // 1 scanlines, 2 vignette, 4 noise.
    layers: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> crt: Crt;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    var a = 0.0;
    if (crt.layers & 1u) != 0u {
        // One dark canvas row in three, as the sprite rows were.
        let period = 3.0 * max(crt.scale, 0.001);
        if fract(pos.y / period) < 1.0 / 3.0 {
            a = a + 0.22;
        }
    }
    if (crt.layers & 2u) != 0u {
        let uv = pos.xy / max(crt.size, vec2<f32>(1.0));
        let v = uv * (1.0 - uv);
        let k = pow(clamp(v.x * v.y * 16.0, 0.0, 1.0), 0.3);
        a = a + (1.0 - k) * 0.55;
    }
    if (crt.layers & 4u) != 0u {
        // Fresh at 24 frames a second, in two-pixel grains.
        let frame = floor(crt.time * 24.0);
        let n = hash(floor(pos.xy / 2.0) + vec2<f32>(frame * 13.0, frame * 7.0));
        a = a + n * 0.07;
    }
    return vec4<f32>(0.0, 0.0, 0.0, clamp(a, 0.0, 0.85));
}
