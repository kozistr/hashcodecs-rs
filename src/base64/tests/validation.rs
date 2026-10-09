use crate::base64::alphabet::{
    DECODE_STORE_PADDING, INVALID_VALUE, MIXED_DECODE, STANDARD_ALPHABET, STANDARD_DECODE,
    URLSAFE_ALPHABET, URLSAFE_DECODE, decode_table,
};
use crate::base64::backend::{self, Backend};
use crate::base64::runtime_dispatch::{
    decode_with_backend, decode_with_backend_ptr, validate_with_backend,
};
use crate::base64::{Base64Error, DecodeAlphabet, validate_alphabet};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::base64::{decode_valid_prefix, runtime_dispatch::decode_valid_prefix_with_backend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use base64::Engine;

#[test]
fn validate_without_output() {
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

        for (alphabet, symbol) in [
            (DecodeAlphabet::Standard, b'/'),
            (DecodeAlphabet::UrlSafe, b'_'),
            (DecodeAlphabet::Mixed, b'-'),
        ] {
            let mut input = vec![symbol; 273];
            let consumed = validate_with_backend(&input, backend, alphabet).unwrap();

            if backend == Backend::Scalar {
                assert_eq!(consumed, 0);

                continue;
            }

            assert!(consumed >= 16);
            assert!(input.len() - consumed < 16);

            input[15] = b'!';
            assert_eq!(
                validate_with_backend(&input, backend, alphabet),
                Err(Base64Error::InvalidInput)
            );
            input[15] = symbol;
            input[consumed - 1] = b'!';
            assert_eq!(
                validate_with_backend(&input, backend, alphabet),
                Err(Base64Error::InvalidInput)
            );
        }
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
    assert_eq!(
        validate_with_backend(b"AAAA", unsupported, DecodeAlphabet::Standard),
        Ok(0)
    );

    if backend::is_supported(Backend::Avx2) {
        assert_eq!(
            validate_with_backend(&[b'A'; 32], Backend::Avx2, DecodeAlphabet::Standard),
            Ok(32)
        );
        let mut input = vec![b'A'; 48];
        assert_eq!(
            validate_with_backend(&input, Backend::Avx2, DecodeAlphabet::Standard),
            Ok(48)
        );
        input[0] = b'!';
        assert_eq!(
            validate_with_backend(&input, Backend::Avx2, DecodeAlphabet::Standard),
            Err(Base64Error::InvalidInput)
        );
    }

    for (alphabet, input) in [
        (DecodeAlphabet::Standard, b"A".as_slice()),
        (DecodeAlphabet::UrlSafe, b"_".as_slice()),
        (DecodeAlphabet::Mixed, b"-".as_slice()),
    ] {
        assert_eq!(validate_alphabet(input, alphabet), Ok(()));
    }

    assert_eq!(
        validate_alphabet(b"!", DecodeAlphabet::Standard),
        Err(Base64Error::InvalidInput)
    );

    let mut input = vec![b'A'; 273];
    assert_eq!(validate_alphabet(&input, DecodeAlphabet::Standard), Ok(()));

    input[272] = b'!';
    assert_eq!(
        validate_alphabet(&input, DecodeAlphabet::Standard),
        Err(Base64Error::InvalidInput)
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn stop_at_invalid_block() {
    let input = [b'A'; 32];
    let mut output = [0xa5; 24];

    if let Some((consumed, written)) =
        unsafe { decode_valid_prefix(&input, output.as_mut_ptr(), DecodeAlphabet::Standard) }
    {
        assert_eq!((consumed, written), (32, 24));
        assert_eq!(output, [0; 24]);
    }

    for backend in [
        Backend::Ssse3,
        Backend::Sse41,
        Backend::Avx2,
        Backend::Avx512Vbmi,
    ] {
        if !backend::is_supported(backend) {
            continue;
        }

        for length in [64, 65, 67, 68, 95, 127, 128, 129, 160, 191, 192, 193] {
            for (alphabet, symbol) in [
                (DecodeAlphabet::Standard, b'A'),
                (DecodeAlphabet::UrlSafe, b'_'),
                (DecodeAlphabet::Mixed, b'-'),
            ] {
                let input = vec![symbol; length];
                let mut output = vec![0xa5; length / 4 * 3 + 16];

                let block = if backend == Backend::Avx512Vbmi {
                    4
                } else {
                    16
                };

                let expected_input = length / block * block;
                let expected_output = expected_input / 4 * 3;
                assert_eq!(
                    unsafe {
                        decode_valid_prefix_with_backend(
                            &input,
                            output.as_mut_ptr(),
                            backend,
                            alphabet,
                        )
                    },
                    (expected_input, expected_output),
                    "backend={backend:?} length={length} alphabet={alphabet:?}",
                );
                assert!(output[expected_output..].iter().all(|&byte| byte == 0xa5));
            }
        }

        for invalid_at in 0..160 {
            let mut input = vec![b'A'; 160];
            input[invalid_at] = b'!';
            let mut output = vec![0xa5; 120];

            let block = if backend == Backend::Avx512Vbmi {
                4
            } else {
                16
            };

            let expected_input = invalid_at / block * block;
            assert_eq!(
                unsafe {
                    decode_valid_prefix_with_backend(
                        &input,
                        output.as_mut_ptr(),
                        backend,
                        DecodeAlphabet::Standard,
                    )
                },
                (expected_input, expected_input / 4 * 3),
                "backend={backend:?} invalid_at={invalid_at}"
            );
            assert!(
                output[..expected_input / 4 * 3]
                    .iter()
                    .all(|&byte| byte == 0)
            );
            assert!(
                output[expected_input / 4 * 3..]
                    .iter()
                    .all(|&byte| byte == 0xa5)
            );
        }
    }

    assert_eq!(
        unsafe {
            decode_valid_prefix_with_backend(
                b"AAAA",
                std::ptr::dangling_mut(),
                Backend::Neon,
                DecodeAlphabet::Standard,
            )
        },
        (0, 0)
    );
}

#[test]
fn classify_each_byte() {
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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn classify_byte_pairs() {
    for (alphabet, table) in [
        (DecodeAlphabet::Standard, &STANDARD_DECODE),
        (DecodeAlphabet::UrlSafe, &URLSAFE_DECODE),
        (DecodeAlphabet::Mixed, &MIXED_DECODE),
    ] {
        for word in 0..=u16::MAX {
            let bytes = word.to_ne_bytes();
            let mut input = [0; 128];

            for pair in input.as_chunks_mut::<2>().0 {
                pair.copy_from_slice(&bytes);
            }

            let expected = if bytes
                .into_iter()
                .all(|byte| table[byte as usize] != INVALID_VALUE)
            {
                Ok(input.len())
            } else {
                Err(Base64Error::InvalidInput)
            };

            let decoded = expected.is_ok().then(|| {
                let standard = input.map(|byte| match byte {
                    b'-' => b'+',
                    b'_' => b'/',
                    byte => byte,
                });
                base64::engine::general_purpose::STANDARD
                    .decode(standard)
                    .unwrap()
            });

            for backend in [Backend::Ssse3, Backend::Sse41, Backend::Avx2]
                .into_iter()
                .filter(|candidate| backend::is_supported(*candidate))
            {
                assert_eq!(
                    validate_with_backend(&input, backend, alphabet),
                    expected,
                    "backend={backend:?} alphabet={alphabet:?} word={word:#06x}"
                );
                let mut output = [0xa5; 98];
                let result = decode_with_backend(&input, &mut output[1..97], backend, alphabet);

                if let Some(decoded) = &decoded {
                    assert_eq!(result, Ok((128, 96)));
                    assert_eq!(&output[1..97], decoded);
                } else {
                    assert_eq!(result, Err(Base64Error::InvalidInput));
                }

                assert_eq!(output[0], 0xa5);
                assert_eq!(output[97], 0xa5);
            }
        }
    }
}

#[test]
fn check_decode_tables() {
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
