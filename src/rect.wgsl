struct Uniforms {
  screen_w: f32,
  screen_h: f32,
  r: f32,
  g: f32,
  b: f32,
  a: f32,
};

@group(0) @binding(0) var<uniform> U: Uniforms;

struct VsIn {
  @location(0) pos: vec2<f32>,
};

struct VsOut {
  @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  let ndc_x = (v.pos.x / U.screen_w) * 2.0 - 1.0;
  let ndc_y = (v.pos.y / U.screen_h) * -2.0 + 1.0;
  o.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
  return o;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
  return vec4<f32>(U.r, U.g, U.b, U.a);
}
