use super::*;
use crate::bindings::buffer::with_bytearray;
use ::base64::Engine;

#[test]
fn encode_large_inputs() {
    Python::initialize();
    Python::attach(|py| {
        for length in [
            DIRECT_WRAPPED_INPUT_THRESHOLD,
            DIRECT_WRAPPED_INPUT_THRESHOLD + 1,
        ] {
            let input = vec![0xfb; length];
            let reference = ::base64::engine::general_purpose::STANDARD.encode(&input);
            let source = PyBytes::new(py, &input);
            let source = BytesLike::Bytes(&source);

            for padded in [false, true] {
                let contiguous = if padded {
                    reference.as_bytes()
                } else {
                    reference.trim_end_matches('=').as_bytes()
                };

                for wrapcol in [
                    None,
                    Some(76),
                    Some(reference.len()),
                    Some(reference.len() + 4),
                ] {
                    let mut expected = Vec::new();

                    for chunk in contiguous.chunks(wrapcol.unwrap_or(contiguous.len())) {
                        if !expected.is_empty() {
                            expected.push(b'\n');
                        }

                        expected.extend_from_slice(chunk);
                    }

                    let encoder = PreparedEncoder::new(None, padded, wrapcol);
                    let allocated = encode_with_prepared(py, &source, &encoder).unwrap();
                    assert_eq!(allocated.as_bytes(), expected);

                    let output = PyByteArray::new(py, &vec![0xa5; expected.len() + 16]);
                    let written = encode_into(&source, &output, &encoder).unwrap();
                    assert_eq!(written, expected.len());
                    with_bytearray(&output, || {
                        let output = unsafe { output.as_bytes() };
                        assert_eq!(&output[..written], expected);
                        assert_eq!(&output[written..], &[0xa5; 16]);
                    });
                }
            }
        }
    });
}

#[test]
fn encode_custom_alphabets() {
    let input = (0..=256)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();

    for altchars in [*b"@#", *b"==", [0, u8::MAX], [62, 63]] {
        for length in 0..=256 {
            for padded in [false, true] {
                let encoder = PreparedEncoder::new(Some(altchars), padded, None);
                let output_len = encoder.output_len(length);
                let mut actual = vec![0xa5; output_len + 1];
                unsafe { encoder.encode_to_ptr(&input[..length], actual.as_mut_ptr()) };

                let mut expected = crate::base64::b64encode(&input[..length]).into_bytes();

                if !padded {
                    while expected.last() == Some(&b'=') {
                        expected.pop();
                    }
                }

                for byte in &mut expected {
                    if *byte == b'+' {
                        *byte = altchars[0];
                    } else if *byte == b'/' {
                        *byte = altchars[1];
                    }
                }

                assert_eq!(&actual[..output_len], expected);
                assert_eq!(actual[output_len], 0xa5);
            }
        }
    }
}

#[test]
fn wrap_custom_unpadded_inputs() {
    let input = (0..DIRECT_WRAPPED_INPUT_THRESHOLD + 2)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let encoder = PreparedEncoder::new(Some(*b"@#"), false, Some(76));
    let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
    unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

    let mut contiguous = crate::base64::b64encode(&input).into_bytes();

    while contiguous.last() == Some(&b'=') {
        contiguous.pop();
    }

    for byte in &mut contiguous {
        if *byte == b'+' {
            *byte = b'@';
        } else if *byte == b'/' {
            *byte = b'#';
        }
    }

    let mut expected = Vec::with_capacity(encoder.output_len(input.len()));

    for (line, chunk) in contiguous.chunks(76).enumerate() {
        if line != 0 {
            expected.push(b'\n');
        }

        expected.extend_from_slice(chunk);
    }

    assert_eq!(&actual[..expected.len()], expected);
    assert_eq!(actual[expected.len()], 0xa5);
}

#[test]
fn use_alphabet_table() {
    let input = (0..=256)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let alphabet = EncodeAlphabet::from_table([b'Z'; 64]);
    let encoder = PreparedEncoder::with_alphabet(alphabet, false, Some(76));
    let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
    unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

    let data_len = unpadded_encoded_len(input.len());
    let mut expected = Vec::with_capacity(encoder.output_len(input.len()));

    for (line, length) in (0..data_len).step_by(76).enumerate() {
        if line != 0 {
            expected.push(b'\n');
        }

        expected.extend(std::iter::repeat_n(b'Z', (data_len - length).min(76)));
    }

    assert_eq!(&actual[..expected.len()], expected);
    assert_eq!(actual[expected.len()], 0xa5);
}

#[test]
fn wrap_standard_padded_inputs() {
    let input = (0..DIRECT_WRAPPED_INPUT_THRESHOLD + 1)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let encoder = PreparedEncoder::new(None, true, Some(76));
    let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
    unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

    let contiguous = crate::base64::b64encode(&input).into_bytes();
    let mut expected = Vec::with_capacity(encoder.output_len(input.len()));

    for (line, chunk) in contiguous.chunks(76).enumerate() {
        if line != 0 {
            expected.push(b'\n');
        }

        expected.extend_from_slice(chunk);
    }

    assert_eq!(&actual[..expected.len()], expected);
    assert_eq!(actual[expected.len()], 0xa5);
}

#[test]
fn wrap_short_custom_inputs() {
    let input = [0xfb; 15];
    let alphabet = CustomEncodeAlphabet::new(*b"@#");
    let mut actual = [0xa5; 25];
    unsafe { encode_wrapped_to_ptr_custom(&input, actual.as_mut_ptr(), &alphabet, true, 4) };

    let mut contiguous = crate::base64::b64encode(&input).into_bytes();

    for byte in &mut contiguous {
        match *byte {
            b'+' => *byte = b'@',
            b'/' => *byte = b'#',
            _ => {}
        }
    }

    let mut expected = Vec::new();

    for (line, chunk) in contiguous.chunks(4).enumerate() {
        if line != 0 {
            expected.push(b'\n');
        }

        expected.extend_from_slice(chunk);
    }

    assert_eq!(&actual[..expected.len()], expected);
    assert_eq!(actual[expected.len()], 0xa5);
}
