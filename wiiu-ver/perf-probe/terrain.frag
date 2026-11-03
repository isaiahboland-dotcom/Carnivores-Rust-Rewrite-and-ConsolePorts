#version 330
uniform sampler2D terrainAtlas;
in vec2 vTex;
in float vLight;
in float vCutout;
in vec4 vFog;
in vec3 vView;
in vec3 vDaylight;
in float vTorch;
out vec4 color;
void main() {
    vec4 texel = texture(terrainAtlas, vTex);
    if (vCutout > 0.5 && texel.a < 0.5) discard;
    vec3 base = texel.rgb;
    vec3 dawn = min(base * vec3(282.0, 266.0, 210.0) / 256.0, vec3(1.0));
    vec3 night = vec3(base.g / 8.0, base.g, base.g / 8.0);
    texel.rgb = mix(mix(base, dawn, vDaylight.x), night, vDaylight.y);
    float dist = length(vView) * 100000.0;
    float cone = smoothstep(0.88, 0.98, vView.z / max(length(vView), 0.00001));
    float beam = vTorch * cone * (1.0 - smoothstep(1024.0, 5120.0, dist));
    float illumination = max(vLight * vDaylight.z, beam);
    float fog = clamp(vFog.a, 0.0, 1.0);
    if (vFog.a < -0.5) {
        float d = max(length(vView) - 1.002, 0.0);
        fog = d / (0.4024 + d);
    }
    color = vec4(mix(texel.rgb * illumination, vFog.rgb, fog), 1.0);
}
