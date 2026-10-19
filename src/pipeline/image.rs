use std::collections::HashMap;
use std::sync::Arc;

use prism::drawable::shape::Variant;
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferUsages, DepthStencilState, Device, FragmentState, IndexFormat, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, ShaderModule, ShaderStages,
    TextureFormat, TextureSampleType, TextureViewDimension, VertexBufferLayout, VertexState,
};

use super::ImageItem;
use super::atlas::ImageAtlas;
use super::buffer::{DynamicBufferDescriptor, DynamicBuffer, QuadIndexBuffer};
use super::vertex::{Vertex, ImageVertex, ShapeVertex, RoundedRectangleVertex};

pub struct ImageRenderer {
    atlas: ImageAtlas,
    bind_group_layout: BindGroupLayout,
    sampler: Sampler,
    ellipse_renderer: GenericImageRenderer,
    rectangle_renderer: GenericImageRenderer,
    rounded_rectangle_renderer: GenericImageRenderer,
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
            &bind_group_layout, shader, ImageVertex::<ShapeVertex>::layout()
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("image/rectangle.wgsl"));
        let rectangle_renderer = GenericImageRenderer::new(
            device, texture_format, multisample, depth_stencil.clone(),
            &bind_group_layout, shader, ImageVertex::<ShapeVertex>::layout()
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("image/rounded_rectangle.wgsl"));
        let rounded_rectangle_renderer = GenericImageRenderer::new(
            device, texture_format, multisample, depth_stencil.clone(),
            &bind_group_layout, shader, ImageVertex::<RoundedRectangleVertex>::layout()
        );
        ImageRenderer {
            atlas: ImageAtlas::default(), bind_group_layout, sampler,
            ellipse_renderer, rectangle_renderer, rounded_rectangle_renderer,
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, width: f32, height: f32, items: Vec<ImageItem>) {
        self.atlas.trim();
        let (mut ellipses, mut rects, mut rounded_rects) = (vec![], vec![], vec![]);
        for (item, pixels, color) in items {
            let image = self.atlas.get(queue, device, &self.bind_group_layout, &self.sampler, &pixels);
            let size = (item.2.width, item.2.height);
            match item.2.variant {
                Variant::Ellipse => ellipses.push((ImageVertex::new(
                    ShapeVertex::new(width, height, item), &pixels, size, color
                ), image)),
                Variant::Rectangle => rects.push((ImageVertex::new(
                    ShapeVertex::new(width, height, item), &pixels, size, color
                ), image)),
                Variant::RoundedRectangle(radius) => rounded_rects.push((ImageVertex::new(
                    RoundedRectangleVertex::new(width, height, item, radius), &pixels, size, color
                ), image)),
            }
        }
        self.ellipse_renderer.prepare(device, queue, ellipses);
        self.rectangle_renderer.prepare(device, queue, rects);
        self.rounded_rectangle_renderer.prepare(device, queue, rounded_rects);
    }

    pub fn render(&self, render_pass: &mut RenderPass<'_>) {
        self.ellipse_renderer.render(render_pass);
        self.rectangle_renderer.render(render_pass);
        self.rounded_rectangle_renderer.render(render_pass);
    }
}

struct GenericImageRenderer {
    render_pipeline: RenderPipeline,
    vertex_buffer: DynamicBuffer,
    index_buffer: QuadIndexBuffer,
    indices: HashMap<Arc<BindGroup>, Vec<(u32, u32)>>,
}
impl GenericImageRenderer {
    fn new(
        device: &Device, texture_format: &TextureFormat, multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>, bind_group_layout: &BindGroupLayout,
        shader: ShaderModule, vertex_layout: VertexBufferLayout
    ) -> Self {
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
        GenericImageRenderer {render_pipeline, vertex_buffer, index_buffer, indices: HashMap::new()}
    }

    fn prepare<V: bytemuck::Pod>(&mut self, device: &Device, queue: &Queue, image_vertices: Vec<([V; 4], Arc<BindGroup>)>) {
        self.indices.clear();
        let quads = image_vertices.len() as u32;
        let (vertices, indices_buffer) = image_vertices.into_iter().enumerate().fold(
            (vec![], HashMap::<Arc<BindGroup>, Vec<(u32, u32)>>::new()),
            |mut a, (quad, (vertices, image))| {
                a.0.extend(vertices);
                let start = quad as u32 * 6;
                let index = (start, start + 6);
                match a.1.get_mut(&image) {
                    Some(indices) => indices.push(index),
                    None => {a.1.insert(image, vec![index]);}
                }
                a
            }
        );
        self.indices = indices_buffer;
        self.vertex_buffer.write_buffer(device, queue, bytemuck::cast_slice(&vertices));
        self.index_buffer.prepare(device, queue, quads);
    }

    fn render(&self, render_pass: &mut RenderPass<'_>) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().slice(..));
        render_pass.set_index_buffer(self.index_buffer.as_ref().slice(..), IndexFormat::Uint32);
        for (bind_group, indices) in &self.indices {
            render_pass.set_bind_group(0, Some(&**bind_group), &[]);
            for (start, end) in indices {render_pass.draw_indexed(*start..*end, 0, 0..1);}
        }
    }
}
