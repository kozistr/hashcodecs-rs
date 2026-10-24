//! Base64 decoding APIs and kernels.
//!
//! Raw decoders require non-overlapping output covering the decoded length;
//! layout arguments must describe the supplied input. Validated variants also
//! require a checked alphabet. Architecture entry points require their declared
//! target features; x86 kernels use `x86_contracts::{Decoder, Store}` for load and
//! store widths.

#[cfg(target_arch = "aarch64")]
pub(super) mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx512;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod sse41;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse_kernels;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod ssse3;
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
mod tables;
#[cfg(all(feature = "python", any(target_arch = "x86", target_arch = "x86_64")))]
pub(crate) use tables::{STANDARD_HIGH_CLASSES, STANDARD_LOW_CLASSES_COMPLEMENT};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86_contracts;

mod api;

#[cfg(any(feature = "python", test, kani))]
pub(crate) use api::*;
pub use api::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64decoded_len,
};
