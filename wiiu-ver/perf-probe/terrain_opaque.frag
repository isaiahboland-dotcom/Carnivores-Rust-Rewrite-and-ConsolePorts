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
    // The atlas packs its 128-texel tiles edge to edge (16 across). A cell's
    // UVs sit ~0.95 texels inside its tile, but from mip level 1 on that is
    // less than the bilinear half-texel, so distant cell edges read the
    // neighbouring tile (from level 2, a visible grid over the ground). Keep the read half a texel
    // inside the tile at the level drawn: the nearest of the 5 levels to
    // the GL rule's log2(rho), read at that level itself so both agree.
    vec2 texels = vTex * 2048.0;
    float rho = max(length(dFdx(texels)), length(dFdy(texels)));
    float level = clamp(floor(log2(max(rho, 1.0)) + 0.5), 0.0, 4.0);
    float half_texel = exp2(level) * 0.5;
    vec2 tile = floor(vTex * 16.0) * 128.0;
    vec2 uv = clamp(texels, tile + half_texel, tile + (128.0 - half_texel)) / 2048.0;
    vec4 texel = textureLod(terrainAtlas, uv, level);
    float fog = clamp(vFog.a, 0.0, 1.0);
    if (vFog.a < -0.5) {
        float d = max(length(vView) - 1.002, 0.0);
        fog = d / (0.4024 + d);
    }
    color = vec4(mix(texel.rgb * vLight, vFog.rgb, fog), 1.0);
}
