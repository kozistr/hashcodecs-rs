use super::*;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::base64::{b64encode, encode_scalar};

#[test]
fn preserve_invalid_suffix() {
    for backend in [
        Backend::Avx512Vbmi,
        Backend::Avx2,
        Backend::Sse41,
        Backend::Ssse3,
        Backend::Neon,
    ] {
        if !backend::is_supported(backend) {
            continue;
        }

        for symbols in [32, 128] {
            let mut input = vec![b'A'; symbols];
            input.extend_from_slice(&[b'!'; 32]);

            for alphabet in [
                DecodeAlphabet::Standard,
                DecodeAlphabet::UrlSafe,
                DecodeAlphabet::Mixed,
            ] {
                for slack in [false, true] {
                    let mut output = vec![0xa5; input.len()];
                    assert_eq!(
                        unsafe {
                            decode_with_backend_ptr_mode(
                                &input,
                                output.as_mut_ptr(),
                                backend,
                                alphabet,
                                slack,
                                ErrorWritePolicy::ValidatedBlocksOnly,
                            )
                        },
                        Err(Base64Error::InvalidInput)
                    );
                    assert!(
                        output[symbols / 4 * 3..].iter().all(|&byte| byte == 0xa5),
                        "{backend:?}, {symbols}, {alphabet:?}, slack={slack}"
                    );
                }
            }
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn select_x86_kernels() {
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
            decode_x86_kernel::<x86_contracts::StandardDecoder, x86_contracts::ExactStore>(backend)
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
        decode_valid_prefix_x86_kernel::<x86_contracts::StandardDecoder>(Backend::Scalar).is_none()
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_avx2_store_modes() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    let input = vec![0x5a_u8; 192];
    let expected = b64encode(&input);
    let mut storage = vec![0xa5_u8; expected.len() + 16];
    let offset = storage.as_mut_ptr().align_offset(16);
    let output = &mut storage[offset..offset + expected.len()];

    for streaming_stores in [false, true] {
        output.fill(0xa5);
        let consumed = unsafe {
            encode_x86::<false>(&input, output.as_mut_ptr(), Backend::Avx2, streaming_stores)
        };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], false);
        assert_eq!(
            output,
            expected.as_bytes(),
            "streaming_stores={streaming_stores}"
        );
    }
}

#[cfg(all(feature = "python", any(target_arch = "x86", target_arch = "x86_64")))]
#[test]
fn preserve_custom_store_bounds() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    let input = (0..288)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let alphabet = CustomEncodeAlphabet::new(*b"@#");
    let mut expected = b64encode(&input).into_bytes();

    for byte in &mut expected {
        match *byte {
            b'+' => *byte = b'@',
            b'/' => *byte = b'#',
            _ => {}
        }
    }

    let mut storage = vec![0xa5_u8; expected.len() + 32];
    let offset = storage.as_mut_ptr().align_offset(32);
    let output = &mut storage[offset..offset + expected.len()];

    for streaming_stores in [false, true] {
        output.fill(0xa5);
        let consumed = unsafe {
            encode_custom_with_backend_inner(
                &input,
                output.as_mut_ptr(),
                Backend::Avx2,
                &alphabet,
                streaming_stores,
            )
        };
        let written = consumed / 3 * 4;
        assert_eq!(&output[..written], &expected[..written]);
        assert!(output[written..].iter().all(|&byte| byte == 0xa5));
    }
}
