//! Camera conventions and constants.
//!
//! The camera never rotates: fixed pitch, fixed yaw, orthographic. That is a
//! deliberate constraint, not a limitation waiting to be lifted — see
//! `docs/architecture/coordinates.md`. It buys a constant billboard rotation, a
//! constant light direction, and trivial culling.
//!
//! See `docs/architecture/coordinates.md` for the axis conventions this
//! depends on.

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
/// # Decided: 30°
///
/// Chosen from rendered comparisons of the same seed at 30°, 45° and 60°
/// (unit `0002`, group F), not from the table above.
///
/// What the pictures showed: cliff faces are clearly legible at 30° and have
/// nearly vanished by 60°, exactly as `cos θ` predicts — and cliff faces are
/// what make a stepped heightmap read as terrain at all. 30° also matches the
/// Mad Island reference and costs an upright billboard only 13% of its height
/// against 50% at 60°.
///
/// Changing this after sprite art exists invalidates the art. It is settled.
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

use glam::{Mat4, Vec3};

/// How far back the camera sits from its focus point, in world units.
///
/// Under an orthographic projection this does not affect the image at all —
/// only which geometry falls between the near and far planes. Set generously
/// so terrain never clips, and paired with [`FAR_PLANE`] below.
const EYE_DISTANCE: f32 = 1000.0;
const NEAR_PLANE: f32 = 1.0;
const FAR_PLANE: f32 = 2000.0;

/// A fixed-orientation orthographic camera.
///
/// Only the focus point and the zoom ever change; pitch and yaw are compile-
/// time constants. That is what makes billboard orientation a single constant
/// in the next unit rather than per-object work.
#[derive(Copy, Clone, Debug)]
pub struct Camera {
    /// The world point the view is centred on.
    pub focus: Vec3,
    /// Half the height of the visible area, in world units. Smaller is more
    /// zoomed in. At 20.0 roughly 40 tiles of height are visible.
    pub half_height: f32,
    /// Viewport width / height.
    pub aspect: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            half_height: 44.0,
            aspect: 16.0 / 9.0,
        }
    }
}

impl Camera {
    /// Unit vector pointing from the camera toward its focus.
    ///
    /// Trigonometry is fine here. The no-transcendentals rule applies to world
    /// *generation*, where a platform difference would change the world; the
    /// camera only affects what is drawn, and both platforms draw the same
    /// world from the same matrix.
    pub fn forward() -> Vec3 {
        let pitch = CAMERA_PITCH_DEGREES.to_radians();
        let yaw = CAMERA_YAW_DEGREES.to_radians();
        // Pitch is measured up from the ground plane, so a larger pitch means
        // looking more steeply downward.
        Vec3::new(
            -yaw.sin() * pitch.cos(),
            -pitch.sin(),
            -yaw.cos() * pitch.cos(),
        )
        .normalize()
    }

    /// Where the camera sits. Derived, never set directly.
    pub fn eye(&self) -> Vec3 {
        self.focus - Self::forward() * EYE_DISTANCE
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.focus, Vec3::Y)
    }

    pub fn projection(&self) -> Mat4 {
        let half_width = self.half_height * self.aspect.max(0.01);
        orthographic_projection(
            -half_width,
            half_width,
            -self.half_height,
            self.half_height,
            NEAR_PLANE,
            FAR_PLANE,
        )
    }

    pub fn view_projection(&self) -> Mat4 {
        self.projection() * self.view()
    }

    /// Is any part of this world-space box potentially visible?
    ///
    /// Projects all eight corners and rejects the box only if every one falls
    /// outside the same clip plane. Conservative — it can keep a box that is
    /// not actually visible, never discard one that is — which is the correct
    /// direction to err for culling.
    pub fn is_box_visible(&self, min: Vec3, max: Vec3) -> bool {
        let view_proj = self.view_projection();

        let mut outside_left = true;
        let mut outside_right = true;
        let mut outside_below = true;
        let mut outside_above = true;
        let mut outside_near = true;
        let mut outside_far = true;

        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            );
            let clip = view_proj * corner.extend(1.0);

            // Orthographic keeps w at 1, so clip coordinates are already NDC.
            outside_left &= clip.x < -1.0;
            outside_right &= clip.x > 1.0;
            outside_below &= clip.y < -1.0;
            outside_above &= clip.y > 1.0;
            // wgpu depth range is 0..1, not -1..1.
            outside_near &= clip.z < 0.0;
            outside_far &= clip.z > 1.0;
        }

        !(outside_left
            || outside_right
            || outside_below
            || outside_above
            || outside_near
            || outside_far)
    }
}
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

#[cfg(test)]
mod camera_tests {
    use super::*;

    fn cam() -> Camera {
        Camera {
            focus: Vec3::new(100.0, 0.0, 100.0),
            half_height: 20.0,
            aspect: 16.0 / 9.0,
        }
    }

    /// Pitch is measured up from the ground, so the camera must look downward
    /// and toward -Z at yaw 0. Getting the sign wrong gives a camera under the
    /// world looking up, which renders a plausible-looking but inverted scene.
    #[test]
    fn camera_looks_down_and_along_negative_z() {
        let f = Camera::forward();
        assert!(f.y < 0.0, "camera must look downward, got {f:?}");
        assert!(
            f.z < 0.0,
            "at yaw 0 the camera must look along -Z, got {f:?}"
        );
        assert!(
            f.x.abs() < 1e-6,
            "at yaw 0 there should be no sideways component"
        );
        assert!(
            (f.length() - 1.0).abs() < 1e-6,
            "forward must be normalised"
        );
    }

