//! Select a Base64 backend from the detected CPU features.

use std::sync::OnceLock;

use crate::backend::{self as cpu, Capabilities, SimdBackend};

#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
use super::encode::cache;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    Neon,
    Ssse3,
    Sse41,
    Avx2,
    Avx512Vbmi,
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeBackend {
    pub(super) backend: Backend,
    cached_input_limit: Option<usize>,
}

impl RuntimeBackend {
    #[inline]
    pub(super) fn use_streaming_stores(self, input_len: usize, output: *mut u8) -> bool {
        #[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
        {
            cache::use_streaming_stores(self.cached_input_limit, input_len, output)
        }
        #[cfg(all(target_arch = "x86_64", any(kani, miri)))]
        {
            let _ = (self.cached_input_limit, input_len, output);
            false
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let _ = (self.cached_input_limit, input_len, output);
            false
        }
    }
}

static SELECTED_BACKEND: OnceLock<RuntimeBackend> = OnceLock::new();

#[inline]
pub(super) fn selected_backend() -> RuntimeBackend {
    *SELECTED_BACKEND.get_or_init(detect_runtime_backend)
}

fn detect_runtime_backend() -> RuntimeBackend {
    RuntimeBackend {
        backend: select_backend(cpu::capabilities()),
        #[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
        cached_input_limit: cache::cached_input_limit(),
        #[cfg(all(target_arch = "x86_64", any(kani, miri)))]
        cached_input_limit: None,
        #[cfg(not(target_arch = "x86_64"))]
        cached_input_limit: None,
    }
}

#[inline]
pub(super) fn select_backend(capabilities: Capabilities) -> Backend {
    match capabilities.select_supported_backend(&[
        SimdBackend::Avx512Vbmi,
        SimdBackend::Avx2,
        SimdBackend::Sse41,
        SimdBackend::Ssse3,
        SimdBackend::Neon,
    ]) {
        SimdBackend::Avx512Vbmi => Backend::Avx512Vbmi,
        SimdBackend::Avx2 => Backend::Avx2,
        SimdBackend::Sse41 => Backend::Sse41,
        SimdBackend::Ssse3 => Backend::Ssse3,
        SimdBackend::Neon => Backend::Neon,
        SimdBackend::Scalar | SimdBackend::Avx512 => Backend::Scalar,
    }
}

#[cfg(test)]
pub(super) fn is_supported(backend: Backend) -> bool {
    let required = match backend {
        Backend::Scalar => SimdBackend::Scalar,
        Backend::Neon => SimdBackend::Neon,
        Backend::Ssse3 => SimdBackend::Ssse3,
        Backend::Sse41 => SimdBackend::Sse41,
        Backend::Avx2 => SimdBackend::Avx2,
        Backend::Avx512Vbmi => SimdBackend::Avx512Vbmi,
    };
    cpu::capabilities().supports(required)
}
