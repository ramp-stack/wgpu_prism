use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferUsages, DepthStencilState, Device, FragmentState, IndexFormat, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, ShaderModule, ShaderStages,
    TextureFormat, TextureSampleType, TextureViewDimension, VertexBufferLayout, VertexState,
};

use std::ops::Range;
use std::sync::Arc;

use crate::drawable::shape::Variant;
use super::ImageItem;
use super::atlas::ImageAtlas;
use super::buffer::{DynamicBufferDescriptor, DynamicBuffer, QuadIndexBuffer};
use super::vertex::{Vertex, ImageVertex, ShapeVertex, RoundedRectangleVertex};

pub struct ImageRenderer {
    atlas: ImageAtlas,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
    ellipse_renderer: GenericImageRenderer<ImageVertex<ShapeVertex>>,
    rectangle_renderer: GenericImageRenderer<ImageVertex<ShapeVertex>>,
    rounded_rectangle_renderer: GenericImageRenderer<ImageVertex<RoundedRectangleVertex>>,
    items: Vec<(Variant, usize)>,
}
impl ImageRenderer {
    pub fn new(device: &Device, texture_format: &TextureFormat, multisample: MultisampleState, depth_stencil: Option<DepthStencilState>) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0, visibility: ShaderStages::FRAGMENT, count: None,
                    ty: BindingType::Texture {
                        multisampled: false, view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float {filterable: true},
                    },
                },
                BindGroupLayoutEntry {
                    binding: 1, visibility: ShaderStages::FRAGMENT, count: None,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("image/ellipse.wgsl"));
        let ellipse_renderer = GenericImageRenderer::new(
            device, texture_format, multisample, depth_stencil.clone(),
            &bind_group_layout, shader
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("image/rectangle.wgsl"));
        let rectangle_renderer = GenericImageRenderer::new(
            device, texture_format, multisample, depth_stencil.clone(),
            &bind_group_layout, shader
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("image/rounded_rectangle.wgsl"));
        let rounded_rectangle_renderer = GenericImageRenderer::new(
            device, texture_format, multisample, depth_stencil.clone(),
            &bind_group_layout, shader
        );
        ImageRenderer {
            atlas: ImageAtlas::default(), bind_group_layout, sampler,
            ellipse_renderer, rectangle_renderer, rounded_rectangle_renderer, items: vec![],
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, width: f32, height: f32, items: Vec<ImageItem>) {
        self.atlas.trim();
        self.items.clear();
        for (item, pixels, color) in items {
            let image = self.atlas.get(queue, device, &self.bind_group_layout, &self.sampler, &pixels);
            let dimensions = pixels.dimensions;
            let size = (item.2.width, item.2.height);
            match item.2.variant {
                Variant::Ellipse => {
                    self.items.push((item.2.variant, self.ellipse_renderer.images.len()));
                    self.ellipse_renderer.images.push((ImageVertex::new(
                        ShapeVertex::new(width, height, item), dimensions, size, color
                    ), image));
                }
                Variant::Rectangle => {
                    self.items.push((item.2.variant, self.rectangle_renderer.images.len()));
                    self.rectangle_renderer.images.push((ImageVertex::new(
                        ShapeVertex::new(width, height, item), dimensions, size, color
                    ), image));
                }
                Variant::RoundedRectangle(radius) => {
                    self.items.push((item.2.variant, self.rounded_rectangle_renderer.images.len()));
                    self.rounded_rectangle_renderer.images.push((ImageVertex::new(
                        RoundedRectangleVertex::new(width, height, item, radius), dimensions, size, color
                    ), image));
                },
            }
        }
        self.ellipse_renderer.prepare(device, queue);
        self.rectangle_renderer.prepare(device, queue);
        self.rounded_rectangle_renderer.prepare(device, queue);
    }

    pub fn render(&self, render_pass: &mut RenderPass<'_>, range: Range<usize>) {
        let mut start = range.start;
        while start < range.end {
            let (variant, index) = self.items[start];
            let mut end = start + 1;
            while end < range.end
                && std::mem::discriminant(&variant)
                    == std::mem::discriminant(&self.items[end].0) {end += 1;}
            let range = index..self.items[end - 1].1 + 1;
            match variant {
                Variant::Ellipse => self.ellipse_renderer.render(render_pass, range),
                Variant::Rectangle => self.rectangle_renderer.render(render_pass, range),
                Variant::RoundedRectangle(_) => {
                    self.rounded_rectangle_renderer.render(render_pass, range);
                }
            }
            start = end;
        }
    }
}

struct GenericImageRenderer<V> {
    render_pipeline: RenderPipeline,
    vertex_buffer: DynamicBuffer,
    index_buffer: QuadIndexBuffer,
    images: Vec<([V; 4], Arc<BindGroup>)>,
    vertices: Vec<V>,
    bind_groups: Vec<Arc<BindGroup>>,
}
impl<V: Vertex> GenericImageRenderer<V> {
    fn new(
        device: &Device, texture_format: &TextureFormat, multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>, bind_group_layout: &BindGroupLayout,
        shader: ShaderModule,
    ) -> Self {
        let attributes = V::attributes();
        let vertex_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<V>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes,
        };
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[bind_group_layout], push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None, layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader, entry_point: Some("vs_main"), buffers: &[vertex_layout],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader, entry_point: Some("fs_main"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: *texture_format, blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(), depth_stencil, multisample,
            multiview: None, cache: None,
        });
        let vertex_buffer = DynamicBuffer::new(device, &DynamicBufferDescriptor {
            label: None, usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
        let index_buffer = QuadIndexBuffer::new(device);
        GenericImageRenderer {
            render_pipeline, vertex_buffer, index_buffer,
            images: vec![], vertices: vec![], bind_groups: vec![],
        }
    }

    fn prepare(&mut self, device: &Device, queue: &Queue) {
        self.vertices.clear();
        self.bind_groups.clear();
        let quads = self.images.len() as u32;
        for (vertices, image) in self.images.drain(..) {
            self.vertices.extend(vertices);
            self.bind_groups.push(image);
        }
        self.vertex_buffer.write_buffer(device, queue, bytemuck::cast_slice(&self.vertices));
        self.index_buffer.prepare(device, queue, quads);
    }

    fn render(&self, render_pass: &mut RenderPass<'_>, range: Range<usize>) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().slice(..));
        render_pass.set_index_buffer(self.index_buffer.as_ref().slice(..), IndexFormat::Uint32);
        let mut start = range.start;
        while start < range.end {
            let mut end = start + 1;
            while end < range.end
                && Arc::ptr_eq(&self.bind_groups[start], &self.bind_groups[end]) {end += 1;}
            render_pass.set_bind_group(0, Some(&*self.bind_groups[start]), &[]);
            render_pass.draw_indexed(start as u32 * 6..end as u32 * 6, 0, 0..1);
            start = end;
        }
    }
}
