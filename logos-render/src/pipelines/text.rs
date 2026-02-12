//! Text render pipeline — instanced rendering of textured glyph quads.
//!
//! Uses a shared unit quad with per-instance glyph data (position, size,
//! UV region in the atlas, color).  One draw call renders all glyphs.

use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry,
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device,
    Extent3d, FilterMode, FragmentState, FrontFace, IndexFormat,
    MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderStages, Texture,
    TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, TextureViewDimension, VertexState,
};

use crate::vertex::{CameraUniform, QuadVertex, TextInstance};

/// Maximum glyph instances per draw call.
const MAX_TEXT_INSTANCES: usize = 65_536;

/// Owns the wgpu pipeline, buffers, texture, and bind groups for text.
pub struct TextPipeline {
    pipeline: RenderPipeline,

    // Geometry (shared unit quad).
    vertex_buffer: Buffer,
    index_buffer: Buffer,

    // Instancing.
    instance_buffer: Buffer,
    instance_count: u32,

    // Camera.
    camera_buffer: Buffer,
    camera_bind_group: BindGroup,
    #[allow(dead_code)]
    camera_bgl: BindGroupLayout,

    // Atlas texture.
    atlas_texture: Texture,
    atlas_bind_group: BindGroup,
    atlas_bgl: BindGroupLayout,
    atlas_size: u32,
}

impl TextPipeline {
    /// Create the text pipeline and allocate GPU buffers.
    ///
    /// `atlas_size` is the width=height of the glyph atlas texture.
    pub fn new(device: &Device, surface_format: TextureFormat, atlas_size: u32) -> Self {
        // ── Shader ──────────────────────────────────────────────
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/text.wgsl").into(),
            ),
        });

        // ── Camera bind group layout (group 0) ──────────────────
        let camera_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("text_camera_bgl"),
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

        // ── Atlas bind group layout (group 1) ───────────────────
        let atlas_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("text_atlas_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Pipeline layout ─────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("text_pipeline_layout"),
            bind_group_layouts: &[&camera_bgl, &atlas_bgl],
            push_constant_ranges: &[],
        });

        // ── Render pipeline ─────────────────────────────────────
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[QuadVertex::layout(), TextInstance::layout()],
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
                cull_mode: None,
                polygon_mode: PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Vertex buffer (unit quad) ───────────────────────────
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("text_quad_vb"),
            size: std::mem::size_of::<[QuadVertex; 4]>() as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Index buffer ────────────────────────────────────────
        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("text_quad_ib"),
            size: std::mem::size_of::<[u16; 6]>() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Instance buffer ─────────────────────────────────────
        let instance_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("text_instances"),
            size: (MAX_TEXT_INSTANCES * std::mem::size_of::<TextInstance>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Camera uniform buffer ───────────────────────────────
        let camera_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("text_camera_ub"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("text_camera_bg"),
            layout: &camera_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // ── Atlas texture (initially blank) ─────────────────────
        let atlas_texture = device.create_texture(&TextureDescriptor {
            label: Some("glyph_atlas"),
            size: Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("glyph_atlas_sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        let atlas_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("text_atlas_bg"),
            layout: &atlas_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&atlas_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            instance_count: 0,
            camera_buffer,
            camera_bind_group,
            camera_bgl,
            atlas_texture,
            atlas_bind_group,
            atlas_bgl,
            atlas_size,
        }
    }

    // ───────────────────── Upload ─────────────────────────────────

    /// Upload the static quad geometry. Call once after creation.
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

    /// Upload glyph instance data for this frame.
    pub fn upload_instances(&mut self, queue: &Queue, instances: &[TextInstance]) -> u32 {
        let count = instances.len().min(MAX_TEXT_INSTANCES);
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

    /// Upload the full atlas texture data (RGBA, atlas_size × atlas_size).
    ///
    /// Call this whenever `atlas.dirty` is true.
    pub fn upload_atlas(&mut self, device: &Device, queue: &Queue, data: &[u8], size: u32) {
        if size != self.atlas_size {
            // Recreate texture if size changed.
            self.atlas_size = size;
            self.atlas_texture = device.create_texture(&TextureDescriptor {
                label: Some("glyph_atlas"),
                size: Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            });

            // Rebuild bind group with new texture view.
            let atlas_view = self.atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let atlas_sampler = device.create_sampler(&SamplerDescriptor {
                label: Some("glyph_atlas_sampler"),
                address_mode_u: AddressMode::ClampToEdge,
                address_mode_v: AddressMode::ClampToEdge,
                mag_filter: FilterMode::Linear,
                min_filter: FilterMode::Linear,
                ..Default::default()
            });

            self.atlas_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("text_atlas_bg"),
                layout: &self.atlas_bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&atlas_view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&atlas_sampler),
                    },
                ],
            });
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4), // RGBA = 4 bytes per pixel
                rows_per_image: Some(size),
            },
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
    }

    // ───────────────────── Draw ───────────────────────────────────

    /// Record draw commands into the render pass.
    ///
    /// **One draw call** for all glyph instances.
    pub fn draw<'a>(&'a self, pass: &mut RenderPass<'a>) {
        if self.instance_count == 0 {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, &self.atlas_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..self.instance_count);
    }

    /// Number of glyph instances that will be drawn.
    pub fn instance_count(&self) -> u32 {
        self.instance_count
    }
}
