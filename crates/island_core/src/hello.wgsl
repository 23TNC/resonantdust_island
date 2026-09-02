// Hello-world shader: a spinning, colour-cycling triangle.
//
// There is no vertex buffer. The three corners are generated from
// `@builtin(vertex_index)`, which keeps the hello-world path free of buffer
// upload machinery — the only thing crossing to the GPU per frame is 16 bytes
// of uniform.

struct Uniforms {
    // Seconds since the first frame.
    time: f32,
    // Viewport width / height, so the triangle stays equilateral instead of
    // stretching with the window.
    aspect: f32,
    // Uniform-address-space structs must be a multiple of 16 bytes.
    padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

const TAU_OVER_3: f32 = 2.0943951;
const RADIUS: f32 = 0.55;

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    // Place corner `idx` a third of the way around a circle, and rotate the
    // whole thing over time.
    let angle = u.time * 0.6 + f32(idx) * TAU_OVER_3;
    var pos = vec2<f32>(cos(angle), sin(angle)) * RADIUS;

    // Squash x rather than stretch y, so the shape never exceeds the viewport.
    pos.x = pos.x / max(u.aspect, 0.0001);

    var out: VsOut;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);

    // Cycle each corner through the colour wheel on its own phase. Motion plus
    // colour change means a single screenshot can prove the loop is running.
    let phase = vec3<f32>(0.0, TAU_OVER_3, 2.0 * TAU_OVER_3);
    out.color = 0.5 + 0.5 * cos(vec3<f32>(u.time) + phase + f32(idx));

    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
