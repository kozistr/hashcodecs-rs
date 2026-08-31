use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::lenient::{
    AlphanumericPrefix, TranslateBytes, decoded_symbol_len, lenient_continues_after_padding,
    select_alphanumeric_prefix, select_translate_bytes,
};
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_ptr_with_unpadded_layout,
    decode_unpadded_layout,
};
use crate::bindings::base64::decode::fallback::decoding_error;
use crate::bindings::base64::decode::output::BytesWriter;
use crate::bindings::base64::decode::plan::DecodeOptions;
use crate::bindings::base64::{STANDARD_ALPHABET, output_too_small};
use crate::bindings::buffer::{BytesLike, contiguous_bytes_like, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

const ADVANCED_STAGING_CAPACITY: usize = 4096;

#[derive(Clone, Copy)]
struct Translation {
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
    translate: TranslateBytes,
}

impl Translation {
    fn new(table: &[u8; 256]) -> Option<Self> {
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

    unsafe fn apply(self, input: &mut [u8]) {
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

#[derive(Clone, Copy)]
enum StrictSpecials {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Many,
}

impl StrictSpecials {
    fn new(table: &[u8; 256], ignored: &[bool; 256], padded: bool) -> Self {
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

    fn find(self, input: &[u8]) -> Option<usize> {
        match self {
            Self::None => None,
            Self::One(first) => memchr::memchr(first, input),
            Self::Two(first, second) => memchr::memchr2(first, second, input),
            Self::Three(first, second, third) => memchr::memchr3(first, second, third, input),
            Self::Many => unreachable!("many special bytes use the generic decoder"),
        }
    }

    fn forbidden(table: &[u8; 256], ignored: &[bool; 256]) -> Self {
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

struct AdvancedDecoder {
    table: [u8; 256],
    ignored: [bool; 256],
    strict_mode: bool,
    padded: bool,
    canonical: bool,
    alphanumeric_prefix: AlphanumericPrefix,
    strict_specials: StrictSpecials,
    strict_forbidden: StrictSpecials,
    translation: Option<Translation>,
}

impl AdvancedDecoder {
    fn new(py: Python<'_>, options: DecodeOptions<'_, '_>) -> PyResult<Self> {
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

    fn preserves_alphanumeric(&self) -> bool {
        STANDARD_ALPHABET[..62]
            .iter()
            .enumerate()
            .all(|(value, &byte)| self.table[usize::from(byte)] == value as u8)
    }

    fn validate_strict(&self, input: &[u8]) -> Option<usize> {
        if !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return self.validate_strict_specials(input);
        }

        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if !saw_padding && preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    last_value = self.table[usize::from(input[source + run - 1])];
                    source += run;
                    continue;
                }
            }
            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];
            if value < 64 {
                if saw_padding {
                    return None;
                }
                symbols += 1;
                last_value = value;
            } else if byte == b'=' && !equals_is_data {
                if !self.padded {
                    return None;
                }
                saw_padding = true;
                padding += 1;
            } else if !self.ignored[usize::from(byte)] {
                return None;
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        Some(decoded_symbol_len(symbols))
    }

    fn validate_lenient(&self, input: &[u8], continue_after_padding: bool) -> Option<usize> {
        let mut symbols = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    pads = 0;
                    quad_pos = (quad_pos + run) % 4;
                    let last_value = self.table[usize::from(input[source + run - 1])];
                    leftchar = match quad_pos {
                        0 => 0,
                        1 => last_value,
                        2 => last_value & 0x0f,
                        3 => last_value & 0x03,
                        _ => unreachable!("Base64 quartet position is bounded"),
                    };
                    source += run;
                    continue;
                }
            }
            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if self.canonical && quad_pos >= 2 && quad_pos + pads >= 4 && leftchar != 0 {
                    return None;
                }
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    return Some(decoded_symbol_len(symbols));
                }
                continue;
            }

            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            symbols += 1;
            pads = 0;
            match quad_pos {
                0 => {
                    quad_pos = 1;
                    leftchar = value;
                }
                1 => {
                    quad_pos = 2;
                    leftchar = value & 0x0f;
                }
                2 => {
                    quad_pos = 3;
                    leftchar = value & 0x03;
                }
                3 => {
                    quad_pos = 0;
                    leftchar = 0;
                }
                _ => unreachable!("Base64 quartet position is bounded"),
            }
        }

        if quad_pos == 1
            || (self.padded && quad_pos != 0 && quad_pos + pads < 4)
            || (self.canonical && matches!(quad_pos, 2 | 3) && leftchar != 0)
        {
            None
        } else {
            Some(decoded_symbol_len(symbols))
        }
    }

    fn validate_strict_specials(&self, input: &[u8]) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut scratch = [0_u8; ADVANCED_STAGING_CAPACITY / 4 * 3];
        let mut staged = 0;
        let mut symbols = 0;
        let mut last_value = 0;
        let equals_is_padding = self.padded && self.table[usize::from(b'=')] >= 64;
        let data_end = if equals_is_padding {
            memchr::memchr(b'=', input).unwrap_or(input.len())
        } else {
            input.len()
        };
        if self.strict_forbidden.find(&input[..data_end]).is_some() {
            return None;
        }

        let mut source = 0;
        while source < data_end {
            let special = self.strict_specials.find(&input[source..data_end]);
            let run_end = special.map_or(data_end, |offset| source + offset);
            while source < run_end {
                let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                staging[staged..staged + copied].copy_from_slice(&input[source..source + copied]);
                symbols += copied;
                staged += copied;
                source += copied;
                last_value = self.table[usize::from(input[source - 1])];
                if staged == ADVANCED_STAGING_CAPACITY {
                    if let Some(translation) = self.translation {
                        unsafe { translation.apply(&mut staging) };
                    }
                    if !validate_advanced_staging(&staging, &mut scratch) {
                        return None;
                    }
                    staged = 0;
                }
            }
            if source == data_end {
                break;
            }

            let byte = input[source];
            source += 1;
            debug_assert!(
                self.table[usize::from(byte)] >= 64 && self.ignored[usize::from(byte)],
                "strict special-byte search only returns discarded bytes"
            );
        }

        if staged != 0 {
            if let Some(translation) = self.translation {
                unsafe { translation.apply(&mut staging[..staged]) };
            }
            if !validate_advanced_staging(&staging[..staged], &mut scratch) {
                return None;
            }
        }

        let mut padding = 0;
        if data_end < input.len() {
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

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        Some(decoded_symbol_len(symbols))
    }

    fn decoded_len(&self, input: &[u8], continue_after_padding: bool) -> Option<usize> {
        if self.strict_mode {
            self.validate_strict(input)
        } else {
            self.validate_lenient(input, continue_after_padding)
        }
    }

    unsafe fn decode_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> Option<usize> {
        if self.strict_mode {
            unsafe { self.decode_strict_checked_to_ptr(input, output) }
        } else {
            unsafe { self.decode_lenient_checked_to_ptr(input, output, continue_after_padding) }
        }
    }

    unsafe fn decode_strict_checked_to_ptr(&self, input: &[u8], output: *mut u8) -> Option<usize> {
        if !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return unsafe { self.decode_strict_specials_to_ptr(input, output) };
        }

        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if !saw_padding && preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    last_value = self.table[usize::from(input[source + run - 1])];
                    unsafe {
                        stage_advanced_symbols(
                            &input[source..source + run],
                            &mut staging,
                            &mut staged,
                            output,
                            &mut written,
                        )
                    };
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];
            if value < 64 {
                if saw_padding {
                    return None;
                }
                symbols += 1;
                last_value = value;
                unsafe {
                    stage_advanced_value(value, &mut staging, &mut staged, output, &mut written)
                };
            } else if byte == b'=' && !equals_is_data {
                if !self.padded {
                    return None;
                }
                saw_padding = true;
                padding += 1;
            } else if !self.ignored[usize::from(byte)] {
                return None;
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        unsafe { finish_advanced_staging(&staging, staged, output, written) }.into()
    }

    unsafe fn decode_strict_specials_to_ptr(&self, input: &[u8], output: *mut u8) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut symbols = 0;
        let mut last_value = 0;
        let equals_is_padding = self.padded && self.table[usize::from(b'=')] >= 64;
        let data_end = if equals_is_padding {
            memchr::memchr(b'=', input).unwrap_or(input.len())
        } else {
            input.len()
        };
        if self.strict_forbidden.find(&input[..data_end]).is_some() {
            return None;
        }

        let mut source = 0;
        while source < data_end {
            let special = self.strict_specials.find(&input[source..data_end]);
            let run_end = special.map_or(data_end, |offset| source + offset);
            while source < run_end {
                let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                staging[staged..staged + copied].copy_from_slice(&input[source..source + copied]);
                symbols += copied;
                staged += copied;
                source += copied;
                last_value = self.table[usize::from(input[source - 1])];
                if staged == ADVANCED_STAGING_CAPACITY {
                    if let Some(translation) = self.translation {
                        unsafe { translation.apply(&mut staging) };
                    }
                    written +=
                        unsafe { try_decode_advanced_staging(&staging, output.add(written))? };
                    staged = 0;
                }
            }
            if source == data_end {
                break;
            }

            let byte = input[source];
            source += 1;
            debug_assert!(
                self.table[usize::from(byte)] >= 64 && self.ignored[usize::from(byte)],
                "strict special-byte search only returns discarded bytes"
            );
        }

        let mut padding = 0;
        if data_end < input.len() {
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

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        if staged != 0 {
            if let Some(translation) = self.translation {
                unsafe { translation.apply(&mut staging[..staged]) };
            }
            written +=
                unsafe { try_decode_advanced_staging(&staging[..staged], output.add(written))? };
        }
        Some(written)
    }

    unsafe fn decode_lenient_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    unsafe {
                        stage_advanced_symbols(
                            &input[source..source + run],
                            &mut staging,
                            &mut staged,
                            output,
                            &mut written,
                        )
                    };
                    pads = 0;
                    quad_pos = (quad_pos + run) & 3;
                    let last_value = self.table[usize::from(input[source + run - 1])];
                    leftchar = match quad_pos {
                        0 => 0,
                        1 => last_value,
                        2 => last_value & 0x0f,
                        3 => last_value & 0x03,
                        _ => unreachable!("Base64 quartet position is bounded"),
                    };
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if self.canonical && quad_pos >= 2 && quad_pos + pads >= 4 && leftchar != 0 {
                    return None;
                }
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    return Some(unsafe {
                        finish_advanced_staging(&staging, staged, output, written)
                    });
                }
                continue;
            }

            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            unsafe { stage_advanced_value(value, &mut staging, &mut staged, output, &mut written) };
            pads = 0;
            match quad_pos {
                0 => {
                    quad_pos = 1;
                    leftchar = value;
                }
                1 => {
                    quad_pos = 2;
                    leftchar = value & 0x0f;
                }
                2 => {
                    quad_pos = 3;
                    leftchar = value & 0x03;
                }
                3 => {
                    quad_pos = 0;
                    leftchar = 0;
                }
                _ => unreachable!("Base64 quartet position is bounded"),
            }
        }

        if quad_pos == 1
            || (self.padded && quad_pos != 0 && quad_pos + pads < 4)
            || (self.canonical && matches!(quad_pos, 2 | 3) && leftchar != 0)
        {
            None
        } else {
            Some(unsafe { finish_advanced_staging(&staging, staged, output, written) })
        }
    }

    unsafe fn decode_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> usize {
        if self.strict_mode
            && !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return unsafe { self.decode_strict_specials_to_ptr(input, output) }
                .expect("validated strict advanced Base64 remains valid");
        }

        // Keep enough translated symbols on the stack to amortize SIMD decoder
        // dispatch without allocating a normalized copy of the whole input.
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut source = 0;
        let mut written = 0;
        let mut quad_pos = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    let run_end = source + run;
                    while source < run_end {
                        let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                        staging[staged..staged + copied]
                            .copy_from_slice(&input[source..source + copied]);
                        staged += copied;
                        source += copied;
                        quad_pos = (quad_pos + copied) & 3;
                        if staged == ADVANCED_STAGING_CAPACITY {
                            written +=
                                unsafe { decode_advanced_staging(&staging, output.add(written)) };
                            staged = 0;
                        }
                    }
                    pads = 0;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    break;
                }
                continue;
            }
            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            pads = 0;
            staging[staged] = STANDARD_ALPHABET[usize::from(value)];
            staged += 1;
            quad_pos = (quad_pos + 1) & 3;
            if staged == ADVANCED_STAGING_CAPACITY {
                written += unsafe { decode_advanced_staging(&staging, output.add(written)) };
                staged = 0;
            }
        }

        if staged != 0 {
            written += unsafe { decode_advanced_staging(&staging[..staged], output.add(written)) };
        }
        written
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

