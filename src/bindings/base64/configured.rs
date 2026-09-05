//! Configured alphabet translation and strict/lenient scanning.

use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::lenient::decoded_symbol_len;
use super::policy::{ErrorWrites, Padding, PreparedPolicy, Validation};
use super::scan::{AlphanumericPrefix, TranslateBytes, decode_byte_kernels};
use super::staging::{BytesWriter, StagingValidator, StagingWriter};
use crate::base64::{
    Base64Error, DecodeAlphabet, STANDARD_ALPHABET, decode_layout, decode_unpadded_layout,
    validate_alphabet,
};
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::compatibility::PythonSemantics;
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

#[derive(Clone, Copy)]
pub(super) struct Translation {
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
    translate: TranslateBytes,
}

impl Translation {
    pub(super) fn new(
        table: &[u8; 256],
        altchars: Option<[u8; 2]>,
        translate: TranslateBytes,
    ) -> Option<Self> {
        let altchars = altchars?;
        let mut sources = [0_u8; 2];
        let mut targets = [0_u8; 2];
        let mut count = 0;
        for byte in altchars {
            if sources[..count].contains(&byte) {
                continue;
            }
            let value = table[usize::from(byte)];
            if value < 64 && STANDARD_ALPHABET[usize::from(value)] != byte {
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
            translate,
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

const INVALID_CONFIGURED_VALUE: u8 = 64;
const IGNORED_CONFIGURED_VALUE: u8 = 65;

#[inline]
fn is_ignored_value(value: u8) -> bool {
    value == IGNORED_CONFIGURED_VALUE
}

pub(super) struct ConfiguredDecoder {
    pub(super) table: [u8; 256],
    pub(super) validation: Validation,
    pub(super) padding: Padding,
    pub(super) canonical: bool,
    pub(super) alphanumeric_prefix: AlphanumericPrefix,
    pub(super) strict_specials: StrictSpecials,
    pub(super) strict_forbidden: StrictSpecials,
    pub(super) translation: Option<Translation>,
}

impl ConfiguredDecoder {
    pub(super) fn new(policy: &PreparedPolicy) -> Self {
        let kernels = decode_byte_kernels();
        let altchars = policy.altchars;
        let ignored = policy.ignored.unwrap_or_default();

        let mut table = [INVALID_CONFIGURED_VALUE; 256];
        for byte in u8::MIN..=u8::MAX {
            if ignored.contains(byte) {
                table[usize::from(byte)] = IGNORED_CONFIGURED_VALUE;
            }
        }
        for (value, &byte) in STANDARD_ALPHABET[..62].iter().enumerate() {
            table[usize::from(byte)] = value as u8;
        }
        let custom_alphabet = altchars.is_some() && policy.ignorechars_specified;
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

        let strict_specials = StrictSpecials::new(&table, policy.padding.is_padded());
        let strict_forbidden = StrictSpecials::forbidden(&table);
        let translation = Translation::new(&table, altchars, kernels.translate);
        Self {
            table,
            validation: policy.validation,
            padding: policy.padding,
            canonical: policy.canonical,
            alphanumeric_prefix: kernels.scanner,
            strict_specials,
            strict_forbidden,
            translation,
        }
    }

    pub(super) fn preserves_alphanumeric(&self) -> bool {
        STANDARD_ALPHABET[..62]
            .iter()
            .enumerate()
            .all(|(value, &byte)| self.table[usize::from(byte)] == value as u8)
    }
}

#[derive(Clone, Copy)]
pub(super) enum StrictSpecials {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Many,
}

impl StrictSpecials {
    pub(super) fn new(table: &[u8; 256], padded: bool) -> Self {
        let equals_is_padding = padded && table[usize::from(b'=')] >= 64;
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            let discarded = is_ignored_value(value) && !(equals_is_padding && byte == b'=');
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

    pub(super) fn forbidden(table: &[u8; 256]) -> Self {
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for &byte in STANDARD_ALPHABET {
            if table[usize::from(byte)] == INVALID_CONFIGURED_VALUE {
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
            _ => unreachable!("strict forbidden-byte count is bounded"),
        }
    }
}

fn translated_strict_decoded_len(
    input: &[u8],
    altchars: [u8; 2],
    padded: bool,
) -> Result<usize, Base64Error> {
    if padded {
        if altchars.contains(&b'=') {
            return (input.len() & 3 == 0)
                .then(|| input.len() / 4 * 3)
                .ok_or(Base64Error::InvalidInput);
        }
        return decode_layout(input).map(|layout| layout.output_len());
    }
    if !altchars.contains(&b'=') && input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    decode_unpadded_layout(input).map(|layout| layout.output_len())
}

pub(super) fn decode_configured_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: [u8; 2],
    decoder: &ConfiguredDecoder,
    error_writes: ErrorWrites,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return decode_configured_strict_into(
            &BytesLike::OwnedVec(input),
            output,
            altchars,
            decoder,
            error_writes,
        );
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            if error_writes.validated_prefix_only() {
                let Some(required) = decoder.decoded_len(input, false) else {
                    return Err(Base64Error::InvalidInput);
                };
                if provided < required {
                    return Err(Base64Error::OutputTooSmall { required, provided });
                }
                let written = decoder.decode_validated_to_ptr(input, output, false);
                debug_assert_eq!(written, required);
                return Ok(written);
            }

            let required =
                translated_strict_decoded_len(input, altchars, decoder.padding.is_padded())?;
            if provided < required {
                return Err(Base64Error::OutputTooSmall { required, provided });
            }
            decoder
                .decode_checked_to_ptr(input, output, false)
                .ok_or(Base64Error::InvalidInput)
        })
    })
}

pub(super) fn decode_configured<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    decoder: &ConfiguredDecoder,
    semantics: PythonSemantics,
) -> PyResult<Result<Bound<'py, PyBytes>, Base64Error>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_configured(py, &BytesLike::OwnedVec(input), decoder, semantics);
    }
    let continue_after_padding = semantics.continues_after_padding;
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decoder.decode_checked_to_ptr(
                    input,
                    output_address as *mut u8,
                    continue_after_padding,
                )
            };
            if detach { py.detach(decode) } else { decode() }
        })
    };
    let Some(written) = result else {
        return Ok(Err(Base64Error::InvalidInput));
    };
    unsafe { writer.finish(py, written).map(Ok) }
}

