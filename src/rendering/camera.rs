/// Converts spherical coordinates (azimuth, elevation, distance) around a
/// `target` center into Cartesian 3D coordinates `[x, y, z]`.
#[inline(always)]
pub fn spherical_to_cartesian(
    azimuth: f32,
    elevation: f32,
    distance: f32,
    target: [f32; 3],
) -> [f32; 3] {
    [
        target[0] + distance * azimuth.cos() * elevation.cos(),
        target[1] + distance * elevation.sin(),
        target[2] + distance * azimuth.sin() * elevation.cos(),
    ]
}

/// Orbit camera: azimuth/elevation (radians) and distance around a target
/// point. Dragging mutates the angles, zooming the distance; the target is
/// normally the model center and never moves.
#[derive(Clone, PartialEq, Debug)]
pub struct CameraState {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: [f32; 3],
}

impl CameraState {
    /// Default view angles and distance; `Default` and the "Reset" toolbar
    /// preset both read from here so there is a single source of truth.
    pub const DEFAULT: Self = Self {
        azimuth: 0.5,
        elevation: 0.5,
        // Framed relative to the origin-centered model (size ~1). The far
        // plane is `max_size * 100`, so an eye this far out would clip the
        // model away; the camera presets use the same 2.5-3.0 range.
        distance: 3.0,
        target: [0.0, 0.0, 0.0],
    };

    /// Computes the 3D eye position in world space for this orbit camera.
    pub fn eye_position(&self) -> [f32; 3] {
        spherical_to_cartesian(self.azimuth, self.elevation, self.distance, self.target)
    }

    /// Rotate the camera around the target using mouse delta coordinates in pixels.
    pub fn orbit(&self, delta_x: f32, delta_y: f32) -> Self {
        let max_elev = std::f32::consts::FRAC_PI_2 - 0.001;
        let mut next = self.clone();
        next.azimuth -= delta_x * 0.01;
        next.elevation = (next.elevation - delta_y * 0.01).clamp(-max_elev, max_elev);
        next
    }

    /// Zoom camera distance by a multiplicative factor (clamped to positive distances).
    pub fn zoom(&self, factor: f32) -> Self {
        let mut next = self.clone();
        next.distance = (next.distance * factor).max(0.01);
        next
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
    pub fn apply(&self, current: &CameraState) -> CameraState {
        CameraState {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: self.distance,
            target: current.target,
        }
    }
}
