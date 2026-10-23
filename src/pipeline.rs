use prism::drawable::{Color, Instruction, Item, Offset, Rect, shape::{Shape, Variant}};
use ::image::RgbaImage;
use wgpu::{DepthStencilState, Device, MultisampleState, Queue, RenderPass, TextureFormat};

use std::sync::Arc;

mod buffer;
mod vertex;
mod color;
use color::ColorRenderer;
mod image;
use image::ImageRenderer;
mod atlas;
use atlas::TextAtlas;

pub type ShapeItem = (Offset, Rect, Shape, f32);
pub type ImageItem = (ShapeItem, Arc<RgbaImage>, Option<Color>);

pub struct Pipeline {
    color: ColorRenderer,
    image: ImageRenderer,
    text: TextAtlas,
}
impl Pipeline {
    pub fn new(device: &Device, format: &TextureFormat, multisample: MultisampleState, depth: Option<DepthStencilState>) -> Self {
        Self {
            color: ColorRenderer::new(device, format, multisample, depth.clone()),
            image: ImageRenderer::new(device, format, multisample, depth),
            text: TextAtlas::default(),
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, size: (f32, f32), scale: f32, items: &[Instruction]) {
        self.text.trim();
        let count = items.len() as f32 + 1.0;
        let mut colors = Vec::new();
        let mut images = Vec::new();
        for (index, Instruction {offset, bound, item}) in items.iter().enumerate() {
            let (offset, bound) = (*offset, *bound);
            if bound.2 <= 0.0 || bound.3 <= 0.0 {continue}
            let z = (index as f32 + 1.0) / count;
            match item {
                Item::Shape(shape) => {
                    if shape.width > 0.0 && shape.height > 0.0 {colors.push((offset, bound, *shape, z));}
                }
                Item::Image(shape, _, color) | Item::Svg(shape, _, color) => {
                    if shape.width > 0.0 && shape.height > 0.0 {
                        if offset.0 >= bound.0 + bound.2 || offset.1 >= bound.1 + bound.3
                            || offset.0 + shape.width <= bound.0 || offset.1 + shape.height <= bound.1 {
                            continue
                        }
                        let pixels = match item {
                            Item::Image(_, image, _) => Arc::clone(image),
                            Item::Svg(_, svg, _) => {
                                let (width, height) = svg.dimensions();
                                let dimensions = (f64::from(width), f64::from(height));
                                let limit = f64::from(device.limits().max_texture_dimension_2d);
                                // Cover the destination shape; the shader crops the excess.
                                let ratio = (f64::from(shape.width) * f64::from(scale) / dimensions.0)
                                    .max(f64::from(shape.height) * f64::from(scale) / dimensions.1)
                                    .min(limit / dimensions.0).min(limit / dimensions.1);
                                svg.rasterize((
                                    (dimensions.0 * ratio).ceil().clamp(1.0, limit) as u32,
                                    (dimensions.1 * ratio).ceil().clamp(1.0, limit) as u32,
                                ))
                            }
                            _ => unreachable!(),
                        };
                        images.push(((offset, bound, *shape, z), pixels, *color));
                    }
                }
                Item::Text(text) => {
                    for character in text.characters() {
                        let (_, _, width, height) = character.1;
                        if width <= 0.0 || height <= 0.0 {continue}
                        if let Some((image, bearing)) = self.text.get(&character, scale) {
                            let shape = Shape::new(
                                image.width() as f32 / scale, image.height() as f32 / scale,
                                Color::white(), Variant::Rectangle,
                            );
                            let offset = (
                                offset.0 + character.7.0 + bearing.0 / scale,
                                offset.1 + character.7.1 + bearing.1 / scale,
                            );
                            images.push(((offset, bound, shape, z), image, character.3));
                        }
                    }
                }
            }
        }
        for (offset, bound, shape, _) in colors.iter_mut().chain(images.iter_mut().map(|item| &mut item.0)) {
            *offset = (offset.0 * scale, offset.1 * scale);
            *bound = (bound.0 * scale, bound.1 * scale, bound.2 * scale, bound.3 * scale);
            shape.width *= scale;
            shape.height *= scale;
            shape.stroke *= scale;
            if let Variant::RoundedRectangle(radius) = &mut shape.variant {*radius *= scale;}
        }
        self.color.prepare(device, queue, size.0, size.1, colors);
        self.image.prepare(device, queue, size.0, size.1, images);
    }

    pub fn render<'a>(&'a self, pass: &mut RenderPass<'a>) {
        self.color.render(pass);
        self.image.render(pass);
    }
}
