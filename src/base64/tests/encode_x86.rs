use crate::base64::backend::{self, Backend};
use crate::base64::encode as encode_backend;
use crate::base64::runtime_dispatch::encode_with_backend;
use crate::base64::{b64encode, b64encode_urlsafe, encode_scalar};
use base64::Engine;

#[test]
fn preserve_avx2_load_guards() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    const GUARD: usize = 32;
    const CANARY: u8 = 0xa5;

    // 32 activates the special first load, 52 activates the first shifted
    // load, and 220 activates the assembly helper after a shorter group stays
    // in the inline loop. The other explicit boundaries exercise the AVX2 and
    // scalar terminal tails.
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
fn preserve_avx2_loop_guards() {
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

#[cfg(target_arch = "x86_64")]
#[test]
fn match_avx2_streaming() {
    if !backend::is_supported(Backend::Avx2) {
        return;
    }

    for (input_len, urlsafe) in (0..=256)
        .chain([(64 << 10) + 96, (64 << 10) + 192])
        .flat_map(|length| [(length, false), (length, true)])
    {
        let input: Vec<u8> = (0..input_len)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();

        let expected = if urlsafe {
            base64::engine::general_purpose::URL_SAFE.encode(&input)
        } else {
            base64::engine::general_purpose::STANDARD.encode(&input)
        };

        for alignment in [0, 16] {
            let mut guarded_output = vec![0xa5_u8; expected.len() + 64];
            let aligned_offset = guarded_output.as_mut_ptr().align_offset(32);
            let output_offset = aligned_offset + alignment;
            let output = &mut guarded_output[output_offset..output_offset + expected.len()];

            let consumed = unsafe {
                if urlsafe {
                    encode_backend::avx2::encode_with_store::<true>(
                        &input,
                        output.as_mut_ptr(),
                        encode_backend::avx2::Avx2StoreMode::Streaming,
                    )
                } else {
                    encode_backend::avx2::encode_with_store::<false>(
                        &input,
                        output.as_mut_ptr(),
                        encode_backend::avx2::Avx2StoreMode::Streaming,
                    )
                }
            };
            encode_scalar(&input[consumed..], &mut output[consumed / 3 * 4..], urlsafe);

            assert_eq!(output, expected.as_bytes());
            assert!(
                guarded_output[..output_offset]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
            assert!(
                guarded_output[output_offset + expected.len()..]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }
}

#[test]
fn check_avx512_indices() {
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
}

#[test]
fn preserve_avx512_tails() {
    if !backend::is_supported(Backend::Avx512Vbmi) {
        return;
    }

    for length in 48..96 {
        let input = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();

        for urlsafe in [false, true] {
            let expected = if urlsafe {
                b64encode_urlsafe(&input)
            } else {
                b64encode(&input)
            };

            let consumed_expected = length / 3 * 3;
            let written_expected = consumed_expected / 3 * 4;
            let mut encoded = vec![0xa5; expected.len() + 16];
            let consumed = encode_with_backend(&input, &mut encoded, Backend::Avx512Vbmi, urlsafe);

            assert_eq!(consumed, consumed_expected);
            assert_eq!(
                &encoded[..written_expected],
                &expected.as_bytes()[..written_expected]
            );
            assert!(encoded[written_expected..].iter().all(|&byte| byte == 0xa5));
        }
    }
}
