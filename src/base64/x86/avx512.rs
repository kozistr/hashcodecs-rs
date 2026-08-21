#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::{Base64Error, STANDARD_ALPHABET, URLSAFE_ALPHABET};
use super::{Avx2StoreMode, Decoder, Store, decode_avx2, encode_avx2_with_store};

const INPUT_MASK_48: __mmask64 = (1_u64 << 48) - 1;

pub(in crate::base64) const ENCODE_SHUFFLE: [u8; 64] = encode_shuffle();
pub(in crate::base64) const DECODE_SHUFFLE: [u8; 64] = decode_shuffle();
pub(in crate::base64) const MULTISHIFT_SHIFTS: [u8; 8] = [10, 4, 22, 16, 42, 36, 54, 48];

const fn encode_shuffle() -> [u8; 64] {
    let mut shuffle = [0; 64];
    let mut group = 0;
    while group < 16 {
        let input = group * 3;
        let output = group * 4;
        shuffle[output] = (input + 1) as u8;
        shuffle[output + 1] = input as u8;
        shuffle[output + 2] = (input + 2) as u8;
        shuffle[output + 3] = (input + 1) as u8;
        group += 1;
    }
    shuffle
}

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

#[target_feature(enable = "avx512vbmi")]
pub(in crate::base64) unsafe fn encode<const URLSAFE: bool>(
    input: &[u8],
    output: *mut u8,
) -> usize {
    if input.len() < 48 {
        return unsafe { encode_avx2_with_store::<URLSAFE>(input, output, Avx2StoreMode::Cached) };
    }

    let alphabet = if URLSAFE {
        URLSAFE_ALPHABET
    } else {
        STANDARD_ALPHABET
    };
    let table = unsafe { _mm512_loadu_si512(alphabet.as_ptr().cast()) };
    let shuffle = unsafe { _mm512_loadu_si512(ENCODE_SHUFFLE.as_ptr().cast()) };
    let shifts = _mm512_set1_epi64(i64::from_le_bytes(MULTISHIFT_SHIFTS));

    let mut source = 0;
    let mut destination = 0;
    while source + 192 <= input.len() {
        unsafe {
            encode_48(
                input.as_ptr().add(source),
                output.add(destination),
                shuffle,
                shifts,
                table,
            )
        };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 48),
                output.add(destination + 64),
                shuffle,
                shifts,
                table,
            )
        };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 96),
                output.add(destination + 128),
                shuffle,
                shifts,
                table,
            )
        };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 144),
                output.add(destination + 192),
                shuffle,
                shifts,
                table,
            )
        };
        source += 192;
        destination += 256;
    }
    while source + 48 <= input.len() {
        unsafe {
            encode_48(
                input.as_ptr().add(source),
                output.add(destination),
                shuffle,
                shifts,
                table,
            )
        };
        source += 48;
        destination += 64;
    }

    if input.len() - source >= 32 {
        source
            + unsafe {
                encode_avx2_with_store::<URLSAFE>(
                    &input[source..],
                    output.add(destination),
                    Avx2StoreMode::Cached,
                )
            }
    } else {
        source
    }
}

#[target_feature(enable = "avx512vbmi")]
#[inline]
unsafe fn encode_48(
    input: *const u8,
    output: *mut u8,
    shuffle: __m512i,
    shifts: __m512i,
    table: __m512i,
) {
    let input = unsafe { _mm512_maskz_loadu_epi8(INPUT_MASK_48, input.cast()) };
    let shuffled = _mm512_permutexvar_epi8(shuffle, input);
    let indices = _mm512_multishift_epi64_epi8(shifts, shuffled);
    unsafe { _mm512_storeu_si512(output.cast(), _mm512_permutexvar_epi8(indices, table)) };
}

#[target_feature(enable = "avx512vbmi")]
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
            )
        };
        let (second, second_invalid) = unsafe {
            decode_64(
                input.as_ptr().add(source + 64),
                lower_table,
                upper_table,
                decode_shuffle,
            )
        };
        if first_invalid | second_invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), INPUT_MASK_48, first) };
        unsafe {
            _mm512_mask_storeu_epi8(output.add(destination + 48).cast(), INPUT_MASK_48, second)
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
            )
        };
        if invalid != 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { _mm512_mask_storeu_epi8(output.add(destination).cast(), INPUT_MASK_48, decoded) };
        source += 64;
        destination += 48;
    }

    if input.len() - source >= 16 {
        let (tail_source, tail_destination) =
            unsafe { decode_avx2::<A, S>(&input[source..], output.add(destination)) }?;
        Ok((source + tail_source, destination + tail_destination))
    } else {
        Ok((source, destination))
    }
}

#[target_feature(enable = "avx512vbmi")]
#[inline]
unsafe fn decode_64(
    input: *const u8,
    lower_table: __m512i,
    upper_table: __m512i,
    decode_shuffle: __m512i,
) -> (__m512i, __mmask64) {
    let ascii = unsafe { _mm512_loadu_si512(input.cast()) };
    let indices = _mm512_permutex2var_epi8(lower_table, ascii, upper_table);
    let invalid = _mm512_movepi8_mask(_mm512_or_si512(indices, ascii));
    let merged = _mm512_maddubs_epi16(indices, _mm512_set1_epi32(0x0140_0140));
    let packed = _mm512_madd_epi16(merged, _mm512_set1_epi32(0x0001_1000));
    (_mm512_permutexvar_epi8(decode_shuffle, packed), invalid)
}
