//! This module implements reference-compatible MurmurHash3 functions.
//!
//! SIMD kernels mix independent input words in parallel. The kernels keep the specified order for dependent state transitions.

mod block_buffer;
mod primitives;
mod x64_128;
mod x86_128;
mod x86_32;

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
mod dispatch;

pub use x64_128::{Murmur3X64Hasher128, murmur3_x64_128};
pub use x86_32::{Murmur3X86Hasher32, murmur3_x86_32};
pub use x86_128::{Murmur3X86Hasher128, murmur3_x86_128};

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
