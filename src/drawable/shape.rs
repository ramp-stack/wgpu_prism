use super::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Variant { Ellipse, Rectangle, RoundedRectangle(f32) }

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub stroke: f32,
    pub width: f32,
    pub height: f32,
    pub angle: f32,
    pub color: Color,
    pub variant: Variant,
}

impl Shape {
    pub fn new(width: f32, height: f32, color: Color, variant: Variant) -> Self {
        Shape {stroke: 0.0, width, height, angle: 0.0, color, variant}
    }

    pub fn size(&self) -> (f32, f32) {
        let theta = self.angle.to_radians();
        let cos = theta.cos().abs();
        let sin = theta.sin().abs();
        let w = self.width;
        let h = self.height;
        match self.variant {
            Variant::Ellipse => {
                let rx = w * 0.5;
                let ry = h * 0.5;
                let rx2 = rx * rx;
                let ry2 = ry * ry;

                // Width and height of the bounding box
                let bb_width = 2.0 * (rx2 * cos * cos + ry2 * sin * sin).sqrt();
                let bb_height = 2.0 * (rx2 * sin * sin + ry2 * cos * cos).sqrt();

                (bb_width, bb_height)
            },
            Variant::Rectangle => {
                let half_w_proj = (w * 0.5) * cos + (h * 0.5) * sin;
                let half_h_proj = (w * 0.5) * sin + (h * 0.5) * cos;
                (2.0 * half_w_proj, 2.0 * half_h_proj)
            },
            Variant::RoundedRectangle(radius) => {
                let r = radius.min(w * 0.5).min(h * 0.5);
                let inner_w = w - 2.0 * r;
                let inner_h = h - 2.0 * r;
                let half_w_proj = (inner_w * 0.5) * cos + (inner_h * 0.5) * sin;
                let half_h_proj = (inner_w * 0.5) * sin + (inner_h * 0.5) * cos;
                (2.0 * half_w_proj + 2.0 * r, 2.0 * half_h_proj + 2.0 * r)
            }
        }
    }
}
