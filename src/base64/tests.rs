#[cfg(target_arch = "aarch64")]
mod aarch64;

mod api;
mod backends;
mod buffers;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod decode_x86;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod encode_x86;
mod validation;
#[cfg(feature = "python")]
mod wrapping;
