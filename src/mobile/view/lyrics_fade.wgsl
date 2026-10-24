fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let color = sample_effect_image(input, input.uv);
    let y = input.uv.y * input.size.y;
    let widths = max(params.slots[0].xy, vec2<f32>(1.0));
    let top = smoothstep(0.0, widths.x, y);
    let bottom = smoothstep(0.0, widths.y, input.size.y - y);
    return vec4<f32>(color.rgb, color.a * top * bottom);
}
