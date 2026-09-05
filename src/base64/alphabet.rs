pub(crate) const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub(super) const URLSAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(super) const INVALID_VALUE: u8 = u8::MAX;
pub(super) const STANDARD_DECODE: [u8; 256] = decode_table(false, false);
pub(super) const URLSAFE_DECODE: [u8; 256] = decode_table(true, false);
pub(super) const MIXED_DECODE: [u8; 256] = decode_table(true, true);
pub(super) const DECODE_STORE_PADDING: usize = 4;

#[derive(Clone, Copy, Debug)]
pub(crate) enum DecodeAlphabet {
    Standard,
    UrlSafe,
    #[cfg_attr(not(feature = "python"), allow(dead_code))]
    Mixed,
}

pub(super) const fn decode_table(urlsafe: bool, mixed: bool) -> [u8; 256] {
    let mut table = [INVALID_VALUE; 256];
    let mut index = 0;

    while index < 26 {
        table[b'A' as usize + index] = index as u8;
        table[b'a' as usize + index] = index as u8 + 26;
        index += 1;
    }

    index = 0;
    while index < 10 {
        table[b'0' as usize + index] = index as u8 + 52;
        index += 1;
    }

    if urlsafe || mixed {
        table[b'-' as usize] = 62;
        table[b'_' as usize] = 63;
    }

    if !urlsafe || mixed {
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
    }

    table
}
