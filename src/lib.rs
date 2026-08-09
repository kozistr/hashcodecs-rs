//! Fast binary codecs with portable APIs and runtime SIMD dispatch.
//!
//! Base64 dispatches among in-crate AVX-512, AVX2, SSE4.1, SSSE3, NEON, and scalar kernels at runtime.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod base64;
mod murmur3;

pub use base64::{
    Base64Error, b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into,
    b64decoded_len, b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into,
    b64encoded_len,
};
pub use murmur3::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};

#[cfg(feature = "python")]
mod python;
