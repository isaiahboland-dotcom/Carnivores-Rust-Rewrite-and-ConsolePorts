#version 330
uniform sampler2D menuTexture;
in vec2 vTex;
out vec4 color;
void main() {
    color = vec4(texture(menuTexture, vTex).rgb, 1.0);
}
