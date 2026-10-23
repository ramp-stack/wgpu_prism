# wgpu_prism

`wgpu_prism` renders Prism drawing instructions with WGPU. Prism owns the instruction, shape,
text, color, and image types; this crate owns the GPU surface, pipelines, and texture caches.
It depends on Prism. Prism does not depend on this renderer.

## Drawing

Create one `Renderer` for a window, using its physical dimensions. Pass Prism instructions in
logical coordinates and the window's display scale to `draw`:

```rust
use prism::drawable::{Color, Instruction, Item, shape::{Shape, Variant}};
use wgpu_prism::Renderer;

let mut renderer = Renderer::new(window, physical_width, physical_height).await;
let instructions = vec![Instruction {
    offset: (20.0, 20.0),
    bound: (0.0, 0.0, logical_width, logical_height),
    item: Item::Shape(Shape::new(100.0, 100.0, Color::cyan(), Variant::Rectangle)),
}];
let presented = renderer.draw(instructions, display_scale);
```

`Instruction::bound` is a clip rectangle in `(x, y, width, height)` order. Pass shapes, text,
image dimensions, offsets, and clip rectangles in logical units. The renderer scales prepared
geometry to physical pixels. Raster display resizing reuses the same decoded pixels and GPU texture.
On the first upload only, an image exceeding the GPU texture limit is reduced while preserving its
aspect ratio; the original shared pixels remain the texture cache key. SVGs are separate instructions
and rasterize for their physical display size. Raster pixel access is lazy; the backend supplies
its corrupted-image placeholder when encoded content cannot decode.

`draw` and `render` return whether a frame was presented. A temporary surface timeout returns
`false`; surface loss triggers reconfiguration and one retry. Exhausted recovery or an
unrecoverable surface error stops with a diagnostic panic because the user cannot correct it.

Call `resize(physical_width, physical_height)` when the window changes size. Ramp implements
Maverick's renderer trait with a wrapper that forwards the window dimensions, display scale,
and Prism instructions to this API.

## Color and opacity

Prism colors contain unpremultiplied sRGB bytes and a linear alpha byte. RGB is converted to
linear light for rendering; alpha is divided by 255 without a color-space conversion. For
example, alpha 128 supplies about 50% opacity to blending. Recolored images multiply their
pixel alpha by this opacity; original image pixels keep their existing alpha.

The former `wgpu_canvas` vertex conversion also applied the sRGB transfer function to alpha,
which made intermediate opacity values too transparent. Preserve an older design by adjusting
its authored alpha values, rather than applying that conversion in the renderer or color API.

## Example application

The Pelican UI checkout contains the maintained application demo using this renderer through Ramp.

See [api.txt](api.txt) for the public interface.

## Discord

https://discord.gg/53ERRpS4S4
