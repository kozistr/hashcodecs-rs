use crate::backend::{Capabilities, CpuFeature};
use crate::base64::backend::{self, Backend};
use crate::base64::runtime_dispatch::{decode_with_backend, encode_with_backend};
use crate::base64::{
    Base64Error, DecodeAlphabet, b64encode, b64encode_urlsafe, decode_layout,
    decode_to_slice_with_layout_and_alphabet, encode_scalar,
};
use base64::Engine;

#[cfg(feature = "python")]
use crate::base64::{encode::CustomEncodeAlphabet, runtime_dispatch::encode_custom_with_backend};

#[test]
fn select_backend() {
    for (features, expected) in [
        (&[][..], Backend::Scalar),
        (&[CpuFeature::Neon][..], Backend::Neon),
        (&[CpuFeature::Ssse3][..], Backend::Ssse3),
        (&[CpuFeature::Sse41][..], Backend::Scalar),
        (&[CpuFeature::Sse41, CpuFeature::Ssse3][..], Backend::Sse41),
        (&[CpuFeature::Avx2][..], Backend::Scalar),
        (&[CpuFeature::Avx2, CpuFeature::Ssse3][..], Backend::Avx2),
        (
            &[
                CpuFeature::Avx512F,
                CpuFeature::Avx512Bw,
                CpuFeature::Avx512Vbmi,
            ][..],
            Backend::Scalar,
        ),
        (
            &[
                CpuFeature::Avx512F,
                CpuFeature::Avx512Bw,
                CpuFeature::Avx512Vbmi,
                CpuFeature::Avx2,
                CpuFeature::Ssse3,
            ][..],
            Backend::Avx512Vbmi,
        ),
    ] {
        assert_eq!(
            backend::select_backend(Capabilities::from_features(features)),
            expected,
            "features={features:?}",
        );
    }
    assert!(backend::is_supported(Backend::Scalar));
}

#[test]
fn preserve_unsupported_outputs() {
    let input: Vec<u8> = (0..96).map(|value| value as u8).collect();
    let expected = b64encode(&input);
    let mut scalar = vec![0; expected.len()];
    assert_eq!(
        encode_with_backend(&input, &mut scalar, Backend::Scalar, false),
        0
    );
    encode_scalar(&input, &mut scalar, false);
    assert_eq!(scalar, expected.as_bytes());
    let mut scalar_decoded = vec![0; input.len()];
    assert_eq!(
        decode_with_backend(
            expected.as_bytes(),
            &mut scalar_decoded,
            Backend::Scalar,
            DecodeAlphabet::Standard,
        )
        .unwrap(),
        (0, 0)
    );

    for backend in [
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Avx2,
        Backend::Avx512Vbmi,
    ]
    .into_iter()
    .filter(|candidate| !backend::is_supported(*candidate))
    {
        let mut encoded_guard = vec![0xa5; expected.len()];
        assert_eq!(
            encode_with_backend(&input, &mut encoded_guard, backend, false),
            0,
            "backend={backend:?}"
        );
        assert!(encoded_guard.iter().all(|byte| *byte == 0xa5));

        let mut decoded_guard = vec![0xa5; input.len()];
        assert_eq!(
            decode_with_backend(
                expected.as_bytes(),
                &mut decoded_guard,
                backend,
                DecodeAlphabet::Standard,
            ),
            Ok((0, 0)),
            "backend={backend:?}"
        );
        assert!(decoded_guard.iter().all(|byte| *byte == 0xa5));
    }
}

