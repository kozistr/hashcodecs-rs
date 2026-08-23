//! AVX2 secret initialization and long-input accumulation kernels.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::xxhash::long::{initial_accumulator, long_schedule};
use crate::xxhash::primitives::{P32_1, SECRET};

#[repr(align(64))]
#[derive(Clone, Copy)]
struct AlignedAccumulator([u64; 8]);

#[derive(Clone, Copy)]
struct Accumulator {
    low: __m256i,
    high: __m256i,
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support.
pub(super) unsafe fn init_secret(seed: u64) -> [u8; 192] {
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
    output
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
unsafe fn initial() -> Accumulator {
    let initial = AlignedAccumulator(initial_accumulator());
    Accumulator {
        low: unsafe { _mm256_load_si256(initial.0.as_ptr().cast()) },
        high: unsafe { _mm256_load_si256(initial.0.as_ptr().add(4).cast()) },
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_registers(acc: &mut Accumulator, data: *const u8, secret: *const u8) {
    acc.low = unsafe { accumulate_vector(acc.low, data, secret) };
    acc.high = unsafe { accumulate_vector(acc.high, data.add(32), secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn scramble_registers(acc: &mut Accumulator, secret: *const u8) {
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
unsafe fn finish(acc: Accumulator) -> [u64; 8] {
    let mut output = AlignedAccumulator([0; 8]);
    unsafe { store(acc, &mut output) };
    output.0
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_1024(data: &[u8], secret: &[u8]) -> [u64; 8] {
    let zero = _mm256_setzero_si256();
    // Split the 16 stripes across independent chains, then reduce once.
    let mut acc0 = unsafe { initial() };
    let mut acc1 = Accumulator {
        low: zero,
        high: zero,
    };
    let mut acc2 = acc1;
    let mut acc3 = acc1;

    macro_rules! stripe {
        ($acc:ident, $stripe:expr, $secret:expr) => {
            unsafe {
                accumulate_registers(
                    &mut $acc,
                    data.as_ptr().add($stripe * 64),
                    secret.as_ptr().add($secret),
                )
            }
        };
    }
    stripe!(acc0, 0, 0);
    stripe!(acc1, 1, 8);
    stripe!(acc2, 2, 16);
    stripe!(acc3, 3, 24);
    stripe!(acc0, 4, 32);
    stripe!(acc1, 5, 40);
    stripe!(acc2, 6, 48);
    stripe!(acc3, 7, 56);
    stripe!(acc0, 8, 64);
    stripe!(acc1, 9, 72);
    stripe!(acc2, 10, 80);
    stripe!(acc3, 11, 88);
    stripe!(acc0, 12, 96);
    stripe!(acc1, 13, 104);
    stripe!(acc2, 14, 112);
    stripe!(acc3, 15, 121);

    acc0.low = _mm256_add_epi64(_mm256_add_epi64(acc0.low, acc1.low), acc2.low);
    acc0.low = _mm256_add_epi64(acc0.low, acc3.low);
    acc0.high = _mm256_add_epi64(_mm256_add_epi64(acc0.high, acc1.high), acc2.high);
    acc0.high = _mm256_add_epi64(acc0.high, acc3.high);
    unsafe { finish(acc0) }
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support. `data` must be in XXH3 long
/// mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    if data.len() == 1024 {
        return unsafe { accumulate_1024(data, secret) };
    }
    let schedule = long_schedule(data.len());
    let initial = AlignedAccumulator(initial_accumulator());
    let mut low = unsafe { _mm256_load_si256(initial.0.as_ptr().cast()) };
    let mut high = unsafe { _mm256_load_si256(initial.0.as_ptr().add(4).cast()) };
    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        if block + 2 <= schedule.full_blocks {
            unsafe { _mm_prefetch::<_MM_HINT_T0>(data.as_ptr().add((block + 2) * 1024).cast()) };
        }
        for stripe in 0..16 {
            let input = unsafe { data.as_ptr().add(offset + stripe * 64) };
            let key = unsafe { secret.as_ptr().add(stripe * 8) };
            low = unsafe { accumulate_vector(low, input, key) };
            high = unsafe { accumulate_vector(high, input.add(32), key.add(32)) };
        }
        let key = unsafe { secret.as_ptr().add(128) };
        low = unsafe { scramble_vector(low, key) };
        high = unsafe { scramble_vector(high, key.add(32)) };
    }
    for stripe in 0..schedule.tail_stripes {
        let input = unsafe { data.as_ptr().add(schedule.tail_offset + stripe * 64) };
        let key = unsafe { secret.as_ptr().add(stripe * 8) };
        low = unsafe { accumulate_vector(low, input, key) };
        high = unsafe { accumulate_vector(high, input.add(32), key.add(32)) };
    }
    let input = unsafe { data.as_ptr().add(schedule.last_offset) };
    let key = unsafe { secret.as_ptr().add(121) };
    low = unsafe { accumulate_vector(low, input, key) };
    high = unsafe { accumulate_vector(high, input.add(32), key.add(32)) };

    let mut output = AlignedAccumulator([0; 8]);
    unsafe {
        _mm256_store_si256(output.0.as_mut_ptr().cast(), low);
        _mm256_store_si256(output.0.as_mut_ptr().add(4).cast(), high);
    }
    output.0
}

/// Interleaves four independent hashes so load and multiply latency from one
/// input can overlap useful work from the other three.
#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support. All inputs must have the same
/// long-mode length and `secret` must contain at least 192 bytes.
pub(in crate::xxhash) unsafe fn accumulate_batch4(
    data: [&[u8]; 4],
    secret: &[u8],
) -> [[u64; 8]; 4] {
    let mut acc0 = unsafe { initial() };
    let mut acc1 = acc0;
    let mut acc2 = acc0;
    let mut acc3 = acc0;
    let length = data[0].len();
    let schedule = long_schedule(length);

    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        if block + 2 <= schedule.full_blocks {
            let prefetch_offset = (block + 2) * 1024;
            unsafe {
                _mm_prefetch::<_MM_HINT_T0>(data[0].as_ptr().add(prefetch_offset).cast());
                _mm_prefetch::<_MM_HINT_T0>(data[1].as_ptr().add(prefetch_offset).cast());
                _mm_prefetch::<_MM_HINT_T0>(data[2].as_ptr().add(prefetch_offset).cast());
                _mm_prefetch::<_MM_HINT_T0>(data[3].as_ptr().add(prefetch_offset).cast());
            }
        }
        for stripe in 0..16 {
            let input_offset = offset + stripe * 64;
            let key = unsafe { secret.as_ptr().add(stripe * 8) };
            unsafe {
                accumulate_registers(&mut acc0, data[0].as_ptr().add(input_offset), key);
                accumulate_registers(&mut acc1, data[1].as_ptr().add(input_offset), key);
                accumulate_registers(&mut acc2, data[2].as_ptr().add(input_offset), key);
                accumulate_registers(&mut acc3, data[3].as_ptr().add(input_offset), key);
            }
        }
        let key = unsafe { secret.as_ptr().add(128) };
        unsafe {
            scramble_registers(&mut acc0, key);
            scramble_registers(&mut acc1, key);
            scramble_registers(&mut acc2, key);
            scramble_registers(&mut acc3, key);
        }
    }

    for stripe in 0..schedule.tail_stripes {
        let input_offset = schedule.tail_offset + stripe * 64;
        let key = unsafe { secret.as_ptr().add(stripe * 8) };
        unsafe {
            accumulate_registers(&mut acc0, data[0].as_ptr().add(input_offset), key);
            accumulate_registers(&mut acc1, data[1].as_ptr().add(input_offset), key);
            accumulate_registers(&mut acc2, data[2].as_ptr().add(input_offset), key);
            accumulate_registers(&mut acc3, data[3].as_ptr().add(input_offset), key);
        }
    }
    let input_offset = schedule.last_offset;
    let key = unsafe { secret.as_ptr().add(121) };
    unsafe {
        accumulate_registers(&mut acc0, data[0].as_ptr().add(input_offset), key);
        accumulate_registers(&mut acc1, data[1].as_ptr().add(input_offset), key);
        accumulate_registers(&mut acc2, data[2].as_ptr().add(input_offset), key);
        accumulate_registers(&mut acc3, data[3].as_ptr().add(input_offset), key);
    }
    unsafe { [finish(acc0), finish(acc1), finish(acc2), finish(acc3)] }
}
