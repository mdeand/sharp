struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    // Per-instance attributes
    @location(2) instance_position: vec3<f32>,
    @location(3) instance_scale: f32,
    @location(4) instance_half_height: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
};

const LIGHT_DIR: vec3<f32> = vec3<f32>(0.4, 0.7, 0.5);
const AMBIENT: f32 = 0.15;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Capsule / pill shape: offset the top and bottom hemispheres of the
    // unit sphere along Y by half_height.  When half_height == 0 this is
    // just a sphere.
    let y_offset = sign(model.position.y) * model.instance_half_height;
    let local_pos = vec3<f32>(
        model.position.x * model.instance_scale,
        model.position.y * model.instance_scale + y_offset,
        model.position.z * model.instance_scale,
    );
    let world_pos = local_pos + model.instance_position;

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color = model.color;
    out.world_normal = model.position; // unit-sphere normal (good enough for lighting)
    out.world_pos = world_pos;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    let L = normalize(LIGHT_DIR);

    // Diffuse (Lambert)
    let diffuse = max(dot(N, L), 0.0);

    // Simple specular (Blinn-Phong)
    // Approximate view direction as looking down -Z; good enough without passing eye pos
    let V = normalize(-in.world_pos);
    let H = normalize(L + V);
    let specular = pow(max(dot(N, H), 0.0), 32.0) * 0.5;

    let lit = in.color * (AMBIENT + diffuse) + vec3<f32>(specular);
    return vec4<f32>(lit, 1.0);
}