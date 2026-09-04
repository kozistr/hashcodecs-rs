use super::staging::{StagingValidator, StagingWriter};
use super::{ConfiguredDecoder, StrictSpecials, Translation};
use crate::bindings::base64::decode::lenient::decoded_symbol_len;
use crate::bindings::base64::decode::policy::Validation;

trait ScanSink: Sized {
    fn set_translation(&mut self, translation: Option<Translation>);
    fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8], validate: bool) -> Option<()>;
    fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()>;
    fn finish<const CHECKED: bool>(self, expected: usize) -> Option<usize>;
}

struct CountSink {
    translation: Option<Translation>,
    validator: Option<StagingValidator>,
}

impl CountSink {
    fn new() -> Self {
        Self {
            translation: None,
            validator: None,
        }
    }
}

impl ScanSink for CountSink {
    fn set_translation(&mut self, translation: Option<Translation>) {
        self.translation = translation;
    }

    fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8], validate: bool) -> Option<()> {
        let _ = CHECKED;
        if validate {
            self.validator
                .get_or_insert_with(|| StagingValidator::new(self.translation))
                .push(input)?;
        }
        Some(())
    }

    fn push_value<const CHECKED: bool>(&mut self, _value: u8) -> Option<()> {
        let _ = CHECKED;
        Some(())
    }

    fn finish<const CHECKED: bool>(self, expected: usize) -> Option<usize> {
        let _ = CHECKED;
        if let Some(validator) = self.validator {
            validator.finish()?;
        }
        Some(expected)
    }
}

struct WriteSink<'a> {
    writer: &'a mut StagingWriter,
}

impl<'a> WriteSink<'a> {
    fn new(writer: &'a mut StagingWriter) -> Self {
        Self { writer }
    }
}

impl ScanSink for WriteSink<'_> {
    fn set_translation(&mut self, translation: Option<Translation>) {
        self.writer.set_translation(translation);
    }

    fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8], _validate: bool) -> Option<()> {
        self.writer.push_symbols::<CHECKED>(input)
    }

    fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()> {
        self.writer.push_value::<CHECKED>(value)
    }

    fn finish<const CHECKED: bool>(self, expected: usize) -> Option<usize> {
        let written = self.writer.finish::<CHECKED>()?;
        if CHECKED {
            debug_assert_eq!(written, expected);
        }
        Some(written)
    }
}

impl ConfiguredDecoder {
    #[cfg(test)]
    pub(super) fn validate_strict(&self, input: &[u8]) -> Option<usize> {
        debug_assert_eq!(self.validation, Validation::Strict);
        self.decoded_len(input, false)
    }

