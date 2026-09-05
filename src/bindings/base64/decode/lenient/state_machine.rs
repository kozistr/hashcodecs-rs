use super::symbols::{decode_byte_kernels, is_lenient_symbol, lenient_symbol_count};
use crate::base64::{
    DecodeAlphabet, decode_to_ptr_with_unpadded_layout, decode_unpadded_layout, decode_valid_prefix,
};
use crate::bindings::base64::STANDARD_ALPHABET;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::bindings::base64::decode) enum LenientDecodeError {
    InvalidInput,
    OutputTooSmall,
}

pub(in crate::bindings::base64::decode) fn lenient_decode_table(
    altchars: Option<[u8; 2]>,
) -> [u8; 256] {
    let mut table = [64_u8; 256];
    for (value, &byte) in STANDARD_ALPHABET.iter().enumerate() {
        table[usize::from(byte)] = value as u8;
    }
    if let Some([plus, slash]) = altchars {
        table[usize::from(plus)] = 62;
        table[usize::from(slash)] = 63;
    }
    table
}

#[inline]
pub(in crate::bindings::base64::decode) fn decoded_symbol_len(symbols: usize) -> usize {
    symbols / 4 * 3
        + match symbols % 4 {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

/// Count the output from CPython's non-strict padding and discard rules.
///
/// Current CPython versions continue after a complete padding sequence, so a
/// branchless symbol count plus the trailing padding state determines the
/// result. Older versions scan alphabet runs and stop at the first complete
/// padding sequence.
pub(in crate::bindings::base64::decode) fn lenient_decoded_len(
    input: &[u8],
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> Result<usize, LenientDecodeError> {
    if continue_after_padding {
        let symbols = lenient_symbol_count(input, altchars);
        let quad_pos = symbols % 4;
        let pads = if padded && !is_lenient_symbol(b'=', altchars) && quad_pos != 0 {
            input
                .iter()
                .rev()
                .take_while(|&&byte| !is_lenient_symbol(byte, altchars))
                .filter(|&&byte| byte == b'=')
                .count()
        } else {
            0
        };
        return if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
            Err(LenientDecodeError::InvalidInput)
        } else {
            Ok(decoded_symbol_len(symbols))
        };
    }

    let mut source = 0;
    let mut symbols = 0;
    let mut pads = 0;
    let symbol_prefix = decode_byte_kernels().symbol_prefix;

    while source < input.len() {
        let run = unsafe { symbol_prefix(&input[source..], altchars) };
        if run != 0 {
            symbols += run;
            pads = 0;
            source += run;
            continue;
        }

        let byte = input[source];
        source += 1;
        if padded && byte == b'=' && !is_lenient_symbol(b'=', altchars) {
            pads += 1;
            let quad_pos = symbols % 4;
            if quad_pos >= 2 && quad_pos + pads >= 4 {
                return Ok(decoded_symbol_len(symbols));
            }
            continue;
        }
    }

    let quad_pos = symbols % 4;
    if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
        Err(LenientDecodeError::InvalidInput)
    } else {
        Ok(decoded_symbol_len(symbols))
    }
}

/// Decode with CPython's non-strict padding and invalid-character semantics.
///
/// # Safety
///
/// `output` must be valid for writes of `provided` bytes and must not overlap
/// `input`.
pub(in crate::bindings::base64::decode) unsafe fn decode_lenient_to_ptr<const WRITE: bool>(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    table: &[u8; 256],
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> Result<usize, LenientDecodeError> {
    let mut source = 0;
    let mut written = 0;
    let mut quad_pos = 0;
    let mut leftchar = 0;
    let mut pads = 0;
    let fast_alphabet = match altchars {
        None => Some(DecodeAlphabet::Standard),
        Some(altchars) if altchars == *b"-_" => Some(DecodeAlphabet::Mixed),
        Some(_) => None,
    };
    let symbol_prefix = decode_byte_kernels().symbol_prefix;

    while source < input.len() {
        if quad_pos == 0 {
            while source < input.len() && table[usize::from(input[source])] >= 64 {
                if padded && input[source] == b'=' {
                    pads += 1;
                }
                source += 1;
            }
            if source == input.len() {
                break;
            }
        }

        let mut prefix_kernel_available = false;
        if WRITE
            && quad_pos == 0
            && let Some(alphabet) = fast_alphabet
        {
            let input_capacity = provided.saturating_sub(written) / 12 * 16;
            let candidate_len = (input.len() - source).min(input_capacity);
            if let Some((consumed, decoded)) = unsafe {
                decode_valid_prefix(
                    &input[source..source + candidate_len],
                    output.add(written),
                    alphabet,
                )
            } {
                prefix_kernel_available = true;
                if consumed != 0 {
                    debug_assert_eq!(decoded, consumed / 4 * 3);
                    source += consumed;
                    written += decoded;
                    pads = 0;
                }
            }
        }
        if !prefix_kernel_available
            && quad_pos == 0
            && let Some(alphabet) = fast_alphabet
        {
            let run = unsafe { symbol_prefix(&input[source..], altchars) };
            let run = run / 4 * 4;
            if run >= 16 {
                let decoded = run / 4 * 3;
                if provided.saturating_sub(written) < decoded {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    let layout = decode_unpadded_layout(&input[source..source + run])
                        .expect("a quartet-aligned run has a valid layout");
                    unsafe {
                        decode_to_ptr_with_unpadded_layout(
                            &input[source..source + run],
                            output.add(written),
                            layout,
                            alphabet,
                        )
                    }
                    .expect("the SIMD scanner accepted every symbol in the run");
                }
                source += run;
                written += decoded;
                pads = 0;
                continue;
            }
        }
        while quad_pos == 0 && source + 4 <= input.len() {
            let first = table[usize::from(input[source])];
            let second = table[usize::from(input[source + 1])];
            let third = table[usize::from(input[source + 2])];
            let fourth = table[usize::from(input[source + 3])];
            if first | second | third | fourth >= 64 {
                break;
            }
            if provided.saturating_sub(written) < 3 {
                return Err(LenientDecodeError::OutputTooSmall);
            }
            let decoded = [
                (first << 2) | (second >> 4),
                (second << 4) | (third >> 2),
                (third << 6) | fourth,
            ];
            if WRITE {
                unsafe {
                    output
                        .add(written)
                        .copy_from_nonoverlapping(decoded.as_ptr(), 3)
                };
            }
            written += 3;
            source += 4;
        }
        if source == input.len() {
            break;
        }

        let byte = input[source];
        source += 1;
        if padded && byte == b'=' && table[usize::from(b'=')] >= 64 {
            pads += 1;
            if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                return Ok(written);
            }
            continue;
        }

        let value = table[usize::from(byte)];
        if value >= 64 {
            continue;
        }
        pads = 0;
        match quad_pos {
            0 => {
                quad_pos = 1;
                leftchar = value;
            }
            1 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 2) | (value >> 4)) };
                }
                written += 1;
                quad_pos = 2;
                leftchar = value & 0x0f;
            }
            2 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 4) | (value >> 2)) };
                }
                written += 1;
                quad_pos = 3;
                leftchar = value & 0x03;
            }
            3 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 6) | value) };
                }
                written += 1;
                quad_pos = 0;
                leftchar = 0;
            }
            _ => unreachable!("Base64 quartet position is bounded"),
        }
    }

    if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
        Err(LenientDecodeError::InvalidInput)
    } else {
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_lenient_sizing_rejects_incomplete_input() {
        assert_eq!(
            lenient_decoded_len(b"A", None, false, false),
            Err(LenientDecodeError::InvalidInput)
        );
        assert_eq!(
            lenient_decoded_len(b"AA", None, true, false),
            Err(LenientDecodeError::InvalidInput)
        );
    }
}
