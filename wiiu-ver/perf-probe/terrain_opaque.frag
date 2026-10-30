#version 330
// terrain.frag without the cut-out discard, for terrain and water (always
// opaque): a shader that never discards lets the GPU test depth before it
// shades, so hidden ground costs no texture reads.
uniform sampler2D terrainAtlas;
in vec2 vTex;
in float vLight;
in float vCutout;
in vec4 vFog;
in vec3 vView;
out vec4 color;
void main() {
    vec4 texel = texture(terrainAtlas, vTex);
    float fog = clamp(vFog.a, 0.0, 1.0);
    if (vFog.a < -0.5) {
        float d = max(length(vView) - 1.002, 0.0);
        fog = d / (0.4024 + d);
    }
    color = vec4(mix(texel.rgb * vLight, vFog.rgb, fog), 1.0);
}
