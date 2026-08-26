#![no_main]

use base64::Engine;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use hashcodecs::base64::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64encode,
    b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into,
};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 1024 * 1024;

fuzz_target!(|bytes: &[u8]| {
    let input = &bytes[..bytes.len().min(MAX_INPUT)];
    let permissive = GeneralPurposeConfig::new()
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(DecodePaddingMode::RequireCanonical);
    let standard_decoder = GeneralPurpose::new(&alphabet::STANDARD, permissive);
    let urlsafe_decoder = GeneralPurpose::new(&alphabet::URL_SAFE, permissive);

    let actual = b64decode(input);
    let expected = standard_decoder.decode(input);
    assert_eq!(actual.is_ok(), expected.is_ok());
    if let (Ok(actual), Ok(expected)) = (actual, expected) {
        assert_eq!(actual, expected);
    }

    let actual = b64decode_urlsafe(input);
    let expected = urlsafe_decoder.decode(input);
    assert_eq!(actual.is_ok(), expected.is_ok());
    if let (Ok(actual), Ok(expected)) = (actual, expected) {
        assert_eq!(actual, expected);
    }

    let standard = base64::engine::general_purpose::STANDARD.encode(input);
    let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(input);

    assert_eq!(b64encode(input), standard);
    assert_eq!(b64encode_urlsafe(input), urlsafe);
    assert_eq!(b64decode(standard.as_bytes()).as_deref(), Ok(input));
    assert_eq!(b64decode_urlsafe(urlsafe.as_bytes()).as_deref(), Ok(input));

    let mut encoded = vec![0xa5; standard.len() + 8];
    assert_eq!(b64encode_into(input, &mut encoded), Ok(standard.len()));
    assert_eq!(&encoded[..standard.len()], standard.as_bytes());
    assert!(encoded[standard.len()..].iter().all(|byte| *byte == 0xa5));

    let mut url_encoded = vec![0xa5; urlsafe.len() + 8];
    assert_eq!(
        b64encode_urlsafe_into(input, &mut url_encoded),
        Ok(urlsafe.len())
    );
    assert_eq!(&url_encoded[..urlsafe.len()], urlsafe.as_bytes());
    assert!(
        url_encoded[urlsafe.len()..]
            .iter()
            .all(|byte| *byte == 0xa5)
    );

    let mut decoded = vec![0xa5; input.len() + 8];
    assert_eq!(
        b64decode_into(standard.as_bytes(), &mut decoded),
        Ok(input.len())
    );
    assert_eq!(&decoded[..input.len()], input);
    assert!(decoded[input.len()..].iter().all(|byte| *byte == 0xa5));

    let mut url_decoded = vec![0xa5; input.len() + 8];
    assert_eq!(
        b64decode_urlsafe_into(urlsafe.as_bytes(), &mut url_decoded),
        Ok(input.len())
    );
    assert_eq!(&url_decoded[..input.len()], input);
    assert!(url_decoded[input.len()..].iter().all(|byte| *byte == 0xa5));

});