    /// The declared pitch must be the actual pitch. A camera whose real angle
    /// differs from `CAMERA_PITCH_DEGREES` would silently invalidate the art
    /// the constant is supposed to pin down.
    #[test]
    fn forward_matches_the_declared_pitch() {
        let f = Camera::forward();
        let pitch = (-f.y).asin().to_degrees();
        assert!(
            (pitch - CAMERA_PITCH_DEGREES).abs() < 1e-3,
            "forward vector is {pitch}° above horizontal, constant says {CAMERA_PITCH_DEGREES}°"
        );
    }

    #[test]
    fn eye_sits_above_and_behind_the_focus() {
        let c = cam();
        let eye = c.eye();
        assert!(eye.y > c.focus.y, "camera should be above its focus");
        assert!(
            eye.z > c.focus.z,
            "at yaw 0 the camera should be on the +Z side"
        );
    }

    /// The focus point must land in the centre of the screen.
    #[test]
    fn focus_projects_to_the_centre_of_the_view() {
        let c = cam();
        let clip = c.view_projection() * c.focus.extend(1.0);
        assert!(
            clip.x.abs() < 1e-4,
            "focus x should be centred, got {}",
            clip.x
        );
        assert!(
            clip.y.abs() < 1e-4,
            "focus y should be centred, got {}",
            clip.y
        );
        assert!(
            (0.0..=1.0).contains(&clip.z),
            "focus must be within the depth range"
        );
    }

    /// Higher ground must draw higher on screen, or the pitch sign is wrong.
    #[test]
    fn higher_ground_appears_higher_on_screen() {
        let c = cam();
        let vp = c.view_projection();
        let low = vp * Vec3::new(100.0, 0.0, 100.0).extend(1.0);
        let high = vp * Vec3::new(100.0, 10.0, 100.0).extend(1.0);
        assert!(
            high.y > low.y,
            "raising a point should move it up the screen"
        );
    }

    /// Ground further from the camera must draw higher on screen, which is
    /// what makes a pitched top-down view readable at all.
    #[test]
    fn distant_ground_appears_higher_on_screen() {
        let c = cam();
        let vp = c.view_projection();
        let near = vp * Vec3::new(100.0, 0.0, 110.0).extend(1.0);
        let far = vp * Vec3::new(100.0, 0.0, 90.0).extend(1.0);
        assert!(far.y > near.y, "-Z is further away and should draw higher");
        assert!(
            far.z > near.z,
            "further ground must be deeper in the depth buffer"
        );
    }

    #[test]
    fn zoom_changes_how_much_world_is_visible() {
        let mut c = cam();
        let probe = Vec3::new(100.0, 0.0, 130.0);

        c.half_height = 40.0;
        let wide = c.view_projection() * probe.extend(1.0);
        c.half_height = 10.0;
        let tight = c.view_projection() * probe.extend(1.0);

        assert!(
            tight.y.abs() > wide.y.abs(),
            "zooming in should push a fixed point further toward the screen edge"
        );
    }

    // ---- Culling ----

    #[test]
    fn a_box_at_the_focus_is_visible() {
        let c = cam();
        assert!(c.is_box_visible(Vec3::new(95.0, -2.0, 95.0), Vec3::new(105.0, 2.0, 105.0)));
    }

    #[test]
    fn a_box_far_off_to_the_side_is_culled() {
        let c = cam();
        assert!(!c.is_box_visible(Vec3::new(5000.0, -2.0, 95.0), Vec3::new(5010.0, 2.0, 105.0)));
        assert!(!c.is_box_visible(
            Vec3::new(95.0, -2.0, -5000.0),
            Vec3::new(105.0, 2.0, -4990.0)
        ));
    }

    /// Culling must never discard something partly on screen. A box straddling
    /// the edge is the case a naive centre-point test gets wrong.
    #[test]
    fn a_box_straddling_the_screen_edge_is_kept() {
        let c = cam();
        // Wide enough to run off both sides of the view.
        assert!(c.is_box_visible(Vec3::new(-500.0, -2.0, 95.0), Vec3::new(700.0, 2.0, 105.0)));
    }

    /// Sanity check against the real thing: culling a generated world from its
    /// centre must keep some chunks and drop others. Keeping everything means
    /// culling does nothing; keeping nothing means it is broken.
    #[test]
    fn culling_keeps_some_chunks_and_drops_others() {
        use crate::world::WorldParams;
        let map = WorldParams::default().generate();
        let c = Camera {
            focus: Vec3::new(map.width() as f32 * 0.5, 0.0, map.depth() as f32 * 0.5),
            half_height: 44.0,
            aspect: 16.0 / 9.0,
        };

        let (mut kept, mut dropped) = (0, 0);
        for chunk in map.chunks() {
            let (min, max) = map.chunk_bounds(chunk).unwrap();
            if c.is_box_visible(min, max) {
                kept += 1;
            } else {
                dropped += 1;
            }
        }
        assert!(kept > 0, "culling discarded the entire world");
        assert!(
            dropped > 0,
            "culling kept every chunk; it is not doing anything"
        );
    }
}
