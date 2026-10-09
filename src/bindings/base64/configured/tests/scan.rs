use super::configured_decoder;
use crate::base64::STANDARD_ALPHABET;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::bindings::base64::scan::x86;
use crate::bindings::base64::{
    configured::{
        ConfiguredDecoder, IGNORED_CONFIGURED_VALUE, StrictSpecials, preserves_alphanumeric,
    },
    lenient::lenient_decode_table,
    policy::{Padding, PreparedPolicy, Validation, WarningScan},
    scan::{
        is_lenient_symbol, lenient_symbol_count,
        scalar::{alphanumeric_prefix_scalar, symbol_prefix_scalar, translate_bytes_scalar},
    },
};

#[test]
fn count_symbols() {
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
fn bound_scalar_scans() {
    assert_eq!(alphanumeric_prefix_scalar(b""), 0);
    assert_eq!(alphanumeric_prefix_scalar(b"abcXYZ09"), 8);
    assert_eq!(alphanumeric_prefix_scalar(b"abc!XYZ"), 3);
    assert_eq!(symbol_prefix_scalar(b"A+/9-_!", Some(*b"-_")), 6);

    let mut input = *b"@a#b@#";
    translate_bytes_scalar(&mut input, b'@', b'+', b'#', b'/');
    assert_eq!(&input, b"+a/b+/");
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_x86_scans() {
    if !std::is_x86_feature_detected!("sse2") {
        return;
    }

    let valid = vec![b'A'; 97];
    assert_eq!(unsafe { x86::alphanumeric_prefix_sse2(&valid) }, 97);
    let mut interrupted = valid;
    interrupted[47] = b'!';
    assert_eq!(unsafe { x86::alphanumeric_prefix_sse2(&interrupted) }, 47);
    assert_eq!(
        unsafe { x86::symbol_count_sse2(&interrupted, Some(*b"@#")) },
        interrupted
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, Some(*b"@#")))
            .count()
    );
    let mut symbols: Vec<u8> = b"A+/_".iter().copied().cycle().take(97).collect();
    for length in [0, 1, 15, 16, 17, 31, 32, 97] {
        assert_eq!(
            unsafe { x86::symbol_prefix_sse2(&symbols[..length], Some(*b"-_")) },
            length
        );
    }
    symbols[96] = b'!';
    assert_eq!(
        unsafe { x86::symbol_prefix_sse2(&symbols, Some(*b"-_")) },
        96
    );
    symbols[47] = b'!';
    assert_eq!(
        unsafe { x86::symbol_prefix_sse2(&symbols, Some(*b"-_")) },
        47
    );

    let original: Vec<u8> = b"@#ab".iter().copied().cycle().take(67).collect();
    let mut expected = original.clone();
    translate_bytes_scalar(&mut expected, b'@', b'+', b'#', b'/');
    let mut translated = original;
    unsafe { x86::translate_sse2(&mut translated, b'@', b'+', b'#', b'/') };
    assert_eq!(translated, expected);

    if std::is_x86_feature_detected!("avx2") {
        for altchars in [None, Some(*b"-_"), Some(*b"@#"), Some(*b"=_")] {
            for byte in 0..=u8::MAX {
                let input = [byte; 32];
                let expected = usize::from(is_lenient_symbol(byte, altchars)) * input.len();
                assert_eq!(
                    unsafe { x86::symbol_count_avx2(&input, altchars) },
                    expected,
                    "byte={byte:#04x} altchars={altchars:?}",
                );
                assert_eq!(
                    unsafe { x86::symbol_prefix_avx2(&input, altchars) },
                    expected,
                    "byte={byte:#04x} altchars={altchars:?}",
                );
            }
        }

        assert_eq!(unsafe { x86::alphanumeric_prefix_avx2(&interrupted) }, 47);
        assert_eq!(
            unsafe { x86::symbol_count_avx2(&interrupted, Some(*b"@#")) },
            interrupted
                .iter()
                .filter(|&&byte| is_lenient_symbol(byte, Some(*b"@#")))
                .count()
        );
        assert_eq!(
            unsafe { x86::symbol_prefix_avx2(&symbols, Some(*b"-_")) },
            47
        );
        let mut translated: Vec<u8> = b"@#ab".iter().copied().cycle().take(99).collect();
        let mut expected = translated.clone();
        translate_bytes_scalar(&mut expected, b'@', b'+', b'#', b'/');
        unsafe { x86::translate_avx2(&mut translated, b'@', b'+', b'#', b'/') };
        assert_eq!(translated, expected);
    }
}

#[test]
fn mark_ignored_bytes() {
    assert!(std::mem::size_of::<ConfiguredDecoder>() <= 320);
    let decoder = configured_decoder(b"!", true, true, false);
    assert_eq!(decoder.table[usize::from(b'!')], IGNORED_CONFIGURED_VALUE);
    assert_eq!(decoder.table[usize::from(b'?')], 64);
}

#[test]
fn cache_alphanumeric_flag() {
    for (altchars, expected) in [
        (None, true),
        (Some(*b"-_"), true),
        (Some(*b"@#"), true),
        (Some(*b"A#"), false),
        (Some(*b"#z"), false),
    ] {
        let decoder = ConfiguredDecoder::new(&PreparedPolicy {
            altchars,
            warning_altchars: altchars,
            warning_scan: WarningScan::Pending,
            urlsafe_warning: false,
            alphabet: None,
            validation: Validation::Lenient,
            padding: Padding::Padded,
            ignorechars_specified: true,
            ignored: None,
            canonical: false,
        });
        assert_eq!(decoder.preserves_alphanumeric, expected);
        assert_eq!(
            decoder.preserves_alphanumeric,
            preserves_alphanumeric(&decoder.table)
        );
    }
}

#[test]
fn select_special_search() {
    let table = lenient_decode_table(None);

    for (ignored_bytes, expected) in [
        (b"".as_slice(), 0),
        (b"!".as_slice(), 1),
        (b"!?".as_slice(), 2),
        (b"!?~".as_slice(), 3),
        (b"!?~%".as_slice(), 4),
    ] {
        let mut table = table;

        for &byte in ignored_bytes {
            table[usize::from(byte)] = IGNORED_CONFIGURED_VALUE;
        }

        let specials = StrictSpecials::new(&table);
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

        let forbidden = StrictSpecials::forbidden(&table);
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
#[should_panic(expected = "many special bytes use the generic scanner")]
fn reject_generic_search() {
    StrictSpecials::Many.find(b"abc");
}
