use super::*;
use base64::Engine;

fn backend_supported(backend: Backend) -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        match backend {
            Backend::Scalar => true,
            Backend::Neon => false,
            Backend::Ssse3 => std::is_x86_feature_detected!("ssse3"),
            Backend::Sse41 => {
                std::is_x86_feature_detected!("ssse3") && std::is_x86_feature_detected!("sse4.1")
            }
            Backend::Sse42 => {
                std::is_x86_feature_detected!("ssse3")
                    && std::is_x86_feature_detected!("sse4.1")
                    && std::is_x86_feature_detected!("sse4.2")
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
fn backend_selection_and_kernels_match_scalar_output() {
    assert_eq!(
        select_x86_backend(false, false, false, false, false),
        Backend::Scalar
    );
    assert_eq!(
        select_x86_backend(false, false, false, false, true),
        Backend::Ssse3
    );
    assert_eq!(
        select_x86_backend(false, false, false, true, true),
        Backend::Sse41
    );
    assert_eq!(
        select_x86_backend(false, false, true, true, true),
        Backend::Sse42
    );
    assert_eq!(
        select_x86_backend(false, false, true, false, true),
        Backend::Ssse3
    );
    assert_eq!(
        select_x86_backend(false, true, false, false, false),
        Backend::Avx2
    );
    assert_eq!(
        select_x86_backend(true, false, false, false, false),
        Backend::Avx512
    );
    assert_eq!(select_aarch64_backend(false), Backend::Scalar);
    assert_eq!(select_aarch64_backend(true), Backend::Neon);
    assert!(backend_supported(Backend::Scalar));
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
    #[cfg(coverage)]
    {
        assert_eq!(
            encode_with_backend(&input, &mut scalar, Backend::Avx512, false),
            0
        );
        assert_eq!(
            decode_with_backend(
                expected.as_bytes(),
                &mut scalar_decoded,
                Backend::Avx512,
                DecodeAlphabet::Standard,
            ),
            Ok((0, 0))
        );
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        assert_eq!(
            encode_sse4(&input, scalar.as_mut_ptr(), false, false, false),
            0
        );
        assert_eq!(
            decode_sse4(
                expected.as_bytes(),
                scalar_decoded.as_mut_ptr(),
                DecodeAlphabet::Standard,
                false,
                false,
                false,
            ),
            Ok((0, 0))
        );
    }

    let expected_urlsafe = b64encode_urlsafe(&input);
    let mixed = b"-///".repeat(32);
    let mixed_expected = [0xfb, 0xff, 0xff].repeat(32);
    for backend in [
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Sse42,
        Backend::Avx2,
        Backend::Avx512,
    ]
    .into_iter()
    .filter(|backend| backend_supported(*backend))
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
fn every_byte_is_classified_consistently_by_each_simd_decoder() {
    for backend in [
        Backend::Neon,
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Sse42,
        Backend::Avx2,
        Backend::Avx512,
    ]
    .into_iter()
    .filter(|backend| backend_supported(*backend))
    {
        for (alphabet, table) in [
            (DecodeAlphabet::Standard, &STANDARD_DECODE),
            (DecodeAlphabet::UrlSafe, &URLSAFE_DECODE),
            (DecodeAlphabet::Mixed, &MIXED_DECODE),
        ] {
            for byte in 0..=u8::MAX {
                let encoded = [byte; 16];
                let mut decoded = [0xa5; 16];
                let result = decode_with_backend(&encoded, &mut decoded[..12], backend, alphabet);
                let value = table[byte as usize];
                if value == INVALID_VALUE {
                    assert_eq!(result, Err(Base64Error::InvalidInput));
                    continue;
                }

                assert_eq!(result, Ok((16, 12)));
                let expected = [
                    (value << 2) | (value >> 4),
                    (value << 4) | (value >> 2),
                    (value << 6) | value,
                ];
                assert_eq!(&decoded[..12], expected.repeat(4));

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
                    assert_eq!(padded, Ok((16, 12)));
                    assert_eq!(&decoded[..12], expected.repeat(4));
                }
            }
        }
    }
}

#[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
#[test]
fn avx512_control_vectors_describe_the_base64_transforms() {
    let input: Vec<u8> = (0..48)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect();
    let mut shuffled = [0_u8; 64];
    for (destination, &source) in x86_avx512::ENCODE_SHUFFLE.iter().enumerate() {
        shuffled[destination] = input[source as usize];
    }

    let mut indices = [0_u8; 64];
    for lane in 0..8 {
        let lane_start = lane * 8;
        let word = u64::from_le_bytes(shuffled[lane_start..lane_start + 8].try_into().unwrap());
        for byte in 0..8 {
            let shift = x86_avx512::MULTISHIFT_SHIFTS[byte];
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

    let mut lane_packed = [0_u8; 64];
    for lane in 0..4 {
        for byte in 0..12 {
            lane_packed[lane * 16 + byte] = (lane * 12 + byte) as u8;
        }
        assert_eq!(
            &x86_avx512::PACK_SHUFFLE[lane * 16..lane * 16 + 16],
            &[
                2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 0x80, 0x80, 0x80, 0x80
            ]
        );
    }
    let compacted: Vec<u8> = x86_avx512::COMPACT_SHUFFLE[..48]
        .iter()
        .map(|&index| lane_packed[index as usize])
        .collect();
    assert_eq!(compacted, (0..48).collect::<Vec<u8>>());
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
            let mut output = vec![CANARY; GUARD + layout.output_len + DECODE_STORE_PADDING + GUARD];
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

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let has_ssse3 = std::is_x86_feature_detected!("ssse3");
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let has_ssse3 = false;
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
