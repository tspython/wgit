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
  @location(0) unit: vec2<f32>,
  @location(1) rect: vec4<f32>,
  @location(2) fill_top: vec4<f32>,
  @location(3) fill_bottom: vec4<f32>,
  @location(4) stroke: vec4<f32>,
  @location(5) shadow: vec4<f32>,
  @location(6) radius_soft_border_blur: vec4<f32>,
  @location(7) shadow_offset_spread: vec4<f32>,
};

struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) local: vec2<f32>,
  @location(1) fill_half: vec2<f32>,
  @location(2) fill_top: vec4<f32>,
  @location(3) fill_bottom: vec4<f32>,
  @location(4) stroke: vec4<f32>,
  @location(5) shadow: vec4<f32>,
  @location(6) params0: vec4<f32>,
  @location(7) params1: vec4<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  let center = vec2<f32>(v.rect.x + v.rect.z * 0.5, v.rect.y + v.rect.w * 0.5);
  let fill_half = v.rect.zw * 0.5;

  let shadow_blur = v.radius_soft_border_blur.w;
  let shadow_spread = v.shadow_offset_spread.z;
  let shadow_extent = shadow_blur + shadow_spread +
    max(abs(v.shadow_offset_spread.x), abs(v.shadow_offset_spread.y)) + 2.0;

  let draw_half = fill_half + vec2<f32>(shadow_extent, shadow_extent);
  let local = (v.unit * 2.0 - vec2<f32>(1.0, 1.0)) * draw_half;
  let world = center + local;

  let ndc_x = (world.x / U.screen_w) * 2.0 - 1.0;
  let ndc_y = (world.y / U.screen_h) * -2.0 + 1.0;
  o.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);

  o.local = local;
  o.fill_half = fill_half;
  o.fill_top = v.fill_top;
  o.fill_bottom = v.fill_bottom;
  o.stroke = v.stroke;
  o.shadow = v.shadow;
  o.params0 = v.radius_soft_border_blur;
  o.params1 = v.shadow_offset_spread;
  return o;
}

fn sd_round_rect(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
  let r = max(radius, 0.0);
  let q = abs(p) - (half_size - vec2<f32>(r, r));
  return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn over(top: vec4<f32>, bottom: vec4<f32>) -> vec4<f32> {
  let out_a = top.a + bottom.a * (1.0 - top.a);
  if out_a <= 0.0001 {
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
  }

  let out_rgb = (top.rgb * top.a + bottom.rgb * bottom.a * (1.0 - top.a)) / out_a;
  return vec4<f32>(out_rgb, out_a);
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
  let radius = min(i.params0.x, min(i.fill_half.x, i.fill_half.y));
  let soft = max(i.params0.y, 0.8);
  let border = max(i.params0.z, 0.0);
  let blur = max(i.params0.w, 0.5);
  let shadow_offset = i.params1.xy;
  let shadow_spread = i.params1.z;
  // Glassy top-edge highlight intensity (0 = disabled, ~1 = strong).
  // Packed into shadow_offset_spread.w; defaults to 0 for legacy callers.
  let highlight = clamp(i.params1.w, 0.0, 4.0);

  let d_fill = sd_round_rect(i.local, i.fill_half, radius);
  let fill_a = 1.0 - smoothstep(0.0, soft, d_fill);

  let inner_half = max(i.fill_half - vec2<f32>(border, border), vec2<f32>(1.0, 1.0));
  let inner_radius = max(radius - border, 0.0);
  let d_inner = sd_round_rect(i.local, inner_half, inner_radius);
  let inner_a = 1.0 - smoothstep(0.0, soft, d_inner);
  let border_a = max(fill_a - inner_a, 0.0);

  let grad_t = clamp((i.local.y / max(i.fill_half.y, 1.0)) * 0.5 + 0.5, 0.0, 1.0);
  var fill_color = mix(i.fill_top, i.fill_bottom, grad_t);

  // ── Chrome shading ───────────────────────────────────────────────
  // A 1 px luminous rim at the top edge, a soft ambient lift that
  // fades over the upper half, and a 1 px inner shadow at the bottom
  // edge. Together these sell the "lifted slice" depth for sidebar
  // items / chrome surfaces. Neutral tint, no hue.
  if highlight > 0.0001 {
    let from_top = i.local.y + i.fill_half.y;
    let from_bottom = i.fill_half.y - i.local.y;

    // 1 px hairline rim at the top
    let rim_thickness = 1.0;
    let rim = (1.0 - smoothstep(0.0, rim_thickness, from_top)) * inner_a;

    // Soft ambient brightening fading out over ~half the height
    let ambient_falloff = max(i.fill_half.y * 0.6, 8.0);
    let ambient = (1.0 - smoothstep(0.0, ambient_falloff, from_top)) * inner_a;

    // 1 px inner shadow at the bottom (mirror of rim)
    let bot_thickness = 1.0;
    let bot = (1.0 - smoothstep(0.0, bot_thickness, from_bottom)) * inner_a;

    let rim_strength = clamp(rim * highlight * 0.40, 0.0, 0.55);
    let ambient_strength = clamp(ambient * highlight * 0.05, 0.0, 0.14);
    let bot_strength = clamp(bot * highlight * 0.30, 0.0, 0.42);

    let lift_color = vec3<f32>(1.0, 1.0, 1.0);
    let shade_color = vec3<f32>(0.0, 0.0, 0.0);
    fill_color = vec4<f32>(
      clamp(
        fill_color.rgb
          + lift_color * (rim_strength + ambient_strength)
          - shade_color * 0.0
          - vec3<f32>(bot_strength) * 0.55,
        vec3<f32>(0.0),
        vec3<f32>(1.0)
      ),
      fill_color.a
    );
  }

  let fill_rgba = vec4<f32>(fill_color.rgb, fill_color.a * inner_a);
  let stroke_rgba = vec4<f32>(i.stroke.rgb, i.stroke.a * border_a);
  let fg_rgba = over(stroke_rgba, fill_rgba);

  let shadow_half = i.fill_half + vec2<f32>(shadow_spread, shadow_spread);
  let shadow_radius = radius + shadow_spread * 0.5;
  let d_shadow = sd_round_rect(i.local - shadow_offset, shadow_half, shadow_radius);
  let shadow_a = 1.0 - smoothstep(-blur, blur, d_shadow);
  let shadow_rgba = vec4<f32>(i.shadow.rgb, i.shadow.a * shadow_a * (1.0 - fg_rgba.a));

  return over(fg_rgba, shadow_rgba);
}
