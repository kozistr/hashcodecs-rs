use super::configured_decoder;
use crate::base64::Base64Error;
use crate::bindings::base64::{
    configured::{
        StrictSpecials, decode_configured_into,
        decode_configured_strict_into as decode_prepared_strict_into,
    },
    policy::{DecodePolicy, ErrorWrites, PreparedDecoder},
    staging::CONFIGURED_STAGING_CAPACITY,
};
use crate::bindings::buffer::BytesLike;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

fn decode_configured_strict_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: [u8; 2],
    padded: bool,
    validated_prefix_only: bool,
) -> PyResult<Result<usize, Base64Error>> {
    let prepared = PreparedDecoder::new(
        py,
        DecodePolicy::new(Some(altchars), Some(true), padded, None, false),
    )?;
    decode_prepared_strict_into(
        input,
        output,
        altchars,
        prepared.strict_custom(),
        if validated_prefix_only {
            ErrorWrites::ValidatedPrefix
        } else {
            ErrorWrites::MayWrite
        },
    )
}

#[test]
fn reject_malformed_input() {
    let decoder = configured_decoder(b"!?#$", true, true, false);

    for (input, expected) in [
        (b"AAAA".as_slice(), 3),
        (b"AA==".as_slice(), 1),
        (b"AAA=".as_slice(), 2),
    ] {
        assert_eq!(decoder.validate_strict(input), Some(expected));
        let mut output = [0xa5; 8];
        assert_eq!(
            unsafe { decoder.decode_strict_checked_to_ptr(input, output.as_mut_ptr()) },
            Some(expected)
        );
    }

    for input in [
        b"AA==A".as_slice(),
        b"AA~=".as_slice(),
        b"A===".as_slice(),
        b"AA=".as_slice(),
    ] {
        assert_eq!(decoder.validate_strict(input), None);
        let mut output = [0xa5; 8];
        assert_eq!(
            unsafe { decoder.decode_strict_checked_to_ptr(input, output.as_mut_ptr()) },
            None
        );
    }

    let unpadded = configured_decoder(b"!?#$", true, false, false);
    assert_eq!(unpadded.validate_strict(b"AA=="), None);
    let mut output = [0xa5; 8];
    assert_eq!(
        unsafe { unpadded.decode_strict_checked_to_ptr(b"AA==", output.as_mut_ptr()) },
        None
    );

    let canonical = configured_decoder(b"!?#$", true, true, true);
    assert_eq!(canonical.validate_strict(b"AB=="), None);
    assert_eq!(
        unsafe { canonical.decode_strict_checked_to_ptr(b"AB==", output.as_mut_ptr()) },
        None
    );
    assert_eq!(canonical.validate_strict(b"AAB="), None);
    assert_eq!(
        unsafe { canonical.decode_strict_checked_to_ptr(b"AAB=", output.as_mut_ptr()) },
        None
    );
}

#[test]
fn check_strict_symbols() {
    let decoder = configured_decoder(b"!", true, true, false);
    let mut output = vec![0xa5; CONFIGURED_STAGING_CAPACITY];

    assert_eq!(decoder.validate_strict(b"AA!!=="), Some(1));
    assert_eq!(
        unsafe { decoder.decode_strict_checked_to_ptr(b"AA!!==", output.as_mut_ptr()) },
        Some(1)
    );
    assert_eq!(output[0], 0);

    for input in [b"A".as_slice(), b"AA=".as_slice(), b"AA==A".as_slice()] {
        assert_eq!(decoder.validate_strict(input), None);
        assert_eq!(
            unsafe { decoder.decode_strict_checked_to_ptr(input, output.as_mut_ptr()) },
            None
        );
    }

    let canonical = configured_decoder(b"!", true, true, true);

    for input in [b"AB==".as_slice(), b"AAB=".as_slice()] {
        assert_eq!(canonical.validate_strict(input), None);
        assert_eq!(
            unsafe { canonical.decode_strict_checked_to_ptr(input, output.as_mut_ptr()) },
            None
        );
    }

    let mut forbidden = configured_decoder(b"!", true, true, false);
    forbidden.table[usize::from(b'A')] = 64;
    forbidden.strict_forbidden = StrictSpecials::forbidden(&forbidden.table);
    assert_eq!(forbidden.validate_strict(b"AAAA"), None);
    assert_eq!(
        unsafe { forbidden.decode_strict_checked_to_ptr(b"AAAA", output.as_mut_ptr()) },
        None
    );

    let symbols = vec![b'A'; CONFIGURED_STAGING_CAPACITY];
    let expected = CONFIGURED_STAGING_CAPACITY / 4 * 3;
    assert_eq!(decoder.validate_strict(&symbols), Some(expected));
    assert_eq!(
        unsafe { decoder.decode_strict_checked_to_ptr(&symbols, output.as_mut_ptr()) },
        Some(expected)
    );
    assert_eq!(
        unsafe { decoder.decode_to_ptr(&symbols, output.as_mut_ptr(), true) },
        expected
    );
}

