use crate::base64::backend::{self, Backend};
use crate::base64::encode as encode_backend;
use crate::base64::runtime_dispatch::{
    encode_custom_with_backend, encode_wrapped_custom_with_backend,
};
use crate::base64::{b64encode, b64encode_urlsafe, encoded_len};

#[test]
fn match_wrapped_backends() {
    let input = (0..=1024)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();

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

        for length in [0, 1, 2, 15, 16, 31, 32, 47, 48, 63, 64, 241, 1024] {
            for urlsafe in [false, true] {
                for padded in [false, true] {
                    let mut contiguous = if urlsafe {
                        b64encode_urlsafe(&input[..length]).into_bytes()
                    } else {
                        b64encode(&input[..length]).into_bytes()
                    };

                    if !padded {
                        while contiguous.last() == Some(&b'=') {
                            contiguous.pop();
                        }
                    }

                    for width in [4, 8, 12, 20, 76, 80, 256] {
                        let mut expected = Vec::new();

                        for (line, chunk) in contiguous.chunks(width).enumerate() {
                            if line != 0 {
                                expected.push(b'\n');
                            }

                            expected.extend_from_slice(chunk);
                        }

                        let mut actual = vec![0xa5; expected.len() + 16];
                        unsafe {
                            encode_backend::encode_wrapped_to_ptr_with_backend(
                                &input[..length],
                                actual.as_mut_ptr(),
                                urlsafe,
                                padded,
                                width,
                                backend,
                            )
                        };
                        assert_eq!(&actual[..expected.len()], expected);
                        assert!(actual[expected.len()..].iter().all(|&byte| byte == 0xa5));
                    }
                }
            }
        }
    }
}

#[test]
fn preserve_custom_bounds() {
    let input = (0..=1024)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    let alphabet = encode_backend::CustomEncodeAlphabet::new(*b"@#");

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

        for length in [16, 31, 32, 47, 48, 63, 64, 241, 1024] {
            for width in [4, 20, 76] {
                let data_len = encoded_len(length);
                let mut actual = vec![0xa5; data_len + data_len / width + 16];
                let mut output = encode_backend::WrappedOutput::new(actual.as_mut_ptr(), width);
                let consumed = unsafe {
                    encode_wrapped_custom_with_backend(
                        &input[..length],
                        &mut output,
                        backend,
                        &alphabet,
                    )
                };

                let mut contiguous = b64encode(&input[..consumed]).into_bytes();

                for byte in &mut contiguous {
                    if *byte == b'+' {
                        *byte = b'@';
                    } else if *byte == b'/' {
                        *byte = b'#';
                    }
                }

                let mut expected = Vec::new();

                for (line, chunk) in contiguous.chunks(width).enumerate() {
                    if line != 0 {
                        expected.push(b'\n');
                    }

                    expected.extend_from_slice(chunk);
                }

                assert_eq!(
                    &actual[..expected.len()],
                    expected,
                    "backend={backend:?} length={length} width={width}",
                );
                assert!(actual[expected.len()..].iter().all(|&byte| byte == 0xa5));
            }
        }
    }

    let unavailable = [Backend::Neon, Backend::Avx2]
        .into_iter()
        .find(|&backend| !backend::is_supported(backend))
        .unwrap();
    let mut actual = [0xa5; 32];
    let mut output = encode_backend::WrappedOutput::new(actual.as_mut_ptr(), 4);
    assert_eq!(
        unsafe {
            encode_wrapped_custom_with_backend(&input[..16], &mut output, unavailable, &alphabet)
        },
        0,
    );
    assert_eq!(
        unsafe {
            encode_custom_with_backend(&input[..16], actual.as_mut_ptr(), unavailable, &alphabet)
        },
        0,
    );
    assert_eq!(actual, [0xa5; 32]);
}
