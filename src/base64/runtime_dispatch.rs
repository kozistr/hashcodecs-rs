//! Select and call Base64 SIMD kernels at runtime.

use super::backend::{self, Backend};
#[cfg(target_arch = "aarch64")]
use super::decode::aarch64 as decode_aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::decode::{self as decode_backend, x86_contracts};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::encode as encode_backend;
#[cfg(target_arch = "aarch64")]
use super::encode::aarch64 as encode_aarch64;
use super::{Base64Error, DecodeAlphabet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorWritePolicy {
    Partial,
    ValidatedBlocksOnly,
}

#[inline]
pub(super) unsafe fn encode_with_runtime_backend(
    input: &[u8],
    output: *mut u8,
    urlsafe: bool,
    allow_streaming_stores: bool,
) -> usize {
    let selection = backend::selected_backend();
    unsafe {
        encode_with_backend_ptr(
            input,
            output,
            selection.backend,
            urlsafe,
            allow_streaming_stores && selection.use_streaming_stores(input.len(), output),
        )
    }
}

#[inline]
pub(super) unsafe fn decode_with_runtime_backend(
    input: &[u8],
    output: *mut u8,
    alphabet: DecodeAlphabet,
    output_has_store_slack: bool,
    error_write_policy: ErrorWritePolicy,
) -> Result<(usize, usize), Base64Error> {
    unsafe {
        decode_with_backend_ptr_mode(
            input,
            output,
            backend::selected_backend().backend,
            alphabet,
            output_has_store_slack,
            error_write_policy,
        )
    }
}

#[inline]
pub(super) fn validate_with_runtime_backend(
    input: &[u8],
    alphabet: DecodeAlphabet,
) -> Result<usize, Base64Error> {
    validate_with_backend_inner(input, backend::selected_backend().backend, alphabet)
}

#[inline]
pub(super) unsafe fn decode_valid_prefix_with_runtime_backend(
    input: &[u8],
    output: *mut u8,
    alphabet: DecodeAlphabet,
) -> Option<(usize, usize)> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let backend = backend::selected_backend().backend;
        unsafe { decode_valid_prefix_x86_alphabet(input, output, backend, alphabet) }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let _ = (input, output, alphabet);
        None
    }
}

