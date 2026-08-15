#![no_main]

use base64::Engine;
use hashcodecs::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64encode,
    b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into,
};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 1024 * 1024;

fuzz_target!(|bytes: &[u8]| {
    let input = &bytes[..bytes.len().min(MAX_INPUT)];
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

    if !standard.is_empty() {
        let mut malformed = standard.into_bytes();
        let malformed_index = input[0] as usize % malformed.len();
        malformed[malformed_index] = b'!';
        let _ = b64decode(&malformed);
    }
});
