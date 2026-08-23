#[cfg(target_arch = "aarch64")]
use super::aarch64;
use super::backend::{self, Backend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86;
use super::{Base64Error, DecodeAlphabet};

#[inline]
pub(super) unsafe fn encode_simd_ptr(input: &[u8], output: *mut u8, urlsafe: bool) -> usize {
    let backend = backend::selected();
    unsafe {
        encode_with_backend_ptr(
            input,
            output,
            backend.kind,
            urlsafe,
            backend.use_streaming_stores(input.len(), output),
        )
    }
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
            backend::selected().kind,
            alphabet,
            padded_stores,
            transactional_errors,
        )
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
    if !backend::is_supported(backend) {
        return 0;
    }
    unsafe { encode_with_backend_ptr(input, output.as_mut_ptr(), backend, urlsafe, false) }
}

#[inline]
unsafe fn encode_with_backend_ptr(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    urlsafe: bool,
    streaming_stores: bool,
) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if urlsafe {
            unsafe { encode_x86::<true>(input, output, backend, streaming_stores) }
        } else {
            unsafe { encode_x86::<false>(input, output, backend, streaming_stores) }
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
        let _ = (input, output, backend, urlsafe, streaming_stores);
        0
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn encode_x86<const URLSAFE: bool>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    streaming_stores: bool,
) -> usize {
    if backend == Backend::Avx2 {
        let store_mode = if streaming_stores {
            x86::Avx2StoreMode::Streaming
        } else {
            x86::Avx2StoreMode::Cached
        };
        return unsafe { x86::encode_avx2_with_store::<URLSAFE>(input, output, store_mode) };
    }
    let Some(kernel) = encode_x86_kernel::<URLSAFE>(backend) else {
        return 0;
    };
    unsafe { kernel(input, output) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type EncodeKernel = unsafe fn(&[u8], *mut u8) -> usize;

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn encode_x86_kernel<const URLSAFE: bool>(backend: Backend) -> Option<EncodeKernel> {
    match backend {
        Backend::Avx512 => Some(x86::encode_avx512::<URLSAFE>),
        Backend::Avx2 => Some(x86::encode_avx2::<URLSAFE>),
        Backend::Sse41 | Backend::Ssse3 => Some(x86::encode_ssse3::<URLSAFE>),
        Backend::Scalar | Backend::Neon => None,
    }
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
    if !backend::is_supported(backend) {
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
    let Some(kernel) = decode_x86_kernel::<A, S>(backend) else {
        return Ok((0, 0));
    };
    unsafe { kernel(input, output) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type DecodeKernel = unsafe fn(&[u8], *mut u8) -> Result<(usize, usize), Base64Error>;

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn decode_x86_kernel<A: x86::Decoder, S: x86::Store>(backend: Backend) -> Option<DecodeKernel> {
    match backend {
        Backend::Avx512 => Some(x86::decode_avx512::<A, S>),
        Backend::Avx2 => Some(x86::decode_avx2::<A, S>),
        Backend::Sse41 => Some(x86::decode_sse41::<A, S>),
        Backend::Ssse3 => Some(x86::decode_ssse3::<A, S>),
        Backend::Scalar | Backend::Neon => None,
    }
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;
    use crate::base64::{b64encode, encode_scalar};

    #[test]
    fn every_x86_backend_resolves_without_running_unsupported_instructions() {
        for backend in [
            Backend::Avx512,
            Backend::Avx2,
            Backend::Sse41,
            Backend::Ssse3,
        ] {
            assert!(encode_x86_kernel::<false>(backend).is_some());
            assert!(decode_x86_kernel::<x86::StandardDecoder, x86::ExactStore>(backend).is_some());
        }
        assert!(encode_x86_kernel::<true>(Backend::Scalar).is_none());
        assert!(
            decode_x86_kernel::<x86::UrlSafeDecoder, x86::PaddedStore>(Backend::Neon).is_none()
        );
    }

    #[test]
    fn avx2_cached_wrapper_and_streaming_dispatch_match() {
        if backend::is_supported(Backend::Avx2) {
            check_avx2_cached_wrapper_and_streaming_dispatch();
        }
    }

    fn check_avx2_cached_wrapper_and_streaming_dispatch() {
        let input = vec![0x5a_u8; 192];
        let expected = b64encode(&input);
        let mut storage = vec![0xa5_u8; expected.len() + 16];
        let offset = storage.as_mut_ptr().align_offset(16);
        let output = &mut storage[offset..offset + expected.len()];

        let consumed = unsafe { x86::encode_avx2::<false>(&input, output.as_mut_ptr()) };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], false);
        assert_eq!(output, expected.as_bytes());

        output.fill(0xa5);
        let consumed =
            unsafe { encode_x86::<false>(&input, output.as_mut_ptr(), Backend::Avx2, true) };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], false);
        assert_eq!(output, expected.as_bytes());
    }
}
