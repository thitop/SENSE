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
    /// Distance from the camera along the view direction (clip-space w).
    @location(3) view_depth: f32,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_position = model.position;
    out.world_normal = model.normal;
    out.color = model.color;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.view_depth = out.clip_position.w;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Light from above and behind the player, so the side of a target facing you is lit.
    let light_dir = normalize(vec3<f32>(0.25, 0.8, 0.55));
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

// ---------- Room ----------
// Faint world-space grid lines give a sense of depth and make camera turns visible,
// while staying low-contrast so the yellow targets stay the brightest thing on screen.

const FOG_COLOR: vec3<f32> = vec3<f32>(0.035, 0.036, 0.045);
const MINOR_LINE: vec3<f32> = vec3<f32>(0.075, 0.078, 0.095);
const MAJOR_LINE: vec3<f32> = vec3<f32>(0.11, 0.113, 0.135);
/// Overall surface brightness: about the old background, so the yellow targets keep their pop.
const SURFACE_GAIN: f32 = 0.55;

/// Coverage (0..1) of grid lines every `spacing` metres, about `width_px` pixels wide,
/// anti-aliased in screen space. `shift` moves the lines by a fraction of a cell. Lines fade
/// out where cells shrink below a few pixels, so distant surfaces don't shimmer.
fn grid(coord: vec2<f32>, spacing: f32, shift: vec2<f32>, width_px: f32) -> f32 {
    let c = coord / spacing + shift;
    let fw = max(fwidth(c), vec2<f32>(1e-5));
    let dist_px = abs(fract(c - 0.5) - 0.5) / fw;
    let line = 1.0 - clamp(min(dist_px.x, dist_px.y) - (width_px * 0.5 - 0.5), 0.0, 1.0);
    let cell_px = 1.0 / max(fw.x, fw.y);
    return line * smoothstep(3.0, 10.0, cell_px);
}

@fragment
fn fs_room(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let p = in.world_position;
    let an = abs(n);

    // The two in-plane axes of whichever surface this is.
    let on_floor_or_ceiling = an.y > 0.5;
    let on_x_wall = an.x > 0.5;
    let uv = select(select(p.xy, p.zy, on_x_wall), p.xz, on_floor_or_ceiling);

    // Floor darkest, ceiling a little lighter, walls lightest; walls darken toward the floor.
    let floor_factor = select(1.0, 0.55, n.y > 0.5);
    let ceiling_factor = select(1.0, 0.75, n.y < -0.5);
    let wall_base = select(mix(0.8, 1.0, smoothstep(0.0, 3.0, p.y)), 1.0, on_floor_or_ceiling);
    var rgb = in.color.rgb * floor_factor * ceiling_factor * wall_base * SURFACE_GAIN;

    // Shift the horizontal axes half a cell so no line sits right behind the crosshair at the
    // start (x = 0 ahead, z = 0 on the floor). Wall heights stay on whole metres, away from
    // the 1.5 m eye height.
    let shift = select(vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 0.5), on_floor_or_ceiling);
    let minor = grid(uv, 1.0, shift, 1.0);
    let major = grid(uv, 5.0, shift, 1.4); // at ±2.5 m, ±7.5 m: on top of minor lines
    rgb = mix(rgb, MINOR_LINE, minor * 0.45);
    rgb = mix(rgb, MAJOR_LINE, major * 0.6);

    // Far surfaces sink slightly into the background for depth.
    let fog = smoothstep(8.0, 26.0, in.view_depth) * 0.45;
    rgb = mix(rgb, FOG_COLOR, fog);

    return vec4<f32>(rgb, 1.0);
}
