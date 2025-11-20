use crate::trace_span;
// meth iz hard
// fuck math
#[inline(always)]
pub fn create_perspective_matrix(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    trace_span!("create_perspective_matrix");
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
    trace_span!("create_look_at_matrix");
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
    trace_span!("multiply_matrices");
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

//FIXME : replace above, but first fix result,
// it doesn't yield same output as multiply_matrices
//TODO : fix using hints below
// A col major
// B row major
// output =  result[i*4 + j]
#[inline(always)]
pub fn multiply_matrices_unrolled(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];

    let (a00, a01, a02, a03, a10, a11, a12, a13, a20, a21, a22, a23, a30, a31, a32, a33) = (
        a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8], a[9], a[10], a[11], a[12], a[13],
        a[14], a[15],
    );

    for i in 0..4 {
        let bi0 = b[i * 4 + 0];
        let bi1 = b[i * 4 + 1];
        let bi2 = b[i * 4 + 2];
        let bi3 = b[i * 4 + 3];

        result[i + 0] = a00 * bi0 + a01 * bi1 + a02 * bi2 + a03 * bi3;
        result[i + 4] = a10 * bi0 + a11 * bi1 + a12 * bi2 + a13 * bi3;
        result[i + 8] = a20 * bi0 + a21 * bi1 + a22 * bi2 + a23 * bi3;
        result[i + 12] = a30 * bi0 + a31 * bi1 + a32 * bi2 + a33 * bi3;
    }

    result
}
