#version 330
in vec4 aPosition;
in vec4 aTexLight;
out vec2 vTex;
out float vLight;
void main() {
    gl_Position = aPosition;
    vTex = aTexLight.xy;
    vLight = aTexLight.z;
}
