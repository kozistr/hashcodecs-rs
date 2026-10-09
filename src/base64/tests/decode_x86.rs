use crate::base64::alphabet::{MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE};
use crate::base64::backend::{self, Backend};
use crate::base64::decode::{self as decode_backend, x86_contracts};
use crate::base64::runtime_dispatch::{decode_valid_prefix_with_backend, decode_with_backend};
use crate::base64::{Base64Error, DecodeAlphabet, b64encode, b64encode_urlsafe};
use base64::Engine;

#[test]
fn preserve_sse_bounds() {
    const CANARY: u8 = 0xa5;

    for backend in [Backend::Ssse3, Backend::Sse41] {
        if !backend::is_supported(backend) {
            continue;
        }

        for length in [36, 48, 60, 96, 108] {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect();

            for (encoded, alphabet) in [
                (b64encode(&input), DecodeAlphabet::Standard),
                (b64encode_urlsafe(&input), DecodeAlphabet::UrlSafe),
                (b64encode(&input), DecodeAlphabet::Mixed),
            ] {
                for offset in 0..16 {
                    let start = 16 + offset;
                    let mut output = vec![CANARY; start + length + 16];
                    let decoded = &mut output[start..start + length];
                    assert_eq!(
                        decode_with_backend(encoded.as_bytes(), decoded, backend, alphabet),
                        Ok((encoded.len(), length))
                    );
                    assert_eq!(decoded, input);
                    assert!(output[..start].iter().all(|&byte| byte == CANARY));
                    assert!(output[start + length..].iter().all(|&byte| byte == CANARY));

                    // A failing group must not leave overlap bytes past the last
                    // valid block, including when scalar decoding retries there.
                    for invalid_at in 0..=encoded.len() {
                        let mut source = encoded.as_bytes().to_vec();

                        if invalid_at < source.len() {
                            source[invalid_at] = 0xff;
                        }

                        output.fill(CANARY);
                        let consumed = invalid_at / 16 * 16;
                        let written = consumed / 4 * 3;
                        assert_eq!(
                            unsafe {
                                decode_valid_prefix_with_backend(
                                    &source,
                                    output.as_mut_ptr().add(start),
                                    backend,
                                    alphabet,
                                )
                            },
                            (consumed, written),
                            "backend={backend:?} length={length} offset={offset} invalid_at={invalid_at}"
                        );
                        assert_eq!(&output[start..start + written], &input[..written]);
                        assert!(output[..start].iter().all(|&byte| byte == CANARY));
                        assert!(output[start + written..].iter().all(|&byte| byte == CANARY));
                    }
                }
            }
        }
    }
}

#[test]
fn preserve_avx512_tails() {
    if !backend::is_supported(Backend::Avx512Vbmi) {
        return;
    }

    for length in [64, 68, 76, 80, 92, 96, 108, 112, 124, 128, 132, 140, 144] {
        let encoded = vec![b'A'; length];
        let expected_written = length / 4 * 3;
        let mut decoded = vec![0xa5; expected_written + 16];
        let (consumed, written) = decode_with_backend(
            &encoded,
            &mut decoded,
            Backend::Avx512Vbmi,
            DecodeAlphabet::Standard,
        )
        .unwrap();

        assert_eq!((consumed, written), (length, expected_written));
        assert!(decoded[..written].iter().all(|byte| *byte == 0));
        assert!(decoded[written..].iter().all(|byte| *byte == 0xa5));

        for invalid_index in 64..length {
            let mut invalid = encoded.clone();
            invalid[invalid_index] = b'!';
            assert_eq!(
                decode_with_backend(
                    &invalid,
                    &mut decoded,
                    Backend::Avx512Vbmi,
                    DecodeAlphabet::Standard,
                ),
                Err(Base64Error::InvalidInput),
                "length={length} invalid_index={invalid_index}",
            );
        }
    }
}

#[test]
fn preserve_avx2_stores() {
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
fn preserve_avx2_overlaps() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    const GUARD: usize = 64;
    const CANARY: u8 = 0xa5;
    const LENGTH: usize = 96;

    let input: Vec<u8> = (0..LENGTH)
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
        for cache_line_offset in [0, 32] {
            let mut output = vec![CANARY; GUARD + 64 + LENGTH + GUARD];
            let aligned = output.as_mut_ptr().align_offset(64);
            let start = aligned + cache_line_offset;
            let decoded = &mut output[start..start + LENGTH];

            assert_eq!(decoded.as_ptr().addr() & 63, cache_line_offset);
            assert_eq!(
                decode_with_backend(encoded.as_bytes(), decoded, Backend::Avx2, alphabet),
                Ok((encoded.len(), LENGTH))
            );
            assert_eq!(decoded, input);
            assert!(output[..start].iter().all(|&byte| byte == CANARY));
            assert!(output[start + LENGTH..].iter().all(|&byte| byte == CANARY));

            output.fill(CANARY);
            assert_eq!(
                unsafe {
                    decode_valid_prefix_with_backend(
                        encoded.as_bytes(),
                        output.as_mut_ptr().add(start),
                        Backend::Avx2,
                        alphabet,
                    )
                },
                (encoded.len(), LENGTH)
            );
            assert_eq!(&output[start..start + LENGTH], input);
            assert!(output[..start].iter().all(|&byte| byte == CANARY));
            assert!(output[start + LENGTH..].iter().all(|&byte| byte == CANARY));
        }
    }
}

#[test]
fn check_avx512_layout() {
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

    let packed = core::array::from_fn::<_, 64, _>(|index| index as u8);
    let mut expected = Vec::with_capacity(48);

    for lane in 0..4 {
        for group in 0..4 {
            let source = lane * 16 + group * 4;
            expected.extend_from_slice(&[packed[source + 2], packed[source + 1], packed[source]]);
        }
    }

    let decoded: Vec<u8> = decode_backend::avx512::DECODE_SHUFFLE[..48]
        .iter()
        .map(|&index| packed[index as usize])
        .collect();
    assert_eq!(decoded, expected);
}
