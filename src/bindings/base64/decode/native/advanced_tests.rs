use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::lenient::{
    LenientDecodeError, alphanumeric_prefix_scalar, decode_lenient_to_ptr, decoded_symbol_len,
    is_lenient_symbol, lenient_decode_table, lenient_decoded_len, lenient_symbol_count,
    translate_bytes_scalar, version_continues_after_padding,
};
use super::{
    ADVANCED_STAGING_CAPACITY, AdvancedDecoder, StagingValidator, StagingWriter, StrictSpecials,
    Translation, decode_advanced_strict_into,
};
use crate::base64::Base64Error;
use crate::bindings::base64::STANDARD_ALPHABET;
use crate::bindings::buffer::BytesLike;

fn advanced_decoder(
    ignored_bytes: &[u8],
    strict_mode: bool,
    padded: bool,
    canonical: bool,
) -> AdvancedDecoder {
    let table = lenient_decode_table(None);
    let mut ignored = [false; 256];
    for &byte in ignored_bytes {
        ignored[usize::from(byte)] = true;
    }
    AdvancedDecoder {
        table,
        ignored,
        strict_mode,
        padded,
        canonical,
        alphanumeric_prefix: alphanumeric_prefix_scalar,
        strict_specials: StrictSpecials::new(&table, &ignored, padded),
        strict_forbidden: StrictSpecials::forbidden(&table, &ignored),
        translation: Translation::new(&table),
    }
}

#[test]
fn simd_lenient_symbol_count_matches_scalar_for_all_bytes_and_alignments() {
    let input: Vec<u8> = (0_u8..=u8::MAX).cycle().take(1024).collect();
    for altchars in [None, Some(*b"-_"), Some(*b"@#"), Some(*b"=_")] {
        for offset in 0..32 {
            for tail in 0..32 {
                let input = &input[offset..input.len() - tail];
                let expected = input
                    .iter()
                    .filter(|&&byte| is_lenient_symbol(byte, altchars))
                    .count();
                assert_eq!(lenient_symbol_count(input, altchars), expected);
            }
        }
    }
}

#[test]
fn scalar_prefix_and_translation_cover_boundaries() {
    assert_eq!(unsafe { alphanumeric_prefix_scalar(b"") }, 0);
    assert_eq!(unsafe { alphanumeric_prefix_scalar(b"abcXYZ09") }, 8);
    assert_eq!(unsafe { alphanumeric_prefix_scalar(b"abc!XYZ") }, 3);

    let mut input = *b"@a#b@#";
    unsafe { translate_bytes_scalar(&mut input, b'@', b'+', b'#', b'/') };
    assert_eq!(&input, b"+a/b+/");
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn x86_prefix_and_translation_kernels_match_scalar() {
    if !std::is_x86_feature_detected!("sse2") {
        return;
    }

    let valid = vec![b'A'; 97];
    assert_eq!(
        unsafe { super::super::lenient::lenient_count_x86::alphanumeric_prefix_sse2(&valid) },
        97
    );
    assert_eq!(
        unsafe { super::super::lenient::alphanumeric_prefix_sse2(&valid) },
        97
    );
    let mut interrupted = valid.clone();
    interrupted[47] = b'!';
    assert_eq!(
        unsafe { super::super::lenient::lenient_count_x86::alphanumeric_prefix_sse2(&interrupted) },
        47
    );
    assert_eq!(
        unsafe { super::super::lenient::lenient_count_x86::sse2(&interrupted, Some(*b"@#")) },
        interrupted
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, Some(*b"@#")))
            .count()
    );

    let original: Vec<u8> = b"@#ab".iter().copied().cycle().take(67).collect();
    let mut expected = original.clone();
    unsafe { translate_bytes_scalar(&mut expected, b'@', b'+', b'#', b'/') };
    let mut translated = original.clone();
    unsafe {
        super::super::lenient::lenient_count_x86::translate_sse2(
            &mut translated,
            b'@',
            b'+',
            b'#',
            b'/',
        )
    };
    assert_eq!(translated, expected);
    let mut translated = original;
    unsafe { super::super::lenient::translate_bytes_sse2(&mut translated, b'@', b'+', b'#', b'/') };
    assert_eq!(translated, expected);

    if std::is_x86_feature_detected!("avx2") {
        assert_eq!(
            unsafe {
                super::super::lenient::lenient_count_x86::alphanumeric_prefix_avx2(&interrupted)
            },
            47
        );
        assert_eq!(
            unsafe { super::super::lenient::lenient_count_x86::avx2(&interrupted, Some(*b"@#")) },
            interrupted
                .iter()
                .filter(|&&byte| is_lenient_symbol(byte, Some(*b"@#")))
                .count()
        );
        let mut translated: Vec<u8> = b"@#ab".iter().copied().cycle().take(99).collect();
        let mut expected = translated.clone();
        unsafe { translate_bytes_scalar(&mut expected, b'@', b'+', b'#', b'/') };
        unsafe {
            super::super::lenient::lenient_count_x86::translate_avx2(
                &mut translated,
                b'@',
                b'+',
                b'#',
                b'/',
            )
        };
        assert_eq!(translated, expected);
    }
}

