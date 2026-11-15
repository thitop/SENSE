// Shaders for Aim Trainer Scene & Targets (Enhanced with subtle target edge glow)

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = model.position;
    out.world_normal = model.normal;
    out.color = model.color;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.2, 0.9, -0.4));
    let normal = normalize(in.world_normal);

    let diffuse = max(dot(normal, light_dir), 0.0);
    let ambient = 0.40;
    let lighting = ambient + diffuse * 0.60;

    // Subtle edge rim-lighting/glow effect for targets
    let view_dir = normalize(-in.world_position);
    let rim = 1.0 - max(dot(view_dir, normal), 0.0);
    let rim_glow = pow(rim, 2.8) * 0.35;

    let base_rgb = in.color.rgb * lighting;
    let final_rgb = base_rgb + vec3<f32>(rim_glow, rim_glow * 0.9, 0.0);

    return vec4<f32>(final_rgb, in.color.a);
}
