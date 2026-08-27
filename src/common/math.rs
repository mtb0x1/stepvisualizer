use core::arch::wasm32::*;

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

// @todo use of [f32; 4] the forth slot is padding,
// this might yield a better wasm code using simd
// https://rust.godbolt.org/z/sWGW7cq5s
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
    // @todo remove the block if tests are okay
    if false {
        let mut r = [0.0; 16];

        for col in 0..4 {
            let b0 = b[col * 4];
            let b1 = b[col * 4 + 1];
            let b2 = b[col * 4 + 2];
            let b3 = b[col * 4 + 3];

            r[col * 4] = a[0] * b0 + a[4] * b1 + a[8] * b2 + a[12] * b3;
            r[col * 4 + 1] = a[1] * b0 + a[5] * b1 + a[9] * b2 + a[13] * b3;
            r[col * 4 + 2] = a[2] * b0 + a[6] * b1 + a[10] * b2 + a[14] * b3;
            r[col * 4 + 3] = a[3] * b0 + a[7] * b1 + a[11] * b2 + a[15] * b3;
        }

        r
    } else {
        // god this might not be safe, we livin on the Edge (not microsft :D) 
        unsafe { *as_f32x16(&multiply_matrices_manual_simd(as_v128x4(a), as_v128x4(b))) }
    }
}

#[inline(always)]
pub fn as_v128x4(m: &[f32; 16]) -> &[v128; 4] {
    unsafe { core::mem::transmute::<&[f32; 16], &[v128; 4]>(m) }
}

pub fn as_f32x16(m: &[v128; 4]) -> &[f32; 16] {
    unsafe { core::mem::transmute::<&[v128; 4], &[f32; 16]>(m) }
}

#[inline(always)]
unsafe fn multiply_matrices_manual_simd(a: &[v128; 4], b: &[v128; 4]) -> [v128; 4] {
    let mut out : [v128; 4] = [f32x4_splat(0.0); 4];

    // @todo: use a fully unrolled code 
    // i8x16_shuffle, f32x4_add, f32x4_mul ?
    for i in 0..4 {
        let x = f32x4_extract_lane::<0>(b[i]);
        let y = f32x4_extract_lane::<1>(b[i]);
        let z = f32x4_extract_lane::<2>(b[i]);
        let w = f32x4_extract_lane::<3>(b[i]);

        let mut r = f32x4_mul(a[0], f32x4_splat(x));
        r = f32x4_add(r, f32x4_mul(a[1], f32x4_splat(y)));
        r = f32x4_add(r, f32x4_mul(a[2], f32x4_splat(z)));
        r = f32x4_add(r, f32x4_mul(a[3], f32x4_splat(w)));

        out[i] = r;
    }

    out
}
