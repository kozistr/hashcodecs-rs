use crate::base64::{DecodeAlphabet, decode_to_ptr_with_unpadded_layout, decode_unpadded_layout};
use crate::bindings::base64::STANDARD_ALPHABET;

use super::config::Translation;

pub(super) const ADVANCED_STAGING_CAPACITY: usize = 4096;

#[inline]
unsafe fn decode_staging<const CHECKED: bool>(input: &[u8], output: *mut u8) -> Option<usize> {
    let layout = if CHECKED {
        decode_unpadded_layout(input).ok()?
    } else {
        decode_unpadded_layout(input).expect("validated advanced Base64 staging remains valid")
    };
    let decoded = unsafe {
        decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard)
    };
    if CHECKED {
        decoded.ok()?;
    } else {
        decoded.expect("validated advanced Base64 staging remains valid");
    }
    Some(layout.output_len())
}

pub(super) struct StagingWriter {
    staging: [u8; ADVANCED_STAGING_CAPACITY],
    staged: usize,
    output: *mut u8,
    written: usize,
    translation: Option<Translation>,
}

impl StagingWriter {
    pub(super) fn new(output: *mut u8, translation: Option<Translation>) -> Self {
        Self {
            staging: [0; ADVANCED_STAGING_CAPACITY],
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
            let copied = (input.len() - source).min(ADVANCED_STAGING_CAPACITY - self.staged);
            self.staging[self.staged..self.staged + copied]
                .copy_from_slice(&input[source..source + copied]);
            self.staged += copied;
            source += copied;
            if self.staged == ADVANCED_STAGING_CAPACITY {
                self.flush::<CHECKED>()?;
            }
        }
        Some(())
    }

    pub(super) fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()> {
        self.staging[self.staged] = STANDARD_ALPHABET[usize::from(value)];
        self.staged += 1;
        if self.staged == ADVANCED_STAGING_CAPACITY {
            self.flush::<CHECKED>()?;
        }
        Some(())
    }

    fn flush<const CHECKED: bool>(&mut self) -> Option<()> {
        if let Some(translation) = self.translation {
            translation.apply(&mut self.staging[..self.staged]);
        }
        self.written += unsafe {
            decode_staging::<CHECKED>(&self.staging[..self.staged], self.output.add(self.written))?
        };
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
    staging: [u8; ADVANCED_STAGING_CAPACITY],
    scratch: [u8; ADVANCED_STAGING_CAPACITY / 4 * 3],
    staged: usize,
    translation: Option<Translation>,
}

impl StagingValidator {
    pub(super) fn new(translation: Option<Translation>) -> Self {
        Self {
            staging: [0; ADVANCED_STAGING_CAPACITY],
            scratch: [0; ADVANCED_STAGING_CAPACITY / 4 * 3],
            staged: 0,
            translation,
        }
    }

    pub(super) fn push(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = (input.len() - source).min(ADVANCED_STAGING_CAPACITY - self.staged);
            self.staging[self.staged..self.staged + copied]
                .copy_from_slice(&input[source..source + copied]);
            self.staged += copied;
            source += copied;
            if self.staged == ADVANCED_STAGING_CAPACITY {
                self.flush()?;
            }
        }
        Some(())
    }

    fn flush(&mut self) -> Option<()> {
        if let Some(translation) = self.translation {
            translation.apply(&mut self.staging[..self.staged]);
        }
        let layout = decode_unpadded_layout(&self.staging[..self.staged]).ok()?;
        unsafe {
            decode_to_ptr_with_unpadded_layout(
                &self.staging[..self.staged],
                self.scratch.as_mut_ptr(),
                layout,
                DecodeAlphabet::Standard,
            )
        }
        .ok()?;
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
