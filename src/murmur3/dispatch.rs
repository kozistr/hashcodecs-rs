//! Select MurmurHash3 backends from the input length and CPU capabilities.

use crate::backend::{Capabilities, CpuFeature};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    Sse41,
    Avx2,
}

pub(super) const X86_32_SSE41_MIN: usize = 16;
const X86_32_AVX2_MIN: usize = 32;
pub(super) const X86_128_AVX2_MIN: usize = 256;
const X86_128_SSE41_MIN: usize = 16 * 1024 * 1024;
const X64_128_SSE41_MIN: usize = 16;
const X64_128_SSE41_MAX: usize = 8 * 1024 * 1024;
const X64_128_AVX2_MIN: usize = 32;

#[inline(always)]
pub(super) fn select_x86_32_backend(length: usize, capabilities: Capabilities) -> Backend {
    if capabilities.supports(CpuFeature::Avx2) && length >= X86_32_AVX2_MIN {
        Backend::Avx2
    } else if capabilities.supports(CpuFeature::Sse41) && length >= X86_32_SSE41_MIN {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}

#[inline(always)]
pub(super) fn select_x86_128_backend(length: usize, capabilities: Capabilities) -> Backend {
    if capabilities.supports(CpuFeature::Avx2) && length >= X86_128_AVX2_MIN {
        Backend::Avx2
    } else if capabilities.supports(CpuFeature::Sse41) && length >= X86_128_SSE41_MIN {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}

#[inline(always)]
pub(super) fn select_x64_128_backend(length: usize, capabilities: Capabilities) -> Backend {
    if capabilities.supports(CpuFeature::Avx2) && length >= X64_128_AVX2_MIN {
        Backend::Avx2
    } else if capabilities.supports(CpuFeature::Sse41)
        && (X64_128_SSE41_MIN..=X64_128_SSE41_MAX).contains(&length)
    {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}
