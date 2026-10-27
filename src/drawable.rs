use media::Image;

pub mod shape;
use shape::Shape;
pub mod text;
mod svg;
pub use svg::Svg;

pub type Offset = (f32, f32);
pub type Rect = (f32, f32, f32, f32);
pub type Size = (f32, f32);

pub enum Item {
    Shape(Shape),
    Text(text::Text),
    Image(Shape, Image, Option<Color>),
    Svg(Shape, Svg, Option<Color>),
}

pub struct Instruction {
    pub bound: Rect,
    pub offset: Offset,
    pub item: Item,
}

#[derive(Debug, Clone)]
pub struct Icon(pub Svg, pub Color);

/// Unpremultiplied sRGB bytes with a linear alpha byte (0 transparent, 255 opaque).
#[derive(Default, Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);
impl Color {
    pub fn white() -> Self {Color(255, 255, 255, 255)}
    pub fn black() -> Self {Color(0, 0, 0, 255)}
    pub fn transparent() -> Self {Color(0, 0, 0, 0)}
    pub fn red() -> Self {Color(255, 0, 0, 255)}
    pub fn green() -> Self {Color(0, 255, 0, 255)}
    pub fn blue() -> Self {Color(0, 0, 255, 255)}
    pub fn yellow() -> Self {Color(255, 255, 0, 255)}
    pub fn magenta() -> Self {Color(255, 0, 255, 255)}
    pub fn cyan() -> Self {Color(0, 255, 255, 255)}
}
