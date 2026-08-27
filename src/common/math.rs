#[inline(always)]
pub fn create_perspective_matrix(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y / 2.0).tan();
    let nf = 1.0 / (near - far);

    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) * nf,
        -1.0,
        0.0,
        0.0,
        (2.0 * far * near) * nf,
        0.0,
    ]
}

#[inline(always)]
pub fn create_look_at_matrix(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = [center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]];
    let f_len = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
    let f = [f[0] / f_len, f[1] / f_len, f[2] / f_len];

    let s = [
        f[1] * up[2] - f[2] * up[1],
        f[2] * up[0] - f[0] * up[2],
        f[0] * up[1] - f[1] * up[0],
    ];
    let s_len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
    let s = [s[0] / s_len, s[1] / s_len, s[2] / s_len];

    let u = [
        s[1] * f[2] - s[2] * f[1],
        s[2] * f[0] - s[0] * f[2],
        s[0] * f[1] - s[1] * f[0],
    ];

    let tx = -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]);
    let ty = -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]);
    let tz = -(-f[0] * eye[0] + -f[1] * eye[1] + -f[2] * eye[2]);

    [
        s[0], s[1], s[2], 0.0, u[0], u[1], u[2], 0.0, -f[0], -f[1], -f[2], 0.0, tx, ty, tz, 1.0,
    ]
}

#[inline(always)]
pub fn multiply_matrices(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + j] * b[i * 4 + k];
            }
            result[i * 4 + j] = sum;
        }
    }
    result
}