#[test]
fn handle_aliases_and_errors() {
    Python::initialize();
    Python::attach(|py| {
        let shared = PyByteArray::new(py, b"@#8=");
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::ByteArray(&shared),
                &shared,
                *b"@#",
                true,
                false,
            )
            .unwrap(),
            Ok(2)
        );
        assert_eq!(&shared.to_vec()[..2], b"\xfb\xff");

        let valid = PyBytes::new(py, b"@#8=");
        let output = PyByteArray::new(py, &[0xa5; 2]);
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&valid),
                &output,
                *b"@#",
                true,
                true,
            )
            .unwrap(),
            Ok(2)
        );
        assert_eq!(output.to_vec(), b"\xfb\xff");

        let invalid = PyBytes::new(py, b"AA=");
        let output = PyByteArray::new(py, &[0xa5; 2]);
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&invalid),
                &output,
                *b"@#",
                true,
                true,
            )
            .unwrap(),
            Err(Base64Error::InvalidInput)
        );
        assert_eq!(output.to_vec(), [0xa5; 2]);

        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&invalid),
                &output,
                *b"@#",
                false,
                false,
            )
            .unwrap(),
            Err(Base64Error::InvalidInput)
        );
        assert_eq!(output.to_vec(), [0xa5; 2]);

        let invalid_custom_padding = PyBytes::new(py, b"A=#");
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&invalid_custom_padding),
                &output,
                *b"=#",
                true,
                false,
            )
            .unwrap(),
            Err(Base64Error::InvalidInput)
        );
        assert_eq!(output.to_vec(), [0xa5; 2]);

        let unpadded = PyBytes::new(py, b"@#8");
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&unpadded),
                &output,
                *b"@#",
                false,
                false,
            )
            .unwrap(),
            Ok(2)
        );
        assert_eq!(output.to_vec(), b"\xfb\xff");

        let invalid_alphabet = PyBytes::new(py, b"AA!=");
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&invalid_alphabet),
                &output,
                *b"@#",
                true,
                false,
            )
            .unwrap(),
            Err(Base64Error::InvalidInput)
        );

        let output = PyByteArray::new(py, &[0xa5]);
        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&valid),
                &output,
                *b"@#",
                true,
                false,
            )
            .unwrap(),
            Err(Base64Error::OutputTooSmall {
                required: 2,
                provided: 1,
            })
        );
        assert_eq!(output.to_vec(), [0xa5]);

        assert_eq!(
            decode_configured_strict_into(
                py,
                &BytesLike::Bytes(&valid),
                &output,
                *b"@#",
                true,
                true,
            )
            .unwrap(),
            Err(Base64Error::OutputTooSmall {
                required: 2,
                provided: 1,
            })
        );
        assert_eq!(output.to_vec(), [0xa5]);
    });
}

#[test]
fn snapshot_aliased_input() {
    Python::initialize();
    Python::attach(|py| {
        let shared = PyByteArray::new(py, b"@#8=");
        let prepared = PreparedDecoder::new(
            py,
            DecodePolicy::new(Some(*b"@#"), Some(false), true, None, false),
        )
        .unwrap();
        assert_eq!(
            decode_configured_into(
                py,
                &BytesLike::ByteArray(&shared),
                &shared,
                prepared.configured(),
                prepared.semantics,
            )
            .unwrap(),
            Ok(2)
        );
        assert_eq!(&shared.to_vec()[..2], b"\xfb\xff");
    });
}
