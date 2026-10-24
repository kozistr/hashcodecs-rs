#[cfg(target_arch = "aarch64")]
pub(super) mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx512;
#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
pub(super) mod cache;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod ssse3;

mod api;

#[cfg(any(feature = "python", test, kani))]
pub(crate) use api::*;
pub use api::{
    b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into, b64encoded_len,
};