#[test]
fn match_scalar() {
    let input: Vec<u8> = (0..96).map(|value| value as u8).collect();
    let expected = b64encode(&input);
    let expected_urlsafe = b64encode_urlsafe(&input);
    let mixed = b"-///".repeat(32);
    let mixed_expected = [0xfb, 0xff, 0xff].repeat(32);

    for backend in [
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Avx2,
        Backend::Avx512Vbmi,
    ]
    .into_iter()
    .filter(|candidate| backend::is_supported(*candidate))
    {
        let expected_offsets = (expected.len(), input.len());

        let mut encoded = vec![0; expected.len()];
        let consumed = encode_with_backend(&input, &mut encoded, backend, false);
        encode_scalar(&input[consumed..], &mut encoded[consumed / 3 * 4..], false);
        assert_eq!(encoded, expected.as_bytes(), "backend={backend:?}");

        let mut urlsafe_encoded = vec![0; expected_urlsafe.len()];
        let consumed = encode_with_backend(&input, &mut urlsafe_encoded, backend, true);
        encode_scalar(
            &input[consumed..],
            &mut urlsafe_encoded[consumed / 3 * 4..],
            true,
        );
        assert_eq!(
            urlsafe_encoded,
            expected_urlsafe.as_bytes(),
            "backend={backend:?}"
        );

        let mut decoded = vec![0; input.len()];
        assert_eq!(
            decode_with_backend(
                expected.as_bytes(),
                &mut decoded,
                backend,
                DecodeAlphabet::Standard,
            )
            .unwrap(),
            expected_offsets
        );
        assert_eq!(decoded, input, "backend={backend:?}");

        let mut urlsafe_decoded = vec![0; input.len()];
        assert_eq!(
            decode_with_backend(
                expected_urlsafe.as_bytes(),
                &mut urlsafe_decoded,
                backend,
                DecodeAlphabet::UrlSafe,
            )
            .unwrap(),
            expected_offsets
        );
        assert_eq!(urlsafe_decoded, input, "backend={backend:?}");

        let mut mixed_decoded = vec![0; mixed_expected.len()];
        assert_eq!(
            decode_with_backend(&mixed, &mut mixed_decoded, backend, DecodeAlphabet::Mixed,)
                .unwrap(),
            (mixed.len(), mixed_expected.len())
        );
        assert_eq!(mixed_decoded, mixed_expected, "backend={backend:?}");

        let mut invalid_output = [0; 12];
        let invalid = decode_with_backend(
            b"AAAAAAAAAAAAAAA!",
            &mut invalid_output,
            backend,
            DecodeAlphabet::Standard,
        );
        assert_eq!(invalid, Err(Base64Error::InvalidInput));

        let mut invalid_wide = [b'A'; 64];
        invalid_wide[63] = b'!';
        assert_eq!(
            decode_with_backend(
                &invalid_wide,
                &mut [0; 48],
                backend,
                DecodeAlphabet::Standard,
            ),
            Err(Base64Error::InvalidInput)
        );

        let mut invalid_double_block = [b'A'; 128];
        invalid_double_block[127] = b'!';
        assert_eq!(
            decode_with_backend(
                &invalid_double_block,
                &mut [0; 96],
                backend,
                DecodeAlphabet::Standard,
            ),
            Err(Base64Error::InvalidInput)
        );
    }

    let mut scalar_mixed = [0; 3];
    decode_to_slice_with_layout_and_alphabet(
        b"-///",
        &mut scalar_mixed,
        decode_layout(b"-///").unwrap(),
        DecodeAlphabet::Mixed,
    )
    .unwrap();
    assert_eq!(scalar_mixed, [0xfb, 0xff, 0xff]);
}

#[test]
fn encode_unaligned_inputs() {
    const GUARD: usize = 16;
    const CANARY: u8 = 0xa5;

    for input_offset in 0..16 {
        for length in 0..=32 {
            let mut guarded_input = vec![CANARY; input_offset + length + GUARD];

            for (index, byte) in guarded_input[input_offset..input_offset + length]
                .iter_mut()
                .enumerate()
            {
                *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
            }

            let input = &guarded_input[input_offset..input_offset + length];

            for urlsafe in [false, true] {
                let expected = if urlsafe {
                    base64::engine::general_purpose::URL_SAFE.encode(input)
                } else {
                    base64::engine::general_purpose::STANDARD.encode(input)
                };

                let mut guarded_output = vec![CANARY; GUARD + expected.len() + GUARD];
                encode_scalar(
                    input,
                    &mut guarded_output[GUARD..GUARD + expected.len()],
                    urlsafe,
                );

                assert_eq!(
                    &guarded_output[GUARD..GUARD + expected.len()],
                    expected.as_bytes(),
                    "length={length} input_offset={input_offset} urlsafe={urlsafe}"
                );
                assert!(guarded_output[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(
                    guarded_output[GUARD + expected.len()..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }
        }
    }
}

#[cfg(feature = "python")]
#[test]
fn encode_custom_alphabets() {
    let input = (0..=1024)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let alphabet = CustomEncodeAlphabet::new(*b"@#");

    for backend in [
        Backend::Scalar,
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Avx2,
        Backend::Avx512Vbmi,
    ] {
        if !backend::is_supported(backend) {
            continue;
        }

        for length in [16, 31, 32, 47, 48, 63, 64, 95, 96, 212, 1024] {
            let mut expected = b64encode(&input[..length]).into_bytes();

            for byte in &mut expected {
                if *byte == b'+' {
                    *byte = b'@';
                } else if *byte == b'/' {
                    *byte = b'#';
                }
            }

            let mut actual = vec![0xa5; expected.len() + 16];
            let consumed = unsafe {
                encode_custom_with_backend(
                    &input[..length],
                    actual.as_mut_ptr(),
                    backend,
                    &alphabet,
                )
            };
            let written = consumed / 3 * 4;
            assert_eq!(
                &actual[..written],
                &expected[..written],
                "backend={backend:?}"
            );
            assert!(actual[written..].iter().all(|&byte| byte == 0xa5));
        }
    }
}
