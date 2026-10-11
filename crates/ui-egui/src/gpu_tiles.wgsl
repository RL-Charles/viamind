// Colour conversion/dithering conventions adapted from egui-wgpu 0.36.2.
// Copyright (c) 2018-2021 Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
// 
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
// 
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

// Geometry-only renderer; egui 0.36.2 colour/texture conventions (MIT OR Apache-2.0).
// Camera mapping is relative to the callback viewport, retaining cached pixel snapping.
struct Camera { viewport: vec4<f32>, transform: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vertex(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> Output {
    var result: Output;
    let transform = camera.transform;
    let screen = position * transform.x + transform.yz;
    let local = (screen - camera.viewport.zw) / camera.viewport.xy;
    result.position = vec4<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0, 0.0, 1.0);
    result.uv = uv;
    result.color = color;
    return result;
}
fn gamma_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), c / 12.92, c < vec3<f32>(0.04045));
}
@fragment fn fragment(input: Output) -> @location(0) vec4<f32> {
    let sampled = input.color * textureSample(atlas, atlas_sampler, input.uv);
    // Same interleaved gradient noise used by egui; keep edge quantization consistent.
    let seed = fract(dot(input.position.xy, vec2<f32>(0.06711056, 0.00583715)));
    let noise = (fract(52.9829189 * seed) - 0.5) * 0.95 / 255.0;
    let gamma = sampled.rgb + vec3<f32>(noise);
    if LINEAR_TARGET { return vec4<f32>(gamma_to_linear(gamma), sampled.a); }
    return vec4<f32>(gamma, sampled.a);
}
