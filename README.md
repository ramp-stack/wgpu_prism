# wgpu_prism

`wgpu_prism` owns drawing instructions, shapes, colors, text layout, SVGs, and an optional WGPU
renderer. Raster images use the shared `media2::Image` type. This crate has no dependency on
Prism; Prism reexports the drawing primitives and adds UI layout, events, and `Drawable`
implementations.

The default `drawable` feature provides the CPU drawing types, font measurement, text hit
testing, and SVG parsing without enabling WGPU. The optional `renderer` feature enables
`drawable` and the GPU surface, pipelines, SVG tessellation, and texture caches. Ramp enables
`renderer` when assembling a graphical application. A neighboring local crate can enable it with:

```toml
[dependencies]
wgpu_prism = { path = "../wgpu_prism", features = ["renderer"] }
```

With `default-features = false`, select `drawable` explicitly to use only the drawing types.
Disabling both features exposes neither module. CPU drawing still depends on `media2` for
images, on the font libraries for measurement, and on `usvg` for SVG parsing.

## Drawing

Create one `Renderer` for a window, using its physical dimensions. Pass drawing instructions in
logical coordinates and the window's display scale to `draw`:

```rust
use wgpu_prism::drawable::{Color, Instruction, Item, shape::{Shape, Variant}};
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
aspect ratio. The texture cache uses the pixel pointer and intrinsic dimensions as its key and
retains an `Image` clone to own those pixels. Clones share storage until mutation, which gets a
new texture upload. Drawing a video frame clones only its image handle. Visible images stay
resident; recently hidden images are retained within a
32-image, 64 MiB combined pixel and texture budget. Spare textures also have bounded retention.
SVGs remain separate instructions. `Svg::read` parses and validates their vector content once;
clones share that parsed content. The renderer retains path geometry and composes source colors,
transparency, groups, and clip paths on the GPU. Moving, rotating, and recoloring reuse the GPU
output; resizing redraws retained geometry at the new physical size. No CPU SVG rasterizer or
pixel-upload fallback remains. Whole-SVG tint and destination masks apply after the SVG's paths
have been composed, preserving overlapping transparency.

The supported SVG subset includes solid fills and strokes, path and group opacity, transforms,
and nested clip paths, using SVG’s default sRGB source compositing. Gradients, patterns, masks,
filters, embedded images, text, other blend/interpolation modes, and dashed strokes return an
error from `Svg::read`. Raster-image instructions already
contain decoded pixels; media decoding and corruption concealment stay in `media2`.

`draw` and `render` return whether a frame was presented. A temporary surface timeout returns
`false`; surface loss triggers reconfiguration and one retry. Exhausted recovery or an
unrecoverable surface error stops with a diagnostic panic because the user cannot correct it.

Call `resize(physical_width, physical_height)` when the window changes size. Ramp implements
Maverick's renderer trait with a wrapper that forwards the window dimensions, display scale,
and Prism instructions to this API.

## Color and opacity

Colors contain unpremultiplied sRGB bytes and a linear alpha byte. RGB is converted to
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
