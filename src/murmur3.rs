//! MurmurHash3 reference-compatible functions with runtime SIMD dispatch.
//!
//! SIMD kernels premix independent input words in parallel. The canonical
//! loop-carried state transitions remain ordered exactly as specified.

mod incremental;
mod primitives;
mod x64_128;
mod x86_128;
mod x86_32;

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
mod dispatch;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

pub use x64_128::{Murmur3X64Hasher128, murmur3_x64_128};
pub use x86_32::{Murmur3X86Hasher32, murmur3_x86_32};
pub use x86_128::{Murmur3X86Hasher128, murmur3_x86_128};

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
