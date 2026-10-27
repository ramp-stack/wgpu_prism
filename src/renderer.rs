use wgpu::{
    CommandEncoderDescriptor, CompareFunction, DepthBiasState, DepthStencilState, Device,
    DeviceDescriptor, Extent3d, Features, Instance, InstanceDescriptor, Limits, LoadOp,
    MultisampleState, Operations, PowerPreference, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, RequestAdapterOptions, StencilState,
    StoreOp, Surface, SurfaceConfiguration, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor, Trace, WindowHandle,
};

use crate::drawable::{Instruction, Item};
use crate::pipeline::Pipeline;

pub struct Renderer<'surface> {
    _instance: Instance,
    surface: Surface<'surface>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    msaa: TextureView,
    depth: TextureView,
    pipeline: Pipeline,
    instructions: Vec<Instruction>,
    scale: f32,
    dirty: bool,
    first_frame: bool,
}
impl<'surface> Renderer<'surface> {
    pub async fn new<W: WindowHandle + 'surface>(window: W, width: u32, height: u32) -> Self {
        let instance = Instance::new(&InstanceDescriptor::default());
        let surface = instance.create_surface(window).unwrap();
        let adapter = instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::None,
            compatible_surface: Some(&surface), force_fallback_adapter: false,
        }).await.unwrap();
        let mut limits = Limits::downlevel_webgl2_defaults();
        limits.max_texture_dimension_2d = adapter.limits().max_texture_dimension_2d;
        let (device, queue) = adapter.request_device(&DeviceDescriptor {
            required_features: Features::empty(), required_limits: limits, label: None,
            memory_hints: Default::default(), trace: Trace::Off,
        }).await.unwrap();
        let capabilities = surface.get_capabilities(&adapter);
        let max = device.limits().max_texture_dimension_2d;
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            width: width.clamp(1, max), height: height.clamp(1, max),
            format: capabilities.formats[0], present_mode: capabilities.present_modes[0],
            alpha_mode: capabilities.alpha_modes[0], view_formats: vec![capabilities.formats[0]],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let multisample = MultisampleState {count: 4, mask: !0, alpha_to_coverage_enabled: true};
        let depth = DepthStencilState {
            format: TextureFormat::Depth32Float, depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual, stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        };
        let pipeline = Pipeline::new(&device, &config.format, multisample, Some(depth));
        let msaa = Self::attachment(&device, &config, config.format);
        let depth = Self::attachment(&device, &config, TextureFormat::Depth32Float);
        Self {
            _instance: instance, surface, device, queue, config, msaa, depth, pipeline,
            instructions: vec![], scale: 1.0, dirty: true, first_frame: true,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {return}
        let max = self.device.limits().max_texture_dimension_2d;
        let (width, height) = (width.min(max), height.min(max));
        if self.config.width == width && self.config.height == height {return}
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.msaa = Self::attachment(&self.device, &self.config, self.config.format);
        self.depth = Self::attachment(&self.device, &self.config, TextureFormat::Depth32Float);
        self.dirty = true;
    }

    pub fn update(&mut self, instructions: Vec<Instruction>, scale: f32) -> bool {
        let same = self.scale == scale && self.instructions.len() == instructions.len()
            && self.instructions.iter().zip(&instructions).all(|(left, right)| {
                left.offset == right.offset && left.bound == right.bound && match (&left.item, &right.item) {
                    (Item::Shape(left), Item::Shape(right)) => left == right,
                    (Item::Text(left), Item::Text(right)) => left == right,
                    (Item::Image(ls, li, lc), Item::Image(rs, ri, rc)) => {
                        ls == rs && lc == rc && li.dimensions == ri.dimensions
                            && li.as_ptr() == ri.as_ptr()
                    }
                    (Item::Svg(ls, li, lc), Item::Svg(rs, ri, rc)) => {
                        ls == rs && lc == rc && li == ri
                    }
                    _ => false,
                }
            });
        if !same {
            self.instructions = instructions;
            self.scale = scale;
            self.dirty = true;
        }
        self.dirty
    }

    pub fn draw(&mut self, instructions: Vec<Instruction>, scale: f32) -> bool {
        self.update(instructions, scale);
        self.render()
    }

    pub fn render(&mut self) -> bool {
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Timeout) => return false,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(output) => output,
                    Err(wgpu::SurfaceError::Timeout) => return false,
                    Err(error) => panic!("Cannot recover the drawing surface: {error}"),
                }
            }
            Err(error) => panic!("Cannot acquire the drawing surface: {error}"),
        };
        let view = output.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor::default());
        if self.dirty {
            let size = (self.config.width as f32, self.config.height as f32);
            self.pipeline.prepare(
                &self.device, &self.queue, &mut encoder, size, self.scale, &self.instructions,
            );
            self.dirty = false;
        }
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.msaa, resolve_target: Some(&view),
                ops: Operations {load: LoadOp::Clear(wgpu::Color::TRANSPARENT), store: StoreOp::Discard},
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.depth, stencil_ops: None,
                depth_ops: Some(Operations {load: LoadOp::Clear(0.0), store: StoreOp::Discard}),
            }),
            occlusion_query_set: None, timestamp_writes: None,
        });
        self.pipeline.render(&mut pass);
        drop(pass);
        let submission = self.queue.submit(Some(encoder.finish()));
        if self.first_frame {
            // The native launch view may be removed after this call; finish the first frame first.
            self.device.poll(wgpu::PollType::WaitForSubmissionIndex(submission)).unwrap();
            self.first_frame = false;
        }
        output.present();
        true
    }

    fn attachment(device: &Device, config: &SurfaceConfiguration, format: TextureFormat) -> TextureView {
        device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: config.width, height: config.height, depth_or_array_layers: 1,
            },
            mip_level_count: 1, sample_count: 4, dimension: TextureDimension::D2,
            format, usage: TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        }).create_view(&TextureViewDescriptor::default())
    }
}
