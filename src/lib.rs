//! Fast binary codecs with portable APIs and runtime SIMD dispatch.
//!
//! Base64 and XXH3 dispatch among in-crate AVX-512, AVX2, SSE4.1, SSSE3, NEON,
//! and scalar kernels at runtime.

// Native allocators and CPU intrinsics are outside Kani's and Miri's interpreters.
#[cfg(not(any(kani, miri)))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod backend;
mod base64;
mod murmur3;
mod xxhash;

pub use base64::{
    Base64Error, b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into,
    b64decoded_len, b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into,
    b64encoded_len,
};
pub use murmur3::{
    Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128, murmur3_x86_32,
    murmur3_x86_128,
};
pub use xxhash::{xxh3_64, xxh3_64_batch, xxh3_128, xxh3_128_batch};

#[cfg(feature = "python")]
mod python;
