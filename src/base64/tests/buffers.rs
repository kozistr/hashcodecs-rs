use crate::base64::alphabet::{
    DECODE_STORE_PADDING, INVALID_VALUE, MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE,
};
use crate::base64::backend::{self, Backend};
use crate::base64::runtime_dispatch::{
    decode_standard_validated_with_backend, decode_with_backend_ptr,
};
use crate::base64::{
    Base64Error, DecodeAlphabet, b64decode, b64decode_into, b64decode_urlsafe,
    b64decode_urlsafe_into, b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into,
    decode_layout, decode_standard_validated_to_ptr, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet_validated_blocks,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks, decode_unpadded_layout,
};
use base64::Engine;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::base64::decode::x86_contracts;

#[test]
fn preserve_validated_bounds() {
    let input: Vec<u8> = (0..204)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    let encoded = b64encode(&input);

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

        let mut output = vec![0xa5; input.len() + 16];
        let (consumed, written) = unsafe {
            decode_standard_validated_with_backend(encoded.as_bytes(), output.as_mut_ptr(), backend)
        };
        assert_eq!(written, consumed / 4 * 3, "{backend:?}");
        assert_eq!(&output[..written], &input[..written], "{backend:?}");
        assert!(output[written..].iter().all(|&byte| byte == 0xa5));
    }

    let unsupported = [
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Avx2,
        Backend::Avx512Vbmi,
    ]
    .into_iter()
    .find(|&backend| !backend::is_supported(backend))
    .expect("every host has an unsupported architecture-specific backend");
    let mut output = [0xa5; 16];
    assert_eq!(
        unsafe {
            decode_standard_validated_with_backend(
                encoded.as_bytes(),
                output.as_mut_ptr(),
                unsupported,
            )
        },
        (0, 0)
    );
    assert_eq!(output, [0xa5; 16]);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    assert_eq!(
        <x86_contracts::ValidatedStandardDecoder as x86_contracts::Decoder>::decode_table(),
        &STANDARD_DECODE
    );
}

#[test]
fn decode_unpadded_tails() {
    const GUARD: usize = 16;

    for length in 0..=257 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();
        let encoded = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&input);
        let mut guarded = vec![0xa5; GUARD + input.len() + GUARD];

        let written = unsafe {
            decode_standard_validated_to_ptr(encoded.as_bytes(), guarded.as_mut_ptr().add(GUARD))
        };

        assert_eq!(written, input.len());
        assert_eq!(
            &guarded[GUARD..GUARD + input.len()],
            input,
            "length={length}"
        );
        assert!(guarded[..GUARD].iter().all(|&byte| byte == 0xa5));
        assert!(
            guarded[GUARD + input.len()..]
                .iter()
                .all(|&byte| byte == 0xa5)
        );
    }
}

