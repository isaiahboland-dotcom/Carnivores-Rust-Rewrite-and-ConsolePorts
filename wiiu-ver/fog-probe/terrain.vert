#version 330
in vec4 aPosition;
in vec4 aTexLight;
in vec4 aFog;
out vec2 vTex;
out float vLight;
out float vCutout;
out vec4 vFog;
out vec3 vView;
void main() {
    gl_Position = aPosition;
    vTex = aTexLight.xy;
    vLight = aTexLight.z;
    vCutout = aTexLight.w;
    vFog = aFog;
    vView = vec3(aPosition.x / 0.75, aPosition.y / 1.333, aPosition.w) / 100000.0;
}
