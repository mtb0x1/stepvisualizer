use glam::Vec3;

pub use crate::common::utils::spherical_to_cartesian;

/// Orbit camera: azimuth/elevation (radians) and distance around a target
/// point. Dragging mutates the angles, zooming the distance; the target is
/// normally the model center and never moves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CameraState {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: Vec3,
}

impl CameraState {
    /// Default view angles and distance; `Default` and the "Reset" toolbar
    /// preset both read from here so there is a single source of truth.
    pub const DEFAULT: Self = Self {
        azimuth: 0.5,
        elevation: 0.5,
        distance: 3.0,
        target: Vec3::ZERO,
    };

    /// Computes the 3D eye position in world space for this orbit camera.
    pub fn eye_position(&self) -> Vec3 {
        spherical_to_cartesian(self.azimuth, self.elevation, self.distance, self.target)
    }

    /// Rotate the camera around the target using mouse delta coordinates in pixels.
    pub fn orbit(&self, delta_x: f32, delta_y: f32) -> Self {
        const MAX_ELEVATION: f32 = std::f32::consts::FRAC_PI_2 - 0.001;
        const CAMERA_SENSITIVITY: f32 = 0.01;
        Self {
            azimuth: self.azimuth - delta_x * CAMERA_SENSITIVITY,
            elevation: (self.elevation - delta_y * CAMERA_SENSITIVITY)
                .clamp(-MAX_ELEVATION, MAX_ELEVATION),
            distance: self.distance,
            target: self.target,
        }
    }

    /// Zoom camera distance by a multiplicative factor (clamped to positive distances).
    pub fn zoom(&self, factor: f32) -> Self {
        Self {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: (self.distance * factor).max(0.01),
            target: self.target,
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
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
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
        assert_eq!(camera.target, Vec3::ZERO);
        assert_eq!(camera.distance, 3.0);
        assert_eq!(camera.azimuth, 0.5);
        assert_eq!(camera.elevation, 0.5);
    }

    #[wasm_bindgen_test]
    fn test_spherical_to_cartesian() {
        let pos = spherical_to_cartesian(0.0, 0.0, 5.0, Vec3::ZERO);
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
}
