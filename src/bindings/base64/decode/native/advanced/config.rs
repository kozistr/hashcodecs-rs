use pyo3::prelude::*;

use super::super::lenient::{
    AlphanumericPrefix, TranslateBytes, select_alphanumeric_prefix, select_translate_bytes,
};
use super::specials::StrictSpecials;
use crate::bindings::base64::STANDARD_ALPHABET;
use crate::bindings::base64::decode::plan::DecodeOptions;
use crate::bindings::buffer::contiguous_bytes_like;

#[derive(Clone, Copy)]
pub(super) struct Translation {
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
    translate: TranslateBytes,
}

impl Translation {
    pub(super) fn new(table: &[u8; 256]) -> Option<Self> {
        let mut sources = [0_u8; 2];
        let mut targets = [0_u8; 2];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            if value < 64 && STANDARD_ALPHABET[usize::from(value)] != byte {
                assert!(count < 2, "a Base64 alphabet translates at most two bytes");
                sources[count] = byte;
                targets[count] = STANDARD_ALPHABET[usize::from(value)];
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        if count == 1 {
            sources[1] = sources[0];
            targets[1] = targets[0];
        }
        Some(Self {
            source0: sources[0],
            target0: targets[0],
            source1: sources[1],
            target1: targets[1],
            translate: select_translate_bytes(),
        })
    }

    pub(super) fn apply(self, input: &mut [u8]) {
        unsafe {
            (self.translate)(
                input,
                self.source0,
                self.target0,
                self.source1,
                self.target1,
            )
        };
    }
}

pub(super) struct AdvancedDecoder {
    pub(super) table: [u8; 256],
    pub(super) ignored: [bool; 256],
    pub(super) strict_mode: bool,
    pub(super) padded: bool,
    pub(super) canonical: bool,
    pub(super) alphanumeric_prefix: AlphanumericPrefix,
    pub(super) strict_specials: StrictSpecials,
    pub(super) strict_forbidden: StrictSpecials,
    pub(super) translation: Option<Translation>,
}

impl AdvancedDecoder {
    pub(super) fn new(py: Python<'_>, options: DecodeOptions<'_, '_>) -> PyResult<Self> {
        let DecodeOptions {
            altchars,
            padded,
            ignorechars,
            canonical,
            ..
        } = options;
        let mut ignored = [false; 256];
        if let Some(ignorechars) = ignorechars {
            let ignorechars = contiguous_bytes_like(py, ignorechars, "ignorechars")?;
            unsafe {
                ignorechars.with_bytes(|bytes| {
                    for &byte in bytes {
                        ignored[usize::from(byte)] = true;
                    }
                })
            };
        }

        let mut table = [64; 256];
        for (value, &byte) in STANDARD_ALPHABET[..62].iter().enumerate() {
            table[usize::from(byte)] = value as u8;
        }
        let custom_alphabet = altchars.is_some() && ignorechars.is_some();
        if !custom_alphabet {
            table[usize::from(b'+')] = 62;
            table[usize::from(b'/')] = 63;
        }
        if let Some([plus, slash]) = altchars {
            if !custom_alphabet || plus != b'=' {
                table[usize::from(plus)] = 62;
            }
            if !custom_alphabet || slash != b'=' {
                table[usize::from(slash)] = 63;
            }
        }

        let strict_specials = StrictSpecials::new(&table, &ignored, padded);
        let strict_forbidden = StrictSpecials::forbidden(&table, &ignored);
        let translation = Translation::new(&table);
        Ok(Self {
            table,
            ignored,
            strict_mode: options.strict_mode(),
            padded,
            canonical,
            alphanumeric_prefix: select_alphanumeric_prefix(),
            strict_specials,
            strict_forbidden,
            translation,
        })
    }

    pub(super) fn preserves_alphanumeric(&self) -> bool {
        STANDARD_ALPHABET[..62]
            .iter()
            .enumerate()
            .all(|(value, &byte)| self.table[usize::from(byte)] == value as u8)
    }
}
