use bytemuck::{Pod, Zeroable};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers, math::point, path::Path,
};
use usvg::{Node, Paint, PaintOrder, Transform, tiny_skia_path::PathSegment};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferDescriptor, BufferUsages,
    CommandEncoder, CompareFunction, DepthBiasState, DepthStencilState, Device, Extent3d,
    FragmentState, IndexFormat, LoadOp, MultisampleState, Operations, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPass, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, StencilState,
    StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension, VertexBufferLayout, VertexState,
    VertexStepMode,
};

use std::ops::Range;

use crate::drawable::{Color, Svg, shape::Variant};
use super::ShapeItem;
use super::buffer::{DynamicBuffer, DynamicBufferDescriptor};

pub struct SvgRenderer {
    paint: RenderPipeline,
    composite: RenderPipeline,
    output: RenderPipeline,
    sample_layout: BindGroupLayout,
    output_layout: BindGroupLayout,
    sampler: Sampler,
    samples: u32,
    meshes: Vec<(Svg, Buffer, Buffer, SvgCommand, bool)>,
    layers: Vec<SvgLayer>,
    instances: Vec<SvgInstance>,
    instance_buffer: DynamicBuffer,
    draws: Vec<(usize, u32)>,
    pending: Vec<(usize, usize)>,
    passes: Vec<usize>,
}
impl SvgRenderer {
    pub fn new(device: &Device, format: &TextureFormat, multisample: MultisampleState, depth: Option<DepthStencilState>) -> Self {
        let source = include_str!("svg.wgsl");
        let source = if multisample.count == 1 {
            source.replace("texture_multisampled_2d<f32>", "texture_2d<f32>")
        } else {source.to_owned()};
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None, source: ShaderSource::Wgsl(source.into()),
        });
        let sample_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[BindGroupLayoutEntry {
                binding: 0, visibility: ShaderStages::FRAGMENT, count: None,
                ty: BindingType::Texture {
                    multisampled: multisample.count > 1, view_dimension: TextureViewDimension::D2,
                    sample_type: TextureSampleType::Float {filterable: false},
                },
            }],
        });
        let output_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 2, visibility: ShaderStages::FRAGMENT, count: None,
                    ty: BindingType::Texture {
                        multisampled: false, view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float {filterable: true},
                    },
                },
                BindGroupLayoutEntry {
                    binding: 3, visibility: ShaderStages::FRAGMENT, count: None,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let vertex = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4, 2 => Float32];
        let instance = wgpu::vertex_attr_array![
            3 => Float32x4, 4 => Float32x4, 5 => Float32x4,
            6 => Float32x4, 7 => Float32x4, 8 => Float32x4,
        ];
        let instance_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<SvgInstance>() as u64,
            step_mode: VertexStepMode::Instance, attributes: &instance,
        };
        let paint_buffers = [
            VertexBufferLayout {
                array_stride: std::mem::size_of::<SvgVertex>() as u64,
                step_mode: VertexStepMode::Vertex, attributes: &vertex,
            },
            instance_layout.clone(),
        ];
        let quad_buffers = [instance_layout];
        let paint_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor::default());
        let composite_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&sample_layout, &sample_layout],
            push_constant_ranges: &[],
        });
        let final_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&output_layout], push_constant_ranges: &[],
        });
        let targets = [Some(wgpu::ColorTargetState {
            format: TextureFormat::Rgba8Unorm,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        // MSAA represents geometry coverage; alpha remains opacity throughout composition.
        let multisample = MultisampleState {alpha_to_coverage_enabled: false, ..multisample};
        let descriptor = RenderPipelineDescriptor {
            label: None, layout: Some(&paint_layout),
            vertex: VertexState {
                module: &shader, entry_point: Some("vs_paint"), buffers: &paint_buffers,
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader, entry_point: Some("fs_paint"), targets: &targets,
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float, depth_write_enabled: true,
                depth_compare: CompareFunction::Greater, stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample, multiview: None, cache: None,
        };
        let paint = device.create_render_pipeline(&descriptor);
        let composite = device.create_render_pipeline(&RenderPipelineDescriptor {
            layout: Some(&composite_layout),
            vertex: VertexState {
                entry_point: Some("vs_quad"), buffers: &quad_buffers, ..descriptor.vertex.clone()
            },
            fragment: Some(FragmentState {
                entry_point: Some("fs_layer"), ..descriptor.fragment.clone().unwrap()
            }),
            ..descriptor.clone()
        });
        let final_targets = [Some(wgpu::ColorTargetState {
            format: *format, blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let output = device.create_render_pipeline(&RenderPipelineDescriptor {
            layout: Some(&final_layout),
            vertex: VertexState {
                entry_point: Some("vs_quad"), buffers: &quad_buffers, ..descriptor.vertex.clone()
            },
            fragment: Some(FragmentState {
                entry_point: Some("fs_output"), targets: &final_targets,
                ..descriptor.fragment.clone().unwrap()
            }),
            depth_stencil: depth, ..descriptor
        });
        Self {
            paint, composite, output, sample_layout, output_layout, sampler,
            samples: multisample.count, meshes: vec![], layers: vec![], instances: vec![],
            draws: vec![], pending: vec![], passes: vec![],
            instance_buffer: DynamicBuffer::new(device, &DynamicBufferDescriptor {
                label: None, usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            }),
        }
    }

    pub fn begin(&mut self) {
        self.instances.clear();
        self.draws.clear();
        self.pending.clear();
        self.passes.clear();
        let mut count = self.meshes.len();
        let mut bytes = self.meshes.iter().map(|(_, v, i, _, _)| v.size() + i.size()).sum::<u64>();
        self.meshes.retain_mut(|(_, vertices, indices, _, used)| {
            let keep = *used || (count <= 32 && bytes <= 64 * 1024 * 1024);
            if !keep {count -= 1; bytes -= vertices.size() + indices.size();}
            *used = false;
            keep
        });
        let mut count = self.layers.len();
        let mut bytes = self.layers.iter().map(|layer| layer.bytes).sum::<u64>();
        self.layers.retain_mut(|layer| {
            let keep = layer.used || (count <= 32 && bytes <= 64 * 1024 * 1024);
            if !keep {count -= 1; bytes -= layer.bytes;}
            layer.used = false;
            keep
        });
    }

    pub fn push(
        &mut self, device: &Device, queue: &Queue, viewport: (f32, f32), scale: f32,
        item: ShapeItem, svg: &Svg, tint: Option<Color>,
    ) -> usize {
        let mesh = self.meshes.iter().position(|(cached, _, _, _, _)| cached == svg)
            .unwrap_or_else(|| {
                let mut geometry = VertexBuffers::new();
                let mut depth = 0;
                let mut command = SvgCommand::group(
                    svg.as_ref().root(), Transform::identity(), svg.dimensions(),
                    &mut geometry, &mut depth,
                );
                let depth = (depth + 1) as f32;
                for vertex in &mut geometry.vertices {vertex.depth /= depth;}
                command.normalize(depth);
                let vertices = device.create_buffer(&BufferDescriptor {
                    label: None, size: (geometry.vertices.len()
                        * std::mem::size_of::<SvgVertex>()).max(4) as u64,
                    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let indices = device.create_buffer(&BufferDescriptor {
                    label: None, size: (geometry.indices.len() * 4).max(4) as u64,
                    usage: BufferUsages::INDEX | BufferUsages::COPY_DST, mapped_at_creation: false,
                });
                if !geometry.vertices.is_empty() {
                    queue.write_buffer(&vertices, 0, bytemuck::cast_slice(&geometry.vertices));
                    queue.write_buffer(&indices, 0, bytemuck::cast_slice(&geometry.indices));
                }
                self.meshes.push((svg.clone(), vertices, indices, command, true));
                self.meshes.len() - 1
            });
        self.meshes[mesh].4 = true;
        let size = (item.2.width * scale, item.2.height * scale);
        let layer = self.layers.iter().position(|layer| {
            layer.svg.as_ref() == Some(svg) && layer.size == size
        }).unwrap_or_else(|| {
            let index = self.layer(device, size);
            self.layers[index].svg = Some(svg.clone());
            index
        });
        self.layers[layer].used = true;
        if !self.layers[layer].ready && !self.pending.iter().any(|(pending, _)| *pending == layer) {
            self.pending.push((layer, mesh));
        }
        let instance = self.instances.len() as u32;
        self.instances.push(SvgInstance::new(viewport, scale, item, &self.layers[layer], tint));
        self.draws.push((layer, instance));
        self.draws.len() - 1
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder) {
        for (layer, mesh) in std::mem::take(&mut self.pending) {
            let command = self.meshes[mesh].3.clone();
            let instance = self.instances.len() as u32;
            self.instances.push(SvgInstance::paint(&self.layers[layer], &self.meshes[mesh].0));
            self.layers[layer].mesh = mesh;
            self.layers[layer].instance = instance;
            self.plan(device, layer, command);
            self.passes.push(layer);
        }
        // Every pass has distinct parameters before any GPU work is submitted.
        self.instance_buffer.write_buffer(device, queue, bytemuck::cast_slice(&self.instances));
        for &index in &self.passes {
            let layer = &self.layers[index];
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &layer.view, resolve_target: layer.resolve.as_ref(),
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::TRANSPARENT), store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &layer.depth, stencil_ops: None,
                    depth_ops: Some(Operations {load: LoadOp::Clear(0.0), store: StoreOp::Discard}),
                }),
                occlusion_query_set: None, timestamp_writes: None,
            });
            let (_, vertices, indices, _, _) = &self.meshes[layer.mesh];
            for command in &layer.commands {
                match command {
                    SvgCommand::Paint(range) => {
                        if range.is_empty() {continue}
                        pass.set_pipeline(&self.paint);
                        pass.set_vertex_buffer(0, vertices.slice(..));
                        pass.set_vertex_buffer(1, self.instance_buffer.as_ref().slice(..));
                        pass.set_index_buffer(indices.slice(..), IndexFormat::Uint32);
                        pass.draw_indexed(range.clone(), 0, layer.instance..layer.instance + 1);
                    }
                    SvgCommand::Composite {source, clip, instance} => {
                        pass.set_pipeline(&self.composite);
                        pass.set_vertex_buffer(0, self.instance_buffer.as_ref().slice(..));
                        pass.set_bind_group(0, &self.layers[*source].sample_group, &[]);
                        pass.set_bind_group(
                            1, &self.layers[clip.unwrap_or(*source)].sample_group, &[],
                        );
                        pass.draw(0..6, *instance..*instance + 1);
                    }
                    SvgCommand::Group {..} => unreachable!(),
                }
            }
        }
        for index in self.passes.drain(..) {
            self.layers[index].ready = true;
            self.layers[index].commands.clear();
        }
    }

    pub fn render<'a>(&'a self, pass: &mut RenderPass<'a>, index: usize) {
        let (layer, instance) = self.draws[index];
        pass.set_pipeline(&self.output);
        pass.set_vertex_buffer(0, self.instance_buffer.as_ref().slice(..));
        pass.set_bind_group(0, &self.layers[layer].output_group, &[]);
        pass.draw(0..6, instance..instance + 1);
    }

    fn layer(&mut self, device: &Device, size: (f32, f32)) -> usize {
        let index = self.layers.iter().position(|layer| {
            !layer.used && layer.svg.is_none() && layer.size == size
        }).unwrap_or_else(|| {
            self.layers.push(SvgLayer::new(
                device, &self.sample_layout, &self.output_layout, &self.sampler, size, self.samples,
            ));
            self.layers.len() - 1
        });
        self.layers[index].used = true;
        self.layers[index].ready = false;
        self.layers[index].commands.clear();
        index
    }

    fn plan(&mut self, device: &Device, target: usize, command: SvgCommand) {
        match command {
            SvgCommand::Paint(_) => self.layers[target].commands.push(command),
            SvgCommand::Group {children, clip, opacity, depth} => {
                if clip.is_none() && opacity == 1.0 {
                    for child in children {self.plan(device, target, child);}
                    return
                }
                let size = self.layers[target].size;
                let mesh = self.layers[target].mesh;
                let paint_instance = self.layers[target].instance;
                let source = self.layer(device, size);
                self.layers[source].mesh = mesh;
                self.layers[source].instance = paint_instance;
                for child in children {self.plan(device, source, child);}
                self.passes.push(source);
                let clip = clip.map(|clip| {
                    let index = self.layer(device, size);
                    self.layers[index].mesh = mesh;
                    self.layers[index].instance = paint_instance;
                    self.plan(device, index, *clip);
                    self.passes.push(index);
                    index
                });
                let instance = self.instances.len() as u32;
                self.instances.push(SvgInstance::layer(
                    &self.layers[target], depth, opacity, clip.is_some(),
                ));
                self.layers[target].commands.push(SvgCommand::Composite {source, clip, instance});
            }
            SvgCommand::Composite {..} => unreachable!(),
        }
    }
}

#[derive(Clone)]
enum SvgCommand {
    Paint(Range<u32>),
    Group {
        children: Vec<Self>,
        clip: Option<Box<Self>>,
        opacity: f32,
        depth: f32,
    },
    Composite {
        source: usize,
        clip: Option<usize>,
        instance: u32,
    },
}
impl SvgCommand {
    fn group(
        group: &usvg::Group, transform: Transform, dimensions: (f32, f32),
        geometry: &mut VertexBuffers<SvgVertex, u32>, depth: &mut u32,
    ) -> Self {
        let mut children = Vec::new();
        for child in group.children() {
            match child {
                Node::Group(group) => {
                    children.push(Self::group(group, transform, dimensions, geometry, depth));
                }
                Node::Path(path) if path.is_visible() => {
                    children.extend(Self::paint(path, transform, dimensions, geometry, depth));
                }
                Node::Path(_) => {}
                Node::Image(_) | Node::Text(_) => unreachable!(),
            }
        }
        let clip = group.clip_path().map(|clip| Box::new(Self::clip(
            clip, transform.pre_concat(group.abs_transform()), dimensions, geometry, depth,
        )));
        *depth += 1;
        Self::Group {children, clip, opacity: group.opacity().get(), depth: *depth as f32}
    }

    fn clip(
        clip: &usvg::ClipPath, transform: Transform, dimensions: (f32, f32),
        geometry: &mut VertexBuffers<SvgVertex, u32>, depth: &mut u32,
    ) -> Self {
        let content = Self::group(
            clip.root(), transform.pre_concat(clip.transform()), dimensions, geometry, depth,
        );
        let clip = clip.clip_path().map(|clip| {
            Box::new(Self::clip(clip, transform, dimensions, geometry, depth))
        });
        *depth += 1;
        Self::Group {children: vec![content], clip, opacity: 1.0, depth: *depth as f32}
    }

    fn paint(
        path: &usvg::Path, transform: Transform, dimensions: (f32, f32),
        geometry: &mut VertexBuffers<SvgVertex, u32>, depth: &mut u32,
    ) -> Vec<Self> {
        let transform = transform.pre_concat(path.abs_transform());
        let magnification = (transform.sx.abs() + transform.kx.abs())
            .max(transform.ky.abs() + transform.sy.abs()).max(f32::EPSILON);
        let tolerance = dimensions.0.min(dimensions.1) / (65536.0 * magnification);
        let data = Self::path(path);
        let order = match path.paint_order() {
            PaintOrder::FillAndStroke => [true, false],
            PaintOrder::StrokeAndFill => [false, true],
        };
        let mut commands = Vec::new();
        for is_fill in order {
            let start = geometry.indices.len() as u32;
            *depth += 1;
            if is_fill {
                let Some(paint) = path.fill() else {continue};
                let Paint::Color(color) = paint.paint() else {unreachable!()};
                let rule = match paint.rule() {
                    usvg::FillRule::NonZero => FillRule::NonZero,
                    usvg::FillRule::EvenOdd => FillRule::EvenOdd,
                };
                let options = FillOptions::tolerance(tolerance).with_fill_rule(rule);
                FillTessellator::new().tessellate_path(&data, &options, &mut BuffersBuilder::new(
                    geometry, |vertex: FillVertex<'_>| {
                        SvgVertex::new(
                            vertex.position(), transform, *color, paint.opacity().get(), *depth,
                        )
                    },
                )).unwrap();
            } else {
                let Some(paint) = path.stroke() else {continue};
                let Paint::Color(color) = paint.paint() else {unreachable!()};
                let cap = match paint.linecap() {
                    usvg::LineCap::Butt => LineCap::Butt,
                    usvg::LineCap::Round => LineCap::Round,
                    usvg::LineCap::Square => LineCap::Square,
                };
                let join = match paint.linejoin() {
                    usvg::LineJoin::Miter => LineJoin::Miter,
                    usvg::LineJoin::MiterClip => LineJoin::MiterClip,
                    usvg::LineJoin::Round => LineJoin::Round,
                    usvg::LineJoin::Bevel => LineJoin::Bevel,
                };
                let options = StrokeOptions::tolerance(tolerance)
                    .with_line_width(paint.width().get()).with_line_cap(cap)
                    .with_line_join(join).with_miter_limit(paint.miterlimit().get());
                StrokeTessellator::new().tessellate_path(&data, &options, &mut BuffersBuilder::new(
                    geometry, |vertex: StrokeVertex<'_, '_>| {
                        SvgVertex::new(
                            vertex.position(), transform, *color, paint.opacity().get(), *depth,
                        )
                    },
                )).unwrap();
            }
            commands.push(Self::Paint(start..geometry.indices.len() as u32));
        }
        commands
    }

    fn path(path: &usvg::Path) -> Path {
        let mut builder = Path::builder();
        let mut open = false;
        for segment in path.data().segments() {
            match segment {
                PathSegment::MoveTo(p) => {
                    if open {builder.end(false);}
                    builder.begin(point(p.x, p.y));
                    open = true;
                }
                PathSegment::LineTo(p) => {builder.line_to(point(p.x, p.y));}
                PathSegment::QuadTo(p, end) => {
                    builder.quadratic_bezier_to(point(p.x, p.y), point(end.x, end.y));
                }
                PathSegment::CubicTo(a, b, end) => {
                    builder.cubic_bezier_to(point(a.x, a.y), point(b.x, b.y), point(end.x, end.y));
                }
                PathSegment::Close => {builder.end(true); open = false;}
            }
        }
        if open {builder.end(false);}
        builder.build()
    }

    fn normalize(&mut self, divisor: f32) {
        if let Self::Group {children, clip, depth, ..} = self {
            *depth /= divisor;
            for child in children {child.normalize(divisor);}
            if let Some(clip) = clip {clip.normalize(divisor);}
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SvgVertex {
    position: [f32; 2],
    color: [f32; 4],
    depth: f32,
}
impl SvgVertex {
    fn new(position: lyon_tessellation::math::Point, transform: Transform, color: usvg::Color, opacity: f32, depth: u32) -> Self {
        let mut position = usvg::tiny_skia_path::Point::from_xy(position.x, position.y);
        transform.map_point(&mut position);
        Self {
            position: [position.x, position.y], depth: depth as f32,
            color: [
                f32::from(color.red) / 255.0, f32::from(color.green) / 255.0,
                f32::from(color.blue) / 255.0, opacity,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SvgInstance {
    transform: [f32; 4],
    cover: [f32; 4],
    shape: [f32; 4],
    viewport: [f32; 4],
    bounds: [f32; 4],
    tint: [f32; 4],
}
impl SvgInstance {
    fn new(viewport: (f32, f32), scale: f32, item: ShapeItem, layer: &SvgLayer, tint: Option<Color>) -> Self {
        let (offset, bounds, shape, z) = item;
        let size = shape.size();
        let width = shape.width * scale;
        let height = shape.height * scale;
        let (sin, cos) = shape.angle.to_radians().sin_cos();
        let (variant, radius) = match shape.variant {
            Variant::Rectangle => (0.0, 0.0),
            Variant::Ellipse => (1.0, 0.0),
            Variant::RoundedRectangle(radius) => (2.0, radius.min(shape.width * 0.5)
                .min(shape.height * 0.5) * scale),
        };
        Self {
            transform: [
                (offset.0 + size.0 * 0.5) * scale, (offset.1 + size.1 * 0.5) * scale, cos, sin,
            ],
            cover: [
                layer.dimensions.0 as f32 / layer.scale,
                layer.dimensions.1 as f32 / layer.scale, 0.0, variant,
            ],
            shape: [width, height, radius, shape.stroke.min(shape.width.min(shape.height)) * scale],
            viewport: [viewport.0, viewport.1, z, 1.0],
            bounds: [bounds.0 * scale, bounds.1 * scale, bounds.2 * scale, bounds.3 * scale],
            tint: tint.map_or([0.0, 0.0, 0.0, -1.0], Self::color),
        }
    }

    fn paint(layer: &SvgLayer, svg: &Svg) -> Self {
        let dimensions = svg.dimensions();
        let width = layer.size.0 * layer.scale;
        let height = layer.size.1 * layer.scale;
        let ratio = (width / dimensions.0).max(height / dimensions.1);
        Self {
            transform: [0.0; 4], shape: [0.0; 4], bounds: [0.0; 4], tint: [0.0; 4],
            cover: [
                ratio, (width - dimensions.0 * ratio) * 0.5,
                (height - dimensions.1 * ratio) * 0.5, 0.0,
            ],
            viewport: [layer.dimensions.0 as f32, layer.dimensions.1 as f32, 0.0, 1.0],
        }
    }

    fn layer(layer: &SvgLayer, depth: f32, opacity: f32, clipped: bool) -> Self {
        let (width, height) = (layer.dimensions.0 as f32, layer.dimensions.1 as f32);
        Self {
            transform: [width * 0.5, height * 0.5, 1.0, 0.0],
            cover: [width, height, 0.0, if clipped {1.0} else {0.0}],
            shape: [width, height, 0.0, 0.0], viewport: [width, height, depth, opacity],
            bounds: [0.0, 0.0, width, height], tint: [0.0; 4],
        }
    }

    fn color(color: Color) -> [f32; 4] {
        let linear = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.04045 {value / 12.92} else {((value + 0.055) / 1.055).powf(2.4)}
        };
        [linear(color.0), linear(color.1), linear(color.2), f32::from(color.3) / 255.0]
    }
}

struct SvgLayer {
    svg: Option<Svg>,
    size: (f32, f32),
    dimensions: (u32, u32),
    scale: f32,
    view: TextureView,
    resolve: Option<TextureView>,
    depth: TextureView,
    sample_group: BindGroup,
    output_group: BindGroup,
    bytes: u64,
    used: bool,
    ready: bool,
    mesh: usize,
    instance: u32,
    commands: Vec<SvgCommand>,
}
impl SvgLayer {
    fn new(
        device: &Device, sample_layout: &BindGroupLayout, output_layout: &BindGroupLayout,
        sampler: &Sampler, size: (f32, f32), samples: u32,
    ) -> Self {
        let limit = device.limits().max_texture_dimension_2d as f32;
        let scale = (limit / size.0).min(limit / size.1).min(1.0);
        let dimensions = ((size.0 * scale).ceil().clamp(1.0, limit) as u32,
            (size.1 * scale).ceil().clamp(1.0, limit) as u32);
        let texture = |format, sample_count, usage| {
            device.create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: dimensions.0, height: dimensions.1, depth_or_array_layers: 1,
                },
                mip_level_count: 1, sample_count, dimension: TextureDimension::D2,
                format, usage, view_formats: &[],
            }).create_view(&TextureViewDescriptor::default())
        };
        let view = texture(TextureFormat::Rgba8Unorm, samples,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING);
        let resolve = (samples > 1).then(|| texture(TextureFormat::Rgba8Unorm, 1,
            TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING));
        let depth = texture(TextureFormat::Depth32Float, samples, TextureUsages::RENDER_ATTACHMENT);
        let sample_group = device.create_bind_group(&BindGroupDescriptor {
            label: None, layout: sample_layout,
            entries: &[BindGroupEntry {binding: 0, resource: BindingResource::TextureView(&view)}],
        });
        let output_group = device.create_bind_group(&BindGroupDescriptor {
            label: None, layout: output_layout,
            entries: &[
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(resolve.as_ref().unwrap_or(&view)),
                },
                BindGroupEntry {binding: 3, resource: BindingResource::Sampler(sampler)},
            ],
        });
        let bytes = u64::from(dimensions.0) * u64::from(dimensions.1)
            * u64::from(samples * 8 + if samples > 1 {4} else {0});
        Self {
            svg: None, size, dimensions, scale, view, resolve, depth, sample_group, output_group,
            bytes, used: true, ready: false, mesh: 0, instance: 0, commands: vec![],
        }
    }
}
