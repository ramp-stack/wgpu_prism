@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

struct ShapeInput {
    @location(0) uv: vec2<f32>,
    @location(1) position: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) bounds: vec4<f32>,
    @location(4) z: f32,
    @location(5) stroke: f32,
    @location(6) color: vec4<f32>,
    @location(7) texture: vec2<f32>,
    @location(8) recolor: u32
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) size: vec2<f32>,
    @location(2) @interpolate(flat) bounds: vec4<f32>,
    @location(3) @interpolate(flat) stroke: f32,
    @location(4) @interpolate(flat) color: vec4<f32>,
    @location(5) texture: vec2<f32>,
    @location(6) vertex_position: vec2<f32>,
    @location(7) @interpolate(flat) recolor: u32
};

@vertex
fn vs_main(shape: ShapeInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(shape.position, shape.z, 1.0);
    out.uv = shape.uv;

    out.size = shape.size;

    out.bounds = shape.bounds;
    out.stroke = shape.stroke;
    out.color = shape.color;
    out.texture = shape.texture;
    out.recolor = shape.recolor;
    out.vertex_position = shape.position;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.recolor != 0u && in.color[3] == 0.0 {discard;}
    if in.vertex_position.x < in.bounds[0] || in.vertex_position.x > in.bounds[2] ||
       in.vertex_position.y > in.bounds[1] || in.vertex_position.y < in.bounds[3] {
        discard;
    }
    if in.stroke > 0 {
        if in.uv.x > in.stroke && in.uv.x < in.size.x-in.stroke &&
           in.uv.y > in.stroke && in.uv.y < in.size.y-in.stroke {
            discard;
        }
    }
    var color = textureSample(t_diffuse, s_diffuse, in.texture);
    if in.recolor != 0u {
        color = vec4<f32>(in.color[0], in.color[1], in.color[2], in.color[3]*color[3]);
    }
    return color;
}
