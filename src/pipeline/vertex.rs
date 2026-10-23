use bytemuck::{Pod, Zeroable};
use prism::drawable::Color;
use wgpu::{VertexAttribute, VertexFormat};

use std::fmt::Debug;

use super::ShapeItem;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ShapeVertex {
    pub uv: [f32; 2],
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub bounds: [f32; 4],
    pub z_index: f32,
    pub stroke: f32,
}
impl ShapeVertex {
    pub fn new(width: f32, height: f32, (offset, bound, shape, z): ShapeItem) -> [Self; 4] {
        let (sin, cos) = shape.angle.to_radians().sin_cos();
        let size = shape.size();
        let center = (offset.0 + size.0 * 0.5, offset.1 + size.1 * 0.5);
        let half = (shape.width * 0.5, shape.height * 0.5);
        let point = |x: f32, y: f32| [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0];
        let positions = [
            (-half.0, -half.1), (half.0, -half.1), (-half.0, half.1), (half.0, half.1),
        ].map(|(x, y)| point(center.0 + x * cos - y * sin, center.1 + x * sin + y * cos));
        let min = point(bound.0, bound.1);
        let max = point(bound.0 + bound.2, bound.1 + bound.3);
        let bounds = [min[0], min[1], max[0], max[1]];
        let size = [shape.width, shape.height];
        let stroke = shape.stroke.min(shape.width.min(shape.height));
        let uv = [[0.0, 0.0], [size[0], 0.0], [0.0, size[1]], size];
        std::array::from_fn(|i| Self {
            uv: uv[i], position: positions[i], size, bounds, z_index: z, stroke,
        })
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RoundedRectangleVertex {
    pub shape: ShapeVertex,
    pub corner_radius: f32,
}
impl RoundedRectangleVertex {
    pub fn new(width: f32, height: f32, item: ShapeItem, corner_radius: f32) -> [Self; 4] {
        let corner_radius = corner_radius.min(item.2.width * 0.5).min(item.2.height * 0.5);
        ShapeVertex::new(width, height, item).map(|shape| Self {shape, corner_radius})
    }
}

#[repr(packed, C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ColorVertex<V: Vertex = ShapeVertex> {
    pub shape: V,
    pub color: [f32; 4],
}
impl<V: Vertex> ColorVertex<V> {
    pub fn new(shape: [V; 4], color: Color) -> [Self; 4] {
        let linear = |value: u8| {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {value / 12.92} else {((value + 0.055) / 1.055).powf(2.4)}
        };
        // Alpha is linear opacity, so only RGB channels need the sRGB transfer function.
        let color = [linear(color.0), linear(color.1), linear(color.2), color.3 as f32 / 255.0];
        shape.map(|shape| Self {shape, color})
    }
}

#[repr(packed, C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct ImageVertex<V: Vertex = ShapeVertex> {
    pub color: ColorVertex<V>,
    pub texture: [f32; 2],
    pub recolor: u32,
}
impl<V: Vertex> ImageVertex<V> {
    pub fn new(shape: [V; 4], dimensions: (u32, u32), size: (f32, f32), color: Option<Color>) -> [Self; 4] {
        let (mut x, mut y, mut x2, mut y2) = (0.0, 0.0, 1.0, 1.0);
        let wr = size.0 / dimensions.0 as f32;
        let hr = size.1 / dimensions.1 as f32;
        if hr > wr {
            x = (1.0 - wr / hr) * 0.5;
            x2 = 1.0 - x;
        } else {
            y = (1.0 - hr / wr) * 0.5;
            y2 = 1.0 - y;
        }
        let recolor = u32::from(color.is_some());
        let color = ColorVertex::new(shape, color.unwrap_or_default());
        let texture = [[x, y], [x2, y], [x, y2], [x2, y2]];
        std::array::from_fn(|i| Self {color: color[i], texture: texture[i], recolor})
    }
}

pub trait Vertex: Sized + Debug + Pod + Zeroable {
    fn formats() -> Vec<VertexFormat>;

    fn attributes() -> Vec<VertexAttribute> {
        let mut offset = 0;
        Self::formats().into_iter().enumerate().map(|(index, format)| {
            let attribute = VertexAttribute {format, offset, shader_location: index as u32};
            offset += format.size();
            attribute
        }).collect()
    }
}
impl Vertex for ShapeVertex {
    fn formats() -> Vec<VertexFormat> {
        vec![
            VertexFormat::Float32x2, VertexFormat::Float32x2, VertexFormat::Float32x2,
            VertexFormat::Float32x4, VertexFormat::Float32, VertexFormat::Float32,
        ]
    }
}
impl Vertex for RoundedRectangleVertex {
    fn formats() -> Vec<VertexFormat> {
        [ShapeVertex::formats(), vec![VertexFormat::Float32]].into_iter().flatten().collect()
    }
}
impl<V: Vertex> Vertex for ColorVertex<V> {
    fn formats() -> Vec<VertexFormat> {
        [V::formats(), vec![VertexFormat::Float32x4]].into_iter().flatten().collect()
    }
}
impl<V: Vertex> Vertex for ImageVertex<V> {
    fn formats() -> Vec<VertexFormat> {
        [ColorVertex::<V>::formats(), vec![VertexFormat::Float32x2, VertexFormat::Uint32]]
            .into_iter().flatten().collect()
    }
}
