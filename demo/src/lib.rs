use std::sync::Arc;

use image::{Rgba, RgbaImage};
use maverick_os::{Application, Hardware, LogicalWindow, Window};
use maverick_os::inputs::{DeviceInput, Input, MouseState};
use prism::drawable::{Color, Instruction, Item, shape::{Shape, Variant}};
use prism::hardware::Image;

pub struct Demo {window: LogicalWindow, image: Image, angle: f32}
impl Application for Demo {
    type Renderer = DemoRenderer;

    fn name() -> String {"Prism renderer demo".into()}
    fn new(_hardware: Hardware, window: LogicalWindow) -> Self {
        let image = Arc::new(RgbaImage::from_fn(96, 96, |x, y| {
            if (x / 12 + y / 12) % 2 == 0 {Rgba([255, 190, 30, 255])}
            else {Rgba([30, 100, 210, 255])}
        }));
        Self {window, image, angle: 0.0}
    }
    fn on_input(&mut self, input: Input) {
        match input {
            Input::Resized(window) => self.window = window,
            Input::Device(_, DeviceInput::Mouse(_, MouseState::Pressed(_))) => {
                self.angle = (self.angle + 15.0) % 360.0;
            }
            _ => {}
        }
    }
    fn on_tick(&mut self) -> Vec<Instruction> {
        let bound = (0.0, 0.0, self.window.size.0, self.window.size.1);
        let mut rounded = Shape::new(100.0, 100.0, Color::GREEN, Variant::RoundedRectangle(20.0));
        rounded.angle = self.angle;
        let image = Shape::new(96.0, 96.0, Color::WHITE, Variant::Rectangle);
        let items = [
            ((24.0, 24.0), Item::Shape(Shape::new(100.0, 100.0, Color::CYAN, Variant::Rectangle))),
            ((160.0, 24.0), Item::Shape(Shape::new(100.0, 100.0, Color::MAGENTA, Variant::Ellipse))),
            ((24.0, 160.0), Item::Shape(rounded)),
            ((160.0, 160.0), Item::Image(image, self.image.clone(), None)),
        ];
        items.into_iter().map(|(offset, item)| Instruction {bound, offset, item}).collect()
    }
}

pub struct DemoRenderer {window: Window, renderer: wgpu_prism::Renderer<'static>}
impl maverick_os::Renderer for DemoRenderer {
    type Instructions = Vec<Instruction>;

    fn new(window: Window) -> Self {
        let size = window.physical();
        let renderer = futures::executor::block_on(wgpu_prism::Renderer::new(
            window.clone(), size.width, size.height
        ));
        Self {window, renderer}
    }
    fn resize(&mut self) {
        let size = self.window.physical();
        self.renderer.resize(size.width, size.height);
    }
    fn draw(&mut self, instructions: Vec<Instruction>) {
        let size = self.window.physical();
        if size.width == 0 || size.height == 0 {return}
        self.renderer.draw(instructions, size.scale_factor as f32);
    }
}

maverick_os::start!(Demo);