#[test]
fn preserve_unpadded_bounds() {
    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    for length in 0..=1024 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();

        for (encoded, alphabet) in [
            (
                base64::engine::general_purpose::STANDARD
                    .encode(&input)
                    .trim_end_matches('=')
                    .as_bytes()
                    .to_vec(),
                DecodeAlphabet::Standard,
            ),
            (
                base64::engine::general_purpose::URL_SAFE
                    .encode(&input)
                    .trim_end_matches('=')
                    .as_bytes()
                    .to_vec(),
                DecodeAlphabet::UrlSafe,
            ),
        ] {
            let layout = decode_unpadded_layout(&encoded).unwrap();
            assert_eq!(layout.output_len(), input.len(), "length={length}");
            let mut guarded = vec![CANARY; GUARD + layout.output_len() + GUARD];
            let output = &mut guarded[GUARD..GUARD + layout.output_len()];
            decode_to_slice_with_unpadded_layout_and_alphabet(&encoded, output, layout, alphabet)
                .unwrap();
            assert_eq!(output, input, "length={length} alphabet={alphabet:?}");
            let mut validated_blocks = vec![CANARY; layout.output_len()];
            decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
                &encoded,
                &mut validated_blocks,
                layout,
                alphabet,
            )
            .unwrap();
            assert_eq!(
                validated_blocks, input,
                "length={length} alphabet={alphabet:?}"
            );
            let mut direct = vec![CANARY; layout.output_len()];
            unsafe {
                decode_to_ptr_with_unpadded_layout(&encoded, direct.as_mut_ptr(), layout, alphabet)
            }
            .unwrap();
            assert_eq!(direct, input, "length={length} alphabet={alphabet:?}");
            assert!(guarded[..GUARD].iter().all(|&byte| byte == CANARY));
            assert!(
                guarded[GUARD + layout.output_len()..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }

    assert!(matches!(
        decode_unpadded_layout(b"A"),
        Err(Base64Error::InvalidInput)
    ));
}

#[test]
fn reject_invalid_tails() {
    const CANARY: u8 = 0xa5;

    for alphabet in [
        (DecodeAlphabet::Standard, &STANDARD_DECODE),
        (DecodeAlphabet::UrlSafe, &URLSAFE_DECODE),
        (DecodeAlphabet::Mixed, &MIXED_DECODE),
    ] {
        for tail_len in [2, 3] {
            for position in 0..tail_len {
                for byte in 0..=u8::MAX {
                    if alphabet.1[byte as usize] != INVALID_VALUE {
                        continue;
                    }

                    let mut encoded = vec![b'A'; tail_len];
                    encoded[position] = byte;
                    let layout = decode_unpadded_layout(&encoded).unwrap();
                    let mut output = [CANARY; 2];
                    assert_eq!(
                        decode_to_slice_with_unpadded_layout_and_alphabet(
                            &encoded,
                            &mut output[..layout.output_len()],
                            layout,
                            alphabet.0,
                        ),
                        Err(Base64Error::InvalidInput),
                        "tail_len={tail_len} position={position} byte={byte} alphabet={:?}",
                        alphabet.0,
                    );
                    assert_eq!(output, [CANARY; 2]);
                    assert_eq!(
                        decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
                            &encoded,
                            &mut output[..layout.output_len()],
                            layout,
                            alphabet.0,
                        ),
                        Err(Base64Error::InvalidInput),
                    );
                    assert_eq!(output, [CANARY; 2]);
                }
            }
        }
    }
}

#[test]
fn reject_invalid_prefix() {
    let encoded = b"!AAAaa";
    let layout = decode_unpadded_layout(encoded).unwrap();
    let mut output = [0xa5; 4];

    assert_eq!(
        decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
            encoded,
            &mut output,
            layout,
            DecodeAlphabet::Standard,
        ),
        Err(Base64Error::InvalidInput),
    );
    assert_eq!(output, [0xa5; 4]);
}

#[test]
fn match_validated_blocks() {
    let input: Vec<u8> = (0..96)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    let encoded = b64encode(&input);
    let layout = decode_layout(encoded.as_bytes()).unwrap();
    let mut decoded = vec![0xa5; layout.output_len()];

    decode_to_slice_with_layout_and_alphabet_validated_blocks(
        encoded.as_bytes(),
        &mut decoded,
        layout,
        DecodeAlphabet::Standard,
    )
    .unwrap();

    assert_eq!(decoded, input);
}

