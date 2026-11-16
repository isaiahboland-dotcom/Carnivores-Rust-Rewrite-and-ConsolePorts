// Local fog volumes: the map's fog zones (`FogsMap`) and the `.RSC`'s fog
// list, Hunt2.cpp: CalcFogLevel (reference: c2_core::fog::Zones::level).
// Evaluated per vertex like the original's Direct3D vertex fog (the ground
// per map vertex, models per vertex where the original takes one point per
// model and a height gradient: CalcFogLevel_Gradient).

struct FogZones {
    // x: the camera's zone (0: the camera is in no fog, `CAMERAINFOG`),
    // y: zone fog on (the map has zones, the camera is not under water),
    // z: the sun's glow through the fog (PC option `modern_fog_glow`), 0 off,
    // w: 1 under water with the original's fog (PC option
    // `original_underwater_fog`): every point and the camera are in zone 0,
    // which then holds the water's fog (Hunt2.cpp: zone 127).
    frame: vec4<f32>,
    // Per zone: colour (0..1 RGB), the fog's top (world units).
    colour: array<vec4<f32>, 256>,
    // Per zone: x `Transp`, y `FLimit` (of 255), z 1 if the list has it.
    shape: array<vec4<f32>, 256>,
};

// `FogsMap`, four entries per word.
@group(0) @binding(1) var<storage, read> fog_cells: array<u32>;
@group(0) @binding(2) var<uniform> fog_zones: FogZones;

// The camera is under water with the original's fog on.
fn zone_fog_under_water() -> bool {
    return fog_zones.frame.w > 0.5;
}

fn fog_zone_at(x: f32, z: f32) -> u32 {
    if (x < 0.0 || z < 0.0) {
        return 0u;
    }
    let cx = u32(x) >> 9u;
    let cz = u32(z) >> 9u;
    if (cx >= 512u || cz >= 512u) {
        return 0u;
    }
    let i = cz * 512u + cx;
    if (i / 4u >= arrayLength(&fog_cells)) {
        return 0u;
    }
    let zone = (fog_cells[i / 4u] >> ((i % 4u) * 8u)) & 0xffu;
    return select(0u, zone, fog_zones.shape[zone].z > 0.5);
}

// The zone fog between the camera and `p`: how much of it (a, 0..1) and its
// colour (green only at night, Resources.cpp: `fogRGB &= 0x00FF00` with
// OptDayNight 2) times that, so it interpolates between vertices.
fn zone_fog(p: vec3<f32>) -> vec4<f32> {
    let under = fog_zones.frame.w > 0.5;
    if (fog_zones.frame.y < 0.5 && !under) {
        return vec4<f32>(0.0);
    }
    let camera = u32(fog_zones.frame.x);
    let camera_in = camera != 0u || under;
    var zone = 0u;
    var point_in = true;
    if (!under) {
        zone = fog_zone_at(p.x, p.z);
        if (zone == 0u) {
            if (camera == 0u) {
                return vec4<f32>(0.0);
            }
            zone = camera;
            point_in = false;
        }
    }
    let c = fog_zones.colour[zone];
    let s = fog_zones.shape[zone];
    let eye = g.cam_pos.xyz;
    var d = distance(p, eye);
    var fla = (c.w - p.y) / 64.0;
    if (!point_in) {
        fla = min(fla, 0.0);
    }
    var flb = (c.w - eye.y) / 64.0;
    if (!camera_in) {
        flb = min(flb, 0.0);
    }
    if (fla < 0.0 && flb < 0.0) {
        return vec4<f32>(0.0);
    }
    if (fla < 0.0) {
        d = d * flb / (flb - fla);
        fla = 0.0;
    }
    if (flb < 0.0) {
        d = d * fla / (fla - flb);
        flb = 0.0;
    }
    let t = max(s.x, 1.0);
    let fl = (fla + flb) * (d + t * 0.5) / t;
    // The water's colour is set each frame and stays as it is.
    let rgb = select(mix(c.rgb, vec3<f32>(0.0, c.g, 0.0), g.misc.x), c.rgb, under);
    let a = clamp(min(fl, s.y) / 255.0, 0.0, 1.0);
    return vec4<f32>(rgb * a, a);
}

// The original's Direct3D vertex fog: towards the zone's colour by the
// vertex's amount, before the distance fade.
fn zone_fogged(rgb: vec3<f32>, f: vec4<f32>) -> vec3<f32> {
    return rgb * (1.0 - f.a) + f.rgb;
}

// Sun through fog (PC option `modern_fog_glow`, not in the original): the
// modernized Modder's Edition's forward-scatter (terrain.frag), a warm glow
// in the zone fog `a` at `world` when looking towards the sun, added after
// the zone's colour, on the ground only, as there.
fn zone_glow(world: vec3<f32>, a: f32) -> vec3<f32> {
    let strength = fog_zones.frame.z;
    if (strength <= 0.0 || a <= 0.01) {
        return vec3<f32>(0.0);
    }
    let view = normalize(world - g.cam_pos.xyz);
    let scatter = pow(max(dot(view, g.sun.xyz), 0.0), 8.0);
    return vec3<f32>(1.0, 0.85, 0.5) * scatter * strength * a;
}
