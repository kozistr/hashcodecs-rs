use super::alphabet::decode_table;
use super::backend::{self, Backend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::decode::{self as decode_backend, x86_contracts};
use super::encode as encode_backend;
use super::runtime_dispatch::{decode_with_backend, decode_with_backend_ptr, encode_with_backend};
use super::*;
use crate::backend::{Capabilities, CpuFeature};

#[cfg(target_arch = "aarch64")]
mod aarch64;

fn select_backend(features: &[CpuFeature]) -> Backend {
    backend::select_backend(Capabilities::from_features(features))
}
use base64::Engine;

#[test]
fn standard_and_url_safe_round_trip() {
    let input = b"the quick brown fox jumps over the lazy dog";
    assert_eq!(
        b64encode(input),
        "dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw=="
    );
    assert_eq!(
        b64decode(b"dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw==").unwrap(),
        input
    );
    assert_eq!(b64encode_urlsafe(b"\xfb\xff"), "-_8=");
    assert_eq!(b64decode_urlsafe(b"-_8=").unwrap(), b"\xfb\xff");
    assert_eq!(b64decode(b"YQ==").unwrap(), b"a");
    assert_eq!(b64decode(b"YWI=").unwrap(), b"ab");
}

#[test]
fn rejects_invalid_input() {
    assert_eq!(b64decode(b"AAAA!AAA"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"abc"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"A"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"A=AA"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"AA=A"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"===="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"Y!=="), Err(Base64Error::InvalidInput));
    assert_eq!(
        b64decode(b"AAAAAAAAAAAAAAA!"),
        Err(Base64Error::InvalidInput)
    );
    assert_eq!(
        b64decode(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!"),
        Err(Base64Error::InvalidInput)
    );
    let mut invalid_wide = [b'A'; 128];
    invalid_wide[127] = b'!';
    assert_eq!(b64decode(&invalid_wide), Err(Base64Error::InvalidInput));
}

#[test]
fn rust_decoders_explicitly_accept_noncanonical_trailing_bits() {
    assert_eq!(b64decode(b"AB==").as_deref(), Ok(&[0][..]));
    assert_eq!(b64decode_urlsafe(b"AB==").as_deref(), Ok(&[0][..]));
}

#[test]
fn matches_the_standard_engine_for_all_short_lengths() {
    for length in 0..=1024 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();
        let expected = base64::engine::general_purpose::STANDARD.encode(&input);
        assert_eq!(b64encode(&input), expected, "length={length}");
        assert_eq!(
            b64decode(expected.as_bytes()).unwrap(),
            input,
            "length={length}"
        );

        let url_safe = base64::engine::general_purpose::URL_SAFE.encode(&input);
        assert_eq!(b64encode_urlsafe(&input), url_safe, "length={length}");
        assert_eq!(
            b64decode_urlsafe(url_safe.as_bytes()).unwrap(),
            input,
            "url-safe length={length}"
        );
    }
}

#[test]
fn scalar_encoder_handles_every_short_length_and_input_alignment() {
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

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn seeded_randomized_inputs_match_the_reference_engine() {
    let mut state = 0x1828_97d4_3c61_5aef_u64;

    for case in 0..128 {
        let length = if case < 16 {
            case * 3
        } else {
            1025 + next_random(&mut state) as usize % (128 * 1024)
        };
        let input: Vec<u8> = (0..length).map(|_| next_random(&mut state) as u8).collect();

        let standard = base64::engine::general_purpose::STANDARD.encode(&input);
        assert_eq!(b64encode(&input), standard, "standard case={case}");
        assert_eq!(
            b64decode(standard.as_bytes()),
            Ok(input.clone()),
            "standard case={case}"
        );

        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(&input);
        assert_eq!(b64encode_urlsafe(&input), urlsafe, "URL-safe case={case}");
        assert_eq!(
            b64decode_urlsafe(urlsafe.as_bytes()),
            Ok(input),
            "URL-safe case={case}"
        );

        if !standard.is_empty() {
            let malformed_index = next_random(&mut state) as usize % standard.len();
            let mut malformed_standard = standard.into_bytes();
            malformed_standard[malformed_index] = b'!';
            assert_eq!(
                b64decode(&malformed_standard),
                Err(Base64Error::InvalidInput)
            );

            let malformed_index = next_random(&mut state) as usize % urlsafe.len();
            let mut malformed_urlsafe = urlsafe.into_bytes();
            malformed_urlsafe[malformed_index] = b'!';
            assert_eq!(
                b64decode_urlsafe(&malformed_urlsafe),
                Err(Base64Error::InvalidInput)
            );
        }
    }
}

#[test]
fn backend_selection_and_kernels_match_scalar_output() {
    assert_eq!(select_backend(&[]), Backend::Scalar);
    assert_eq!(select_backend(&[CpuFeature::Neon]), Backend::Neon);
    assert_eq!(select_backend(&[CpuFeature::Ssse3]), Backend::Ssse3);
    assert_eq!(select_backend(&[CpuFeature::Sse41]), Backend::Scalar);
    assert_eq!(
        select_backend(&[CpuFeature::Sse41, CpuFeature::Ssse3]),
        Backend::Sse41
    );
    assert_eq!(select_backend(&[CpuFeature::Avx2]), Backend::Scalar);
    assert_eq!(
        select_backend(&[CpuFeature::Avx2, CpuFeature::Ssse3]),
        Backend::Avx2
    );
    assert_eq!(
        select_backend(&[
            CpuFeature::Avx512F,
            CpuFeature::Avx512Bw,
            CpuFeature::Avx512Vbmi,
        ]),
        Backend::Scalar
    );
    assert_eq!(
        select_backend(&[
            CpuFeature::Avx512F,
            CpuFeature::Avx512Bw,
            CpuFeature::Avx512Vbmi,
            CpuFeature::Avx2,
            CpuFeature::Ssse3,
        ]),
        Backend::Avx512Vbmi
    );
    assert!(backend::is_supported(Backend::Scalar));
    assert_eq!(
        Base64Error::InvalidInput.to_string(),
        "invalid Base64 input"
    );

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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn avx512_decoder_tail_boundaries_match_scalar_output() {
    if !backend::is_supported(Backend::Avx512Vbmi) {
        return;
    }

    for length in [64, 68, 76, 80, 92, 96, 108, 112, 124, 128, 132, 140, 144] {
        let encoded = vec![b'A'; length];
        let mut decoded = vec![0xa5; length / 4 * 3];
        let (consumed, written) = decode_with_backend(
            &encoded,
            &mut decoded,
            Backend::Avx512Vbmi,
            DecodeAlphabet::Standard,
        )
        .unwrap();
        let remainder = length % 64;
        let expected_tail = remainder / 16 * 16;
        let expected_consumed = length - remainder + expected_tail;
        let expected_written = expected_consumed / 4 * 3;

        assert_eq!((consumed, written), (expected_consumed, expected_written));
        assert!(decoded[..written].iter().all(|byte| *byte == 0));
        assert!(decoded[written..].iter().all(|byte| *byte == 0xa5));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn avx2_encoder_shifted_load_boundaries_match_scalar_and_preserve_guards() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    // 32 activates the special first load, 52 activates the first shifted
    // load, and 124 activates the four-block unrolled shifted loop. The other
    // explicit boundaries exercise the AVX2 and scalar terminal tails.
    for length in (0..=160).chain([191, 192, 195, 196, 219, 220, 255, 256, 4095, 4096]) {
        let mut guarded_input = vec![CANARY; GUARD + length + GUARD];
        for (index, byte) in guarded_input[GUARD..GUARD + length].iter_mut().enumerate() {
            *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
        }
        let input = &guarded_input[GUARD..GUARD + length];

        for urlsafe in [false, true] {
            let expected = if urlsafe {
                base64::engine::general_purpose::URL_SAFE.encode(input)
            } else {
                base64::engine::general_purpose::STANDARD.encode(input)
            };
            let mut guarded_output = vec![CANARY; GUARD + expected.len() + GUARD];
            let output = &mut guarded_output[GUARD..GUARD + expected.len()];
            let consumed = encode_with_backend(input, output, Backend::Avx2, urlsafe);
            let simd_output_len = consumed / 3 * 4;
            let avx2_blocks = if length >= 32 { (length - 4) / 24 } else { 0 };
            let avx2_tail_blocks = if length >= 32 && length - avx2_blocks * 24 >= 16 {
                1
            } else {
                0
            };
            let ssse3_blocks = if (16..32).contains(&length) {
                (length - 4) / 12
            } else {
                0
            };

            assert_eq!(
                consumed,
                avx2_blocks * 24 + (avx2_tail_blocks + ssse3_blocks) * 12,
                "consumed length={length} urlsafe={urlsafe}"
            );

            assert_eq!(
                &output[..simd_output_len],
                &expected.as_bytes()[..simd_output_len],
                "SIMD prefix length={length} urlsafe={urlsafe}"
            );
            assert!(
                output[simd_output_len..].iter().all(|&byte| byte == CANARY),
                "SIMD suffix length={length} urlsafe={urlsafe}"
            );

            encode_scalar(&input[consumed..], &mut output[simd_output_len..], urlsafe);
            assert_eq!(
                output,
                expected.as_bytes(),
                "length={length} urlsafe={urlsafe}"
            );
            assert!(guarded_output[..GUARD].iter().all(|&byte| byte == CANARY));
            assert!(
                guarded_output[GUARD + expected.len()..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(guarded_input[..GUARD].iter().all(|&byte| byte == CANARY));
            assert!(
                guarded_input[GUARD + length..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn avx2_encoder_assembly_loop_matches_scalar_and_preserves_guards() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    for input_offset in 0..32 {
        for length in [64 * 1024, 64 * 1024 + 1, 64 * 1024 + 95, 64 * 1024 + 96] {
            let mut guarded_input = vec![CANARY; GUARD + input_offset + length + GUARD];
            let input = &mut guarded_input[GUARD + input_offset..GUARD + input_offset + length];
            for (index, byte) in input.iter_mut().enumerate() {
                *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
            }

            for urlsafe in [false, true] {
                let expected = if urlsafe {
                    base64::engine::general_purpose::URL_SAFE.encode(&*input)
                } else {
                    base64::engine::general_purpose::STANDARD.encode(&*input)
                };
                let output_offset = input_offset.wrapping_mul(7) & 31;
                let mut guarded_output =
                    vec![CANARY; GUARD + output_offset + expected.len() + GUARD];
                let output = &mut guarded_output
                    [GUARD + output_offset..GUARD + output_offset + expected.len()];
                let consumed = encode_with_backend(input, output, Backend::Avx2, urlsafe);
                let simd_output_len = consumed / 3 * 4;

                assert!(consumed >= 64 * 1024 - 40, "length={length}");
                assert_eq!(
                    &output[..simd_output_len],
                    &expected.as_bytes()[..simd_output_len],
                    "SIMD prefix length={length} input_offset={input_offset} output_offset={output_offset} urlsafe={urlsafe}"
                );

                encode_scalar(&input[consumed..], &mut output[simd_output_len..], urlsafe);
                assert_eq!(
                    output,
                    expected.as_bytes(),
                    "length={length} input_offset={input_offset} output_offset={output_offset} urlsafe={urlsafe}"
                );
                assert!(
                    guarded_output[..GUARD + output_offset]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
                assert!(
                    guarded_output[GUARD + output_offset + expected.len()..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }
        }
    }
}

#[test]
fn every_byte_is_classified_consistently_by_each_simd_decoder() {
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
        for (alphabet, table) in [
            (DecodeAlphabet::Standard, &STANDARD_DECODE),
            (DecodeAlphabet::UrlSafe, &URLSAFE_DECODE),
            (DecodeAlphabet::Mixed, &MIXED_DECODE),
        ] {
            for byte in 0..=u8::MAX {
                // AVX-512 falls through to AVX2 unless it receives a complete 64-byte block.
                let encoded_len = match backend {
                    Backend::Avx512Vbmi => 64,
                    Backend::Avx2 => 128,
                    _ => 16,
                };
                let decoded_len = encoded_len / 4 * 3;
                let encoded = vec![byte; encoded_len];
                let mut decoded = vec![0xa5; decoded_len + DECODE_STORE_PADDING];
                let result =
                    decode_with_backend(&encoded, &mut decoded[..decoded_len], backend, alphabet);
                let value = table[byte as usize];
                if value == INVALID_VALUE {
                    assert_eq!(result, Err(Base64Error::InvalidInput));
                    continue;
                }

                assert_eq!(result, Ok((encoded_len, decoded_len)));
                let expected = [
                    (value << 2) | (value >> 4),
                    (value << 4) | (value >> 2),
                    (value << 6) | value,
                ];
                assert_eq!(&decoded[..decoded_len], expected.repeat(encoded_len / 4));

                if !matches!(alphabet, DecodeAlphabet::Mixed) {
                    let padded = unsafe {
                        decode_with_backend_ptr(
                            &encoded,
                            decoded.as_mut_ptr(),
                            backend,
                            alphabet,
                            true,
                        )
                    };
                    assert_eq!(padded, Ok((encoded_len, decoded_len)));
                    assert_eq!(&decoded[..decoded_len], expected.repeat(encoded_len / 4));
                }
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn avx2_streaming_encoder_matches_scalar() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    for (input_len, urlsafe) in [(64 << 10) + 96, (64 << 10) + 192]
        .into_iter()
        .flat_map(|length| [(length, false), (length, true)])
    {
        let input = vec![0x5a_u8; input_len];
        let expected = if urlsafe {
            base64::engine::general_purpose::URL_SAFE.encode(&input)
        } else {
            base64::engine::general_purpose::STANDARD.encode(&input)
        };
        let mut guarded_output = vec![0xa5_u8; expected.len() + 16];
        let output_offset = guarded_output.as_mut_ptr().align_offset(16);
        let output = &mut guarded_output[output_offset..output_offset + expected.len()];

        let consumed = unsafe {
            if urlsafe {
                encode_backend::avx2::encode_avx2_with_store::<true>(
                    &input,
                    output.as_mut_ptr(),
                    encode_backend::avx2::Avx2StoreMode::Streaming,
                )
            } else {
                encode_backend::avx2::encode_avx2_with_store::<false>(
                    &input,
                    output.as_mut_ptr(),
                    encode_backend::avx2::Avx2StoreMode::Streaming,
                )
            }
        };
        encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], urlsafe);

        assert_eq!(output, expected.as_bytes());
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn avx512_control_vectors_describe_the_base64_transforms() {
    assert_eq!(
        <x86_contracts::StandardDecoder as x86_contracts::Decoder>::decode_table(),
        &STANDARD_DECODE
    );
    assert_eq!(
        <x86_contracts::UrlSafeDecoder as x86_contracts::Decoder>::decode_table(),
        &URLSAFE_DECODE
    );
    assert_eq!(
        <x86_contracts::MixedDecoder as x86_contracts::Decoder>::decode_table(),
        &MIXED_DECODE
    );

    let input: Vec<u8> = (0..48)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    let mut shuffled = [0_u8; 64];
    for (destination, &source) in encode_backend::avx512::ENCODE_SHUFFLE.iter().enumerate() {
        shuffled[destination] = input[source as usize];
    }

    let mut indices = [0_u8; 64];
    for lane in 0..8 {
        let lane_start = lane * 8;
        let word = u64::from_le_bytes(shuffled[lane_start..lane_start + 8].try_into().unwrap());
        for byte in 0..8 {
            let shift = encode_backend::avx512::MULTISHIFT_SHIFTS[byte];
            indices[lane_start + byte] = ((word >> shift) & 0x3f) as u8;
        }
    }

    let mut expected = [0_u8; 64];
    for group in 0..16 {
        let source = group * 3;
        let destination = group * 4;
        let first = input[source];
        let second = input[source + 1];
        let third = input[source + 2];
        expected[destination] = first >> 2;
        expected[destination + 1] = ((first & 0x03) << 4) | (second >> 4);
        expected[destination + 2] = ((second & 0x0f) << 2) | (third >> 6);
        expected[destination + 3] = third & 0x3f;
    }
    assert_eq!(indices, expected);

    let packed = core::array::from_fn::<_, 64, _>(|index| index as u8);
    let mut expected_decoded = Vec::with_capacity(48);
    for lane in 0..4 {
        for group in 0..4 {
            let source = lane * 16 + group * 4;
            expected_decoded.extend_from_slice(&[
                packed[source + 2],
                packed[source + 1],
                packed[source],
            ]);
        }
    }
    let decoded: Vec<u8> = decode_backend::avx512::DECODE_SHUFFLE[..48]
        .iter()
        .map(|&index| packed[index as usize])
        .collect();
    assert_eq!(decoded, expected_decoded);
}

#[test]
fn length_helpers_and_buffer_errors_are_precise() {
    assert_eq!(b64encoded_len(0), Some(0));
    assert_eq!(b64encoded_len(1), Some(4));
    assert_eq!(b64encoded_len(2), Some(4));
    assert_eq!(b64encoded_len(3), Some(4));
    assert_eq!(b64encoded_len(4), Some(8));
    assert_eq!(b64encoded_len(usize::MAX), None);
    assert_eq!(b64decoded_len(b""), Ok(0));
    assert_eq!(b64decoded_len(b"YQ=="), Ok(1));
    assert_eq!(b64decoded_len(b"YWI="), Ok(2));
    assert_eq!(b64decoded_len(b"YWJj"), Ok(3));
    assert_eq!(b64decoded_len(b"abc"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"===="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"A==="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"AA=A"), Err(Base64Error::InvalidInput));

    let error = Base64Error::OutputTooSmall {
        required: 8,
        provided: 3,
    };
    assert_eq!(
        error.to_string(),
        "Base64 output requires 8 bytes but the destination has 3"
    );

    let mut encoded = [0xa5; 3];
    assert_eq!(
        b64encode_into(b"hello", &mut encoded),
        Err(Base64Error::OutputTooSmall {
            required: 8,
            provided: 3,
        })
    );
    assert_eq!(encoded, [0xa5; 3]);

    let mut decoded = [0xa5; 2];
    assert_eq!(
        b64decode_into(b"aGVsbG8=", &mut decoded),
        Err(Base64Error::OutputTooSmall {
            required: 5,
            provided: 2,
        })
    );
    assert_eq!(decoded, [0xa5; 2]);
}

#[test]
#[should_panic(expected = "Base64 output slice must have the exact encoded length")]
fn safe_encoder_rejects_an_inexact_output_slice() {
    let mut output = [0_u8; 3];
    encode_backend::encode_to_slice(b"abc", &mut output, false);
}

#[test]
fn decode_tables_cover_both_alphabets() {
    for (urlsafe, mixed) in [(false, false), (true, false), (true, true)] {
        let table = decode_table(std::hint::black_box(urlsafe), std::hint::black_box(mixed));
        for (index, &byte) in STANDARD_ALPHABET.iter().enumerate() {
            let expected = if urlsafe && !mixed && index >= 62 {
                INVALID_VALUE
            } else {
                index as u8
            };
            assert_eq!(table[byte as usize], expected);
        }
        for (index, &byte) in URLSAFE_ALPHABET.iter().enumerate() {
            let expected = if !urlsafe && !mixed && index >= 62 {
                INVALID_VALUE
            } else {
                index as u8
            };
            assert_eq!(table[byte as usize], expected);
        }
        assert_eq!(table[b'!' as usize], INVALID_VALUE);
    }
}

#[test]
fn unpadded_decoder_matches_padded_reference_without_touching_guards() {
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
            let mut transactional = vec![CANARY; layout.output_len()];
            decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
                &encoded,
                &mut transactional,
                layout,
                alphabet,
            )
            .unwrap();
            assert_eq!(
                transactional, input,
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
fn unpadded_decoder_rejects_invalid_tails_before_storing_them() {
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
                        decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
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
fn unpadded_decoder_propagates_invalid_prefix_errors() {
    let encoded = b"!AAAaa";
    let layout = decode_unpadded_layout(encoded).unwrap();
    let mut output = [0xa5; 4];

    assert_eq!(
        decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
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
fn transactional_decoder_matches_regular_decoder() {
    let input: Vec<u8> = (0..96)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    let encoded = b64encode(&input);
    let layout = decode_layout(encoded.as_bytes()).unwrap();
    let mut decoded = vec![0xa5; layout.output_len()];

    decode_to_slice_with_layout_and_alphabet_transactional(
        encoded.as_bytes(),
        &mut decoded,
        layout,
        DecodeAlphabet::Standard,
    )
    .unwrap();

    assert_eq!(decoded, input);
}

#[test]
fn buffer_apis_respect_exact_slice_boundaries() {
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
            let encoded_len = expected.len();
            let mut encoded = vec![CANARY; encoded_len + GUARD * 2];
            let written = if urlsafe {
                b64encode_urlsafe_into(&input, &mut encoded[GUARD..])
            } else {
                b64encode_into(&input, &mut encoded[GUARD..])
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

            let mut decoded = vec![CANARY; length + GUARD * 2];
            let written = if urlsafe {
                b64decode_urlsafe_into(expected.as_bytes(), &mut decoded[GUARD..])
            } else {
                b64decode_into(expected.as_bytes(), &mut decoded[GUARD..])
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
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn avx2_interior_stores_respect_exact_slice_boundaries() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    for input_offset in 0..32 {
        for length in (24..=384).step_by(24) {
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
                let mut guarded_encoded =
                    vec![CANARY; GUARD + input_offset + encoded.len() + GUARD];
                let encoded_input = &mut guarded_encoded
                    [GUARD + input_offset..GUARD + input_offset + encoded.len()];
                encoded_input.copy_from_slice(encoded.as_bytes());

                let output_offset = input_offset.wrapping_mul(7) & 31;
                let mut output = vec![CANARY; GUARD + output_offset + length + GUARD];
                let decoded = &mut output[GUARD + output_offset..GUARD + output_offset + length];
                let offsets =
                    decode_with_backend(encoded_input, decoded, Backend::Avx2, alphabet).unwrap();

                assert_eq!(offsets, (encoded.len(), length), "length={length}");
                assert_eq!(decoded, input);
                assert!(
                    output[..GUARD + output_offset]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
                assert!(
                    output[GUARD + output_offset + length..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }
        }
    }
}

#[test]
fn padded_decoder_stores_stay_within_four_bytes_of_slack() {
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