pub(super) fn decode_advanced_strict_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: [u8; 2],
    padded: bool,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return decode_advanced_strict_into(
            py,
            &BytesLike::OwnedVec(input),
            output,
            altchars,
            padded,
            transactional_errors,
        );
    }
    let decoder = AdvancedDecoder::new(
        py,
        DecodeOptions::new(Some(altchars), Some(true), padded, None, false),
    )?;
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            if transactional_errors {
                let Some(required) = decoder.decoded_len(input, false) else {
                    return Err(Base64Error::InvalidInput);
                };
                if provided < required {
                    return Err(Base64Error::OutputTooSmall { required, provided });
                }
                let written = decoder.decode_to_ptr(input, output, false);
                debug_assert_eq!(written, required);
                return Ok(written);
            }

            let required = translated_strict_decoded_len(input, altchars, padded)?;
            if provided < required {
                return Err(Base64Error::OutputTooSmall { required, provided });
            }
            decoder
                .decode_checked_to_ptr(input, output, false)
                .ok_or(Base64Error::InvalidInput)
        })
    })
}

#[inline]
unsafe fn decode_advanced_staging(input: &[u8], output: *mut u8) -> usize {
    unsafe { try_decode_advanced_staging(input, output) }
        .expect("validated advanced Base64 staging remains valid")
}

