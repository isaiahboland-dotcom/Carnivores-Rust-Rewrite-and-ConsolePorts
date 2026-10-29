#version 330
in vec4 aPosition;
in vec4 aTexLight;
out vec2 vTex;
out float vLight;
out float vCutout;
void main() {
    gl_Position = aPosition;
    vTex = aTexLight.xy;
    vLight = aTexLight.z;
    vCutout = aTexLight.w;
}
