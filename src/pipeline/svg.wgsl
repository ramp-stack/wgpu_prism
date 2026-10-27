struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) depth: f32,
}

struct InstanceInput {
    @location(3) transform: vec4<f32>,
    @location(4) cover: vec4<f32>,
    @location(5) shape: vec4<f32>,
    @location(6) viewport: vec4<f32>,
    @location(7) bounds: vec4<f32>,
    @location(8) tint: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) shape: vec4<f32>,
    @location(3) @interpolate(flat) bounds: vec4<f32>,
    @location(4) @interpolate(flat) variant: f32,
    @location(5) @interpolate(flat) opacity: f32,
    @location(6) local_uv: vec2<f32>,
}

@group(0) @binding(0) var source_samples: texture_multisampled_2d<f32>;
@group(0) @binding(2) var source_resolved: texture_2d<f32>;
@group(0) @binding(3) var source_sampler: sampler;
@group(1) @binding(0) var clip_samples: texture_multisampled_2d<f32>;

@vertex
fn vs_paint(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    let position = vertex.position * instance.cover.x + instance.cover.yz;
    var output: VertexOutput;
    output.position = vec4<f32>(
        position.x / instance.viewport.x * 2.0 - 1.0,
        1.0 - position.y / instance.viewport.y * 2.0, vertex.depth, 1.0,
    );
    output.color = vertex.color;
    return output;
}

@vertex
fn vs_quad(@builtin(vertex_index) vertex: u32, instance: InstanceInput) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
    );
    let uv = corners[vertex] * instance.shape.xy;
    let local = uv - instance.shape.xy * 0.5;
    let position = vec2<f32>(
        local.x * instance.transform.z - local.y * instance.transform.w,
        local.x * instance.transform.w + local.y * instance.transform.z,
    ) + instance.transform.xy;
    var output: VertexOutput;
    output.position = vec4<f32>(
        position.x / instance.viewport.x * 2.0 - 1.0,
        1.0 - position.y / instance.viewport.y * 2.0, instance.viewport.z, 1.0,
    );
    output.uv = uv / instance.cover.xy;
    output.local_uv = uv;
    output.color = instance.tint;
    output.shape = instance.shape;
    output.bounds = instance.bounds;
    output.variant = instance.cover.w;
    output.opacity = instance.viewport.w;
    return output;
}

@fragment
fn fs_paint(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.color.a <= 0.0 {discard;}
    return vec4<f32>(input.color.rgb * input.color.a, input.color.a);
}

@fragment
fn fs_layer(input: VertexOutput, @builtin(sample_index) sample_index: u32) -> @location(0) vec4<f32> {
    let position = vec2<i32>(input.position.xy);
    var value = textureLoad(source_samples, position, i32(sample_index)) * input.opacity;
    if input.variant != 0.0 {
        value *= textureLoad(clip_samples, position, i32(sample_index)).a;
    }
    if value.a <= 0.0 {discard;}
    return value;
}

fn rectangle(uv: vec2<f32>, size: vec2<f32>, stroke: f32) -> f32 {
    if stroke > 0.0 && all(uv > vec2<f32>(stroke)) && all(uv < size - vec2<f32>(stroke)) {
        return 0.0;
    }
    return 1.0;
}

fn ellipse(uv: vec2<f32>, size: vec2<f32>, stroke: f32) -> f32 {
    let half = size * 0.5;
    let point = (half - uv) / max(half - vec2<f32>(1.0), vec2<f32>(0.001));
    let distance = dot(point, point);
    let feather = 2.0 / max(half.x, 0.001);
    var inner = 1.0;
    if stroke > 0.0 && all(half > vec2<f32>(stroke + 1.0)) {
        let point = (half - uv) / (half - vec2<f32>(stroke + 1.0));
        inner = smoothstep(1.0, 1.0 + feather, dot(point, point));
    }
    return (1.0 - smoothstep(1.0, 1.0 + feather, distance)) * inner;
}

fn rounded_rectangle(uv: vec2<f32>, size: vec2<f32>, stroke: f32, radius: f32) -> f32 {
    if radius <= 0.0 {return rectangle(uv, size, stroke);}
    var point = vec2<f32>(0.0);
    if uv.x < radius && uv.y < radius {
        point = vec2<f32>(radius) - uv;
    } else if uv.x > size.x - radius && uv.y < radius {
        point = vec2<f32>(size.x - radius, radius) - uv;
    } else if uv.x < radius && uv.y > size.y - radius {
        point = vec2<f32>(radius, size.y - radius) - uv;
    } else if uv.x > size.x - radius && uv.y > size.y - radius {
        point = size - vec2<f32>(radius) - uv;
    } else {
        return rectangle(uv, size, stroke);
    }
    let normalized = point / radius;
    let feather = 2.0 / radius;
    var inner = 1.0;
    if stroke > 0.0 && stroke < radius {
        let normalized = point / max(radius - stroke - 0.25, 0.001);
        inner = smoothstep(1.0, 1.0 + feather, dot(normalized, normalized));
    }
    return (1.0 - smoothstep(1.0, 1.0 + feather, dot(normalized, normalized))) * inner;
}

@fragment
fn fs_output(input: VertexOutput) -> @location(0) vec4<f32> {
    let position = input.position.xy;
    if any(position < input.bounds.xy) || any(position > input.bounds.xy + input.bounds.zw) {
        discard;
    }
    let source = textureSample(source_resolved, source_sampler, input.uv);
    if source.a <= 0.0 {discard;}
    let encoded = source.rgb / source.a;
    var color = select(
        pow((encoded + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        encoded / 12.92, encoded <= vec3<f32>(0.04045),
    );
    var alpha = source.a;
    if input.color.a >= 0.0 {
        color = input.color.rgb;
        alpha *= input.color.a;
    }
    var mask = rectangle(input.local_uv, input.shape.xy, input.shape.w);
    if input.variant == 1.0 {
        mask = ellipse(input.local_uv, input.shape.xy, input.shape.w);
    } else if input.variant == 2.0 {
        mask = rounded_rectangle(input.local_uv, input.shape.xy, input.shape.w, input.shape.z);
    }
    alpha *= mask;
    if alpha <= 0.0 {discard;}
    return vec4<f32>(color, alpha);
}
