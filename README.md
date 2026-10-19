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
    item: Item::Shape(Shape::new(100.0, 100.0, Color::CYAN, Variant::Rectangle)),
}];
renderer.draw(instructions, display_scale);
```

`Instruction::bound` is a clip rectangle in `(x, y, width, height)` order. Pass shapes, text,
image dimensions, offsets, and clip rectangles in logical units. The renderer scales prepared
geometry to physical pixels; image pixels are not resized or copied for a different scale.

Call `resize(physical_width, physical_height)` when the window changes size. Ramp implements
Maverick's renderer trait with a wrapper that forwards the window dimensions, display scale,
and Prism instructions to this API.

## Demo

From this checkout, run:

```sh
cargo run --manifest-path demo/Cargo.toml
```

The native demo uses Maverick directly and draws Prism shapes plus a generated checkerboard
image. Click to rotate the rounded rectangle. It does not require Ramp's Air services.

See [api.txt](api.txt) for the public interface.

## Discord

https://discord.gg/53ERRpS4S4