#[inline]
unsafe fn try_decode_advanced_staging(input: &[u8], output: *mut u8) -> Option<usize> {
    let layout = decode_unpadded_layout(input).ok()?;
    unsafe { decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard) }
        .ok()?;
    Some(layout.output_len())
}

fn validate_advanced_staging(
    input: &[u8],
    scratch: &mut [u8; ADVANCED_STAGING_CAPACITY / 4 * 3],
) -> bool {
    let Ok(layout) = decode_unpadded_layout(input) else {
        return false;
    };
    unsafe {
        decode_to_ptr_with_unpadded_layout(
            input,
            scratch.as_mut_ptr(),
            layout,
            DecodeAlphabet::Standard,
        )
    }
    .is_ok()
}

unsafe fn stage_advanced_symbols(
    input: &[u8],
    staging: &mut [u8; ADVANCED_STAGING_CAPACITY],
    staged: &mut usize,
    output: *mut u8,
    written: &mut usize,
) {
    let mut source = 0;
    while source < input.len() {
        let copied = (input.len() - source).min(ADVANCED_STAGING_CAPACITY - *staged);
        staging[*staged..*staged + copied].copy_from_slice(&input[source..source + copied]);
        *staged += copied;
        source += copied;
        if *staged == ADVANCED_STAGING_CAPACITY {
            *written += unsafe { decode_advanced_staging(staging, output.add(*written)) };
            *staged = 0;
        }
    }
}

