//! AVX2 secret initialization and long-input accumulator.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::xxhash::long::{LongInput, Secret, initial_accumulator, long_schedule};
use crate::xxhash::primitives::{P32_1, SECRET};

#[repr(align(64))]
#[derive(Clone, Copy)]
struct AlignedAccumulator([u64; 8]);

#[derive(Clone, Copy)]
pub(super) struct Accumulator {
    low: __m256i,
    high: __m256i,
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support.
pub(in crate::xxhash::long) unsafe fn init_secret(seed: u64) -> Secret {
    let negative = 0_u64.wrapping_sub(seed);
    let delta = _mm256_set_epi64x(negative as i64, seed as i64, negative as i64, seed as i64);
    let mut output = [0_u8; 192];
    for vector in 0..6 {
        let offset = vector * 32;
        let source = unsafe { _mm256_loadu_si256(SECRET.as_ptr().add(offset).cast()) };
        unsafe {
            _mm256_storeu_si256(
                output.as_mut_ptr().add(offset).cast(),
                _mm256_add_epi64(source, delta),
            )
        };
    }
    Secret(output)
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_vector(acc: __m256i, data: *const u8, secret: *const u8) -> __m256i {
    let input = unsafe { _mm256_loadu_si256(data.cast()) };
    let key = unsafe { _mm256_loadu_si256(secret.cast()) };
    let keyed = _mm256_xor_si256(input, key);
    let product = _mm256_mul_epu32(keyed, _mm256_srli_epi64::<32>(keyed));
    let swapped = _mm256_shuffle_epi32::<0x4e>(input);
    _mm256_add_epi64(_mm256_add_epi64(acc, swapped), product)
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn scramble_vector(acc: __m256i, secret: *const u8) -> __m256i {
    let key = unsafe { _mm256_loadu_si256(secret.cast()) };
    let mixed = _mm256_xor_si256(_mm256_xor_si256(acc, _mm256_srli_epi64::<47>(acc)), key);
    let prime = _mm256_set1_epi32(P32_1 as i32);
    let low = _mm256_mul_epu32(mixed, prime);
    let high = _mm256_slli_epi64::<32>(_mm256_mul_epu32(_mm256_srli_epi64::<32>(mixed), prime));
    _mm256_add_epi64(low, high)
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn initial() -> Accumulator {
    let initial = AlignedAccumulator(initial_accumulator());
    Accumulator {
        low: unsafe { _mm256_load_si256(initial.0.as_ptr().cast()) },
        high: unsafe { _mm256_load_si256(initial.0.as_ptr().add(4).cast()) },
    }
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn accumulate_registers(
    acc: &mut Accumulator,
    data: *const u8,
    secret: *const u8,
) {
    acc.low = unsafe { accumulate_vector(acc.low, data, secret) };
    acc.high = unsafe { accumulate_vector(acc.high, data.add(32), secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn scramble_registers(acc: &mut Accumulator, secret: *const u8) {
    acc.low = unsafe { scramble_vector(acc.low, secret) };
    acc.high = unsafe { scramble_vector(acc.high, secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store(acc: Accumulator, output: &mut AlignedAccumulator) {
    unsafe {
        _mm256_store_si256(output.0.as_mut_ptr().cast(), acc.low);
        _mm256_store_si256(output.0.as_mut_ptr().add(4).cast(), acc.high);
    }
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn finish(acc: Accumulator) -> [u64; 8] {
    let mut output = AlignedAccumulator([0; 8]);
    unsafe { store(acc, &mut output) };
    output.0
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn reduce_chains(
    mut acc0: Accumulator,
    acc1: Accumulator,
    acc2: Accumulator,
    acc3: Accumulator,
) -> Accumulator {
    acc0.low = _mm256_add_epi64(_mm256_add_epi64(acc0.low, acc1.low), acc2.low);
    acc0.low = _mm256_add_epi64(acc0.low, acc3.low);
    acc0.high = _mm256_add_epi64(_mm256_add_epi64(acc0.high, acc1.high), acc2.high);
    acc0.high = _mm256_add_epi64(acc0.high, acc3.high);
    acc0
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_group(
    acc: [&mut Accumulator; 4],
    data: *const u8,
    secret: *const u8,
    first_stripe: usize,
) {
    unsafe {
        accumulate_registers(
            acc[0],
            data.add(first_stripe * 64),
            secret.add(first_stripe * 8),
        );
        accumulate_registers(
            acc[1],
            data.add((first_stripe + 1) * 64),
            secret.add((first_stripe + 1) * 8),
        );
        accumulate_registers(
            acc[2],
            data.add((first_stripe + 2) * 64),
            secret.add((first_stripe + 2) * 8),
        );
        accumulate_registers(
            acc[3],
            data.add((first_stripe + 3) * 64),
            secret.add((first_stripe + 3) * 8),
        );
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_block_chains(
    mut acc0: Accumulator,
    data: *const u8,
    secret: *const u8,
) -> Accumulator {
    // Stripe accumulation is additive. Four independent chains shorten the
    // multiply dependency path, then reduce to the canonical accumulator
    // before the caller applies the block scramble.
    let zero = _mm256_setzero_si256();
    let mut acc1 = Accumulator {
        low: zero,
        high: zero,
    };
    let mut acc2 = acc1;
    let mut acc3 = acc1;

    for stripe in (0..16).step_by(4) {
        unsafe {
            accumulate_group(
                [&mut acc0, &mut acc1, &mut acc2, &mut acc3],
                data,
                secret,
                stripe,
            )
        };
    }
    unsafe { reduce_chains(acc0, acc1, acc2, acc3) }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_tail_chains(
    mut acc0: Accumulator,
    data: *const u8,
    secret: *const u8,
    stripes: usize,
    last: *const u8,
) -> Accumulator {
    debug_assert!((3..=15).contains(&stripes));
    let zero = _mm256_setzero_si256();
    let mut acc1 = Accumulator {
        low: zero,
        high: zero,
    };
    let mut acc2 = acc1;
    let mut acc3 = acc1;

    let mut stripe = 0;
    while stripe + 4 <= stripes {
        unsafe {
            accumulate_group(
                [&mut acc0, &mut acc1, &mut acc2, &mut acc3],
                data,
                secret,
                stripe,
            )
        };
        stripe += 4;
    }
    let remaining = stripes - stripe;
    if remaining >= 1 {
        unsafe { accumulate_registers(&mut acc0, data.add(stripe * 64), secret.add(stripe * 8)) };
    }
    if remaining >= 2 {
        unsafe {
            accumulate_registers(
                &mut acc1,
                data.add((stripe + 1) * 64),
                secret.add((stripe + 1) * 8),
            )
        };
    }
    if remaining == 3 {
        unsafe {
            accumulate_registers(
                &mut acc2,
                data.add((stripe + 2) * 64),
                secret.add((stripe + 2) * 8),
            )
        };
    }

    let last_secret = unsafe { secret.add(121) };
    let chains = [&mut acc0, &mut acc1, &mut acc2, &mut acc3];
    unsafe {
        accumulate_registers(chains[stripes % 4], last, last_secret);
        reduce_chains(acc0, acc1, acc2, acc3)
    }
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support.
pub(in crate::xxhash::long) unsafe fn accumulate(
    input: LongInput<'_>,
    secret: &Secret,
) -> [u64; 8] {
    let data = input.as_bytes();
    let secret = secret.as_bytes();
    let schedule = long_schedule(input);
    let mut acc = unsafe { initial() };
    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        if block + 2 <= schedule.full_blocks {
            unsafe { _mm_prefetch::<_MM_HINT_T0>(data.as_ptr().add((block + 2) * 1024).cast()) };
        }
        acc = unsafe { accumulate_block_chains(acc, data.as_ptr().add(offset), secret.as_ptr()) };
        let key = unsafe { secret.as_ptr().add(128) };
        unsafe { scramble_registers(&mut acc, key) };
    }

    let tail = unsafe { data.as_ptr().add(schedule.tail_offset) };
    let last = unsafe { data.as_ptr().add(schedule.last_offset) };
    // The final stripe uses a distinct secret and forms the fourth independent
    // update when the regular tail already contains at least three stripes.
    if schedule.tail_stripes >= 3 {
        acc = unsafe {
            accumulate_tail_chains(acc, tail, secret.as_ptr(), schedule.tail_stripes, last)
        };
    } else {
        for stripe in 0..schedule.tail_stripes {
            let input = unsafe { tail.add(stripe * 64) };
            let key = unsafe { secret.as_ptr().add(stripe * 8) };
            unsafe { accumulate_registers(&mut acc, input, key) };
        }
        let key = unsafe { secret.as_ptr().add(121) };
        unsafe { accumulate_registers(&mut acc, last, key) };
    }
    unsafe { finish(acc) }
}
