//! Camera conventions and constants.
//!
//! The camera never rotates: fixed pitch, fixed yaw, orthographic. That is a
//! deliberate constraint, not a limitation waiting to be lifted — see
//! `docs/architecture/coordinates.md`. It buys a constant billboard rotation, a
//! constant light direction, and trivial culling.
//!
//! At this stage this module holds only the conventions. The camera itself
//! arrives with the terrain renderer.

/// Camera pitch, in **degrees above the horizontal ground plane**.
///
/// The unit matters and is routinely ambiguous: "a 30 degree camera" can mean
/// 30° up from the ground (side-on) or 30° over from straight down (nearly
/// top-down), and those differ by 60°. Here it is always measured **up from
/// the ground plane**, so a smaller number is a more side-on view.
///
/// # This value is expensive to change later
///
/// Billboard art is drawn for one viewing angle. Once sprites exist, changing
/// the pitch makes every one of them subtly wrong — objects lean, float, or
/// sink into the ground. Right now it costs one constant.
///
/// # What the angle trades off
///
/// Under an orthographic camera at pitch θ, per world unit:
///
/// | | screen height | at 30° | at 45° | at 60° |
/// |---|---|---|---|---|
/// | 1 unit of ground depth | `sin θ` | 0.50 | 0.71 | 0.87 |
/// | 1 unit of cliff face   | `cos θ` | 0.87 | 0.71 | 0.50 |
/// | upright billboard      | `cos θ` | 0.87 | 0.71 | 0.50 |
///
/// So a lower pitch shows cliff faces and standing objects nearly full height
/// but foreshortens the ground badly; a higher pitch shows the map clearly but
/// squashes anything standing up. An upright billboard loses 13% of its height
/// at 30° and **half** of it at 60°, which artists must otherwise compensate
/// for by pre-stretching every sprite.
///
/// # Provisional
///
/// 30° matches the Mad Island reference and keeps billboard squash low. It is
/// **not final**: the committing decision is made from rendered comparisons of
/// the same seed once terrain exists, because this is a thing to judge by eye,
/// not by table.
pub const CAMERA_PITCH_DEGREES: f32 = 30.0;

/// Camera yaw, in degrees, about the Y axis.
///
/// Zero: the camera looks along **-Z**, so the tile grid stays axis-aligned on
/// screen — world X runs left-to-right, world Z runs up the screen. No
/// isometric diamond skew.
///
/// Axis-aligned is the right default for a tile game. It keeps tile-to-pixel
/// mapping simple, makes UI and mouse-picking straightforward, and means art
/// does not have to be drawn on a diagonal.
pub const CAMERA_YAW_DEGREES: f32 = 0.0;

/// The orthographic projection to use with wgpu.
///
/// glam groups projections by graphics-API convention, and the obvious choice
/// is the wrong one:
///
/// | module | NDC Z | NDC Y |
/// |---|---|---|
/// | `opengl` | `-1..1` | up |
/// | `vulkan` | `0..1` | **down** |
/// | `directx` | `0..1` | up |
///
/// wgpu's clip space is Z `0..1` with **Y up**, so `directx` is correct — even
/// though `vulkan` is the one that sounds right for a modern GPU API. glam's
/// own doc comment on it reads "for use with DirectX and WebGPU".
///
/// Aliased here so exactly one place in the codebase makes this choice.
pub use glam::camera::rh::proj::directx::orthographic as orthographic_projection;

#[cfg(test)]
mod tests {
    use glam::Vec3;

    /// wgpu's clip space puts depth in `0..1`, unlike OpenGL's `-1..1`.
    ///
    /// Pinned by test rather than comment so a glam upgrade that changed the
    /// convention fails the build instead of quietly changing the picture.
    #[test]
    fn orthographic_projection_matches_wgpu_depth_range() {
        let proj = super::orthographic_projection(-1.0, 1.0, -1.0, 1.0, 0.0, 100.0);

        // Right-handed: the camera looks down -Z, so the near plane is at
        // z = 0 and the far plane at z = -100.
        let near = proj.project_point3(Vec3::new(0.0, 0.0, 0.0));
        let far = proj.project_point3(Vec3::new(0.0, 0.0, -100.0));

        assert!(
            (near.z - 0.0).abs() < 1e-6,
            "near plane should map to z=0 for wgpu, got {}",
            near.z
        );
        assert!(
            (far.z - 1.0).abs() < 1e-6,
            "far plane should map to z=1 for wgpu, got {}",
            far.z
        );
    }

    /// The OpenGL convention is the one we must not use. Asserted so the
    /// distinction is documented by a test that fails if swapped.
    #[test]
    fn opengl_projection_is_the_wrong_one() {
        let proj = glam::camera::rh::proj::opengl::orthographic(-1.0, 1.0, -1.0, 1.0, 0.0, 100.0);
        let near = proj.project_point3(Vec3::new(0.0, 0.0, 0.0));
        assert!(
            (near.z - -1.0).abs() < 1e-6,
            "the OpenGL convention maps near to -1; if this changed, re-check \
             which projection the renderer uses (got {})",
            near.z
        );
    }

    /// The Vulkan convention shares wgpu's depth range but flips Y, which
    /// would render the world upside down while every depth test still
    /// passed. Named explicitly because it is the tempting wrong choice.
    #[test]
    fn vulkan_projection_flips_y_and_is_also_wrong() {
        let vk = glam::camera::rh::proj::vulkan::orthographic(-10.0, 10.0, -10.0, 10.0, 0.0, 100.0);
        let dx = super::orthographic_projection(-10.0, 10.0, -10.0, 10.0, 0.0, 100.0);
        let p = Vec3::new(0.0, 5.0, -1.0);
        assert!(
            vk.project_point3(p).y < 0.0 && dx.project_point3(p).y > 0.0,
            "vulkan should map +Y down and directx +Y up"
        );
    }

    /// Y must point up on screen after projection, or the world renders
    /// upside down — a mistake that is surprisingly easy to "fix" in the
    /// wrong place once art exists.
    #[test]
    fn positive_y_is_up_in_clip_space() {
        let proj = super::orthographic_projection(-10.0, 10.0, -10.0, 10.0, 0.0, 100.0);
        let low = proj.project_point3(Vec3::new(0.0, -5.0, -1.0));
        let high = proj.project_point3(Vec3::new(0.0, 5.0, -1.0));
        assert!(high.y > low.y, "higher world Y must give higher clip Y");
    }
}