#[test]
fn lenient_lengths_cover_padding_policies_and_invalid_tails() {
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

    assert!(!version_continues_after_padding(3, 13, 12));
    assert!(version_continues_after_padding(3, 13, 13));
    assert!(!version_continues_after_padding(3, 14, 3));
    assert!(version_continues_after_padding(3, 14, 4));
    assert!(!version_continues_after_padding(3, 12, 99));
    assert!(version_continues_after_padding(3, 15, 0));
    assert!(version_continues_after_padding(4, 0, 0));
}

#[test]
fn lenient_decoder_reports_each_output_boundary_transactionally() {
    let table = lenient_decode_table(None);
    let mut output = [0xa5; 8];
    assert_eq!(
        unsafe {
            decode_lenient_to_ptr::<true>(
                b"YWJj",
                output.as_mut_ptr(),
                output.len(),
                &table,
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
                true,
                true,
            )
        },
        Err(LenientDecodeError::InvalidInput)
    );
}

#[test]
fn strict_special_search_covers_every_width() {
    let table = lenient_decode_table(None);
    for (ignored_bytes, expected) in [
        (b"".as_slice(), 0),
        (b"!".as_slice(), 1),
        (b"!?".as_slice(), 2),
        (b"!?~".as_slice(), 3),
        (b"!?~%".as_slice(), 4),
    ] {
        let mut ignored = [false; 256];
        for &byte in ignored_bytes {
            ignored[usize::from(byte)] = true;
        }
        let specials = StrictSpecials::new(&table, &ignored, true);
        assert!(matches!(
            (expected, specials),
            (0, StrictSpecials::None)
                | (1, StrictSpecials::One(_))
                | (2, StrictSpecials::Two(_, _))
                | (3, StrictSpecials::Three(_, _, _))
                | (4, StrictSpecials::Many)
        ));
    }
    assert_eq!(StrictSpecials::None.find(b"abc"), None);
    assert_eq!(StrictSpecials::One(b'!').find(b"a!c"), Some(1));
    assert_eq!(StrictSpecials::Two(b'!', b'?').find(b"a?c"), Some(1));
    assert_eq!(
        StrictSpecials::Three(b'!', b'?', b'~').find(b"a~c"),
        Some(1)
    );

    for disabled in 0..=4 {
        let mut table = lenient_decode_table(None);
        for &byte in &STANDARD_ALPHABET[..disabled] {
            table[usize::from(byte)] = 64;
        }
        let forbidden = StrictSpecials::forbidden(&table, &[false; 256]);
        assert!(matches!(
            (disabled, forbidden),
            (0, StrictSpecials::None)
                | (1, StrictSpecials::One(_))
                | (2, StrictSpecials::Two(_, _))
                | (3, StrictSpecials::Three(_, _, _))
                | (4, StrictSpecials::Many)
        ));
    }
}

#[test]
fn translation_and_staging_helpers_cover_full_and_partial_buffers() {
    let table = lenient_decode_table(None);
    assert!(Translation::new(&table).is_none());
    let mut translated_table = table;
    translated_table[usize::from(b'@')] = 62;
    let translation = Translation::new(&translated_table).expect("one translated byte");
    let mut translated = b"A@A@".to_vec();
    translation.apply(&mut translated);
    assert_eq!(&translated, b"A+A+");

    let mut output = vec![0xa5; ADVANCED_STAGING_CAPACITY * 2];
    let symbols = vec![b'A'; ADVANCED_STAGING_CAPACITY * 2];
    let mut writer = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(writer.push_symbols::<true>(&symbols), Some(()));
    let written = writer.finish::<true>().unwrap();
    assert_eq!(written, ADVANCED_STAGING_CAPACITY / 4 * 3 * 2);
    assert!(output[..written].iter().all(|&byte| byte == 0));

    let mut writer = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(
        writer.push_symbols::<true>(&symbols[..ADVANCED_STAGING_CAPACITY - 1]),
        Some(())
    );
    assert_eq!(writer.push_value::<true>(0), Some(()));
    assert_eq!(
        writer.finish::<true>(),
        Some(ADVANCED_STAGING_CAPACITY / 4 * 3)
    );

    assert_eq!(
        StagingWriter::new(output.as_mut_ptr(), None).finish::<true>(),
        Some(0)
    );
    let mut invalid = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(invalid.push_symbols::<true>(b"A"), Some(()));
    assert_eq!(invalid.finish::<true>(), None);

    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"AAA"), Some(()));
    assert_eq!(validator.finish(), Some(()));
    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"A"), Some(()));
    assert_eq!(validator.finish(), None);
    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"AA?"), Some(()));
    assert_eq!(validator.finish(), None);
}

