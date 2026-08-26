
struct VertexOutput {
  @builtin(position) position: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(
  @builtin(vertex_index) index: u32
)-> VertexOutput {
  var positions=array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),

    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>( 1.0,  1.0),
  );

  var uvs=array<vec2<f32>, 6>(
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),

    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 0.0),
  );

  var out: VertexOutput;

  out.position=vec4<f32>(positions[index], 0.0, 1.0);
  out.uv=uvs[index];

  return out;
}

@group(0) @binding(0)
var camera_texture: texture_2d<f32>;

@group(0) @binding(1)
var camera_sampler: sampler;

@fragment
fn fs_main(
  input: VertexOutput
)-> @location(0) vec4<f32> {
  return textureSample(
    camera_texture,
    camera_sampler,
    input.uv
  );
}