unsafe fn stage_advanced_value(
    value: u8,
    staging: &mut [u8; ADVANCED_STAGING_CAPACITY],
    staged: &mut usize,
    output: *mut u8,
    written: &mut usize,
) {
    staging[*staged] = STANDARD_ALPHABET[usize::from(value)];
    *staged += 1;
    if *staged == ADVANCED_STAGING_CAPACITY {
        *written += unsafe { decode_advanced_staging(staging, output.add(*written)) };
        *staged = 0;
    }
}

unsafe fn finish_advanced_staging(
    staging: &[u8; ADVANCED_STAGING_CAPACITY],
    staged: usize,
    output: *mut u8,
    mut written: usize,
) -> usize {
    if staged != 0 {
        written += unsafe { decode_advanced_staging(&staging[..staged], output.add(written)) };
    }
    written
}

pub(in crate::bindings::base64::decode) fn decode_advanced<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced(py, &BytesLike::OwnedVec(input), options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = lenient_continues_after_padding(py);
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
        return Err(decoding_error(py, "Incorrect padding"));
    };
    unsafe { writer.finish(py, written) }
}

unsafe fn decode_advanced_slice_into(
    py: Python<'_>,
    input: &[u8],
    output: *mut u8,
    provided: usize,
    decoder: &AdvancedDecoder,
    continue_after_padding: bool,
) -> PyResult<usize> {
    let Some(required) = decoder.decoded_len(input, continue_after_padding) else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    if provided < required {
        return Err(output_too_small(required, provided));
    }
    let written = unsafe { decoder.decode_to_ptr(input, output, continue_after_padding) };
    debug_assert_eq!(written, required);
    Ok(written)
}

pub(in crate::bindings::base64::decode) fn decode_advanced_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<usize> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced_into(py, &BytesLike::OwnedVec(input), output, options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = lenient_continues_after_padding(py);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_advanced_slice_into(
                py,
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &decoder,
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_advanced_slice_into(
                py,
                input,
                output,
                provided,
                &decoder,
                continue_after_padding,
            )
        })
    }
}

#[cfg(test)]
#[path = "advanced_tests.rs"]
mod tests;
