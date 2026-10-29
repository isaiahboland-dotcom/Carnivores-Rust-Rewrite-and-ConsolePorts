#version 330
uniform sampler2D terrainAtlas;
in vec2 vTex;
in float vLight;
out vec4 color;
void main() {
    color = vec4(texture(terrainAtlas, vTex).rgb * vLight, 1.0);
}
