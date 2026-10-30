#version 330
uniform sampler2D menuTexture;
in vec2 vTex;
out vec4 color;
void main() {
    // The menus are opaque; the hunt overlay blends by this alpha.
    color = texture(menuTexture, vTex);
}
