use std::mem::MaybeUninit;
use std::slice;

use crate::base64::{
    DecodeAlphabet, decode_to_ptr_with_unpadded_layout, decode_unpadded_layout, validate_alphabet,
};
use crate::bindings::base64::STANDARD_ALPHABET;

use super::Translation;

pub(super) const CONFIGURED_STAGING_CAPACITY: usize = 4096;

#[inline]
unsafe fn decode_staging<const CHECKED: bool>(input: &[u8], output: *mut u8) -> Option<usize> {
    let layout = if CHECKED {
        decode_unpadded_layout(input).ok()?
    } else {
        decode_unpadded_layout(input).expect("validated configured Base64 staging remains valid")
    };
    let decoded = unsafe {
        decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard)
    };
    if CHECKED {
        decoded.ok()?;
    } else {
        decoded.expect("validated configured Base64 staging remains valid");
    }
    Some(layout.output_len())
}

pub(super) struct StagingWriter {
    staging: [MaybeUninit<u8>; CONFIGURED_STAGING_CAPACITY],
    staged: usize,
    output: *mut u8,
    written: usize,
    translation: Option<Translation>,
}

impl StagingWriter {
    pub(super) fn new(output: *mut u8, translation: Option<Translation>) -> Self {
        Self {
            staging: [MaybeUninit::uninit(); CONFIGURED_STAGING_CAPACITY],
            staged: 0,
            output,
            written: 0,
            translation,
        }
    }

    pub(super) fn set_translation(&mut self, translation: Option<Translation>) {
        assert_eq!(
            self.staged, 0,
            "translation changes only before staging starts"
        );
        self.translation = translation;
    }

    pub(super) fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = (input.len() - source).min(CONFIGURED_STAGING_CAPACITY - self.staged);
            // The initialized staging range is exactly `0..staged`. Extend it
            // only after copying every byte in the new suffix.
            unsafe {
                self.staging
                    .as_mut_ptr()
                    .add(self.staged)
                    .cast::<u8>()
                    .copy_from_nonoverlapping(input.as_ptr().add(source), copied)
            };
            self.staged += copied;
            source += copied;
            if self.staged == CONFIGURED_STAGING_CAPACITY {
                self.flush::<CHECKED>()?;
            }
        }
        Some(())
    }

    pub(super) fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()> {
        self.staging[self.staged].write(STANDARD_ALPHABET[usize::from(value)]);
        self.staged += 1;
        if self.staged == CONFIGURED_STAGING_CAPACITY {
            self.flush::<CHECKED>()?;
        }
        Some(())
    }

    fn flush<const CHECKED: bool>(&mut self) -> Option<()> {
        // Push methods initialize every byte in this prefix before increasing
        // `staged`; no code reads the uninitialized suffix.
        let staging = unsafe {
            slice::from_raw_parts_mut(self.staging.as_mut_ptr().cast::<u8>(), self.staged)
        };
        if let Some(translation) = self.translation {
            translation.apply(staging);
        }
        self.written +=
            unsafe { decode_staging::<CHECKED>(staging, self.output.add(self.written))? };
        self.staged = 0;
        Some(())
    }

    pub(super) fn finish<const CHECKED: bool>(&mut self) -> Option<usize> {
        if self.staged != 0 {
            self.flush::<CHECKED>()?;
        }
        Some(self.written)
    }
}

pub(super) struct StagingValidator {
    staging: [MaybeUninit<u8>; CONFIGURED_STAGING_CAPACITY],
    staged: usize,
    translation: Option<Translation>,
}

impl StagingValidator {
    pub(super) fn new(translation: Option<Translation>) -> Self {
        Self {
            staging: [MaybeUninit::uninit(); CONFIGURED_STAGING_CAPACITY],
            staged: 0,
            translation,
        }
    }

    pub(super) fn push(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = (input.len() - source).min(CONFIGURED_STAGING_CAPACITY - self.staged);
            // As with `StagingWriter`, `0..staged` is the sole initialized range.
            unsafe {
                self.staging
                    .as_mut_ptr()
                    .add(self.staged)
                    .cast::<u8>()
                    .copy_from_nonoverlapping(input.as_ptr().add(source), copied)
            };
            self.staged += copied;
            source += copied;
            if self.staged == CONFIGURED_STAGING_CAPACITY {
                self.flush()?;
            }
        }
        Some(())
    }

    fn flush(&mut self) -> Option<()> {
        // Every byte in this prefix was initialized by `push`; the remainder
        // of the array stays uninitialized and is never exposed.
        let staging = unsafe {
            slice::from_raw_parts_mut(self.staging.as_mut_ptr().cast::<u8>(), self.staged)
        };
        if let Some(translation) = self.translation {
            translation.apply(staging);
        }
        decode_unpadded_layout(staging).ok()?;
        validate_alphabet(staging, DecodeAlphabet::Standard).ok()?;
        self.staged = 0;
        Some(())
    }

    pub(super) fn finish(mut self) -> Option<()> {
        if self.staged != 0 {
            self.flush()?;
        }
        Some(())
    }
}
