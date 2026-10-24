#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

mod murmur3_x64_hasher128;

#[cfg(any(test, kani, target_arch = "x86", target_arch = "x86_64"))]
pub(super) use murmur3_x64_hasher128::*;
pub use murmur3_x64_hasher128::{Murmur3X64Hasher128, murmur3_x64_128};
