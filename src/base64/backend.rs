//! Select a Base64 backend from the detected CPU features.

use std::sync::OnceLock;

use crate::backend::{self as cpu, Capabilities, CpuFeature};

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
    [
        Backend::Avx512Vbmi,
        Backend::Avx2,
        Backend::Sse41,
        Backend::Ssse3,
        Backend::Neon,
    ]
    .into_iter()
    .find(|&backend| supports_backend(capabilities, backend))
    .unwrap_or(Backend::Scalar)
}

#[inline]
fn supports_backend(capabilities: Capabilities, backend: Backend) -> bool {
    let required: &[CpuFeature] = match backend {
        Backend::Scalar => &[],
        Backend::Neon => &[CpuFeature::Neon],
        Backend::Ssse3 => &[CpuFeature::Ssse3],
        // These kernels delegate their tails to lower-tier implementations.
        Backend::Sse41 => &[CpuFeature::Sse41, CpuFeature::Ssse3],
        Backend::Avx2 => &[CpuFeature::Avx2, CpuFeature::Ssse3],
        Backend::Avx512Vbmi => &[
            CpuFeature::Avx512F,
            CpuFeature::Avx512Bw,
            CpuFeature::Avx512Vbmi,
            CpuFeature::Avx2,
            CpuFeature::Ssse3,
        ],
    };

    capabilities.supports_all(required)
}

#[cfg(test)]
pub(super) fn is_supported(backend: Backend) -> bool {
    supports_backend(cpu::capabilities(), backend)
}