#[test]
fn advanced_strict_decoder_covers_generic_validation_and_decode_errors() {
    let decoder = advanced_decoder(b"!?#$", true, true, false);
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

    let unpadded = advanced_decoder(b"!?#$", true, false, false);
    assert_eq!(unpadded.validate_strict(b"AA=="), None);
    let mut output = [0xa5; 8];
    assert_eq!(
        unsafe { unpadded.decode_strict_checked_to_ptr(b"AA==", output.as_mut_ptr()) },
        None
    );

    let canonical = advanced_decoder(b"!?#$", true, true, true);
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
fn advanced_strict_specials_cover_padding_and_staging_errors() {
    let decoder = advanced_decoder(b"!", true, true, false);
    let mut output = vec![0xa5; ADVANCED_STAGING_CAPACITY];

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

    let canonical = advanced_decoder(b"!", true, true, true);
    for input in [b"AB==".as_slice(), b"AAB=".as_slice()] {
        assert_eq!(canonical.validate_strict(input), None);
        assert_eq!(
            unsafe { canonical.decode_strict_checked_to_ptr(input, output.as_mut_ptr()) },
            None
        );
    }

    let mut forbidden = advanced_decoder(b"!", true, true, false);
    forbidden.table[usize::from(b'A')] = 64;
    forbidden.strict_forbidden = StrictSpecials::forbidden(&forbidden.table, &forbidden.ignored);
    assert_eq!(forbidden.validate_strict(b"AAAA"), None);
    assert_eq!(
        unsafe { forbidden.decode_strict_checked_to_ptr(b"AAAA", output.as_mut_ptr()) },
        None
    );

    let symbols = vec![b'A'; ADVANCED_STAGING_CAPACITY];
    let expected = ADVANCED_STAGING_CAPACITY / 4 * 3;
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
fn advanced_strict_into_snapshots_aliases_and_preserves_transactional_errors() {
    Python::initialize();
    Python::attach(|py| {
        let shared = PyByteArray::new(py, b"@#8=");
        assert_eq!(
            decode_advanced_strict_into(
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

        let invalid = PyBytes::new(py, b"AA=");
        let output = PyByteArray::new(py, &[0xa5; 2]);
        assert_eq!(
            decode_advanced_strict_into(
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

        let valid = PyBytes::new(py, b"@#8=");
        let output = PyByteArray::new(py, &[0xa5]);
        assert_eq!(
            decode_advanced_strict_into(
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
fn advanced_lenient_decoder_covers_dispatch_and_canonical_errors() {
    let decoder = advanced_decoder(b"!", false, true, false);
    let mut output = vec![0xa5; ADVANCED_STAGING_CAPACITY * 2];
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

    let canonical = advanced_decoder(b"!", false, true, true);
    for input in [b"AB==".as_slice(), b"AAB=".as_slice()] {
        assert_eq!(canonical.decoded_len(input, true), None);
        assert_eq!(
            unsafe { canonical.decode_checked_to_ptr(input, output.as_mut_ptr(), true) },
            None
        );
    }

    let symbols = vec![b'A'; ADVANCED_STAGING_CAPACITY * 2];
    let expected = ADVANCED_STAGING_CAPACITY / 4 * 3 * 2;
    assert_eq!(decoder.decoded_len(&symbols, true), Some(expected));
    assert_eq!(
        unsafe { decoder.decode_checked_to_ptr(&symbols, output.as_mut_ptr(), true) },
        Some(expected)
    );
    assert_eq!(
        unsafe { decoder.decode_to_ptr(&symbols, output.as_mut_ptr(), true) },
        expected
    );

    let mut remapped = advanced_decoder(b"!", false, false, false);
    remapped.table[usize::from(b'A')] = 1;
    assert!(!remapped.preserves_alphanumeric());
    assert_eq!(remapped.decoded_len(b"AAAA", true), Some(3));
    assert_eq!(
        unsafe { remapped.decode_checked_to_ptr(b"AAAA", output.as_mut_ptr(), true) },
        Some(3)
    );
    assert_eq!(
        unsafe { remapped.decode_to_ptr(b"AAAA", output.as_mut_ptr(), true) },
        3
    );
}
