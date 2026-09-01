use crate::bindings::base64::STANDARD_ALPHABET;

#[derive(Clone, Copy)]
pub(super) enum StrictSpecials {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Many,
}

impl StrictSpecials {
    pub(super) fn new(table: &[u8; 256], ignored: &[bool; 256], padded: bool) -> Self {
        let equals_is_padding = padded && table[usize::from(b'=')] >= 64;
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            let discarded =
                value >= 64 && ignored[usize::from(byte)] && !(equals_is_padding && byte == b'=');
            if discarded {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict special-byte count is bounded"),
        }
    }

    pub(super) fn find(self, input: &[u8]) -> Option<usize> {
        match self {
            Self::None => None,
            Self::One(first) => memchr::memchr(first, input),
            Self::Two(first, second) => memchr::memchr2(first, second, input),
            Self::Three(first, second, third) => memchr::memchr3(first, second, third, input),
            Self::Many => unreachable!("many special bytes use the generic scanner"),
        }
    }

    pub(super) fn forbidden(table: &[u8; 256], ignored: &[bool; 256]) -> Self {
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for (value, &byte) in STANDARD_ALPHABET.iter().enumerate() {
            if table[usize::from(byte)] >= 64 && !ignored[usize::from(byte)] {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            } else if table[usize::from(byte)] < 64 {
                debug_assert!(
                    table[usize::from(byte)] == value as u8
                        || STANDARD_ALPHABET[usize::from(table[usize::from(byte)])] != byte
                );
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict forbidden-byte count is bounded"),
        }
    }
}
