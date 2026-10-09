use crate::base64::encode;
use crate::base64::{
    Base64Error, b64decode, b64decode_into, b64decode_urlsafe, b64decoded_len, b64encode,
    b64encode_into, b64encode_urlsafe, b64encoded_len,
};
use base64::Engine;

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn round_trip_alphabets() {
    let input = b"the quick brown fox jumps over the lazy dog";
    assert_eq!(
        b64encode(input),
        "dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw=="
    );
    assert_eq!(
        b64decode(b"dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw==").unwrap(),
        input
    );
    assert_eq!(b64encode_urlsafe(b"\xfb\xff"), "-_8=");
    assert_eq!(b64decode_urlsafe(b"-_8=").unwrap(), b"\xfb\xff");
    assert_eq!(b64decode(b"YQ==").unwrap(), b"a");
    assert_eq!(b64decode(b"YWI=").unwrap(), b"ab");
}

#[test]
fn reject_invalid_input() {
    assert_eq!(b64decode(b"AAAA!AAA"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"abc"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"A"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"A=AA"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"AA=A"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"===="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decode(b"Y!=="), Err(Base64Error::InvalidInput));
    assert_eq!(
        b64decode(b"AAAAAAAAAAAAAAA!"),
        Err(Base64Error::InvalidInput)
    );
    assert_eq!(
        b64decode(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!"),
        Err(Base64Error::InvalidInput)
    );

    let mut invalid_wide = [b'A'; 128];
    invalid_wide[127] = b'!';
    assert_eq!(b64decode(&invalid_wide), Err(Base64Error::InvalidInput));
}

#[test]
fn accept_noncanonical_bits() {
    assert_eq!(b64decode(b"AB==").as_deref(), Ok(&[0][..]));
    assert_eq!(b64decode_urlsafe(b"AB==").as_deref(), Ok(&[0][..]));
}

#[test]
fn match_random_inputs() {
    let mut state = 0x1828_97d4_3c61_5aef_u64;

    for case in 0..128 {
        let length = if case < 16 {
            case * 3
        } else {
            1025 + next_random(&mut state) as usize % (128 * 1024)
        };

        let input: Vec<u8> = (0..length).map(|_| next_random(&mut state) as u8).collect();

        let standard = base64::engine::general_purpose::STANDARD.encode(&input);
        assert_eq!(b64encode(&input), standard, "standard case={case}");
        assert_eq!(
            b64decode(standard.as_bytes()),
            Ok(input.clone()),
            "standard case={case}"
        );

        let urlsafe = base64::engine::general_purpose::URL_SAFE.encode(&input);
        assert_eq!(b64encode_urlsafe(&input), urlsafe, "URL-safe case={case}");
        assert_eq!(
            b64decode_urlsafe(urlsafe.as_bytes()),
            Ok(input),
            "URL-safe case={case}"
        );

        if !standard.is_empty() {
            let malformed_index = next_random(&mut state) as usize % standard.len();
            let mut malformed_standard = standard.into_bytes();
            malformed_standard[malformed_index] = b'!';
            assert_eq!(
                b64decode(&malformed_standard),
                Err(Base64Error::InvalidInput)
            );

            let malformed_index = next_random(&mut state) as usize % urlsafe.len();
            let mut malformed_urlsafe = urlsafe.into_bytes();
            malformed_urlsafe[malformed_index] = b'!';
            assert_eq!(
                b64decode_urlsafe(&malformed_urlsafe),
                Err(Base64Error::InvalidInput)
            );
        }
    }
}

#[test]
fn check_length_bounds() {
    assert_eq!(b64encoded_len(0), Some(0));
    assert_eq!(b64encoded_len(1), Some(4));
    assert_eq!(b64encoded_len(2), Some(4));
    assert_eq!(b64encoded_len(3), Some(4));
    assert_eq!(b64encoded_len(4), Some(8));
    assert_eq!(b64encoded_len(usize::MAX), None);
    assert_eq!(b64decoded_len(b""), Ok(0));
    assert_eq!(b64decoded_len(b"YQ=="), Ok(1));
    assert_eq!(b64decoded_len(b"YWI="), Ok(2));
    assert_eq!(b64decoded_len(b"YWJj"), Ok(3));
    assert_eq!(b64decoded_len(b"abc"), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"===="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"A==="), Err(Base64Error::InvalidInput));
    assert_eq!(b64decoded_len(b"AA=A"), Err(Base64Error::InvalidInput));
}

#[test]
fn format_errors() {
    assert_eq!(
        Base64Error::InvalidInput.to_string(),
        "invalid Base64 input"
    );
    let error = Base64Error::OutputTooSmall {
        required: 8,
        provided: 3,
    };
    assert_eq!(
        error.to_string(),
        "Base64 output requires 8 bytes but the destination has 3"
    );
}

#[test]
fn preserve_short_outputs() {
    let mut encoded = [0xa5; 3];
    assert_eq!(
        b64encode_into(b"hello", &mut encoded),
        Err(Base64Error::OutputTooSmall {
            required: 8,
            provided: 3,
        })
    );
    assert_eq!(encoded, [0xa5; 3]);

    let mut decoded = [0xa5; 2];
    assert_eq!(
        b64decode_into(b"aGVsbG8=", &mut decoded),
        Err(Base64Error::OutputTooSmall {
            required: 5,
            provided: 2,
        })
    );
    assert_eq!(decoded, [0xa5; 2]);
}

#[test]
#[should_panic(expected = "Base64 output slice must have the exact encoded length")]
fn reject_inexact_output() {
    let mut output = [0_u8; 3];
    encode::encode_to_slice(b"abc", &mut output, false);
}
