//! In-tree XXH3 implementation.
//!
//! This module follows the public xxHash algorithm specification. The scalar
//! core is the portability baseline; architecture-specific accumulation is
//! deliberately kept behind dispatch points so unsupported CPUs never execute
//! instructions they cannot run.
//!
//! The 128-bit functions return `[low64, high64]`, matching the field order of
//! the official xxHash `XXH128_hash_t` result. This pair contains numeric words,
//! not serialized bytes; choose and document a byte order at protocol boundaries.

mod batch;
mod hash;
mod long;
mod primitives;
mod short;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

pub use batch::{xxh3_64_batch, xxh3_128_batch};
pub use hash::{xxh3_64, xxh3_128};

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
