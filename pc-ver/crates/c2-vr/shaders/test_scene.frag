#version 450
// The VR test scene (no game data): sky, a 1 m floor grid, eight boxes in a
// ring 4 m out (the gold one straight ahead, -z), and a ball per tracked
// controller. Each pixel traces its ray from the eye. Layout matches
// `xr::Push` in src/xr.rs.
layout(push_constant) uniform Push {
    vec4 pos;     // eye position (m, tracking space)
    vec4 rot;     // eye orientation, quaternion x y z w
    vec4 tan;     // tangents of the view's edges: left, right, up, down
    vec4 size;    // image width, height; floor height; unused
    vec4 hand[2]; // left, right: position, w > 0 if tracked
} p;

layout(location = 0) out vec4 color;

vec3 qrot(vec4 q, vec3 v) {
    vec3 t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

float box(vec3 ro, vec3 rd, vec3 c, vec3 h, out vec3 n) {
    vec3 m = 1.0 / rd;
    vec3 o = ro - c;
    vec3 k = abs(m) * h;
    vec3 t1 = -m * o - k;
    vec3 t2 = -m * o + k;
    float tn = max(max(t1.x, t1.y), t1.z);
    float tf = min(min(t2.x, t2.y), t2.z);
    n = -sign(rd) * step(t1.yzx, t1.xyz) * step(t1.zxy, t1.xyz);
    return (tn > tf || tn < 0.0) ? -1.0 : tn;
}

float sphere(vec3 ro, vec3 rd, vec3 c, float r) {
    vec3 oc = ro - c;
    float b = dot(oc, rd);
    float h = b * b - dot(oc, oc) + r * r;
    return h < 0.0 ? -1.0 : -b - sqrt(h);
}

void main() {
    vec2 uv = gl_FragCoord.xy / p.size.xy;
    vec3 d = normalize(vec3(mix(p.tan.x, p.tan.y, uv.x), mix(p.tan.z, p.tan.w, uv.y), -1.0));
    vec3 rd = qrot(p.rot, d);
    vec3 ro = p.pos.xyz;
    float floor_y = p.size.z;
    vec3 sun = normalize(vec3(0.4, 0.8, 0.3));

    vec3 sky = mix(vec3(0.75, 0.80, 0.68), vec3(0.28, 0.42, 0.62), clamp(rd.y * 2.0, 0.0, 1.0));
    vec3 col = sky;
    float best = 1e9;

    if (rd.y < 0.0) {
        float t = (floor_y - ro.y) / rd.y;
        if (t > 0.0) {
            best = t;
            vec3 h = ro + rd * t;
            vec2 g = abs(fract(h.xz) - 0.5);
            float line = step(0.48, max(g.x, g.y)) * exp(-t * 0.08);
            vec3 ground = mix(vec3(0.18, 0.27, 0.14), vec3(0.62, 0.82, 0.50), line);
            col = mix(ground, sky, 1.0 - exp(-t * 0.04));
        }
    }
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.7853982;
        vec3 c = vec3(sin(a) * 4.0, floor_y + 0.5, -cos(a) * 4.0);
        vec3 n;
        float t = box(ro, rd, c, vec3(0.5), n);
        if (t > 0.0 && t < best) {
            best = t;
            vec3 base = i == 0 ? vec3(0.92, 0.76, 0.26) : vec3(0.36, 0.55, 0.30);
            col = base * (0.45 + 0.55 * max(dot(n, sun), 0.0));
        }
    }
    for (int i = 0; i < 2; i++) {
        if (p.hand[i].w > 0.0) {
            float t = sphere(ro, rd, p.hand[i].xyz, 0.05);
            if (t > 0.0 && t < best) {
                best = t;
                vec3 n = normalize(ro + rd * t - p.hand[i].xyz);
                vec3 base = i == 0 ? vec3(0.30, 0.60, 1.00) : vec3(1.00, 0.45, 0.30);
                col = base * (0.4 + 0.6 * max(dot(n, sun), 0.0));
            }
        }
    }
    color = vec4(col, 1.0);
}
