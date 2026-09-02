// Terrain shader: flat-shaded stepped tiles.
//
// Deliberately flat, not smooth. Each face carries one normal and the fragment
// shader does no interpolation trickery, so a step reads as a step. Smoothing
// the normals here would undo the whole point of stepped terrain.

struct Camera {
    view_proj: mat4x4<f32>,
    // Direction the light travels, normalised. Not straight down: a purely
    // vertical light leaves every wall on ambient alone, and the walls are
    // what make elevation legible in the first place.
    light_dir: vec3<f32>,
    // Floor brightness for faces turned away from the light, so a shadowed
    // cliff is dim rather than black.
    ambient: f32,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.normal = in.normal;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let normal = normalize(in.normal);
    let lambert = max(dot(normal, -camera.light_dir), 0.0);
    let light = camera.ambient + (1.0 - camera.ambient) * lambert;
    return vec4<f32>(in.color * light, 1.0);
}
