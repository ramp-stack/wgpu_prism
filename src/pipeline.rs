use media::Image;
use wgpu::{CommandEncoder, DepthStencilState, Device, MultisampleState, Queue, RenderPass, TextureFormat};

use std::ops::Range;

use crate::drawable::{Color, Instruction, Item, Offset, Rect, shape::{Shape, Variant}};

mod buffer;
mod vertex;
mod color;
use color::ColorRenderer;
mod image;
use image::ImageRenderer;
mod svg;
use svg::SvgRenderer;
mod atlas;
use atlas::TextAtlas;

pub type ShapeItem = (Offset, Rect, Shape, f32);
pub type ImageItem = (ShapeItem, Image, Option<Color>);

pub struct Pipeline {
    color: ColorRenderer,
    image: ImageRenderer,
    svg: SvgRenderer,
    text: TextAtlas,
    order: Vec<(Range<usize>, Range<usize>, Option<usize>)>,
}
impl Pipeline {
    pub fn new(device: &Device, format: &TextureFormat, multisample: MultisampleState, depth: Option<DepthStencilState>) -> Self {
        Self {
            color: ColorRenderer::new(device, format, multisample, depth.clone()),
            image: ImageRenderer::new(device, format, multisample, depth.clone()),
            svg: SvgRenderer::new(device, format, multisample, depth),
            text: TextAtlas::default(), order: vec![],
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, encoder: &mut CommandEncoder, size: (f32, f32), scale: f32, items: &[Instruction]) {
        self.text.trim();
        self.svg.begin();
        self.order.clear();
        let count = items.len() as f32 + 1.0;
        let mut colors = Vec::new();
        let mut images = Vec::new();
        for (index, Instruction {offset, bound, item}) in items.iter().enumerate() {
            let (offset, bound) = (*offset, *bound);
            if bound.2 <= 0.0 || bound.3 <= 0.0 {continue}
            let z = (index as f32 + 1.0) / count;
            let (color_start, image_start) = (colors.len(), images.len());
            let mut svg_index = None;
            match item {
                Item::Shape(shape) => {
                    if shape.width > 0.0 && shape.height > 0.0 {colors.push((offset, bound, *shape, z));}
                }
                Item::Image(shape, _, color) | Item::Svg(shape, _, color) => {
                    if shape.width > 0.0 && shape.height > 0.0 {
                        let dimensions = shape.size();
                        if offset.0 >= bound.0 + bound.2 || offset.1 >= bound.1 + bound.3
                            || offset.0 + dimensions.0 <= bound.0 || offset.1 + dimensions.1 <= bound.1 {
                            continue
                        }
                        if let Item::Svg(_, svg, _) = item {
                            svg_index = Some(self.svg.push(
                                device, queue, size, scale, (offset, bound, *shape, z),
                                svg, *color,
                            ));
                        } else if let Item::Image(_, pixels, _) = item {
                            images.push(((offset, bound, *shape, z), pixels.clone(), *color));
                        }
                    }
                }
                Item::Text(text) => {
                    for character in text.characters() {
                        let (_, _, width, height) = character.1;
                        if width <= 0.0 || height <= 0.0 {continue}
                        if let Some((image, bearing)) = self.text.get(&character, scale) {
                            let (width, height) = image.dimensions;
                            let shape = Shape::new(
                                width as f32 / scale, height as f32 / scale,
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
            let order = (color_start..colors.len(), image_start..images.len(), svg_index);
            if order.0.is_empty() && order.1.is_empty() && svg_index.is_none() {continue}
            if let Some(previous) = self.order.last_mut()
                && previous.2.is_none() && svg_index.is_none()
                && previous.0.is_empty() == order.0.is_empty()
                && previous.1.is_empty() == order.1.is_empty() {
                previous.0.end = order.0.end;
                previous.1.end = order.1.end;
            } else {self.order.push(order);}
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
        self.svg.prepare(device, queue, encoder);
    }

    pub fn render<'a>(&'a self, pass: &mut RenderPass<'a>) {
        // Transparent items must blend in instruction order across pipelines and textures.
        for (colors, images, svg) in &self.order {
            self.color.render(pass, colors.clone());
            self.image.render(pass, images.clone());
            if let Some(index) = svg {self.svg.render(pass, *index);}
        }
    }
}
