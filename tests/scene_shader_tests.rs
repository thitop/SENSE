//! The scene shader compiles and has the entry points the renderer binds.
//! A WGSL error would otherwise only show up as a crash when the app starts.

const SCENE_WGSL: &str = include_str!("../src/shaders/scene.wgsl");

fn validated_entry_points() -> Vec<String> {
    let module = naga::front::wgsl::parse_str(SCENE_WGSL).expect("scene.wgsl parses");
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
        .validate(&module)
        .expect("scene.wgsl validates");
    module.entry_points.iter().map(|e| e.name.clone()).collect()
}

#[test]
fn scene_shader_has_target_entry_points() {
    let names = validated_entry_points();
    assert!(names.contains(&"vs_main".to_string()));
    assert!(names.contains(&"fs_main".to_string()));
}

#[test]
fn scene_shader_has_room_entry_point() {
    assert!(validated_entry_points().contains(&"fs_room".to_string()));
}

/// With `FrontFace::Ccw` and back-face culling, a triangle is drawn only when its
/// counter-clockwise winding faces the same way as its normal (toward the viewer).
fn front_faces_match_normals(mesh: &aim_trainer::engine::geometry::Mesh) -> bool {
    use glam::Vec3;
    mesh.indices.chunks(3).all(|t| {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].position));
        let winding = (b - a).cross(c - a);
        let normal = Vec3::from(mesh.vertices[t[0] as usize].normal);
        winding.dot(normal) > 0.0
    })
}

#[test]
fn room_surfaces_face_the_player_inside() {
    let room = aim_trainer::engine::geometry::create_room(30.0, 10.0, 30.0, [0.1; 4], [0.05; 4]);
    assert!(front_faces_match_normals(&room), "room triangles would be culled from inside");
}

#[test]
fn target_spheres_face_outward() {
    let sphere = aim_trainer::engine::geometry::create_sphere(1.0, 24, 16, [1.0; 4]);
    assert!(front_faces_match_normals(&sphere));
}
