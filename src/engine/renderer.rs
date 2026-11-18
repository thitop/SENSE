use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use std::sync::Arc;
use winit::window::Window;

use super::camera::Camera;
use super::geometry::{create_room, create_sphere, Mesh, Vertex};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CrosshairVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl CrosshairVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CrosshairVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Renderer {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,

    // Depth buffer
    pub depth_view: wgpu::TextureView,

    // 3D Scene pipeline
    pub scene_pipeline: wgpu::RenderPipeline,
    pub room_pipeline: wgpu::RenderPipeline,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,

    // Scene Meshes
    pub room_vertex_buffer: wgpu::Buffer,
    pub room_index_buffer: wgpu::Buffer,
    pub room_index_count: u32,

    pub sphere_mesh: Mesh,
    pub dynamic_vertex_buffer: wgpu::Buffer,
    pub dynamic_index_buffer: wgpu::Buffer,

    // Crosshair 2D pipeline
    pub crosshair_pipeline: wgpu::RenderPipeline,
    pub crosshair_vertex_buffer: wgpu::Buffer,

    // Egui renderer integration
    pub egui_renderer: egui_wgpu::Renderer,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create wgpu surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to get GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Primary GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .expect("Failed to create logical device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let width = size.width.max(1);
        let height = size.height.max(1);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let depth_view = Self::create_depth_texture(&device, &config);

        // Camera Uniform Buffer & Bind Group
        let camera_uniform = CameraUniform {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        };

        use wgpu::util::DeviceExt;
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Scene Pipeline (3D Room & Targets)
        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
        });

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Scene Pipeline Layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Room and targets share vertices and camera; only the fragment shading differs.
        let make_scene_pipeline = |label: &str, fragment_entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&scene_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &scene_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &scene_shader,
                    entry_point: Some(fragment_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };
        let scene_pipeline = make_scene_pipeline("Scene Render Pipeline", "fs_main");
        let room_pipeline = make_scene_pipeline("Room Render Pipeline", "fs_room");

        // Crosshair Pipeline (2D HUD)
        let crosshair_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crosshair Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/crosshair.wgsl").into()),
        });

        let crosshair_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Crosshair Pipeline Layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let crosshair_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Crosshair Render Pipeline"),
            layout: Some(&crosshair_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &crosshair_shader,
                entry_point: Some("vs_main"),
                buffers: &[CrosshairVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &crosshair_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let wall_col = [0.08, 0.08, 0.10, 1.0];
        let floor_col = [0.04, 0.04, 0.05, 1.0];
        let room_mesh = create_room(30.0, 10.0, 30.0, wall_col, floor_col);

        let room_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Room Vertex Buffer"),
            contents: bytemuck::cast_slice(&room_mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let room_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Room Index Buffer"),
            contents: bytemuck::cast_slice(&room_mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let room_index_count = room_mesh.indices.len() as u32;

        let sphere_mesh = create_sphere(1.0, 24, 16, [1.0, 0.85, 0.0, 1.0]);

        let dynamic_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic Target Vertex Buffer"),
            size: (std::mem::size_of::<Vertex>() * 20000) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dynamic_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic Target Index Buffer"),
            size: (std::mem::size_of::<u32>() * 60000) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let ch_color = [0.0, 1.0, 0.2, 0.95];
        let crosshair_verts = Self::build_crosshair_verts(width, height, 8.0, 3.0, ch_color);
        let crosshair_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crosshair Vertex Buffer"),
            contents: bytemuck::cast_slice(&crosshair_verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        // Initialize egui_wgpu renderer
        let egui_renderer = egui_wgpu::Renderer::new(&device, config.format, None, 1, false);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            depth_view,
            scene_pipeline,
            room_pipeline,
            camera_buffer,
            camera_bind_group,
            room_vertex_buffer,
            room_index_buffer,
            room_index_count,
            sphere_mesh,
            dynamic_vertex_buffer,
            dynamic_index_buffer,
            crosshair_pipeline,
            crosshair_vertex_buffer,
            egui_renderer,
        }
    }

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// Set the surface up again at the window's current size, after it reported `Outdated`
    /// or `Lost`. The stored size may be stale, which is how the surface got outdated.
    pub fn reconfigure(&mut self) {
        let size = self.window.inner_size();
        self.resize(size);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = Self::create_depth_texture(&self.device, &self.config);

            let ch_color = [0.0, 1.0, 0.2, 0.95];
            let ch_verts = Self::build_crosshair_verts(new_size.width, new_size.height, 8.0, 3.0, ch_color);
            self.queue.write_buffer(&self.crosshair_vertex_buffer, 0, bytemuck::cast_slice(&ch_verts));
        }
    }

    fn build_crosshair_verts(width: u32, height: u32, len_px: f32, gap_px: f32, col: [f32; 4]) -> [CrosshairVertex; 8] {
        let w = width.max(1) as f32;
        let h = height.max(1) as f32;

        let ndc_dx = (len_px / w) * 2.0;
        let ndc_gap_x = (gap_px / w) * 2.0;

        let ndc_dy = (len_px / h) * 2.0;
        let ndc_gap_y = (gap_px / h) * 2.0;

        [
            CrosshairVertex { position: [-ndc_gap_x - ndc_dx, 0.0], color: col },
            CrosshairVertex { position: [-ndc_gap_x, 0.0], color: col },
            CrosshairVertex { position: [ndc_gap_x, 0.0], color: col },
            CrosshairVertex { position: [ndc_gap_x + ndc_dx, 0.0], color: col },
            CrosshairVertex { position: [0.0, ndc_gap_y], color: col },
            CrosshairVertex { position: [0.0, ndc_gap_y + ndc_dy], color: col },
            CrosshairVertex { position: [0.0, -ndc_gap_y - ndc_dy], color: col },
            CrosshairVertex { position: [0.0, -ndc_gap_y], color: col },
        ]
    }

    pub fn update_camera(&mut self, camera: &Camera) {
        let uniform = CameraUniform {
            view_proj: camera.view_proj_matrix().to_cols_array_2d(),
        };
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[uniform]),
        );
    }

    /// Render 3D Scene + 2D Crosshair + egui UI overlay
    pub fn render(
        &mut self,
        target_positions: &[(glam::Vec3, f32)],
        on_target: bool,
        egui_primitives: &[egui::ClippedPrimitive],
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
    ) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Primary Frame Command Encoder"),
            });

        // 1. Prepare target geometry batch
        let mut target_verts: Vec<Vertex> = Vec::new();
        let mut target_indices: Vec<u32> = Vec::new();

        // A tracking target turns green while the crosshair is on it: that is when it scores.
        let target_color = if on_target { [0.25, 0.95, 0.45, 1.0] } else { [1.0, 0.85, 0.05, 1.0] };
        for &(pos, radius) in target_positions {
            let base_idx = target_verts.len() as u32;
            for v in &self.sphere_mesh.vertices {
                let world_pos = glam::Vec3::from_array(v.position) * radius + pos;
                target_verts.push(Vertex {
                    position: world_pos.to_array(),
                    normal: v.normal,
                    color: target_color,
                });
            }
            for idx in &self.sphere_mesh.indices {
                target_indices.push(base_idx + idx);
            }
        }

        if !target_verts.is_empty() {
            self.queue.write_buffer(
                &self.dynamic_vertex_buffer,
                0,
                bytemuck::cast_slice(&target_verts),
            );
            self.queue.write_buffer(
                &self.dynamic_index_buffer,
                0,
                bytemuck::cast_slice(&target_indices),
            );
        }

        // 2. 3D Scene Render Pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene 3D Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.04,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);

            render_pass.set_pipeline(&self.room_pipeline);
            render_pass.set_vertex_buffer(0, self.room_vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.room_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            render_pass.draw_indexed(0..self.room_index_count, 0, 0..1);

            if !target_indices.is_empty() {
                render_pass.set_pipeline(&self.scene_pipeline);
                let v_end = (target_verts.len() * std::mem::size_of::<Vertex>()) as u64;
                let i_end = (target_indices.len() * std::mem::size_of::<u32>()) as u64;

                render_pass.set_vertex_buffer(0, self.dynamic_vertex_buffer.slice(..v_end));
                render_pass.set_index_buffer(self.dynamic_index_buffer.slice(..i_end), wgpu::IndexFormat::Uint32);
                render_pass.draw_indexed(0..target_indices.len() as u32, 0, 0..1);
            }
        }

        // 3. 2D Crosshair Pass
        {
            let mut crosshair_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Crosshair 2D Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            crosshair_pass.set_pipeline(&self.crosshair_pipeline);
            crosshair_pass.set_vertex_buffer(0, self.crosshair_vertex_buffer.slice(..));
            crosshair_pass.draw(0..8, 0..1);
        }

        // 4. Egui UI Overlay Pass
        if !egui_primitives.is_empty() {
            self.egui_renderer.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                egui_primitives,
                screen_descriptor,
            );

            {
                let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Egui Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                self.egui_renderer
                    .render(&mut egui_pass.forget_lifetime(), egui_primitives, screen_descriptor);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

/// What the render loop does when the surface can't hand out a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAction {
    /// Set the surface up again at the window's size (`Renderer::reconfigure`).
    Reconfigure,
    /// Try again next frame.
    SkipFrame,
    /// Nothing can be drawn any more.
    Exit,
    /// Unexpected: log it.
    Report,
}

/// `Outdated` comes with every resize, minimise or alt-tab out of fullscreen until the
/// surface is configured again, so it's routine, not an error.
pub fn on_surface_error(e: &wgpu::SurfaceError) -> SurfaceAction {
    match e {
        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => SurfaceAction::Reconfigure,
        wgpu::SurfaceError::Timeout => SurfaceAction::SkipFrame,
        wgpu::SurfaceError::OutOfMemory => SurfaceAction::Exit,
        _ => SurfaceAction::Report,
    }
}

/// A minimised window has no area; the surface can't be configured or drawn to.
pub fn can_draw(size: winit::dpi::PhysicalSize<u32>) -> bool {
    size.width > 0 && size.height > 0
}