unsafe fn decode_configured_slice_into(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    decoder: &ConfiguredDecoder,
    continue_after_padding: bool,
) -> Result<usize, Base64Error> {
    let Some(required) = decoder.decoded_len(input, continue_after_padding) else {
        return Err(Base64Error::InvalidInput);
    };
    if provided < required {
        return Err(Base64Error::OutputTooSmall { required, provided });
    }
    let written = unsafe { decoder.decode_validated_to_ptr(input, output, continue_after_padding) };
    debug_assert_eq!(written, required);
    Ok(written)
}

pub(super) fn decode_configured_into(
    _py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    decoder: &ConfiguredDecoder,
    semantics: PythonSemantics,
) -> PyResult<Result<usize, Base64Error>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_configured_into(
            _py,
            &BytesLike::OwnedVec(input),
            output,
            decoder,
            semantics,
        );
    }
    let continue_after_padding = semantics.continues_after_padding;
    if let Some(input) = input.snapshot_for_output(output)? {
        return Ok(with_bytearray(output, || unsafe {
            decode_configured_slice_into(
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                decoder,
                continue_after_padding,
            )
        }));
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_configured_slice_into(input, output, provided, decoder, continue_after_padding)
        })
    })
}

trait ScanSink: Sized {
    fn set_translation(&mut self, translation: Option<Translation>);
    fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8], validate: bool) -> Option<()>;
    fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()>;
    fn finish<const CHECKED: bool>(self, expected: usize) -> Option<usize>;
}

struct CountSink {
    translation: Option<Translation>,
    validator: Option<Box<StagingValidator>>,
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
            if self.translation.is_none() {
                validate_alphabet(input, DecodeAlphabet::Standard).ok()?;
            } else {
                self.validator
                    .get_or_insert_with(|| Box::new(StagingValidator::new(self.translation)))
                    .push(input)?;
            }
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
        sink: S,
        continue_after_padding: bool,
    ) -> Option<usize> {
        if self.validation == Validation::Strict
            && !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return self.scan_strict_specials::<S, CHECKED>(input, sink);
        }
        match self.validation {
            Validation::Strict => self.scan_strict::<S, CHECKED>(input, sink),
            Validation::Lenient => {
                self.scan_lenient::<S, CHECKED>(input, sink, continue_after_padding)
            }
        }
    }

    fn scan_strict<S: ScanSink, const CHECKED: bool>(
        &self,
        input: &[u8],
        mut sink: S,
    ) -> Option<usize> {
        let preserves_alphanumeric = self.preserves_alphanumeric();
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let mut source = 0;
        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;

        while source < input.len() {
            if preserves_alphanumeric && !saw_padding {
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    sink.push_symbols::<CHECKED>(&input[source..source + run], false)?;
                    if CHECKED {
                        symbols += run;
                        last_value = self.table[usize::from(input[source + run - 1])];
                    }
                    source += run;
                    continue;
                }
            }
            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];
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
            } else if CHECKED && !is_ignored_value(value) {
                return None;
            }
        }
        self.finish_strict::<S, CHECKED>(sink, symbols, padding, last_value)
    }

    fn scan_lenient<S: ScanSink, const CHECKED: bool>(
        &self,
        input: &[u8],
        mut sink: S,
        continue_after_padding: bool,
    ) -> Option<usize> {
        let preserves_alphanumeric = self.preserves_alphanumeric();
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let mut source = 0;
        let mut symbols = 0;
        let mut padding = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;

        while source < input.len() {
            if preserves_alphanumeric {
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    sink.push_symbols::<CHECKED>(&input[source..source + run], false)?;
                    if CHECKED {
                        symbols += run;
                    }
                    padding = 0;
                    quad_pos = (quad_pos + run) & 3;
                    if CHECKED && self.canonical {
                        let value = self.table[usize::from(input[source + run - 1])];
                        leftchar = partial_value(quad_pos, value);
                    }
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];

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
                leftchar = partial_value(quad_pos, value);
            }
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
                    is_ignored_value(self.table[usize::from(byte)]),
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
                    if value < 64 || !is_ignored_value(value) {
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

fn partial_value(quad_pos: usize, value: u8) -> u8 {
    match quad_pos {
        0 => 0,
        1 => value,
        2 => value & 0x0f,
        3 => value & 0x03,
        _ => unreachable!("Base64 quartet position is bounded"),
    }
}

#[cfg(test)]
#[path = "configured_tests.rs"]
mod tests;
