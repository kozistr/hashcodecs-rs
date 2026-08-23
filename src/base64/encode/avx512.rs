//! AVX-512 VBMI encoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::{STANDARD_ALPHABET, URLSAFE_ALPHABET};
use super::avx2::{Avx2StoreMode, encode_avx2_with_store};

const INPUT_MASK_48: __mmask64 = (1_u64 << 48) - 1;

pub(in crate::base64) const ENCODE_SHUFFLE: [u8; 64] = encode_shuffle();
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
