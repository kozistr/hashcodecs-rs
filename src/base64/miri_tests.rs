use super::*;

#[test]
fn scalar_allocations_and_exact_buffers_are_defined() {
    const LENGTHS: &[usize] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 15, 16, 17, 31, 32, 47, 48, 63, 64, 65, 95, 96, 97, 255, 256,
        257, 1023, 1024, 1025, 4097,
    ];
    for &length in LENGTHS {
        let input = (0..length)
            .map(|index| (index as u8).wrapping_mul(53).wrapping_add(7))
            .collect::<Vec<_>>();
        for urlsafe in [false, true] {
            let encoded = if urlsafe {
                b64encode_urlsafe(&input)
            } else {
                b64encode(&input)
            };
            let decoded = if urlsafe {
                b64decode_urlsafe(encoded.as_bytes())
            } else {
                b64decode(encoded.as_bytes())
            };
            assert_eq!(decoded.as_deref(), Ok(input.as_slice()));

            let mut encoded_into = vec![0xa5; encoded.len() + 8];
            let written = if urlsafe {
                b64encode_urlsafe_into(&input, &mut encoded_into)
            } else {
                b64encode_into(&input, &mut encoded_into)
            };
            assert_eq!(written, Ok(encoded.len()));
            assert_eq!(&encoded_into[..encoded.len()], encoded.as_bytes());
            assert!(
                encoded_into[encoded.len()..]
                    .iter()
                    .all(|byte| *byte == 0xa5)
            );

            let mut decoded_into = vec![0xa5; input.len() + 8];
            let written = if urlsafe {
                b64decode_urlsafe_into(encoded.as_bytes(), &mut decoded_into)
            } else {
                b64decode_into(encoded.as_bytes(), &mut decoded_into)
            };
            assert_eq!(written, Ok(input.len()));
            assert_eq!(&decoded_into[..input.len()], input);
            assert!(decoded_into[input.len()..].iter().all(|byte| *byte == 0xa5));
        }
    }

    assert_eq!(b64decode(b"!!!!"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode_urlsafe(b"!!!!"), Err(Base64Error::InvalidInput));
}
