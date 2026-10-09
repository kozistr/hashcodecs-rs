use crate::bindings::base64::{
    configured::Translation,
    lenient::{decoded_len_upper_bound, lenient_decode_table},
    policy::{DecodePolicy, PreparedDecoder, Validation},
    scan::decode_byte_kernels,
    staging::{CONFIGURED_STAGING_CAPACITY, StagingValidator, StagingWriter},
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[test]
fn stage_partial_blocks() {
    let table = lenient_decode_table(None);
    assert!(Translation::new(&table, None, decode_byte_kernels().translate).is_none());
    let mut translated_table = table;
    translated_table[usize::from(b'@')] = 62;
    let translation = Translation::new(
        &translated_table,
        Some(*b"@#"),
        decode_byte_kernels().translate,
    )
    .expect("one translated byte");
    let mut translated = b"A@A@".to_vec();
    translation.apply(&mut translated);
    assert_eq!(&translated, b"A+A+");

    let mut output = vec![0xa5; CONFIGURED_STAGING_CAPACITY * 2];
    let symbols = vec![b'A'; CONFIGURED_STAGING_CAPACITY * 2];
    let mut writer = StagingWriter::new(output.as_mut_ptr(), None);
    // The output holds both decoded blocks and does not overlap the symbols.
    assert_eq!(unsafe { writer.push_symbols::<true>(&symbols) }, Some(()));
    let written = unsafe { writer.finish::<true>() }.unwrap();
    assert_eq!(written, CONFIGURED_STAGING_CAPACITY / 4 * 3 * 2);
    assert!(output[..written].iter().all(|&byte| byte == 0));

    let mut writer = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(
        unsafe { writer.push_symbols::<true>(&symbols[..CONFIGURED_STAGING_CAPACITY - 1]) },
        Some(())
    );
    assert_eq!(unsafe { writer.push_value::<true>(0) }, Some(()));
    assert_eq!(
        unsafe { writer.finish::<true>() },
        Some(CONFIGURED_STAGING_CAPACITY / 4 * 3)
    );

    assert_eq!(
        unsafe { StagingWriter::new(output.as_mut_ptr(), None).finish::<true>() },
        Some(0)
    );
    let mut invalid = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(unsafe { invalid.push_symbols::<true>(b"A") }, Some(()));
    assert_eq!(unsafe { invalid.finish::<true>() }, None);

    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"AAA"), Some(()));
    assert_eq!(validator.finish(), Some(()));
    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"A"), Some(()));
    assert_eq!(validator.finish(), None);
    let mut validator = StagingValidator::new(None);
    assert_eq!(validator.push(b"AA?"), Some(()));
    assert_eq!(validator.finish(), None);

    let mut validator = StagingValidator::new(None);
    assert_eq!(
        validator.push(&symbols[..CONFIGURED_STAGING_CAPACITY]),
        Some(())
    );
    assert_eq!(validator.finish(), Some(()));
}

#[test]
fn flush_staged_values() {
    let symbols = CONFIGURED_STAGING_CAPACITY + 4;
    let required = symbols / 4 * 3;
    let mut output = vec![0xa5; required + 2];
    let mut writer = StagingWriter::new(unsafe { output.as_mut_ptr().add(1) }, None);
    // The interior has exactly the decoded capacity; guards remain outside it.
    for _ in 0..symbols {
        assert_eq!(unsafe { writer.push_value::<true>(0) }, Some(()));
    }
    assert_eq!(unsafe { writer.finish::<true>() }, Some(required));
    assert_eq!(output[0], 0xa5);
    assert!(output[1..=required].iter().all(|&byte| byte == 0));
    assert_eq!(output[required + 1], 0xa5);
}

#[test]
fn preserve_invalid_tail_guards() {
    Python::initialize();
    Python::attach(|py| {
        let ignored = PyBytes::new(py, b"!");

        for altchars in [None, Some(*b"@#"), Some(*b"=_"), Some(*b"==")] {
            for validation in [Validation::Strict, Validation::Lenient] {
                for padded in [false, true] {
                    for ignorechars_specified in [false, true] {
                        let prepared = PreparedDecoder::new(
                            py,
                            DecodePolicy::new(
                                altchars,
                                Some(validation.is_strict()),
                                padded,
                                ignorechars_specified.then_some(ignored.as_any()),
                                false,
                            ),
                        )
                        .unwrap();
                        let decoder = prepared.configured();

                        for prefix in [0, 4, 128, 4096] {
                            for tail in [
                                b"".as_slice(),
                                b"A",
                                b"AA",
                                b"AAA",
                                b"AA==",
                                b"====",
                                b"AA=A==",
                                b"!!AA==",
                                b"AA!A==",
                            ] {
                                let mut input = vec![b'A'; prefix];
                                input.extend_from_slice(tail);
                                let capacity = decoded_len_upper_bound(&input, &decoder.table);

                                for continue_after_padding in [false, true] {
                                    let mut output = vec![0xa5; capacity + 17];
                                    let written = unsafe {
                                        decoder.decode_checked_to_ptr(
                                            &input,
                                            output.as_mut_ptr().add(1),
                                            continue_after_padding,
                                        )
                                    };
                                    assert!(written.is_none_or(|written| written <= capacity));
                                    assert_eq!(output[0], 0xa5);
                                    assert!(
                                        output[capacity + 1..].iter().all(|&byte| byte == 0xa5)
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

#[test]
fn preserve_fragment_guards() {
    for length in [4, 16, 4092, 4096, 4100, 8192] {
        let input = vec![b'A'; length];
        let required = length / 4 * 3;

        for split in 0..=5.min(length) {
            let mut output = vec![0xa5; required + 17];
            let mut writer = StagingWriter::new(unsafe { output.as_mut_ptr().add(1) }, None);
            // Both fragments share one cursor with space for the full result.
            assert_eq!(
                unsafe { writer.push_symbols::<true>(&input[..split]) },
                Some(())
            );
            assert_eq!(
                unsafe { writer.push_symbols::<true>(&input[split..]) },
                Some(())
            );
            assert_eq!(unsafe { writer.finish::<true>() }, Some(required));
            assert_eq!(output[0], 0xa5);
            assert!(output[1..=required].iter().all(|&byte| byte == 0));
            assert!(output[required + 1..].iter().all(|&byte| byte == 0xa5));
        }
    }

    let mut output = [0xa5; 3];
    let mut writer = StagingWriter::new(output.as_mut_ptr(), None);
    assert_eq!(unsafe { writer.push_symbols::<true>(b"AA!A") }, None);
}
