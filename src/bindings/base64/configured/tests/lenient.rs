use super::configured_decoder;
use crate::bindings::base64::lenient::{
    LenientDecodeError, decode_lenient_to_ptr, decoded_symbol_len, lenient_decode_table,
    lenient_decoded_len,
};
use crate::bindings::base64::staging::CONFIGURED_STAGING_CAPACITY;

#[test]
fn check_decoded_lengths() {
    assert_eq!(decoded_symbol_len(0), 0);
    assert_eq!(decoded_symbol_len(2), 1);
    assert_eq!(decoded_symbol_len(3), 2);
    assert_eq!(decoded_symbol_len(4), 3);

    assert_eq!(lenient_decoded_len(b"AAAAAAAA", None, true, false), Ok(6));
    assert_eq!(lenient_decoded_len(b"AA==AAAA", None, true, false), Ok(1));
    assert_eq!(lenient_decoded_len(b"!!!!!!!!", None, true, false), Ok(0));
    assert_eq!(
        lenient_decoded_len(b"AA==AAAA", None, true, true),
        Err(LenientDecodeError::InvalidInput)
    );
    assert_eq!(lenient_decoded_len(b"AA==", None, true, true), Ok(1));
    assert_eq!(
        lenient_decoded_len(b"A", None, false, true),
        Err(LenientDecodeError::InvalidInput)
    );
    assert_eq!(
        lenient_decoded_len(b"AA", None, true, true),
        Err(LenientDecodeError::InvalidInput)
    );
    assert_eq!(
        lenient_decoded_len(b"====", Some(*b"=_"), true, true),
        Ok(3)
    );
}

#[test]
fn respect_output_capacity() {
    let table = lenient_decode_table(None);
    let mut output = [0xa5; 8];
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                b"YWJj",
                output.as_mut_ptr(),
                output.len(),
                &table,
                None,
                true,
                true,
            )
        },
        Ok(3)
    );
    assert_eq!(&output[..3], b"abc");

    output.fill(0xa5);
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<false>(
                b"YWJj",
                output.as_mut_ptr(),
                output.len(),
                &table,
                None,
                true,
                true,
            )
        },
        Ok(3)
    );
    assert_eq!(output, [0xa5; 8]);

    for (input, provided) in [
        (b"YWJj".as_slice(), 2),
        (b"Y!W".as_slice(), 0),
        (b"YW!J".as_slice(), 1),
        (b"YWJ!j".as_slice(), 2),
    ] {
        assert_eq!(
            unsafe {
                decode_lenient_to_ptr::<true>(
                    input,
                    output.as_mut_ptr(),
                    provided,
                    &table,
                    None,
                    true,
                    true,
                )
            },
            Err(LenientDecodeError::OutputTooSmall)
        );
    }

    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                b"YQ==AAAA",
                output.as_mut_ptr(),
                output.len(),
                &table,
                None,
                true,
                false,
            )
        },
        Ok(1)
    );
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                b"A",
                output.as_mut_ptr(),
                output.len(),
                &table,
                None,
                true,
                true,
            )
        },
        Err(LenientDecodeError::InvalidInput)
    );
}

#[test]
fn skip_noise() {
    let table = lenient_decode_table(None);
    let mut input = b"YWJj".repeat(64);
    input.splice(128..128, *b"!?\r\n");
    let mut output = vec![0xa5; 3 * 64];
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                &input,
                output.as_mut_ptr(),
                output.len(),
                &table,
                None,
                true,
                true,
            )
        },
        Ok(output.len())
    );
    assert_eq!(output, b"abc".repeat(64));

    let table = lenient_decode_table(Some(*b"-_"));
    let input = b"-_8/".repeat(32);
    let mut output = vec![0; 3 * 32];
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                &input,
                output.as_mut_ptr(),
                output.len(),
                &table,
                Some(*b"-_"),
                true,
                true,
            )
        },
        Ok(output.len())
    );
    assert_eq!(output, b"\xfb\xff?".repeat(32));
}

#[test]
fn validate_remapped_symbols() {
    let decoder = configured_decoder(b"!", false, true, false);
    let mut output = vec![0xa5; CONFIGURED_STAGING_CAPACITY * 2];
    assert_eq!(decoder.decoded_len(b"Y!Q==", false), Some(1));
    assert_eq!(
        unsafe { decoder.decode_checked_to_ptr(b"Y!Q==", output.as_mut_ptr(), false) },
        Some(1)
    );
    assert_eq!(output[0], b'a');
    assert_eq!(
        unsafe { decoder.decode_to_ptr(b"Y!Q==", output.as_mut_ptr(), false) },
        1
    );

    let canonical = configured_decoder(b"!", false, true, true);
    assert_eq!(canonical.decoded_len(b"AAAA", true), Some(3));

    for input in [b"AB==".as_slice(), b"AAB=".as_slice()] {
        assert_eq!(canonical.decoded_len(input, true), None);
        assert_eq!(
            unsafe { canonical.decode_checked_to_ptr(input, output.as_mut_ptr(), true) },
            None
        );
    }

    assert_eq!(canonical.decoded_len(b"AB==AA", true), Some(3));
    assert_eq!(
        unsafe { canonical.decode_checked_to_ptr(b"AB==AA", output.as_mut_ptr(), true) },
        Some(3)
    );
    assert_eq!(&output[..3], b"\x00\x10\x00");
    assert_eq!(canonical.decoded_len(b"AB==AA", false), None);

    let symbols = vec![b'A'; CONFIGURED_STAGING_CAPACITY * 2];
    let expected = CONFIGURED_STAGING_CAPACITY / 4 * 3 * 2;
    assert_eq!(decoder.decoded_len(&symbols, true), Some(expected));
    assert_eq!(
        unsafe { decoder.decode_checked_to_ptr(&symbols, output.as_mut_ptr(), true) },
        Some(expected)
    );
    assert_eq!(
        unsafe { decoder.decode_to_ptr(&symbols, output.as_mut_ptr(), true) },
        expected
    );

    let mut remapped = configured_decoder(b"!", false, false, false);
    remapped.table[usize::from(b'A')] = 1;
    remapped.preserves_alphanumeric = false;
    assert_eq!(remapped.decoded_len(b"AAAA", true), Some(3));
    assert_eq!(
        unsafe { remapped.decode_checked_to_ptr(b"AAAA", output.as_mut_ptr(), true) },
        Some(3)
    );
    assert_eq!(
        unsafe { remapped.decode_to_ptr(b"AAAA", output.as_mut_ptr(), true) },
        3
    );

    let mut remapped_canonical = configured_decoder(b"!", false, false, true);
    remapped_canonical.table[usize::from(b'A')] = 1;
    remapped_canonical.preserves_alphanumeric = false;
    assert_eq!(remapped_canonical.decoded_len(b"AAAA", true), Some(3));
}
