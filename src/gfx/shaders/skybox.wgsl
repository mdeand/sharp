struct SkyboxUniform {
    inv_view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> skybox: SkyboxUniform;

@group(1) @binding(0) var t_skybox: texture_cube<f32>;
@group(1) @binding(1) var s_skybox: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_dir: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Fullscreen triangle covering the entire screen
    let uv = vec2<f32>(
        f32((vertex_index << 1u) & 2u),
        f32(vertex_index & 2u),
    );
    let ndc = uv * 2.0 - 1.0;

    var out: VertexOutput;
    // z=0 keeps the triangle inside the [0,1] depth range after w-divide
    out.clip_position = vec4<f32>(ndc, 0.0, 1.0);

    // Unproject two depths through the rotation-only inverse VP to get a ray direction
    let near = skybox.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let far  = skybox.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    out.world_dir = (far.xyz / far.w) - (near.xyz / near.w);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_dir);
    return textureSample(t_skybox, s_skybox, dir);
}
