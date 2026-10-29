#version 330
uniform sampler2D terrainAtlas;
in vec2 vTex;
in float vLight;
in float vCutout;
out vec4 color;
void main() {
    vec4 texel = texture(terrainAtlas, vTex);
    if (vCutout > 0.5 && texel.a < 0.5) discard;
    color = vec4(texel.rgb * vLight, 1.0);
}
