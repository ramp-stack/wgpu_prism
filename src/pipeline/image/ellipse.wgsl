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
fn vs_main(
    shape: ShapeInput,
) -> VertexOutput {
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

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.recolor != 0u && in.color[3] == 0.0 {discard;}
	if in.vertex_position.x < in.bounds[0] || in.vertex_position.x > in.bounds[2] ||
       in.vertex_position.y > in.bounds[1] || in.vertex_position.y < in.bounds[3] {
        discard;
    }

    let a = (in.size.x / 2.0);
    let b = (in.size.y / 2.0);
    let x = (a-(in.uv.x)) / (a - 1.0);
    let y = (b-(in.uv.y)) / (b - 1.0);
    let d = x*x+y*y;
    let p = (2.0/a);

    var stroke = 1.0;
    if in.stroke > 0 {
        let sa = (in.size.x-(in.stroke*2.0)) / 2.0;
        let sb = (in.size.y-(in.stroke*2.0)) / 2.0;
        let sx = (a-(in.uv.x)) / (sa - 1.0);
        let sy = (b-(in.uv.y)) / (sb - 1.0);
        let sd = sx*sx+sy*sy;
        stroke = smoothstep(1.0, 1.0+p, sd);
    }

    var alpha = (1.0-smoothstep(1.0, 1.0+p, d)) * stroke;

    var color = textureSample(t_diffuse, s_diffuse, in.texture);
    if in.recolor != 0u {
        color = vec4<f32>(in.color[0], in.color[1], in.color[2], in.color[3]*color[3]);
    }
    return vec4<f32>(color[0], color[1], color[2], color[3]*alpha);
}
