//! Ultra-fast non-cryptographic hashing for 64-bit keys (STEP entity IDs and vertex keys).
//!
//! # Why `FastU64Hasher` and `FastU64Map` Exist
//!
//! By default, Rust's `std::collections::HashMap` and `std::collections::HashSet` use
//! a cryptographically secure hasher called SipHash-1-3.
//!
//! SipHash is designed to defend internet-facing servers against HashDoS attacks (where
//! a remote adversary submits maliciously crafted keys that collide into the same bucket,
//! degrading O(1) operations into O(n) denial-of-service).
//!
//! In StepVisualizer, however:
//! 1. Client-Side WASM Sandbox: The code runs entirely in the user's local browser tab rendering
//!    CAD geometry. There is no remote network server and no untrusted collision attack vector.
//! 2. Integer Key Domain: Hot-path lookup keys across the engine are almost exclusively `u64`:
//!    - STEP entity IDs (`#10 = ...`, `#500 = ...`) across 20+ tables during AST indexing.
//!    - Packed vertex keys `((pos as u64) << 32) | nor` during geometry welding.
//! 3. Performance Penalty of SipHash: SipHash requires 128-bit key initialization from system
//!    entropy (which requires Web Crypto API calls in WASM) and performs multiple rounds of 64-bit
//!    rotations, additions, and XORs for every key insert and lookup.
//!
//! # How `FastU64Hasher` Works (SplitMix64)
//!
//! `FastU64Hasher` uses **SplitMix64**, a widely-used pseudo-random bit mixer.
//! For any `u64` key, it performs just 3 constant multiplications and bitwise shifts:
//! - Uniformly avalanches and distributes bits across all hash buckets.
//! - Executes in ~3 CPU cycles with zero memory loads, zero branches, and zero state
//!   initialization.
//! - Coupled with `hashbrown::HashMap` (SwissTable SIMD bucketing), it provides maximum throughput
//!   for geometry extraction, AST indexing, and vertex welding.

use std::hash::{BuildHasherDefault, Hasher};

/// Fast 64-bit bit-mixer hasher based on SplitMix64.
///
/// Designed specifically for `u64` keys (entity IDs and packed vertex keys).
#[derive(Default, Clone)]
pub struct FastU64Hasher(u64);

impl Hasher for FastU64Hasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        // SplitMix64 bit mixer: spreads bits evenly across hash buckets in ~3 cycles.
        let mut z = i.wrapping_add(0x9e3779b97f4a7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        self.0 = z ^ (z >> 31);
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[.. chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_ne_bytes(buf));
        }
    }
}

/// BuildHasher for [`FastU64Hasher`].
pub type FastBuildHasher = BuildHasherDefault<FastU64Hasher>;

/// High-performance hash map for `u64` keys backed by `hashbrown` and [`FastU64Hasher`].
pub type FastU64Map<V> = hashbrown::HashMap<u64, V, FastBuildHasher>;

/// High-performance hash set for `u64` keys backed by `hashbrown` and [`FastU64Hasher`].
pub type FastU64Set = hashbrown::HashSet<u64, FastBuildHasher>;
