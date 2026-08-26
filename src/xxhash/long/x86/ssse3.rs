//! SSSE3 long-input accumulator.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::xxhash::long::{LongInput, Secret, initial_accumulator, long_schedule};
use crate::xxhash::primitives::P32_1;

#[repr(align(64))]
struct AlignedAccumulator([u64; 8]);

#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn accumulate_stripe(acc: &mut AlignedAccumulator, data: *const u8, secret: *const u8) {
    for vector in 0..4 {
        let byte_offset = vector * 16;
        let input = unsafe { _mm_loadu_si128(data.add(byte_offset).cast()) };
        let key = unsafe { _mm_loadu_si128(secret.add(byte_offset).cast()) };
        let keyed = _mm_xor_si128(input, key);
        let product = _mm_mul_epu32(keyed, _mm_shuffle_epi32::<0xb1>(keyed));
        let swapped = _mm_shuffle_epi32::<0x4e>(input);
        let old = unsafe { _mm_load_si128(acc.0.as_ptr().add(vector * 2).cast()) };
        unsafe {
            _mm_storeu_si128(
                acc.0.as_mut_ptr().add(vector * 2).cast(),
                _mm_add_epi64(_mm_add_epi64(old, swapped), product),
            )
        };
    }
}

#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn scramble(acc: &mut AlignedAccumulator, secret: *const u8) {
    let prime = _mm_set1_epi32(P32_1 as i32);
    for vector in 0..4 {
        let byte_offset = vector * 16;
        let value = unsafe { _mm_load_si128(acc.0.as_ptr().add(vector * 2).cast()) };
        let key = unsafe { _mm_loadu_si128(secret.add(byte_offset).cast()) };
        let mixed = _mm_xor_si128(_mm_xor_si128(value, _mm_srli_epi64::<47>(value)), key);
        let low = _mm_mul_epu32(mixed, prime);
        let high = _mm_slli_epi64::<32>(_mm_mul_epu32(_mm_srli_epi64::<32>(mixed), prime));
        unsafe {
            _mm_storeu_si128(
                acc.0.as_mut_ptr().add(vector * 2).cast(),
                _mm_add_epi64(low, high),
            )
        };
    }
}

#[target_feature(enable = "ssse3")]
/// # Safety
/// The caller must have detected SSSE3 support.
pub(in crate::xxhash::long) unsafe fn accumulate(
    input: LongInput<'_>,
    secret: &Secret,
) -> [u64; 8] {
    let data = input.as_bytes();
    let secret = secret.as_bytes();
    let schedule = long_schedule(input);
    let mut acc = AlignedAccumulator(initial_accumulator());
    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        for stripe in 0..16 {
            unsafe {
                accumulate_stripe(
                    &mut acc,
                    data.as_ptr().add(offset + stripe * 64),
                    secret.as_ptr().add(stripe * 8),
                )
            };
        }
        unsafe { scramble(&mut acc, secret.as_ptr().add(128)) };
    }
    for stripe in 0..schedule.tail_stripes {
        unsafe {
            accumulate_stripe(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }
    unsafe {
        accumulate_stripe(
            &mut acc,
            data.as_ptr().add(schedule.last_offset),
            secret.as_ptr().add(121),
        )
    };
    acc.0
}
