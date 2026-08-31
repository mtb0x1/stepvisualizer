//! Column-major 4×4 matrix math matching WGSL's `mat4x4` layout (transform
//! application is `M * v`, and compositions read right-to-left). The
//! multiply is hand-vectorized with wasm128 SIMD.
use core::arch::wasm32::*;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Column-major 4×4 matrix, matching WGSL's `mat4x4<f32>` layout. Wraps the raw
/// `[f32; 16]` so the type system distinguishes the model/view/projection
/// matrices from plain arrays and prevents passing the wrong matrix to the GPU
/// (a common, silent bug with bare `[f32; 16]`).
///
/// `repr(transparent)` keeps the on-the-wire layout identical to `[f32; 16]`,
/// so `bytemuck::bytes_of` yields the same 64 bytes and serde serializes it as
/// the flat 16-element array (preserving localStorage compatibility).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, PartialEq, Serialize, Deserialize)]
pub struct Mat4(pub [f32; 16]);

impl Mat4 {
    /// Identity matrix.
    pub const IDENTITY: Mat4 = Mat4([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
}

/// Standard perspective projection (symmetric frustum), column-major.
/// `fov_y` is the vertical field of view in radians; `aspect` is
/// width / height.
#[inline(always)]
pub fn create_perspective_matrix(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    let nf = 1.0 / (near - far);

    Mat4([
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
    ])
}

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

/// Computes the 3-component cross product of two vectors packed into the
/// low three lanes of the given `v128` SIMD values (`(x, y, z, _)`).
///
/// Returns a `v128` where lanes 0..2 contain `(a.y*b.z - a.z*b.y, a.z*b.x - a.x*b.z, a.x*b.y - a.y*b.x)`
/// and lane 3 contains `0.0`.
#[inline(always)]
pub fn cross3(a: v128, b: v128) -> v128 {
    let a_yzx = i32x4_shuffle::<1, 2, 0, 3>(a, a);
    let b_zxy = i32x4_shuffle::<2, 0, 1, 3>(b, b);

    let a_zxy = i32x4_shuffle::<2, 0, 1, 3>(a, a);
    let b_yzx = i32x4_shuffle::<1, 2, 0, 3>(b, b);

    let mul1 = f32x4_mul(a_yzx, b_zxy);
    let mul2 = f32x4_mul(a_zxy, b_yzx);

    f32x4_sub(mul1, mul2)
}

/// Dot product of the low 3 lanes (lane 3 ignored), returned as an `f32`.
///
/// `f32x4.dot` lives behind the relaxed-SIMD surface we don't enable, so the
/// horizontal reduction is done by hand: `mul`, then two add-shuffles fold the
/// four lanes into one. Lane 3 is always 0, so the fold yields `a0*b0 +
/// a1*b1 + a2*b2`. The reduction is ordered left-to-right to match the scalar
/// `a*a + b*b + c*c` evaluation exactly, so results are bit-identical.
#[inline(always)]
fn dot3(a: v128, b: v128) -> f32 {
    let v = f32x4_mul(a, b);
    let v_masked = v128_and(v, i32x4(-1, -1, -1, 0));
    let v1 = f32x4_add(v_masked, i32x4_shuffle::<1, 0, 3, 2>(v_masked, v_masked));
    let v2 = f32x4_add(v1, i32x4_shuffle::<2, 3, 0, 1>(v1, v1));
    f32x4_extract_lane::<0>(v2)
}

/// View matrix placing the camera at `eye` and looking at `center`, with
/// `up` as the world-space up direction. Column-major.
///
/// Hand-vectorized: direction subtraction, the two vector normalizations
/// (`dot3` + `sqrt`), both cross products (lane shuffles) and the three
/// translation dot products all run through wasm128 SIMD. Arithmetic is
/// identical to the scalar formulation, so the result is unchanged.
#[inline(always)]
pub fn create_look_at_matrix(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> Mat4 {
    let eye_v = f32x4(eye[0], eye[1], eye[2], 0.0);
    let center_v = f32x4(center[0], center[1], center[2], 0.0);
    let up_v = f32x4(up[0], up[1], up[2], 0.0);

    // forward = normalize(center - eye)
    let f_sub = f32x4_sub(center_v, eye_v);
    let f_len_sq = dot3(f_sub, f_sub);
    let f = if f_len_sq < 1e-12 {
        f32x4(0.0, 0.0, -1.0, 0.0)
    } else {
        f32x4_div(f_sub, f32x4_splat(f_len_sq.sqrt()))
    };

    // right = normalize(cross(forward, up))
    let s_cross = cross3(f, up_v);
    let s_len_sq = dot3(s_cross, s_cross);
    let s = if s_len_sq < 1e-12 {
        let alt_up = if f32x4_extract_lane::<1>(f).abs() > 0.9 {
            f32x4(1.0, 0.0, 0.0, 0.0)
        } else {
            f32x4(0.0, 1.0, 0.0, 0.0)
        };
        let s_alt = cross3(f, alt_up);
        let s_alt_len = dot3(s_alt, s_alt).sqrt().max(1e-6);
        f32x4_div(s_alt, f32x4_splat(s_alt_len))
    } else {
        f32x4_div(s_cross, f32x4_splat(s_len_sq.sqrt()))
    };

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

    // Safe value-level cast using bytemuck.
    bytemuck::cast::<[v128; 4], Mat4>([c0, c1, c2, c3])
}

/// Column-major `a × b`, hand-vectorized with wasm128 SIMD.
#[inline(always)]
pub fn multiply_matrices(a: &Mat4, b: &Mat4) -> Mat4 {
    // In column-major layout each column is contiguous, so column `c` is the
    // v128 `(m[4*c], m[4*c + 1], m[4*c + 2], m[4*c + 3])`. Building the lanes
    // by value (rather than transmuting a `&[f32; 16]` reference to `&[v128; 4]`)
    // avoids the alignment UB of the old implementation: `[f32; 16]` is only
    // 4-byte aligned while `v128` requires 16-byte alignment, which traps on
    // wasm `v128.load`.
    let a_cols = [
        f32x4(a.0[0], a.0[1], a.0[2], a.0[3]),
        f32x4(a.0[4], a.0[5], a.0[6], a.0[7]),
        f32x4(a.0[8], a.0[9], a.0[10], a.0[11]),
        f32x4(a.0[12], a.0[13], a.0[14], a.0[15]),
    ];
    let b_cols = [
        f32x4(b.0[0], b.0[1], b.0[2], b.0[3]),
        f32x4(b.0[4], b.0[5], b.0[6], b.0[7]),
        f32x4(b.0[8], b.0[9], b.0[10], b.0[11]),
        f32x4(b.0[12], b.0[13], b.0[14], b.0[15]),
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

    // Safe value-level cast using bytemuck (checked at compile time for equal sizes and alignment rules).
    bytemuck::cast::<[v128; 4], Mat4>(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn mat4_identity_constant() {
        let expected = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        assert_eq!(Mat4::IDENTITY.0, expected);
    }

    /// Verifies that Mat4 satisfies bytemuck Pod and Zeroable contracts, producing
    /// a contiguous 64-byte slice for WebGPU uniform buffer uploads.
    #[wasm_bindgen_test]
    fn mat4_bytemuck_pod_zeroable() {
        let bytes = bytemuck::bytes_of(&Mat4::IDENTITY);
        assert_eq!(bytes.len(), 64);

        // Verify zeroed matrix initialization satisfies Pod/Zeroable
        let zeroed: Mat4 = bytemuck::Zeroable::zeroed();
        assert_eq!(zeroed.0, [0.0; 16]);

        // Verify roundtrip cast from byte slice back to Mat4 reference
        let reconstructed: &Mat4 = bytemuck::from_bytes(bytes);
        assert_eq!(reconstructed, &Mat4::IDENTITY);
    }

    /// Verifies that Mat4 serializes as a flat 16-element JSON array for storage
    /// compatibility and deserializes back to an identical Mat4 value.
    #[wasm_bindgen_test]
    fn mat4_serde_roundtrip() {
        let original = Mat4::IDENTITY;
        let json = serde_json::to_string(&original).expect("Serialization failed");

        // Ensure serialized structure is a flat JSON array of 16 floats
        let raw_array: Vec<f32> = serde_json::from_str(&json).expect("Deserialization to array failed");
        assert_eq!(raw_array.len(), 16);
        assert_eq!(raw_array, original.0);

        // Ensure direct deserialization back to Mat4 preserves exact values
        let deserialized: Mat4 = serde_json::from_str(&json).expect("Deserialization to Mat4 failed");
        assert_eq!(deserialized, original);
    }

    /// Verifies that multiplying the identity matrix by itself yields the exact identity matrix.
    #[wasm_bindgen_test]
    fn mat4_identity_multiplication() {
        let result = multiply_matrices(&Mat4::IDENTITY, &Mat4::IDENTITY);
        assert_eq!(result, Mat4::IDENTITY);
    }

    /// Verifies that multiplying an arbitrary non-identity matrix by the identity
    /// matrix on either the left or the right yields the original matrix unchanged.
    #[wasm_bindgen_test]
    fn multiply_identity_left_right() {
        let a = Mat4([
            1.5, 2.5, 3.5, 4.5,
            5.5, 6.5, 7.5, 8.5,
            9.5, 10.5, 11.5, 12.5,
            13.5, 14.5, 15.5, 16.5,
        ]);

        let a_times_i = multiply_matrices(&a, &Mat4::IDENTITY);
        let i_times_a = multiply_matrices(&Mat4::IDENTITY, &a);

        for i in 0..16 {
            approx::assert_relative_eq!(a_times_i.0[i], a.0[i], epsilon = 1e-5);
            approx::assert_relative_eq!(i_times_a.0[i], a.0[i], epsilon = 1e-5);
        }
    }

    /// Verifies SIMD matrix multiplication against hand-calculated analytical matrices
    /// in column-major layout within single-precision floating point tolerance (1e-5).
    #[wasm_bindgen_test]
    fn multiply_known_analytic_matrices() {
        // Matrix A in column-major order
        let a = Mat4([
            1.0, 0.0, 1.0, 0.0,
            2.0, 1.0, 0.0, 0.0,
            0.0, 1.0, 2.0, 0.0,
            0.0, 0.0, 1.0, 1.0,
        ]);

        // Matrix B in column-major order
        let b = Mat4([
            2.0, 1.0, 0.0, 1.0,
            0.0, 2.0, 1.0, 0.0,
            1.0, 0.0, 1.0, 0.0,
            3.0, 1.0, 0.0, 2.0,
        ]);

        // Hand-calculated expected product C = A * B in column-major order
        let expected = Mat4([
            4.0, 1.0, 3.0, 1.0,
            4.0, 3.0, 2.0, 0.0,
            1.0, 1.0, 3.0, 0.0,
            5.0, 1.0, 5.0, 2.0,
        ]);

        let result = multiply_matrices(&a, &b);
        for i in 0..16 {
            approx::assert_relative_eq!(result.0[i], expected.0[i], epsilon = 1e-5);
        }
    }

    /// Verifies that matrix multiplication is non-commutative for general non-diagonal
    /// transformation matrices (A * B != B * A).
    #[wasm_bindgen_test]
    fn multiply_non_commutative() {
        let a = Mat4([
            1.0, 0.0, 0.0, 0.0,
            2.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let b = Mat4([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 3.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let ab = multiply_matrices(&a, &b);
        let ba = multiply_matrices(&b, &a);

        assert_ne!(ab, ba);
    }

    /// Verifies the algebraic associativity property of matrix multiplication:
    /// (A * B) * C == A * (B * C) within single-precision floating-point tolerance (1e-5).
    #[wasm_bindgen_test]
    fn multiply_associativity() {
        let a = Mat4([
            1.0, 0.5, 0.0, 0.0,
            0.2, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.4,
            0.1, 0.0, 0.0, 1.0,
        ]);

        let b = Mat4([
            2.0, 0.0, 0.3, 0.0,
            0.0, 1.5, 0.0, 0.1,
            0.4, 0.0, 2.0, 0.0,
            0.0, 0.2, 0.0, 1.0,
        ]);

        let c = Mat4([
            0.8, 0.1, 0.0, 0.2,
            0.0, 1.2, 0.5, 0.0,
            0.3, 0.0, 0.9, 0.0,
            0.0, 0.0, 0.1, 1.1,
        ]);

        let ab_c = multiply_matrices(&multiply_matrices(&a, &b), &c);
        let a_bc = multiply_matrices(&a, &multiply_matrices(&b, &c));

        for i in 0..16 {
            approx::assert_relative_eq!(ab_c.0[i], a_bc.0[i], epsilon = 1e-5);
        }
    }

    /// Verifies that multiplying diagonal scale matrices produces the component-wise
    /// product of scale factors along corresponding Cartesian axes.
    #[wasm_bindgen_test]
    fn multiply_scale_transformations() {
        let s1 = Mat4([
            2.0, 0.0, 0.0, 0.0,
            0.0, 3.0, 0.0, 0.0,
            0.0, 0.0, 4.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let s2 = Mat4([
            5.0, 0.0, 0.0, 0.0,
            0.0, 6.0, 0.0, 0.0,
            0.0, 0.0, 7.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let expected = Mat4([
            10.0, 0.0, 0.0, 0.0,
            0.0, 18.0, 0.0, 0.0,
            0.0, 0.0, 28.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]);

        let result = multiply_matrices(&s1, &s2);
        for i in 0..16 {
            approx::assert_relative_eq!(result.0[i], expected.0[i], epsilon = 1e-5);
        }
    }

    /// Verifies that multiplying 3D translation matrices accumulates their respective
    /// translation vectors T(1,2,3) * T(4,5,6) == T(5,7,9) in column 3.
    #[wasm_bindgen_test]
    fn multiply_translation_matrices() {
        let t1 = Mat4([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            1.0, 2.0, 3.0, 1.0,
        ]);

        let t2 = Mat4([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            4.0, 5.0, 6.0, 1.0,
        ]);

        let expected = Mat4([
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            5.0, 7.0, 9.0, 1.0,
        ]);

        let result = multiply_matrices(&t1, &t2);
        for i in 0..16 {
            approx::assert_relative_eq!(result.0[i], expected.0[i], epsilon = 1e-5);
        }
    }

    /// Verifies that matrix multiplication operates safely on heap-allocated matrices
    /// that are only 4-byte aligned (e.g. from dynamic slices or byte buffers), preventing
    /// WebAssembly 16-byte alignment memory traps.
    #[wasm_bindgen_test]
    fn multiply_alignment_safety() {
        // Allocate a dynamic buffer of f32s with non-16-byte alignment offset
        let mut buffer = vec![0.0f32; 40];
        // Offset by 1 element (4 bytes), so the slice starts at an odd 4-byte boundary
        for i in 0..16 {
            buffer[1 + i] = if i % 5 == 0 { 1.0 } else { 0.0 };
            buffer[18 + i] = if i % 5 == 0 { 2.0 } else { 0.0 };
        }

        let slice_a: [f32; 16] = buffer[1..17].try_into().unwrap();
        let slice_b: [f32; 16] = buffer[18..34].try_into().unwrap();

        let mat_a = Mat4(slice_a);
        let mat_b = Mat4(slice_b);

        let result = multiply_matrices(&mat_a, &mat_b);
        let expected = Mat4([
            2.0, 0.0, 0.0, 0.0,
            0.0, 2.0, 0.0, 0.0,
            0.0, 0.0, 2.0, 0.0,
            0.0, 0.0, 0.0, 2.0,
        ]);

        assert_eq!(result, expected);
    }

    /// Verifies the SIMD cross product of standard Cartesian basis unit vectors:
    /// i(1,0,0) x j(0,1,0) == k(0,0,1) in the low 3 lanes of the v128 vector.
    #[wasm_bindgen_test]
    fn cross3_unit_vectors() {
        let i_vec = f32x4(1.0, 0.0, 0.0, 0.0);
        let j_vec = f32x4(0.0, 1.0, 0.0, 0.0);

        let k_vec = cross3(i_vec, j_vec);

        approx::assert_relative_eq!(f32x4_extract_lane::<0>(k_vec), 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(f32x4_extract_lane::<1>(k_vec), 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(f32x4_extract_lane::<2>(k_vec), 1.0, epsilon = 1e-6);
    }

    /// Verifies the anti-commutativity law of 3D vector cross products:
    /// a x b == -(b x a) within single-precision floating-point precision (1e-6).
    #[wasm_bindgen_test]
    fn cross3_anti_commutativity() {
        let a = f32x4(1.2, -3.4, 5.6, 0.0);
        let b = f32x4(7.8, 9.1, -2.3, 0.0);

        let ab = cross3(a, b);
        let ba = cross3(b, a);

        let ab_x = f32x4_extract_lane::<0>(ab);
        let ab_y = f32x4_extract_lane::<1>(ab);
        let ab_z = f32x4_extract_lane::<2>(ab);

        let ba_x = f32x4_extract_lane::<0>(ba);
        let ba_y = f32x4_extract_lane::<1>(ba);
        let ba_z = f32x4_extract_lane::<2>(ba);

        approx::assert_relative_eq!(ab_x, -ba_x, epsilon = 1e-6);
        approx::assert_relative_eq!(ab_y, -ba_y, epsilon = 1e-6);
        approx::assert_relative_eq!(ab_z, -ba_z, epsilon = 1e-6);
    }

    /// Verifies that the cross product of collinear/parallel vectors produces
    /// a zero vector (0, 0, 0) across low 3 lanes.
    #[wasm_bindgen_test]
    fn cross3_collinear_vectors() {
        let a = f32x4(2.0, 0.0, 0.0, 0.0);
        let b = f32x4(5.0, 0.0, 0.0, 0.0);

        let result = cross3(a, b);

        approx::assert_relative_eq!(f32x4_extract_lane::<0>(result), 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(f32x4_extract_lane::<1>(result), 0.0, epsilon = 1e-6);
        approx::assert_relative_eq!(f32x4_extract_lane::<2>(result), 0.0, epsilon = 1e-6);
    }

    /// Verifies that the dot product of orthogonal unit vectors (1,0,0) and (0,1,0)
    /// evaluates to exactly 0.0.
    #[wasm_bindgen_test]
    fn dot3_orthogonal_vectors() {
        let u = f32x4(1.0, 0.0, 0.0, 0.0);
        let v = f32x4(0.0, 1.0, 0.0, 0.0);

        let result = dot3(u, v);
        approx::assert_relative_eq!(result, 0.0, epsilon = 1e-6);
    }

    /// Verifies the SIMD dot product against an analytical calculation for known vectors:
    /// (1,2,3) . (4,5,6) == 1*4 + 2*5 + 3*6 == 32.0.
    #[wasm_bindgen_test]
    fn dot3_known_vectors() {
        let u = f32x4(1.0, 2.0, 3.0, 0.0);
        let v = f32x4(4.0, 5.0, 6.0, 0.0);

        let result = dot3(u, v);
        approx::assert_relative_eq!(result, 32.0, epsilon = 1e-6);
    }

    /// Verifies that dot3 strictly evaluates only the low 3 coordinates (X, Y, Z)
    /// and completely ignores non-zero noise/data in the fourth (W) lane.
    #[wasm_bindgen_test]
    fn dot3_ignores_fourth_lane() {
        let u = f32x4(1.0, 2.0, 3.0, 9999.0);
        let v = f32x4(4.0, 5.0, 6.0, -8888.0);

        let result = dot3(u, v);
        approx::assert_relative_eq!(result, 32.0, epsilon = 1e-6);
    }
}

