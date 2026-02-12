//! Rect render pipeline — instanced rendering of rounded rectangles.
//!
//! One draw call renders all rectangles in the scene.

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    Buffer, BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState,
    ColorWrites, Device, FragmentState, FrontFace, IndexFormat, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, Queue, RenderPass, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderStages,
    TextureFormat, VertexState,
};

use crate::vertex::{CameraUniform, QuadVertex, RectInstance};

/// Maximum instances per draw call (64K × 48B = 3 MB of GPU memory).
const MAX_INSTANCES: usize = 65_536;

/// Owns the wgpu pipeline, buffers, and bind groups for rect rendering.
pub struct RectPipeline {
    pipeline: RenderPipeline,

    // Geometry
    vertex_buffer: Buffer,
    index_buffer: Buffer,

    // Instancing
    instance_buffer: Buffer,
    instance_count: u32,

    // Camera
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
}

impl RectPipeline {
    /// Create the pipeline and allocate GPU buffers.
    pub fn new(device: &Device, surface_format: TextureFormat) -> Self {
        // ── Shader ──────────────────────────────────────────────
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/rect.wgsl").into(),
            ),
        });

        // ── Camera bind group layout ────────────────────────────
        let camera_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("camera_bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // ── Pipeline layout ─────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("rect_pipeline_layout"),
            bind_group_layouts: &[&camera_bgl],
            push_constant_ranges: &[],
        });

        // ── Render pipeline ─────────────────────────────────────
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[QuadVertex::layout(), RectInstance::layout()],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: surface_format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None, // 2D — no backface culling
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Vertex buffer (unit quad, static) ───────────────────
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("quad_vb"),
            size: std::mem::size_of::<[QuadVertex; 4]>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Index buffer (static) ───────────────────────────────
        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("quad_ib"),
            size: std::mem::size_of::<[u16; 6]>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Instance buffer (dynamic, resized as needed) ────────
        let instance_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("rect_instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<RectInstance>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Camera uniform buffer ───────────────────────────────
        let camera_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("camera_ub"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = Self::create_camera_bind_group(
            device, &camera_bgl, &camera_buffer,
        );

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_count: 0,
            camera_buffer,
            camera_bind_group,
        }
    }

    fn create_camera_bind_group(
        device: &Device,
        layout: &BindGroupLayout,
        buffer: &Buffer,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("camera_bg"),
            layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    // ───────────────────── Upload ─────────────────────────────────

    /// Upload the static quad geometry.  Call once after creation.
    pub fn upload_quad(&self, queue: &Queue) {
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&QuadVertex::VERTICES),
        );
        queue.write_buffer(
            &self.index_buffer,
            0,
            bytemuck::cast_slice(&QuadVertex::INDICES),
        );
    }

    /// Upload instance data for this frame.
    ///
    /// Returns the number of instances that will be drawn.
    pub fn upload_instances(&mut self, queue: &Queue, instances: &[RectInstance]) -> u32 {
        let count = instances.len().min(MAX_INSTANCES);
        if count == 0 {
            self.instance_count = 0;
            return 0;
        }

        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count]),
        );
        self.instance_count = count as u32;
        self.instance_count
    }

    /// Upload the camera uniform for this frame.
    pub fn upload_camera(&self, queue: &Queue, camera: &CameraUniform) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(camera));
    }

    // ───────────────────── Draw ───────────────────────────────────

    /// Record draw commands into the given render pass.
    ///
    /// **One draw call** for all instances.
    pub fn draw<'a>(&'a self, pass: &mut RenderPass<'a>) {
        if self.instance_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..self.instance_count);
    }

    /// Number of instances that will be drawn.
    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }
}
