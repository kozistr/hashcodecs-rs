//! AVX-512 VBMI decoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::avx2::{decode_avx2, decode_prefix_avx2};
use super::x86_contracts::{Decoder, Store};

const OUTPUT_MASK_48: __mmask64 = (1_u64 << 48) - 1;

#[inline]
const fn active_lane_mask(active_lanes: usize) -> __mmask64 {
    debug_assert!(active_lanes != 0 && active_lanes <= 64);
    u64::MAX >> (64 - active_lanes)
}

pub(in crate::base64) const DECODE_SHUFFLE: [u8; 64] = decode_shuffle();

const fn decode_shuffle() -> [u8; 64] {
    let mut shuffle = [0; 64];
    let mut lane = 0;

    while lane < 4 {
        let mut group = 0;

        while group < 4 {
            let source = lane * 16 + group * 4;
            let destination = lane * 12 + group * 3;

            shuffle[destination] = (source + 2) as u8;
            shuffle[destination + 1] = (source + 1) as u8;
            shuffle[destination + 2] = source as u8;

            group += 1;
        }

        lane += 1;
    }

    shuffle
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
pub(in crate::base64) unsafe fn decode<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    if input.len() < 64 {
        return unsafe { decode_avx2::<A, S>(input, output) };
    }

    let table = A::decode_table();
    let lower_table = unsafe { _mm512_loadu_si512(table.as_ptr().cast()) };
    let upper_table = unsafe { _mm512_loadu_si512(table.as_ptr().add(64).cast()) };
    let decode_shuffle = unsafe { _mm512_loadu_si512(DECODE_SHUFFLE.as_ptr().cast()) };

    let mut source = 0;
    let mut destination = 0;

    while source + 128 <= input.len() {
        let (first, first_invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };
        let (second, second_invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source + 64),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };

        if first_invalid | second_invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }

        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), OUTPUT_MASK_48, first) };
        unsafe {
            _mm512_mask_storeu_epi8(output.add(destination + 48).cast(), OUTPUT_MASK_48, second)
        };

        source += 128;
        destination += 96;
    }

    while source + 64 <= input.len() {
        let (decoded, invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };

        if invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }

        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), OUTPUT_MASK_48, decoded) };

        source += 64;
        destination += 48;
    }

    let complete_input = (input.len() - source) / 4 * 4;
    if complete_input != 0 {
        let (decoded, invalid) = unsafe {
            decode_tail::<A>(
                input.as_ptr().add(source),
                complete_input,
                lower_table,
                upper_table,
                decode_shuffle,
            )
        };
        if invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }

        let complete_output = complete_input / 4 * 3;
        let output_mask = (1_u64 << complete_output) - 1;
        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), output_mask, decoded) };
        source += complete_input;
        destination += complete_output;
    }
    Ok((source, destination))
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
pub(in crate::base64) unsafe fn validate<A: Decoder>(input: &[u8]) -> Result<usize, Base64Error> {
    if input.len() < 64 {
        return unsafe { super::avx2::validate::<A>(input) };
    }

    let table = A::decode_table();
    let lower_table = unsafe { _mm512_loadu_si512(table.as_ptr().cast()) };
    let upper_table = unsafe { _mm512_loadu_si512(table.as_ptr().add(64).cast()) };
    let mut source = 0;
    while source + 64 <= input.len() {
        let invalid =
            unsafe { classify_64(input.as_ptr().add(source), lower_table, upper_table).1 };
        if invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }
        source += 64;
    }
    let remaining = input.len() - source;
    if remaining != 0 {
        let input_mask = (1_u64 << remaining) - 1;
        let invalid = unsafe {
            classify_masked(
                input.as_ptr().add(source),
                input_mask,
                lower_table,
                upper_table,
            )
            .1
        };
        if invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }
        source += remaining;
    }
    Ok(source)
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
pub(in crate::base64) unsafe fn decode_prefix<A: Decoder>(
    input: &[u8],
    output: *mut u8,
) -> (usize, usize) {
    if input.len() < 64 {
        return unsafe { decode_prefix_avx2::<A>(input, output) };
    }

    let table = A::decode_table();
    let lower_table = unsafe { _mm512_loadu_si512(table.as_ptr().cast()) };
    let upper_table = unsafe { _mm512_loadu_si512(table.as_ptr().add(64).cast()) };
    let decode_shuffle = unsafe { _mm512_loadu_si512(DECODE_SHUFFLE.as_ptr().cast()) };
    let mut source = 0;
    let mut destination = 0;
    while source + 128 <= input.len() {
        let (first, first_invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };
        let (second, second_invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source + 64),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };
        if first_invalid | second_invalid != 0 {
            break;
        }
        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), OUTPUT_MASK_48, first) };
        unsafe {
            _mm512_mask_storeu_epi8(output.add(destination + 48).cast(), OUTPUT_MASK_48, second)
        };
        source += 128;
        destination += 96;
    }
    while source + 64 <= input.len() {
        let (decoded, invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source),
                lower_table,
                upper_table,
                decode_shuffle,
                A::CHECK_INPUT,
            )
        };
        if invalid != 0 {
            break;
        }
        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), OUTPUT_MASK_48, decoded) };
        source += 64;
        destination += 48;
    }
    // The full-width loop stopped on this vector when it found an invalid
    // lane. Decode that vector again with a mask so complete quartets before
    // the invalid byte can still be returned as a valid prefix.
    let complete_input = (input.len() - source).min(64) / 4 * 4;
    if complete_input != 0 {
        let (decoded, invalid) = unsafe {
            decode_tail::<A>(
                input.as_ptr().add(source),
                complete_input,
                lower_table,
                upper_table,
                decode_shuffle,
            )
        };
        let valid_input = if invalid == 0 {
            complete_input
        } else {
            ((invalid.trailing_zeros() as usize) / 4 * 4).min(complete_input)
        };
        if valid_input != 0 {
            let valid_output = valid_input / 4 * 3;
            let output_mask = (1_u64 << valid_output) - 1;
            unsafe {
                _mm512_mask_storeu_epi8(output.add(destination).cast(), output_mask, decoded)
            };
            source += valid_input;
            destination += valid_output;
        }
    }
    (source, destination)
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
#[inline]
unsafe fn decode_64(
    input: *const u8,
    lower_table: __m512i,
    upper_table: __m512i,
    decode_shuffle: __m512i,
    check_input: bool,
) -> (__m512i, __mmask64) {
    let ascii = unsafe { _mm512_loadu_si512(input.cast()) };
    let indices = _mm512_permutex2var_epi8(lower_table, ascii, upper_table);
    let invalid = if check_input {
        _mm512_movepi8_mask(_mm512_or_si512(indices, ascii))
    } else {
        0
    };
    let merged = _mm512_maddubs_epi16(indices, _mm512_set1_epi32(0x0140_0140));
    let packed = _mm512_madd_epi16(merged, _mm512_set1_epi32(0x0001_1000));
    (_mm512_permutexvar_epi8(decode_shuffle, packed), invalid)
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
#[inline]
unsafe fn decode_tail<A: Decoder>(
    input: *const u8,
    input_len: usize,
    lower_table: __m512i,
    upper_table: __m512i,
    decode_shuffle: __m512i,
) -> (__m512i, __mmask64) {
    let input_mask = active_lane_mask(input_len);
    let (indices, invalid) =
        unsafe { classify_masked(input, input_mask, lower_table, upper_table) };
    let invalid = if A::CHECK_INPUT { invalid } else { 0 };
    let merged = _mm512_maddubs_epi16(indices, _mm512_set1_epi32(0x0140_0140));
    let packed = _mm512_madd_epi16(merged, _mm512_set1_epi32(0x0001_1000));
    (_mm512_permutexvar_epi8(decode_shuffle, packed), invalid)
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
#[inline]
unsafe fn classify_64(
    input: *const u8,
    lower_table: __m512i,
    upper_table: __m512i,
) -> (__m512i, __mmask64) {
    let ascii = unsafe { _mm512_loadu_si512(input.cast()) };
    let indices = _mm512_permutex2var_epi8(lower_table, ascii, upper_table);
    let invalid = _mm512_movepi8_mask(_mm512_or_si512(indices, ascii));
    (indices, invalid)
}

#[target_feature(enable = "avx512vbmi,avx512bw")]
#[inline]
unsafe fn classify_masked(
    input: *const u8,
    input_mask: __mmask64,
    lower_table: __m512i,
    upper_table: __m512i,
) -> (__m512i, __mmask64) {
    let ascii = unsafe { _mm512_maskz_loadu_epi8(input_mask, input.cast()) };
    let indices = _mm512_permutex2var_epi8(lower_table, ascii, upper_table);
    let invalid = _mm512_movepi8_mask(_mm512_or_si512(indices, ascii)) & input_mask;
    (indices, invalid)
}

#[cfg(test)]
mod tests {
    use super::active_lane_mask;

    #[test]
    fn active_lane_masks_include_one_through_sixty_four_bytes() {
        for active_lanes in 1..=64 {
            assert_eq!(
                active_lane_mask(active_lanes).count_ones(),
                active_lanes as u32
            );
            assert_eq!(
                active_lane_mask(active_lanes).trailing_ones(),
                active_lanes as u32
            );
        }
    }
}
