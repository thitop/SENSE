use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 3]>() * 2) as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Generate a UV sphere for targets (default color: warm vibrant target yellow)
pub fn create_sphere(radius: f32, sectors: u32, stacks: u32, color: [f32; 4]) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let pi = std::f32::consts::PI;
    let two_pi = std::f32::consts::TAU;

    for i in 0..=stacks {
        let v = i as f32 / stacks as f32;
        let phi = v * pi;

        for j in 0..=sectors {
            let u = j as f32 / sectors as f32;
            let theta = u * two_pi;

            let x = phi.sin() * theta.cos();
            let y = phi.cos();
            let z = phi.sin() * theta.sin();

            let normal = Vec3::new(x, y, z).normalize();
            let position = normal * radius;

            vertices.push(Vertex {
                position: position.to_array(),
                normal: normal.to_array(),
                color,
            });
        }
    }

    for i in 0..stacks {
        let k1 = i * (sectors + 1);
        let k2 = k1 + sectors + 1;

        for j in 0..sectors {
            if i != 0 {
                indices.push(k1 + j);
                indices.push(k2 + j);
                indices.push(k1 + j + 1);
            }

            if i != (stacks - 1) {
                indices.push(k1 + j + 1);
                indices.push(k2 + j);
                indices.push(k2 + j + 1);
            }
        }
    }

    Mesh { vertices, indices }
}

/// Generate a simple cube/room environment (inward-facing normals for an aim room)
pub fn create_room(width: f32, height: f32, depth: f32, wall_color: [f32; 4], floor_color: [f32; 4]) -> Mesh {
    let hw = width * 0.5;
    let hh = height;
    let hd = depth * 0.5;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let mut add_quad = |p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], p4: [f32; 3], norm: [f32; 3], col: [f32; 4]| {
        let base_idx = vertices.len() as u32;
        vertices.push(Vertex { position: p1, normal: norm, color: col });
        vertices.push(Vertex { position: p2, normal: norm, color: col });
        vertices.push(Vertex { position: p3, normal: norm, color: col });
        vertices.push(Vertex { position: p4, normal: norm, color: col });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    };

    // Floor (y = 0) facing up
    add_quad(
        [-hw, 0.0, -hd],
        [ hw, 0.0, -hd],
        [ hw, 0.0,  hd],
        [-hw, 0.0,  hd],
        [0.0, 1.0, 0.0],
        floor_color,
    );

    // Ceiling (y = hh) facing down
    add_quad(
        [-hw, hh,  hd],
        [ hw, hh,  hd],
        [ hw, hh, -hd],
        [-hw, hh, -hd],
        [0.0, -1.0, 0.0],
        wall_color,
    );

    // Front Wall (z = -hd) facing player (+Z)
    add_quad(
        [-hw, hh, -hd],
        [ hw, hh, -hd],
        [ hw, 0.0, -hd],
        [-hw, 0.0, -hd],
        [0.0, 0.0, 1.0],
        wall_color,
    );

    // Back Wall (z = hd) facing player (-Z)
    add_quad(
        [ hw, hh,  hd],
        [-hw, hh,  hd],
        [-hw, 0.0,  hd],
        [ hw, 0.0,  hd],
        [0.0, 0.0, -1.0],
        wall_color,
    );

    // Left Wall (x = -hw) facing player (+X)
    add_quad(
        [-hw, hh,  hd],
        [-hw, hh, -hd],
        [-hw, 0.0, -hd],
        [-hw, 0.0,  hd],
        [1.0, 0.0, 0.0],
        wall_color,
    );

    // Right Wall (x = hw) facing player (-X)
    add_quad(
        [hw, hh, -hd],
        [hw, hh,  hd],
        [hw, 0.0,  hd],
        [hw, 0.0, -hd],
        [-1.0, 0.0, 0.0],
        wall_color,
    );

    Mesh { vertices, indices }
}

