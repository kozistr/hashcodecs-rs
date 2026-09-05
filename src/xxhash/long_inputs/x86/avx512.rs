//! AVX-512 long-input accumulator.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::xxhash::long_inputs::{
    LongInput, Secret, build_long_input_schedule, initial_accumulator,
};
use crate::xxhash::primitives::P32_1;

#[repr(align(64))]
struct AlignedAccumulator([u64; 8]);

#[target_feature(enable = "avx512f")]
pub(in crate::xxhash::long_inputs) unsafe fn accumulate(
    input: LongInput<'_>,
    secret: &Secret,
) -> [u64; 8] {
    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn accumulate(acc: &mut AlignedAccumulator, data: *const u8, secret: *const u8) {
        let input = unsafe { _mm512_loadu_si512(data.cast()) };
        let secret_values = unsafe { _mm512_loadu_si512(secret.cast()) };
        let mixed = _mm512_xor_si512(input, secret_values);
        let product = _mm512_mul_epu32(mixed, _mm512_srli_epi64::<32>(mixed));
        let swapped = _mm512_shuffle_epi32::<0x4e>(input);
        let old = unsafe { _mm512_load_si512(acc.0.as_ptr().cast()) };

        unsafe {
            _mm512_storeu_si512(
                acc.0.as_mut_ptr().cast(),
                _mm512_add_epi64(_mm512_add_epi64(old, swapped), product),
            )
        };
    }

    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn scramble(acc: &mut AlignedAccumulator, secret: *const u8) {
        let value = unsafe { _mm512_load_si512(acc.0.as_ptr().cast()) };
        let secret_values = unsafe { _mm512_loadu_si512(secret.cast()) };
        let mixed = _mm512_xor_si512(
            _mm512_xor_si512(value, _mm512_srli_epi64::<47>(value)),
            secret_values,
        );
        let prime = _mm512_set1_epi32(P32_1 as i32);
        let low = _mm512_mul_epu32(mixed, prime);
        let high = _mm512_slli_epi64::<32>(_mm512_mul_epu32(_mm512_srli_epi64::<32>(mixed), prime));

        unsafe { _mm512_store_si512(acc.0.as_mut_ptr().cast(), _mm512_add_epi64(low, high)) };
    }

    let data = input.as_bytes();
    let secret = secret.as_bytes();
    let schedule = build_long_input_schedule(input);
    let mut acc = AlignedAccumulator(initial_accumulator());

    for block in 0..schedule.full_blocks() {
        let offset = block * 1024;
        for stripe in 0..16 {
            unsafe {
                accumulate(
                    &mut acc,
                    data.as_ptr().add(offset + stripe * 64),
                    secret.as_ptr().add(stripe * 8),
                )
            };
        }

        unsafe { scramble(&mut acc, secret.as_ptr().add(128)) };
    }

    for stripe in 0..schedule.tail_stripes() {
        unsafe {
            accumulate(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset() + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }

    unsafe {
        accumulate(
            &mut acc,
            data.as_ptr().add(schedule.last_offset()),
            secret.as_ptr().add(121),
        )
    };

    acc.0
}
