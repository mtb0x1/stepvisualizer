use glam::DVec3;

pub use crate::common::utils::spherical_to_cartesian;

/// Orbit camera: azimuth/elevation (radians) and distance around a target
/// point. Dragging mutates the angles, zooming the distance; the target is
/// normally the model center and never moves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CameraState {
    pub azimuth: f64,
    pub elevation: f64,
    pub distance: f64,
    pub target: DVec3,
}

impl CameraState {
    /// Default view angles and distance; `Default` and the "Reset" toolbar
    /// preset both read from here so there is a single source of truth.
    pub const DEFAULT: Self = Self {
        azimuth: 0.5,
        elevation: 0.5,
        distance: 3.0,
        target: DVec3::ZERO,
    };

    /// Computes the 3D eye position in world space for this orbit camera.
    pub fn eye_position(&self) -> DVec3 {
        spherical_to_cartesian(self.azimuth, self.elevation, self.distance, self.target)
    }

    /// Rotate the camera around the target using mouse delta coordinates in pixels.
    pub fn orbit(&self, delta_x: f64, delta_y: f64) -> Self {
        const MAX_ELEVATION: f64 = std::f64::consts::FRAC_PI_2 - 0.001;
        const CAMERA_SENSITIVITY: f64 = 0.01;
        Self {
            azimuth: self.azimuth - delta_x * CAMERA_SENSITIVITY,
            elevation: (self.elevation - delta_y * CAMERA_SENSITIVITY)
                .clamp(-MAX_ELEVATION, MAX_ELEVATION),
            distance: self.distance,
            target: self.target,
        }
    }

    /// Zoom camera distance by a multiplicative factor (clamped to positive distances).
    pub fn zoom(&self, factor: f64) -> Self {
        Self {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: (self.distance * factor).max(0.01),
            target: self.target,
        }
    }

    /// Pan the camera along its view plane by screen delta coordinates in pixels.
    ///
    /// Translates both eye and target so the object stays under the mouse cursor.
    pub fn pan(
        &self,
        delta_x: f64,
        delta_y: f64,
        canvas_size: crate::common::types::ViewportSize,
    ) -> Self {
        let eye = self.eye_position();
        let forward = (self.target - eye).normalize_or(DVec3::NEG_Z);
        let right = forward.cross(DVec3::Y).normalize_or(DVec3::X);
        let up = right.cross(forward).normalize_or(DVec3::Y);

        // Vertical field-of-view matching the perspective projection in renderer.
        const FOV_Y: f64 = std::f64::consts::FRAC_PI_3;
        let viewport_height = (canvas_size.height as f64).max(1.0);
        let factor = 2.0 * (FOV_Y * 0.5).tan() * self.distance / viewport_height;

        let offset = (-right * delta_x + up * delta_y) * factor;
        Self {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: self.distance,
            target: self.target + offset,
        }
    }

    /// Sets a new orbit target (pivot point) while preserving the current eye position in world space.
    pub fn set_target(&self, new_target: DVec3) -> Self {
        let eye = self.eye_position();
        let diff = eye - new_target;
        let new_distance = diff.length().max(0.01);
        let dir = diff / new_distance;

        let new_elevation = dir.y.clamp(-1.0, 1.0).asin();
        let new_azimuth = dir.z.atan2(dir.x);

        Self {
            azimuth: new_azimuth,
            elevation: new_elevation,
            distance: new_distance,
            target: new_target,
        }
    }
}

impl Default for CameraState {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Named view preset for the viewer toolbar.
#[derive(Clone, Copy)]
pub struct CameraPreset {
    pub label: &'static str,
    pub azimuth: f64,
    pub elevation: f64,
    pub distance: f64,
}

/// Toolbar presets, in display order.
pub const CAMERA_PRESETS: [CameraPreset; 4] = [
    CameraPreset {
        label: "Reset",
        azimuth: CameraState::DEFAULT.azimuth,
        elevation: CameraState::DEFAULT.elevation,
        distance: CameraState::DEFAULT.distance,
    },
    CameraPreset {
        label: "Iso",
        azimuth: 0.8,
        elevation: 0.9,
        distance: 3.0,
    },
    CameraPreset {
        label: "Top",
        azimuth: 0.0,
        elevation: 1.3,
        distance: 2.5,
    },
    CameraPreset {
        label: "Front",
        azimuth: 0.0,
        elevation: 0.0,
        distance: 3.0,
    },
];

impl CameraPreset {
    /// Camera with this preset's angles and distance, keeping the current
    /// target (presets never move the orbit center).
    pub const fn apply(&self, current: &CameraState) -> CameraState {
        CameraState {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: self.distance,
            target: current.target,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_camera_default() {
        let camera = CameraState::default();
        assert_eq!(camera.target, DVec3::ZERO);
        assert_eq!(camera.distance, 3.0);
        assert_eq!(camera.azimuth, 0.5);
        assert_eq!(camera.elevation, 0.5);
    }

    #[wasm_bindgen_test]
    fn test_spherical_to_cartesian() {
        let pos = spherical_to_cartesian(0.0, 0.0, 5.0, DVec3::ZERO);
        approx::assert_relative_eq!(pos.x, 5.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.y, 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(pos.z, 0.0, epsilon = 1e-6);
    }

    #[wasm_bindgen_test]
    fn test_camera_orbit_and_zoom() {
        let camera = CameraState::default();
        let orbited = camera.orbit(10.0, 5.0);
        assert_eq!(orbited.azimuth, 0.5 - 0.1);
        assert_eq!(orbited.elevation, 0.5 - 0.05);

        let zoomed = camera.zoom(2.0);
        assert_eq!(zoomed.distance, 6.0);
    }

    #[wasm_bindgen_test]
    fn test_camera_pan() {
        let camera = CameraState::default();
        let eye_before = camera.eye_position();
        let target_before = camera.target;
        let canvas_size = crate::common::types::ViewportSize::new(800, 600);
        let panned = camera.pan(10.0, 20.0, canvas_size);
        let eye_after = panned.eye_position();
        let target_after = panned.target;

        approx::assert_relative_eq!(
            (eye_before - target_before).x,
            (eye_after - target_after).x,
            epsilon = 1e-6
        );
        approx::assert_relative_eq!(
            (eye_before - target_before).y,
            (eye_after - target_after).y,
            epsilon = 1e-6
        );
        approx::assert_relative_eq!(
            (eye_before - target_before).z,
            (eye_after - target_after).z,
            epsilon = 1e-6
        );
        assert_eq!(camera.distance, panned.distance);
        assert_eq!(camera.azimuth, panned.azimuth);
        assert_eq!(camera.elevation, panned.elevation);
    }

    #[wasm_bindgen_test]
    fn test_camera_set_target() {
        let camera = CameraState::default();
        let eye_before = camera.eye_position();
        let new_target = DVec3::new(1.0, 2.0, -1.0);
        let retargeted = camera.set_target(new_target);
        let eye_after = retargeted.eye_position();

        approx::assert_relative_eq!(eye_before.x, eye_after.x, epsilon = 1e-5);
        approx::assert_relative_eq!(eye_before.y, eye_after.y, epsilon = 1e-5);
        approx::assert_relative_eq!(eye_before.z, eye_after.z, epsilon = 1e-5);
        assert_eq!(retargeted.target, new_target);
    }
}
