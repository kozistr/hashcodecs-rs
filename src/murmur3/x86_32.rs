#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

mod murmur3_x86_hasher32;

#[cfg(any(test, kani, target_arch = "x86", target_arch = "x86_64"))]
pub(super) use murmur3_x86_hasher32::*;
pub use murmur3_x86_hasher32::{Murmur3X86Hasher32, murmur3_x86_32};
