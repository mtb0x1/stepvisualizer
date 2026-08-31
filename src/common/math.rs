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

// @todo
// https://rust.godbolt.org/z/sWGW7cq5s

/// computes the 3 components cross product of two vectors packed into the
/// low three lanes of the given `v128` SIMD values (interpreted as
/// `(x, y, z, _)`). The fourth lane is ignored (typically `0.0`).
///
/// reslt is returned as a `v128` where lanes 0..2 contain the scalar
/// cross product `a × b` and lane 3 holds the preserved fourth lane
/// (usually `0.0`). should be same as scalar
/// formulation `(a.y*b.z - a.z*b.y, a.z*b.x - a.x*b.z, a.x*b.y - a.y*b.x)`.
#[inline(always)]
pub fn cross3(a: v128, b: v128) -> v128 {
    // a_yzx * b - b_yzx * a, followed by a final shuffle
    let a_yzx = i32x4_shuffle::<1, 2, 0, 3>(a, a);
    let b_yzx = i32x4_shuffle::<1, 2, 0, 3>(b, b);

    let mul1 = f32x4_mul(a_yzx, b);
    let mul2 = f32x4_mul(b_yzx, a);
    let sub = f32x4_sub(mul1, mul2);

    // Final shuffle converts (a_y*b_x - b_y*a_x, ...) to the correct output positions
    i32x4_shuffle::<1, 2, 0, 3>(sub, sub)
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
    let v1 = f32x4_add(v, i32x4_shuffle::<1, 0, 3, 2>(v, v));
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
}

