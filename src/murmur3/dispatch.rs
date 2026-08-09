#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    Sse41,
    Avx2,
}

const X86_32_SSE41_MIN: usize = 16;
const X86_32_AVX2_MIN: usize = 32;
const X86_128_AVX2_MIN: usize = 256;
const X86_128_SSE41_MIN: usize = 16 * 1024 * 1024;
const X64_128_SSE41_MIN: usize = 16;
const X64_128_SSE41_MAX: usize = 8 * 1024 * 1024;
const X64_128_AVX2_MIN: usize = 32;

#[inline(always)]
pub(super) fn x86_32(length: usize, avx2: bool, sse41: bool) -> Backend {
    if avx2 && length >= X86_32_AVX2_MIN {
        Backend::Avx2
    } else if sse41 && length >= X86_32_SSE41_MIN {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}

#[inline(always)]
pub(super) fn x86_128(length: usize, avx2: bool, sse41: bool) -> Backend {
    if avx2 && length >= X86_128_AVX2_MIN {
        Backend::Avx2
    } else if sse41 && length >= X86_128_SSE41_MIN {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}

#[inline(always)]
pub(super) fn x64_128(length: usize, avx2: bool, sse41: bool) -> Backend {
    if avx2 && length >= X64_128_AVX2_MIN {
        Backend::Avx2
    } else if sse41 && (X64_128_SSE41_MIN..=X64_128_SSE41_MAX).contains(&length) {
        Backend::Sse41
    } else {
        Backend::Scalar
    }
}
