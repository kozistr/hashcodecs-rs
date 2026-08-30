//! x86 kernels for XXH3 inputs that contain more than 240 bytes.

pub(super) mod avx2;
pub(super) mod avx2_batch;
pub(super) mod avx512;
pub(super) mod ssse3;
