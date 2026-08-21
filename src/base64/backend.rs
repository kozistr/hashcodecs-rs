//! Runtime CPU feature detection for Base64 kernels.

use std::sync::OnceLock;

#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
use super::x86;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    #[cfg_attr(not(any(test, target_arch = "aarch64")), allow(dead_code))]
    Neon,
    #[cfg_attr(
        not(any(test, target_arch = "x86", target_arch = "x86_64")),
        allow(dead_code)
    )]
    Ssse3,
    #[cfg_attr(
        not(any(test, target_arch = "x86", target_arch = "x86_64")),
        allow(dead_code)
    )]
    Sse41,
    #[cfg_attr(
        not(any(test, target_arch = "x86", target_arch = "x86_64")),
        allow(dead_code)
    )]
    Avx2,
    #[cfg_attr(
        not(any(test, target_arch = "x86", target_arch = "x86_64")),
        allow(dead_code)
    )]
    Avx512,
}

#[derive(Clone, Copy)]
pub(super) struct RuntimeBackend {
    pub(super) kind: Backend,
    cached_input_limit: Option<usize>,
}

impl RuntimeBackend {
    #[inline]
    pub(super) fn use_streaming_stores(self, input_len: usize, output: *mut u8) -> bool {
        #[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
        {
            x86::use_streaming_stores(self.cached_input_limit, input_len, output)
        }
        #[cfg(not(all(target_arch = "x86_64", not(any(kani, miri)))))]
        {
            let _ = (self.cached_input_limit, input_len, output);
            false
        }
    }
}

static BACKEND: OnceLock<RuntimeBackend> = OnceLock::new();

#[inline]
pub(super) fn selected() -> RuntimeBackend {
    *BACKEND.get_or_init(detect)
}

#[inline]
fn detect() -> RuntimeBackend {
    #[cfg(any(kani, miri))]
    {
        RuntimeBackend {
            kind: Backend::Scalar,
            cached_input_limit: None,
        }
    }
    #[cfg(all(not(any(kani, miri)), any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let kind = select_x86(
            std::is_x86_feature_detected!("avx512vbmi"),
            std::is_x86_feature_detected!("avx2"),
            std::is_x86_feature_detected!("sse4.1"),
            std::is_x86_feature_detected!("ssse3"),
        );
        RuntimeBackend {
            kind,
            #[cfg(target_arch = "x86_64")]
            cached_input_limit: x86::cached_input_limit(),
            #[cfg(target_arch = "x86")]
            cached_input_limit: None,
        }
    }
    #[cfg(all(not(any(kani, miri)), target_arch = "aarch64"))]
    {
        RuntimeBackend {
            kind: select_aarch64(std::arch::is_aarch64_feature_detected!("neon")),
            cached_input_limit: None,
        }
    }
    #[cfg(all(
        not(any(kani, miri)),
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))
    ))]
    {
        RuntimeBackend {
            kind: Backend::Scalar,
            cached_input_limit: None,
        }
    }
}

#[inline]
#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
pub(super) fn select_x86(avx512: bool, avx2: bool, sse41: bool, ssse3: bool) -> Backend {
    if avx512 {
        Backend::Avx512
    } else if avx2 {
        Backend::Avx2
    } else if sse41 && ssse3 {
        Backend::Sse41
    } else if ssse3 {
        Backend::Ssse3
    } else {
        Backend::Scalar
    }
}

#[inline]
#[cfg(any(test, target_arch = "aarch64"))]
pub(super) fn select_aarch64(neon: bool) -> Backend {
    if neon { Backend::Neon } else { Backend::Scalar }
}

#[cfg(test)]
pub(super) fn is_supported(backend: Backend) -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        match backend {
            Backend::Scalar => true,
            Backend::Neon => false,
            Backend::Ssse3 => std::is_x86_feature_detected!("ssse3"),
            Backend::Sse41 => {
                std::is_x86_feature_detected!("ssse3") && std::is_x86_feature_detected!("sse4.1")
            }
            Backend::Avx2 => std::is_x86_feature_detected!("avx2"),
            Backend::Avx512 => std::is_x86_feature_detected!("avx512vbmi"),
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        match backend {
            Backend::Scalar => true,
            Backend::Neon => std::arch::is_aarch64_feature_detected!("neon"),
            _ => false,
        }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    {
        backend == Backend::Scalar
    }
}
