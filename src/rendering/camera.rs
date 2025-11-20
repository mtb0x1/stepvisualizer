use crate::trace_span;
#[derive(Clone, PartialEq, Debug)]
pub struct CameraState {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: [f32; 3],
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            azimuth: 0.5,
            elevation: 0.5,
            //wtf
            //FIXME: distance is way too big
            // should be around 10-20
            // or maybe it should be computed based on the model size?
            distance: 500.0,
            target: [0.0, 0.0, 0.0],
        }
    }
}

pub fn compute_eye_position(camera: &CameraState) -> [f32; 3] {
    trace_span!("compute_eye_position");
    let azimuth = camera.azimuth;
    let elevation = camera.elevation;
    let distance = camera.distance;

    let eye_x = camera.target[0] + distance * azimuth.cos() * elevation.cos();
    let eye_y = camera.target[1] + distance * elevation.sin();
    let eye_z = camera.target[2] + distance * azimuth.sin() * elevation.cos();

    [eye_x, eye_y, eye_z]
}