#[inline]
#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
pub(super) unsafe fn decode_valid_prefix_with_backend(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> (usize, usize) {
    if !backend::is_supported(backend) {
        return (0, 0);
    }
    unsafe { decode_valid_prefix_x86_alphabet(input, output, backend, alphabet) }.unwrap_or((0, 0))
}

#[inline]
#[cfg(test)]
pub(super) fn validate_with_backend(
    input: &[u8],
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> Result<usize, Base64Error> {
    if !backend::is_supported(backend) {
        return Ok(0);
    }
    validate_with_backend_inner(input, backend, alphabet)
}

#[inline]
fn validate_with_backend_inner(
    input: &[u8],
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> Result<usize, Base64Error> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return match alphabet {
        DecodeAlphabet::Standard => validate_x86::<x86_contracts::StandardDecoder>(input, backend),
        DecodeAlphabet::UrlSafe => validate_x86::<x86_contracts::UrlSafeDecoder>(input, backend),
        DecodeAlphabet::Mixed => validate_x86::<x86_contracts::MixedDecoder>(input, backend),
    };

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        #[cfg(target_arch = "aarch64")]
        if backend == Backend::Neon {
            return unsafe {
                match alphabet {
                    DecodeAlphabet::Standard => decode_aarch64::validate::<false, false>(input),
                    DecodeAlphabet::UrlSafe => decode_aarch64::validate::<true, false>(input),
                    DecodeAlphabet::Mixed => decode_aarch64::validate::<false, true>(input),
                }
            };
        }
        let _ = (input, backend, alphabet);
        Ok(0)
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn validate_x86<A: x86_contracts::Decoder>(
    input: &[u8],
    backend: Backend,
) -> Result<usize, Base64Error> {
    let Some(kernel) = validate_x86_kernel::<A>(backend) else {
        return Ok(0);
    };
    unsafe { kernel(input) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type ValidateKernel = unsafe fn(&[u8]) -> Result<usize, Base64Error>;

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn validate_x86_kernel<A: x86_contracts::Decoder>(backend: Backend) -> Option<ValidateKernel> {
    match backend {
        Backend::Avx512Vbmi => Some(decode_backend::avx512::validate::<A>),
        Backend::Avx2 => Some(decode_backend::avx2::validate::<A>),
        Backend::Sse41 => Some(decode_backend::sse41::validate::<A>),
        Backend::Ssse3 => Some(decode_backend::ssse3::validate::<A>),
        Backend::Scalar | Backend::Neon => None,
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_valid_prefix_x86_alphabet(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> Option<(usize, usize)> {
    match alphabet {
        DecodeAlphabet::Standard => unsafe {
            decode_valid_prefix_x86::<x86_contracts::StandardDecoder>(input, output, backend)
        },
        DecodeAlphabet::UrlSafe => unsafe {
            decode_valid_prefix_x86::<x86_contracts::UrlSafeDecoder>(input, output, backend)
        },
        DecodeAlphabet::Mixed => unsafe {
            decode_valid_prefix_x86::<x86_contracts::MixedDecoder>(input, output, backend)
        },
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type DecodePrefixKernel = unsafe fn(&[u8], *mut u8) -> (usize, usize);

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_valid_prefix_x86<A: x86_contracts::Decoder>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
) -> Option<(usize, usize)> {
    let kernel = decode_valid_prefix_x86_kernel::<A>(backend)?;
    Some(unsafe { kernel(input, output) })
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn decode_valid_prefix_x86_kernel<A: x86_contracts::Decoder>(
    backend: Backend,
) -> Option<DecodePrefixKernel> {
    match backend {
        Backend::Avx512Vbmi => Some(decode_backend::avx512::decode_prefix::<A>),
        Backend::Avx2 => Some(decode_backend::avx2::decode_prefix_avx2::<A>),
        Backend::Sse41 => Some(decode_backend::sse41::decode_prefix_sse41::<A>),
        Backend::Ssse3 => Some(decode_backend::ssse3::decode_prefix_ssse3::<A>),
        Backend::Scalar | Backend::Neon => None,
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
                    encode_aarch64::encode::<true>(input, output)
                } else {
                    encode_aarch64::encode::<false>(input, output)
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
            encode_backend::avx2::Avx2StoreMode::Streaming
        } else {
            encode_backend::avx2::Avx2StoreMode::Cached
        };
        return unsafe {
            encode_backend::avx2::encode_avx2_with_store::<URLSAFE>(input, output, store_mode)
        };
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
        Backend::Avx512Vbmi => Some(encode_backend::avx512::encode::<URLSAFE>),
        Backend::Sse41 | Backend::Ssse3 => Some(encode_backend::ssse3::encode::<URLSAFE>),
        Backend::Avx2 | Backend::Scalar | Backend::Neon => None,
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
    output_has_store_slack: bool,
) -> Result<(usize, usize), Base64Error> {
    if !backend::is_supported(backend) {
        return Ok((0, 0));
    }
    unsafe {
        decode_with_backend_ptr_mode(
            input,
            output,
            backend,
            alphabet,
            output_has_store_slack,
            ErrorWritePolicy::Partial,
        )
    }
}

#[inline]
unsafe fn decode_with_backend_ptr_mode(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
    output_has_store_slack: bool,
    error_write_policy: ErrorWritePolicy,
) -> Result<(usize, usize), Base64Error> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let _ = error_write_policy;
        unsafe { decode_x86_alphabet(input, output, backend, alphabet, output_has_store_slack) }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        #[cfg(target_arch = "aarch64")]
        if backend == Backend::Neon {
            let _ = output_has_store_slack;
            return unsafe {
                if error_write_policy == ErrorWritePolicy::ValidatedBlocksOnly {
                    match alphabet {
                        DecodeAlphabet::Standard => {
                            decode_aarch64::decode_validated_blocks::<false, false>(input, output)
                        }
                        DecodeAlphabet::UrlSafe => {
                            decode_aarch64::decode_validated_blocks::<true, false>(input, output)
                        }
                        DecodeAlphabet::Mixed => {
                            decode_aarch64::decode_validated_blocks::<false, true>(input, output)
                        }
                    }
                } else {
                    match alphabet {
                        DecodeAlphabet::Standard => {
                            decode_aarch64::decode::<false, false>(input, output)
                        }
                        DecodeAlphabet::UrlSafe => {
                            decode_aarch64::decode::<true, false>(input, output)
                        }
                        DecodeAlphabet::Mixed => {
                            decode_aarch64::decode::<false, true>(input, output)
                        }
                    }
                }
            };
        }
        let _ = (
            input,
            output,
            backend,
            alphabet,
            output_has_store_slack,
            error_write_policy,
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
    output_has_store_slack: bool,
) -> Result<(usize, usize), Base64Error> {
    match alphabet {
        DecodeAlphabet::Standard => unsafe {
            decode_x86_store::<x86_contracts::StandardDecoder>(
                input,
                output,
                backend,
                output_has_store_slack,
            )
        },
        DecodeAlphabet::UrlSafe => unsafe {
            decode_x86_store::<x86_contracts::UrlSafeDecoder>(
                input,
                output,
                backend,
                output_has_store_slack,
            )
        },
        DecodeAlphabet::Mixed => unsafe {
            decode_x86::<x86_contracts::MixedDecoder, x86_contracts::ExactStore>(
                input, output, backend,
            )
        },
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_x86_store<A: x86_contracts::Decoder>(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    output_has_store_slack: bool,
) -> Result<(usize, usize), Base64Error> {
    if output_has_store_slack {
        unsafe { decode_x86::<A, x86_contracts::PaddedStore>(input, output, backend) }
    } else {
        unsafe { decode_x86::<A, x86_contracts::ExactStore>(input, output, backend) }
    }
}

#[inline]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn decode_x86<A: x86_contracts::Decoder, S: x86_contracts::Store>(
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
fn decode_x86_kernel<A: x86_contracts::Decoder, S: x86_contracts::Store>(
    backend: Backend,
) -> Option<DecodeKernel> {
    match backend {
        Backend::Avx512Vbmi => Some(decode_backend::avx512::decode::<A, S>),
        Backend::Avx2 => Some(decode_backend::avx2::decode_avx2::<A, S>),
        Backend::Sse41 => Some(decode_backend::sse41::decode_sse41::<A, S>),
        Backend::Ssse3 => Some(decode_backend::ssse3::decode_ssse3::<A, S>),
        Backend::Scalar | Backend::Neon => None,
    }
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;
    use crate::base64::{b64encode, encode_scalar};

    #[test]
    fn every_x86_backend_has_a_dispatch_path_without_running_unsupported_instructions() {
        for backend in [
            Backend::Avx512Vbmi,
            Backend::Avx2,
            Backend::Sse41,
            Backend::Ssse3,
        ] {
            assert_eq!(
                encode_x86_kernel::<false>(backend).is_some(),
                backend != Backend::Avx2
            );
            assert!(
                decode_x86_kernel::<x86_contracts::StandardDecoder, x86_contracts::ExactStore>(
                    backend
                )
                .is_some()
            );
            assert!(validate_x86_kernel::<x86_contracts::StandardDecoder>(backend).is_some());
            assert!(
                decode_valid_prefix_x86_kernel::<x86_contracts::StandardDecoder>(backend).is_some()
            );
        }
        assert!(encode_x86_kernel::<true>(Backend::Scalar).is_none());
        assert!(
            decode_x86_kernel::<x86_contracts::UrlSafeDecoder, x86_contracts::PaddedStore>(
                Backend::Neon
            )
            .is_none()
        );
        assert!(validate_x86_kernel::<x86_contracts::StandardDecoder>(Backend::Scalar).is_none());
        assert!(
            decode_valid_prefix_x86_kernel::<x86_contracts::StandardDecoder>(Backend::Scalar)
                .is_none()
        );
    }

    #[test]
    fn avx2_cached_and_streaming_dispatch_match() {
        if backend::is_supported(Backend::Avx2) {
            check_avx2_cached_and_streaming_dispatch();
        }
    }

    fn check_avx2_cached_and_streaming_dispatch() {
        let input = vec![0x5a_u8; 192];
        let expected = b64encode(&input);
        let mut storage = vec![0xa5_u8; expected.len() + 16];
        let offset = storage.as_mut_ptr().align_offset(16);
        let output = &mut storage[offset..offset + expected.len()];

        let consumed =
            unsafe { encode_x86::<false>(&input, output.as_mut_ptr(), Backend::Avx2, false) };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], false);
        assert_eq!(output, expected.as_bytes());

        output.fill(0xa5);
        let consumed =
            unsafe { encode_x86::<false>(&input, output.as_mut_ptr(), Backend::Avx2, true) };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], false);
        assert_eq!(output, expected.as_bytes());
    }
}
