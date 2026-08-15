use std::sync::OnceLock;

#[cfg(target_arch = "aarch64")]
use super::aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86;
#[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
use super::x86_avx512;
use super::{Base64Error, DecodeAlphabet};

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

static BACKEND: OnceLock<Backend> = OnceLock::new();

#[inline]
pub(super) unsafe fn encode_simd_ptr(input: &[u8], output: *mut u8, urlsafe: bool) -> usize {
    unsafe { encode_with_backend_ptr(input, output, selected_backend(), urlsafe) }
}

#[inline]
pub(super) unsafe fn decode_simd_ptr(
    input: &[u8],
    output: *mut u8,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
    transactional_errors: bool,
) -> Result<(usize, usize), Base64Error> {
    unsafe {
        decode_with_backend_ptr_mode(
            input,
            output,
            selected_backend(),
            alphabet,
            padded_stores,
            transactional_errors,
        )
    }
}

#[inline]
fn selected_backend() -> Backend {
    *BACKEND.get_or_init(detect_backend)
}

#[inline]
fn detect_backend() -> Backend {
    #[cfg(any(kani, miri))]
    {
        Backend::Scalar
    }
    #[cfg(all(not(any(kani, miri)), any(target_arch = "x86", target_arch = "x86_64")))]
    {
        select_x86_backend(
            avx512_supported(),
            std::is_x86_feature_detected!("avx2"),
            std::is_x86_feature_detected!("sse4.1"),
            std::is_x86_feature_detected!("ssse3"),
        )
    }
    #[cfg(all(not(any(kani, miri)), target_arch = "aarch64"))]
    {
        select_aarch64_backend(std::arch::is_aarch64_feature_detected!("neon"))
    }
    #[cfg(all(
        not(any(kani, miri)),
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))
    ))]
    {
        Backend::Scalar
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn avx512_supported() -> bool {
    #[cfg(not(coverage))]
    {
        std::is_x86_feature_detected!("avx512vbmi")
    }
    #[cfg(coverage)]
    {
        false
    }
}

#[inline]
#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
pub(super) fn select_x86_backend(avx512: bool, avx2: bool, sse41: bool, ssse3: bool) -> Backend {
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
pub(super) fn select_aarch64_backend(neon: bool) -> Backend {
    if neon { Backend::Neon } else { Backend::Scalar }
}

#[cfg(test)]
pub(super) fn backend_supported(backend: Backend) -> bool {
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
            Backend::Avx512 => avx512_supported(),
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

#[inline]
#[cfg(test)]
pub(super) fn encode_with_backend(
    input: &[u8],
    output: &mut [u8],
    backend: Backend,
    urlsafe: bool,
) -> usize {
    #[cfg(coverage)]
    if backend == Backend::Avx512 {
        return unsafe { encode_with_backend_ptr(input, output.as_mut_ptr(), backend, urlsafe) };
    }
    if !backend_supported(backend) {
        return 0;
    }
    unsafe { encode_with_backend_ptr(input, output.as_mut_ptr(), backend, urlsafe) }
}

#[inline]
unsafe fn encode_with_backend_ptr(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    urlsafe: bool,
) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if urlsafe {
            unsafe { encode_x86::<true>(input, output, backend) }
        } else {
            unsafe { encode_x86::<false>(input, output, backend) }
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        #[cfg(target_arch = "aarch64")]
        if backend == Backend::Neon {
            return unsafe {
                if urlsafe {
                    aarch64::encode_neon::<true>(input, output)
                } else {
                    aarch64::encode_neon::<false>(input, output)
                }
            };
        }
        let _ = (input, output, backend, urlsafe);
        0
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn encode_x86<const URLSAFE: bool>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
) -> usize {
    match backend {
        Backend::Avx512 => {
            #[cfg(not(coverage))]
            return unsafe { x86_avx512::encode_avx512::<URLSAFE>(input, output) };
            #[cfg(coverage)]
            return 0;
        }
        Backend::Avx2 => {
            return unsafe { x86::encode_avx2::<URLSAFE>(input, output) };
        }
        Backend::Sse41 | Backend::Ssse3 => {
            return unsafe { x86::encode_ssse3::<URLSAFE>(input, output) };
        }
        Backend::Scalar | Backend::Neon => {}
    }
    0
}

#[inline]
#[cfg(test)]
pub(super) fn decode_with_backend(
    input: &[u8],
    output: &mut [u8],
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> Result<(usize, usize), Base64Error> {
    // The slice-backed path must never write beyond the returned output.
    unsafe { decode_with_backend_ptr(input, output.as_mut_ptr(), backend, alphabet, false) }
}

#[inline]
#[cfg(test)]
pub(super) unsafe fn decode_with_backend_ptr(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(usize, usize), Base64Error> {
    #[cfg(coverage)]
    if backend == Backend::Avx512 {
        return unsafe {
            decode_with_backend_ptr_mode(input, output, backend, alphabet, padded_stores, false)
        };
    }
    if !backend_supported(backend) {
        return Ok((0, 0));
    }
    unsafe { decode_with_backend_ptr_mode(input, output, backend, alphabet, padded_stores, false) }
}

#[inline]
unsafe fn decode_with_backend_ptr_mode(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
    transactional_errors: bool,
) -> Result<(usize, usize), Base64Error> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let _ = transactional_errors;
        unsafe { decode_x86_alphabet(input, output, backend, alphabet, padded_stores) }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        #[cfg(target_arch = "aarch64")]
        if backend == Backend::Neon {
            let _ = padded_stores;
            return unsafe {
                if transactional_errors {
                    match alphabet {
                        DecodeAlphabet::Standard => {
                            aarch64::decode_neon_transactional::<false, false>(input, output)
                        }
                        DecodeAlphabet::UrlSafe => {
                            aarch64::decode_neon_transactional::<true, false>(input, output)
                        }
                        DecodeAlphabet::Mixed => {
                            aarch64::decode_neon_transactional::<false, true>(input, output)
                        }
                    }
                } else {
                    match alphabet {
                        DecodeAlphabet::Standard => {
                            aarch64::decode_neon::<false, false>(input, output)
                        }
                        DecodeAlphabet::UrlSafe => {
                            aarch64::decode_neon::<true, false>(input, output)
                        }
                        DecodeAlphabet::Mixed => aarch64::decode_neon::<false, true>(input, output),
                    }
                }
            };
        }
        let _ = (
            input,
            output,
            backend,
            alphabet,
            padded_stores,
            transactional_errors,
        );
        Ok((0, 0))
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_x86_alphabet(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(usize, usize), Base64Error> {
    match alphabet {
        DecodeAlphabet::Standard => unsafe {
            decode_x86_store::<x86::StandardDecoder>(input, output, backend, padded_stores)
        },
        DecodeAlphabet::UrlSafe => unsafe {
            decode_x86_store::<x86::UrlSafeDecoder>(input, output, backend, padded_stores)
        },
        DecodeAlphabet::Mixed => unsafe {
            decode_x86::<x86::MixedDecoder, x86::ExactStore>(input, output, backend)
        },
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_x86_store<A: x86::Decoder>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    padded_stores: bool,
) -> Result<(usize, usize), Base64Error> {
    if padded_stores {
        unsafe { decode_x86::<A, x86::PaddedStore>(input, output, backend) }
    } else {
        unsafe { decode_x86::<A, x86::ExactStore>(input, output, backend) }
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_x86<A: x86::Decoder, S: x86::Store>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
) -> Result<(usize, usize), Base64Error> {
    match backend {
        Backend::Avx512 => {
            #[cfg(not(coverage))]
            return unsafe { x86_avx512::decode_avx512::<A, S>(input, output) };
            #[cfg(coverage)]
            return Ok((0, 0));
        }
        Backend::Avx2 => {
            return unsafe { x86::decode_avx2::<A, S>(input, output) };
        }
        Backend::Sse41 => {
            return unsafe { x86::decode_sse41::<A, S>(input, output) };
        }
        Backend::Ssse3 => {
            return unsafe { x86::decode_ssse3::<A, S>(input, output) };
        }
        Backend::Scalar | Backend::Neon => {}
    }
    Ok((0, 0))
}