    #[cfg(test)]
    pub(super) unsafe fn decode_strict_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
    ) -> Option<usize> {
        debug_assert_eq!(self.validation, Validation::Strict);
        unsafe { self.decode_checked_to_ptr(input, output, false) }
    }

    #[cfg(test)]
    pub(super) unsafe fn decode_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> usize {
        unsafe { self.decode_validated_to_ptr(input, output, continue_after_padding) }
    }

    pub(super) fn decoded_len(&self, input: &[u8], continue_after_padding: bool) -> Option<usize> {
        self.scan::<CountSink, true>(input, CountSink::new(), continue_after_padding)
    }

    pub(super) unsafe fn decode_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> Option<usize> {
        let mut writer = StagingWriter::new(output, None);
        self.scan::<WriteSink, true>(input, WriteSink::new(&mut writer), continue_after_padding)
    }

    pub(super) unsafe fn decode_validated_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> usize {
        let mut writer = StagingWriter::new(output, None);
        self.scan::<WriteSink, false>(input, WriteSink::new(&mut writer), continue_after_padding)
            .expect("validated configured Base64 remains valid")
    }

    fn scan<S: ScanSink, const CHECKED: bool>(
        &self,
        input: &[u8],
        mut sink: S,
        continue_after_padding: bool,
    ) -> Option<usize> {
        if self.validation == Validation::Strict
            && !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return self.scan_strict_specials::<S, CHECKED>(input, sink);
        }

        let preserves_alphanumeric = self.preserves_alphanumeric();
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let mut source = 0;
        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;

        while source < input.len() {
            if preserves_alphanumeric && (self.validation == Validation::Lenient || !saw_padding) {
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    sink.push_symbols::<CHECKED>(&input[source..source + run], false)?;
                    if CHECKED {
                        symbols += run;
                    }
                    if self.validation == Validation::Strict {
                        if CHECKED {
                            last_value = self.table[usize::from(input[source + run - 1])];
                        }
                    } else {
                        padding = 0;
                        quad_pos = (quad_pos + run) & 3;
                        if CHECKED && self.canonical {
                            let value = self.table[usize::from(input[source + run - 1])];
                            leftchar = match quad_pos {
                                0 => 0,
                                1 => value,
                                2 => value & 0x0f,
                                3 => value & 0x03,
                                _ => unreachable!("Base64 quartet position is bounded"),
                            };
                        }
                    }
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];

            if self.validation == Validation::Strict {
                if value < 64 {
                    if CHECKED && saw_padding {
                        return None;
                    }
                    sink.push_value::<CHECKED>(value)?;
                    if CHECKED {
                        symbols += 1;
                        last_value = value;
                    }
                } else if byte == b'=' && !equals_is_data {
                    if CHECKED && !self.padding.is_padded() {
                        return None;
                    }
                    saw_padding = true;
                    if CHECKED {
                        padding += 1;
                    }
                } else if CHECKED && !self.ignored[usize::from(byte)] {
                    return None;
                }
                continue;
            }

            if self.padding.is_padded() && byte == b'=' && !equals_is_data {
                padding += 1;
                if CHECKED
                    && self.canonical
                    && quad_pos >= 2
                    && quad_pos + padding >= 4
                    && leftchar != 0
                {
                    return None;
                }
                if !continue_after_padding && quad_pos >= 2 && quad_pos + padding >= 4 {
                    return sink.finish::<CHECKED>(decoded_symbol_len(symbols));
                }
                continue;
            }
            if value >= 64 {
                continue;
            }
            sink.push_value::<CHECKED>(value)?;
            if CHECKED {
                symbols += 1;
            }
            padding = 0;
            quad_pos = (quad_pos + 1) & 3;
            if CHECKED && self.canonical {
                leftchar = match quad_pos {
                    0 => 0,
                    1 => value,
                    2 => value & 0x0f,
                    3 => value & 0x03,
                    _ => unreachable!("Base64 quartet position is bounded"),
                };
            }
        }

        if self.validation == Validation::Strict {
            return self.finish_strict::<S, CHECKED>(sink, symbols, padding, last_value);
        }
        if CHECKED
            && (quad_pos == 1
                || (self.padding.is_padded() && quad_pos != 0 && quad_pos + padding < 4)
                || (self.canonical && matches!(quad_pos, 2 | 3) && leftchar != 0))
        {
            None
        } else {
            sink.finish::<CHECKED>(decoded_symbol_len(symbols))
        }
    }

    fn scan_strict_specials<S: ScanSink, const CHECKED: bool>(
        &self,
        input: &[u8],
        mut sink: S,
    ) -> Option<usize> {
        sink.set_translation(self.translation);
        let equals_is_padding = self.padding.is_padded() && self.table[usize::from(b'=')] >= 64;
        let data_end = if equals_is_padding {
            memchr::memchr(b'=', input).unwrap_or(input.len())
        } else {
            input.len()
        };
        if CHECKED && self.strict_forbidden.find(&input[..data_end]).is_some() {
            return None;
        }

        let mut source = 0;
        let mut symbols = 0;
        let mut last_value = 0;
        while source < data_end {
            let run_end = self
                .strict_specials
                .find(&input[source..data_end])
                .map_or(data_end, |offset| source + offset);
            if source != run_end {
                sink.push_symbols::<CHECKED>(&input[source..run_end], true)?;
                symbols += run_end - source;
                last_value = self.table[usize::from(input[run_end - 1])];
            }
            source = run_end;
            if source != data_end {
                let byte = input[source];
                debug_assert!(
                    self.table[usize::from(byte)] >= 64 && self.ignored[usize::from(byte)],
                    "strict special-byte search only returns discarded bytes"
                );
                source += 1;
            }
        }

        let mut padding = 0;
        if CHECKED {
            for &byte in &input[data_end..] {
                if byte == b'=' {
                    padding += 1;
                } else {
                    let value = self.table[usize::from(byte)];
                    if value < 64 || !self.ignored[usize::from(byte)] {
                        return None;
                    }
                }
            }
        }
        self.finish_strict::<S, CHECKED>(sink, symbols, padding, last_value)
    }

    fn finish_strict<S: ScanSink, const CHECKED: bool>(
        &self,
        sink: S,
        symbols: usize,
        padding: usize,
        last_value: u8,
    ) -> Option<usize> {
        if CHECKED {
            let remainder = symbols & 3;
            let expected_padding = match remainder {
                0 => 0,
                2 => 2,
                3 => 1,
                _ => return None,
            };
            if self.padding.is_padded() && padding != expected_padding {
                return None;
            }
            if self.canonical
                && ((remainder == 2 && last_value & 0x0f != 0)
                    || (remainder == 3 && last_value & 0x03 != 0))
            {
                return None;
            }
        }
        sink.finish::<CHECKED>(decoded_symbol_len(symbols))
    }
}
