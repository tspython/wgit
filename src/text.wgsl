struct Uniforms {
  screen_w: f32,
  screen_h: f32,
  r: f32,
  g: f32,
  b: f32,
  a: f32,
};
@group(1) @binding(0) var<uniform> U: Uniforms;

struct VsIn {
  @location(0) pos: vec2<f32>,
  @location(1) uv: vec2<f32>,
  @location(2) color: vec4<f32>,
};

struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  let ndc_x = (v.pos.x / U.screen_w) * 2.0 - 1.0;
  let ndc_y = (v.pos.y / U.screen_h) * -2.0 + 1.0; // flip Y: top-left origin
  o.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
  o.uv = v.uv;
  o.color = v.color;
  return o;
}

@group(0) @binding(0) var tex0: texture_2d<f32>;
@group(0) @binding(1) var samp0: sampler;

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  let a = textureSample(tex0, samp0, i.uv).r; // R8Unorm single channel
  return vec4<f32>(i.color.rgb, i.color.a * a);
}
