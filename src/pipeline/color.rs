use wgpu::{
    BufferUsages, DepthStencilState, Device, FragmentState, IndexFormat, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPass,
    RenderPipeline, RenderPipelineDescriptor, ShaderModule, TextureFormat, VertexBufferLayout,
    VertexState,
};

use std::ops::Range;

use crate::drawable::shape::Variant;
use super::ShapeItem;
use super::buffer::{DynamicBufferDescriptor, DynamicBuffer, QuadIndexBuffer};
use super::vertex::{Vertex, ShapeVertex, RoundedRectangleVertex, ColorVertex};

pub struct ColorRenderer {
    ellipse_renderer: GenericColorRenderer,
    rectangle_renderer: GenericColorRenderer,
    rounded_rectangle_renderer: GenericColorRenderer,
    items: Vec<ShapeItem>,
    indices: Vec<usize>,
    size: (f32, f32),
}
impl ColorRenderer {
    pub fn new(device: &Device, texture_format: &TextureFormat, multisample: MultisampleState, depth_stencil: Option<DepthStencilState>) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("color/ellipse.wgsl"));
        let ellipse_renderer = GenericColorRenderer::new::<ColorVertex<ShapeVertex>>(
            device, texture_format, multisample, depth_stencil.clone(), shader
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("color/rectangle.wgsl"));
        let rectangle_renderer = GenericColorRenderer::new::<ColorVertex<ShapeVertex>>(
            device, texture_format, multisample, depth_stencil.clone(), shader
        );
        let shader = device.create_shader_module(wgpu::include_wgsl!("color/rounded_rectangle.wgsl"));
        let rounded_rectangle_renderer = GenericColorRenderer::new::<ColorVertex<RoundedRectangleVertex>>(
            device, texture_format, multisample, depth_stencil.clone(), shader
        );
        ColorRenderer {
            ellipse_renderer, rectangle_renderer, rounded_rectangle_renderer,
            items: vec![], indices: vec![], size: (0.0, 0.0),
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, width: f32, height: f32, items: Vec<ShapeItem>) {
        if self.size == (width, height) && self.items == items {return}
        let (mut ellipses, mut rects, mut rounded_rects) = (vec![], vec![], vec![]);
        self.indices.clear();
        for &item in &items {
            let color = item.2.color;
            let index = match item.2.variant {
                Variant::Ellipse => {
                    let index = ellipses.len();
                    ellipses.push(ColorVertex::new(ShapeVertex::new(width, height, item), color));
                    index
                }
                Variant::Rectangle => {
                    let index = rects.len();
                    rects.push(ColorVertex::new(ShapeVertex::new(width, height, item), color));
                    index
                }
                Variant::RoundedRectangle(radius) => {
                    let index = rounded_rects.len();
                    rounded_rects.push(ColorVertex::new(
                        RoundedRectangleVertex::new(width, height, item, radius), color
                    ));
                    index
                }
            };
            self.indices.push(index);
        }
        self.ellipse_renderer.prepare(device, queue, ellipses);
        self.rectangle_renderer.prepare(device, queue, rects);
        self.rounded_rectangle_renderer.prepare(device, queue, rounded_rects);
        self.items = items;
        self.size = (width, height);
    }

    pub fn render(&self, render_pass: &mut RenderPass<'_>, range: Range<usize>) {
        let mut start = range.start;
        while start < range.end {
            let variant = self.items[start].2.variant;
            let mut end = start + 1;
            while end < range.end
                && std::mem::discriminant(&variant)
                    == std::mem::discriminant(&self.items[end].2.variant) {end += 1;}
            let renderer = match variant {
                Variant::Ellipse => &self.ellipse_renderer,
                Variant::Rectangle => &self.rectangle_renderer,
                Variant::RoundedRectangle(_) => &self.rounded_rectangle_renderer,
            };
            renderer.render(render_pass, self.indices[start]..self.indices[end - 1] + 1);
            start = end;
        }
    }
}

struct GenericColorRenderer {
    render_pipeline: RenderPipeline,
    vertex_buffer: DynamicBuffer,
    index_buffer: QuadIndexBuffer,
}
impl GenericColorRenderer {
    fn new<V: Vertex>(
        device: &Device, texture_format: &TextureFormat, multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>, shader: ShaderModule,
    ) -> Self {
        let attributes = V::attributes();
        let vertex_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<V>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes,
        };
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor::default());
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
        GenericColorRenderer {render_pipeline, vertex_buffer, index_buffer}
    }

    fn prepare<V: bytemuck::Pod>(&mut self, device: &Device, queue: &Queue, vertices: Vec<[V; 4]>) {
        let quads = vertices.len() as u32;
        self.vertex_buffer.write_buffer(device, queue, bytemuck::cast_slice(&vertices));
        self.index_buffer.prepare(device, queue, quads);
    }

    fn render(&self, render_pass: &mut RenderPass<'_>, range: Range<usize>) {
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.as_ref().slice(..));
        render_pass.set_index_buffer(self.index_buffer.as_ref().slice(..), IndexFormat::Uint32);
        render_pass.draw_indexed(range.start as u32 * 6..range.end as u32 * 6, 0, 0..1);
    }
}
