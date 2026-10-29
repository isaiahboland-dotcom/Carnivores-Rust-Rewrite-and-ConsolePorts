#version 330
in vec4 aPosition;
in vec4 aTexLight;
in vec4 aFog;
uniform vec4 uEye;
uniform vec4 uAngles;
out vec2 vTex;
out float vLight;
out float vCutout;
out vec4 vFog;
out vec3 vView;
void main() {
    vec4 position = aPosition;
    vFog = aFog;
    if (aFog.a < -2.5) {
        vec3 view = vec3(position.x / 0.75, position.y / 1.333, position.w);
        vFog.a = clamp((length(view) - 11366.4) / (18944.0 - 11366.4), 0.0, 1.0);
    } else if (aFog.a < -1.5) {
        vec3 w = aPosition.xyz - uEye.xyz;
        float side = w.x * uAngles.y + w.z * uAngles.x;
        float forward = w.x * uAngles.x - w.z * uAngles.y;
        float up = w.y * uAngles.w + forward * uAngles.z;
        float depth = forward * uAngles.w - w.y * uAngles.z;
        position = vec4(side * 0.75, up * 1.333,
                        (32768.0 / (32768.0 - 16.0)) * (depth - 16.0), depth);
        vFog.a = clamp((length(vec3(side, up, depth)) - 15155.2) / (18944.0 - 15155.2), 0.0, 1.0);
    }
    gl_Position = position;
    vTex = aTexLight.xy;
    vLight = aTexLight.z;
    vCutout = aTexLight.w;
    vView = vec3(position.x / 0.75, position.y / 1.333, position.w) / 100000.0;
}
