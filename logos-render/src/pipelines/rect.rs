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
use bytemuck::Pod;

use crate::vertex::{CameraUniform, QuadVertex, RectInstance};

/// Maximum instances per draw call (64K × 48B = 3 MB of GPU memory).
const MAX_INSTANCES: usize = 65_536;

/// Draw-indirect parameters matching `wgpu::DrawIndexedIndirect`.
///
/// 20 bytes — stored in a GPU buffer for `draw_indexed_indirect()`.
/// By keeping this on the GPU, we eliminate CPU-side draw parameter
/// setup on frames where only instance data changes.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedIndirectArgs {
    /// Number of indices (6 for a quad).
    pub index_count: u32,
    /// Number of instances to draw.
    pub instance_count: u32,
    /// First index (always 0).
    pub first_index: u32,
    /// Vertex offset (always 0).
    pub base_vertex: i32,
    /// First instance (always 0).
    pub first_instance: u32,
}

impl DrawIndexedIndirectArgs {
    /// Create args for the unit quad with `n` instances.
    pub fn new(instance_count: u32) -> Self {
        Self {
            index_count: 6,
            instance_count,
            first_index: 0,
            base_vertex: 0,
            first_instance: 0,
        }
    }
}

/// Owns the wgpu pipeline, buffers, and bind groups for rect rendering.
pub struct RectPipeline {
    pipeline: RenderPipeline,

    // Geometry
    vertex_buffer: Buffer,
    index_buffer: Buffer,

    // Instancing
    instance_buffer: Buffer,
    instance_count: u32,

    // Draw-indirect buffer (20 bytes)
    indirect_buffer: Buffer,

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

        // ── Indirect draw buffer (20 bytes, GPU-resident) ───────
        let indirect_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("rect_indirect"),
            size: std::mem::size_of::<DrawIndexedIndirectArgs>() as u64,
            usage: BufferUsages::INDIRECT | BufferUsages::COPY_DST,
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
            indirect_buffer,
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

    /// Upload instance data for this frame (full buffer write).
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

    /// Upload **only the changed slots** of the instance buffer.
    ///
    /// Each `(slot_index, instance)` pair writes exactly 48 bytes at
    /// the correct offset.  For 1 changed slot out of 1000, this writes
    /// 48 bytes instead of 48,000.
    ///
    /// `total_count` sets the draw instance count (must be passed if
    /// this is the first upload after a rebuild).
    pub fn upload_instances_partial(
        &mut self,
        queue: &Queue,
        dirty_slots: &[(usize, &RectInstance)],
        total_count: u32,
    ) {
        let stride = std::mem::size_of::<RectInstance>() as u64;
        for &(slot, inst) in dirty_slots {
            if slot < MAX_INSTANCES {
                queue.write_buffer(
                    &self.instance_buffer,
                    slot as u64 * stride,
                    bytemuck::bytes_of(inst),
                );
            }
        }
        self.instance_count = total_count.min(MAX_INSTANCES as u32);
    }

    /// Upload the draw-indirect arguments to the GPU.
    ///
    /// Call once after instance count changes.  On steady-state frames,
    /// skip this entirely — the GPU-side args are still valid.
    pub fn upload_indirect(&self, queue: &Queue) {
        let args = DrawIndexedIndirectArgs::new(self.instance_count);
        queue.write_buffer(
            &self.indirect_buffer,
            0,
            bytemuck::bytes_of(&args),
        );
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

    /// Record draw commands using the GPU-resident indirect buffer.
    ///
    /// Requires a prior `upload_indirect()` call.  On steady-state
    /// frames the indirect buffer is already populated — the CPU
    /// touches **zero** draw parameters.
    pub fn draw_indirect<'a>(&'a self, pass: &mut RenderPass<'a>) {
        if self.instance_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
        pass.draw_indexed_indirect(&self.indirect_buffer, 0);
    }

    /// Number of instances that will be drawn.
    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_draw_indirect_args_layout() {
        // Must be exactly 20 bytes for wgpu draw_indexed_indirect.
        assert_eq!(std::mem::size_of::<DrawIndexedIndirectArgs>(), 20);
    }

    #[test]
    fn test_draw_indirect_args_new() {
        let args = DrawIndexedIndirectArgs::new(42);
        assert_eq!(args.index_count, 6);
        assert_eq!(args.instance_count, 42);
        assert_eq!(args.first_index, 0);
        assert_eq!(args.base_vertex, 0);
        assert_eq!(args.first_instance, 0);
    }

    #[test]
    fn test_draw_indirect_args_bytemuck() {
        let args = DrawIndexedIndirectArgs::new(1000);
        let bytes = bytemuck::bytes_of(&args);
        assert_eq!(bytes.len(), 20);
        // Round-trip
        let back: &DrawIndexedIndirectArgs = bytemuck::from_bytes(bytes);
        assert_eq!(back.index_count, 6);
        assert_eq!(back.instance_count, 1000);
    }

    #[test]
    fn test_draw_indirect_args_zero_instances() {
        let args = DrawIndexedIndirectArgs::new(0);
        assert_eq!(args.instance_count, 0);
        assert_eq!(args.index_count, 6);
    }

    #[test]
    fn test_max_instances_constant() {
        // Verify 64K limit and memory math.
        assert_eq!(MAX_INSTANCES, 65_536);
        let buffer_bytes = MAX_INSTANCES * std::mem::size_of::<RectInstance>();
        assert_eq!(buffer_bytes, 65_536 * 48); // 3 MB
    }
}