#[test]
fn check_public_buffers() {
    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    for length in 0..=1024 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();

        for urlsafe in [false, true] {
            let expected = if urlsafe {
                base64::engine::general_purpose::URL_SAFE.encode(&input)
            } else {
                base64::engine::general_purpose::STANDARD.encode(&input)
            };

            let encode = if urlsafe {
                b64encode_urlsafe
            } else {
                b64encode
            };
            let decode = if urlsafe {
                b64decode_urlsafe
            } else {
                b64decode
            };
            assert_eq!(
                encode(&input),
                expected,
                "length={length} urlsafe={urlsafe}"
            );
            assert_eq!(decode(expected.as_bytes()).unwrap(), input);

            let encoded_len = expected.len();
            let mut encoded = vec![CANARY; encoded_len + GUARD * 2];
            for extra in [0, GUARD] {
                encoded.fill(CANARY);
                let written = if urlsafe {
                    b64encode_urlsafe_into(&input, &mut encoded[GUARD..GUARD + encoded_len + extra])
                } else {
                    b64encode_into(&input, &mut encoded[GUARD..GUARD + encoded_len + extra])
                }
                .unwrap();
                assert_eq!(written, encoded_len, "encode length={length}");
                assert_eq!(
                    &encoded[GUARD..GUARD + encoded_len],
                    expected.as_bytes(),
                    "encode length={length} urlsafe={urlsafe}"
                );
                assert!(encoded[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(
                    encoded[GUARD + encoded_len..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }

            let mut decoded = vec![CANARY; length + GUARD * 2];
            for extra in [0, GUARD] {
                decoded.fill(CANARY);
                let written = if urlsafe {
                    b64decode_urlsafe_into(
                        expected.as_bytes(),
                        &mut decoded[GUARD..GUARD + length + extra],
                    )
                } else {
                    b64decode_into(
                        expected.as_bytes(),
                        &mut decoded[GUARD..GUARD + length + extra],
                    )
                }
                .unwrap();
                assert_eq!(written, length, "decode length={length}");
                assert_eq!(
                    &decoded[GUARD..GUARD + length],
                    input,
                    "decode length={length} urlsafe={urlsafe}"
                );
                assert!(decoded[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(decoded[GUARD + length..].iter().all(|&byte| byte == CANARY));
            }

            // Capacity failures must not write even the first output byte.
            if encoded_len != 0 {
                encoded.fill(CANARY);
                let output = &mut encoded[GUARD..GUARD + encoded_len - 1];
                let result = if urlsafe {
                    b64encode_urlsafe_into(&input, output)
                } else {
                    b64encode_into(&input, output)
                };
                assert_eq!(
                    result,
                    Err(Base64Error::OutputTooSmall {
                        required: encoded_len,
                        provided: encoded_len - 1,
                    })
                );
                assert!(encoded.iter().all(|&byte| byte == CANARY));
            }
            if length != 0 {
                decoded.fill(CANARY);
                let output = &mut decoded[GUARD..GUARD + length - 1];
                let result = if urlsafe {
                    b64decode_urlsafe_into(expected.as_bytes(), output)
                } else {
                    b64decode_into(expected.as_bytes(), output)
                };
                assert_eq!(
                    result,
                    Err(Base64Error::OutputTooSmall {
                        required: length,
                        provided: length - 1,
                    })
                );
                assert!(decoded.iter().all(|&byte| byte == CANARY));
            }
        }
    }
}

#[test]
fn preserve_padded_guards() {
    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    for length in 0..=1024 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();

        for (encoded, alphabet) in [
            (
                base64::engine::general_purpose::STANDARD.encode(&input),
                DecodeAlphabet::Standard,
            ),
            (
                base64::engine::general_purpose::URL_SAFE.encode(&input),
                DecodeAlphabet::UrlSafe,
            ),
        ] {
            let layout = decode_layout(encoded.as_bytes()).unwrap();
            let mut output =
                vec![CANARY; GUARD + layout.output_len() + DECODE_STORE_PADDING + GUARD];
            unsafe {
                decode_to_ptr_with_layout(
                    encoded.as_bytes(),
                    output.as_mut_ptr().add(GUARD),
                    layout,
                    alphabet,
                    true,
                )
            }
            .unwrap();

            assert_eq!(&output[GUARD..GUARD + length], input);
            assert!(output[..GUARD].iter().all(|&byte| byte == CANARY));
            assert!(
                output[GUARD + length + DECODE_STORE_PADDING..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }

    let has_ssse3 = backend::is_supported(Backend::Ssse3);
    let input: Vec<u8> = (0..96).map(|value| value as u8).collect();

    for (encoded, alphabet) in [
        (b64encode(&input), DecodeAlphabet::Standard),
        (b64encode_urlsafe(&input), DecodeAlphabet::UrlSafe),
    ] {
        let mut output = vec![CANARY; input.len() + DECODE_STORE_PADDING + GUARD];
        let offsets = unsafe {
            decode_with_backend_ptr(
                encoded.as_bytes(),
                output.as_mut_ptr(),
                Backend::Ssse3,
                alphabet,
                true,
            )
        }
        .unwrap();
        let mut expected_offsets = (0, 0);

        if has_ssse3 {
            expected_offsets = (encoded.len(), input.len());
        }

        assert_eq!(offsets, expected_offsets);
        assert!(!has_ssse3 || output[..input.len()] == input);
        assert!(
            output[input.len() + DECODE_STORE_PADDING..]
                .iter()
                .all(|&byte| byte == CANARY)
        );
    }
}
