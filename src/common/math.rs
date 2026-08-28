//! Column-major 4×4 matrix math matching WGSL's `mat4x4` layout (transform
//! application is `M * v`, and compositions read right-to-left). The
//! multiply is hand-vectorized with wasm128 SIMD on the wasm target.
use core::arch::wasm32::*;

/// Standard perspective projection (symmetric frustum), column-major.
/// `fov_y` is the vertical field of view in radians; `aspect` is
/// width / height.
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

/// Cross product of two 3-vectors carried in the low 3 lanes of `a`/`b`
/// (lane 3 is ignored). Standard swizzle idiom: `(a.yzx * b.zxy) - (a.zxy *
/// b.yzx)`.
#[inline(always)]
fn cross3(a: v128, b: v128) -> v128 {
    let a_yzx = i32x4_shuffle::<1, 2, 0, 3>(a, a);
    let a_zxy = i32x4_shuffle::<2, 0, 1, 3>(a, a);
    let b_yzx = i32x4_shuffle::<1, 2, 0, 3>(b, b);
    let b_zxy = i32x4_shuffle::<2, 0, 1, 3>(b, b);
    f32x4_sub(f32x4_mul(a_yzx, b_zxy), f32x4_mul(a_zxy, b_yzx))
}

/// Dot product of the low 3 lanes (lane 3 ignored), returned as an `f32`.
///
/// `f32x4.dot` lives behind the relaxed-SIMD surface we don't enable, so the
/// horizontal reduction is done by hand: `mul`, then two add-shuffles fold the
/// four lanes into one. Lane 3 is always 0, so the fold yields `a0*b0 +
/// a1*b1 + a2*b2`.
#[inline(always)]
fn dot3(a: v128, b: v128) -> f32 {
    let v = f32x4_mul(a, b);
    // Fold left-to-right (x+y first, then +z) to match the scalar
    // `a*a + b*b + c*c` evaluation exactly, so results are bit-identical.
    let v1 = f32x4_add(v, i32x4_shuffle::<1, 0, 3, 2>(v, v));
    let v2 = f32x4_add(v1, i32x4_shuffle::<2, 3, 0, 1>(v1, v1));
    f32x4_extract_lane::<0>(v2)
}

/// View matrix placing the camera at `eye` and looking at `center`, with
/// `up` as the world-space up direction. Column-major.
///
/// Hand-vectorized: direction subtraction, the two vector normalizations
/// (`f32x4_dot` + `sqrt`), both cross products (lane shuffles) and the three
/// translation dot products all run through wasm128 SIMD. Arithmetic is
/// identical to the scalar formulation, so the result is unchanged.
#[inline(always)]
pub fn create_look_at_matrix(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let eye_v = f32x4(eye[0], eye[1], eye[2], 0.0);
    let center_v = f32x4(center[0], center[1], center[2], 0.0);
    let up_v = f32x4(up[0], up[1], up[2], 0.0);

    // forward = normalize(center - eye)
    let f = f32x4_sub(center_v, eye_v);
    let f = f32x4_div(f, f32x4_splat(dot3(f, f).sqrt()));

    // right = normalize(cross(forward, up))
    let s = cross3(f, up_v);
    let s = f32x4_div(s, f32x4_splat(dot3(s, s).sqrt()));

    // true up = cross(right, forward)
    let u = cross3(s, f);

    // translation: -(basis_row · eye). third row is -forward, so its term is
    // +(forward · eye).
    let tx = -dot3(s, eye_v);
    let ty = -dot3(u, eye_v);
    let tz = dot3(f, eye_v);

    // Column-major: columns are (s,0), (u,0), (-f,0), (t,1).
    let c0 = f32x4(
        f32x4_extract_lane::<0>(s),
        f32x4_extract_lane::<1>(s),
        f32x4_extract_lane::<2>(s),
        0.0,
    );
    let c1 = f32x4(
        f32x4_extract_lane::<0>(u),
        f32x4_extract_lane::<1>(u),
        f32x4_extract_lane::<2>(u),
        0.0,
    );
    let c2 = f32x4(
        -f32x4_extract_lane::<0>(f),
        -f32x4_extract_lane::<1>(f),
        -f32x4_extract_lane::<2>(f),
        0.0,
    );
    let c3 = f32x4(tx, ty, tz, 1.0);

    // By-value transmute (same reasoning as `multiply_matrices`): no reference
    // is formed, so the caller's alignment is irrelevant. `[v128; 4]` and
    // `[f32; 16]` are both 64 bytes.
    unsafe { core::mem::transmute::<[v128; 4], [f32; 16]>([c0, c1, c2, c3]) }
}

/// Column-major `a × b`, hand-vectorized with wasm128 SIMD.
#[inline(always)]
pub fn multiply_matrices(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    // Each column of a column-major matrix is stored contiguously, so column
    // `c` is the v128 `(m[c], m[4 + c], m[8 + c], m[12 + c])`.
    let a_cols = [
        f32x4(a[0], a[4], a[8], a[12]),
        f32x4(a[1], a[5], a[9], a[13]),
        f32x4(a[2], a[6], a[10], a[14]),
        f32x4(a[3], a[7], a[11], a[15]),
    ];
    let b_cols = [
        f32x4(b[0], b[4], b[8], b[12]),
        f32x4(b[1], b[5], b[9], b[13]),
        f32x4(b[2], b[6], b[10], b[14]),
        f32x4(b[3], b[7], b[11], b[15]),
    ];

    let mut out = [f32x4_splat(0.0); 4];
    for i in 0..4 {
        // result column i = Σ_k a_column_k * b[i][k]; the four scalar
        // components of b column i broadcast across each a column.
        let bi = b_cols[i];
        let x = f32x4_extract_lane::<0>(bi);
        let y = f32x4_extract_lane::<1>(bi);
        let z = f32x4_extract_lane::<2>(bi);
        let w = f32x4_extract_lane::<3>(bi);

        out[i] = f32x4_add(
            f32x4_mul(a_cols[0], f32x4_splat(x)),
            f32x4_add(
                f32x4_mul(a_cols[1], f32x4_splat(y)),
                f32x4_add(
                    f32x4_mul(a_cols[2], f32x4_splat(z)),
                    f32x4_mul(a_cols[3], f32x4_splat(w)),
                ),
            ),
        );
    }

    // By-value transmute: no reference is formed, so the 4-byte alignment of
    // the caller's `[f32; 16]` is irrelevant to soundness — the v128 lanes
    // already live in properly-aligned registers/SSA values. `[v128; 4]` and
    // `[f32; 16]` are both 64 bytes.
    unsafe { core::mem::transmute::<[v128; 4], [f32; 16]>(out) }
}
