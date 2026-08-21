use crate::backend::{Capabilities, SimdBackend};

use super::{init_secret_scalar, long_accumulate_scalar};

pub(super) mod avx2;
mod avx512;
mod sse;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    Ssse3,
    Sse41,
    Avx2,
    Avx512,
}

#[inline]
pub(super) fn select(capabilities: Capabilities) -> Backend {
    if capabilities.supports(SimdBackend::Avx512) {
        Backend::Avx512
    } else if capabilities.supports(SimdBackend::Avx2) {
        Backend::Avx2
    } else if capabilities.supports(SimdBackend::Sse41) {
        Backend::Sse41
    } else if capabilities.supports(SimdBackend::Ssse3) {
        Backend::Ssse3
    } else {
        Backend::Scalar
    }
}

#[inline]
pub(super) fn init_secret(seed: u64, capabilities: Capabilities) -> [u8; 192] {
    if capabilities.supports(SimdBackend::Avx2) {
        unsafe { avx2::init_secret(seed) }
    } else {
        init_secret_scalar(seed)
    }
}

#[inline]
pub(super) fn long_accumulate(data: &[u8], secret: &[u8], capabilities: Capabilities) -> [u64; 8] {
    match select(capabilities) {
        Backend::Scalar => long_accumulate_scalar(data, secret),
        Backend::Ssse3 | Backend::Sse41 => unsafe { sse::long_accumulate(data, secret) },
        Backend::Avx2 => unsafe { avx2::long_accumulate(data, secret) },
        Backend::Avx512 => avx512::long_accumulate(data, secret),
    }
}
