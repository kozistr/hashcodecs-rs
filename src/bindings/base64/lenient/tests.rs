use super::*;

#[test]
fn check_capacity_bounds() {
    for (input, expected) in [
        (b"".as_slice(), 0),
        (b"=", 0),
        (b"==", 0),
        (b"A", 0),
        (b"AA", 1),
        (b"AA==", 1),
        (b"AAA=", 2),
        (b"AAAA", 3),
        (b"AA!==", 2),
        (b"====", 1),
    ] {
        assert_eq!(
            decoded_len_upper_bound(input, &STANDARD_LENIENT_TABLE),
            expected
        );
        let table = lenient_decode_table(Some(*b"=_"));
        assert_eq!(
            decoded_len_upper_bound(input, &table),
            decoded_symbol_len(input.len())
        );
    }

    for length in [0, 4, 256, 260, 4096] {
        let mut input = vec![b'A'; length];

        if length != 0 {
            input[length - 2..].fill(b'=');
        }

        let expected = length / 4 * 3 - if length > 256 { 2 } else { 0 };
        assert_eq!(
            BytesWriter::capacity_for_input(&input, &STANDARD_LENIENT_TABLE),
            expected
        );
        assert_eq!(
            BytesWriter::capacity_for_input(&input, &lenient_decode_table(Some(*b"=_"))),
            length / 4 * 3
        );
    }
}

#[test]
fn reject_incomplete_input() {
    assert_eq!(
        lenient_decoded_len(b"A", None, false, false),
        Err(LenientDecodeError::InvalidInput)
    );
    assert_eq!(
        lenient_decoded_len(b"AA", None, true, false),
        Err(LenientDecodeError::InvalidInput)
    );
}

#[test]
fn decode_custom_runs() {
    let altchars = Some(*b"@#");
    let table = lenient_decode_table(altchars);

    for quartets in [4, 8, 16, 1023, 1024, 1025, 2048] {
        let mut input = b"@#AA".repeat(quartets);
        input.extend_from_slice(b"AA==AAAA==");

        for padded in [false, true] {
            for continue_after_padding in [false, true] {
                let tail = if padded && !continue_after_padding {
                    1
                } else {
                    4
                };

                let mut expected = [0xfb, 0xf0, 0].repeat(quartets);
                expected.resize(expected.len() + tail, 0);
                let required = expected.len();
                assert_eq!(
                    unsafe {
                        decode_lenient_to_ptr::<false>(
                            &input,
                            std::ptr::null_mut(),
                            required,
                            &table,
                            altchars,
                            padded,
                            continue_after_padding,
                        )
                    },
                    Ok(required)
                );

                for provided in [0, required - 1, required, required + 7] {
                    let mut output = vec![0xa5; provided + 2];
                    let result = unsafe {
                        decode_lenient_to_ptr::<true>(
                            &input,
                            output.as_mut_ptr().add(1),
                            provided,
                            &table,
                            altchars,
                            padded,
                            continue_after_padding,
                        )
                    };
                    assert_eq!(output[0], 0xa5);
                    assert_eq!(output[provided + 1], 0xa5);

                    if provided < required {
                        assert_eq!(result, Err(LenientDecodeError::OutputTooSmall));
                    } else {
                        assert_eq!(result, Ok(required));
                        assert_eq!(&output[1..required + 1], expected);
                        assert!(output[required + 1..].iter().all(|&byte| byte == 0xa5));
                    }
                }
            }
        }
    }
}
