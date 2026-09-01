//! This module implements XXH3 without an external codec crate.
//!
//! The implementation follows the public xxHash algorithm specification. The scalar kernels provide the portability baseline.
//! Runtime dispatch prevents a CPU from executing unsupported SIMD instructions.
//!
//! The 128-bit functions return `[low64, high64]`. This order matches the official `XXH128_hash_t` result.
//! The pair contains numeric words, not serialized bytes. Select and document a byte order at each protocol boundary.

mod batch;
mod long_inputs;
mod one_shot;
mod primitives;
mod short_inputs;

pub use batch::{xxh3_64_batch, xxh3_64_batch_for_each, xxh3_128_batch, xxh3_128_batch_for_each};
pub use one_shot::{xxh3_64, xxh3_128};

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
