//! AVX2 secret initialization and long-input accumulator.

#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

use crate::xxhash::long_inputs::{
    LongInput, Secret, build_long_input_schedule, finalize_long_128, initial_accumulator,
};
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
pub(in crate::xxhash::long_inputs) fn init_secret(seed: u64) -> Secret {
    let negative = 0_u64.wrapping_sub(seed);
    let delta = _mm256_set_epi64x(negative as i64, seed as i64, negative as i64, seed as i64);
    let mut output = [0_u8; 192];

    for vector in 0..6 {
        let offset = vector * 32;
        // SAFETY: The 192-byte secret and output contain every 32-byte vector selected by the
        // bounded offset loop; AVX2 is enabled.
        let source = unsafe { _mm256_loadu_si256(SECRET.as_ptr().add(offset).cast()) };

        // SAFETY: The 192-byte secret and output contain every 32-byte vector selected by the
        // bounded offset loop; AVX2 is enabled.
        unsafe {
            _mm256_storeu_si256(
                output.as_mut_ptr().add(offset).cast(),
                _mm256_add_epi64(source, delta),
            )
        };
    }

    Secret(output)
}

/// # Safety
/// AVX2 must be available. Each data or secret pointer must cover 32 readable bytes.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_vector(acc: __m256i, data: *const u8, secret: *const u8) -> __m256i {
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    let input = unsafe { _mm256_loadu_si256(data.cast()) };
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    let secret_values = unsafe { _mm256_loadu_si256(secret.cast()) };
    let mixed = _mm256_xor_si256(input, secret_values);
    let product = _mm256_mul_epu32(mixed, _mm256_srli_epi64::<32>(mixed));
    let swapped = _mm256_shuffle_epi32::<0x4e>(input);

    _mm256_add_epi64(_mm256_add_epi64(acc, swapped), product)
}

/// # Safety
/// AVX2 must be available. Each data or secret pointer must cover 32 readable bytes.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn scramble_vector(acc: __m256i, secret: *const u8) -> __m256i {
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    let secret_values = unsafe { _mm256_loadu_si256(secret.cast()) };
    let mixed = _mm256_xor_si256(
        _mm256_xor_si256(acc, _mm256_srli_epi64::<47>(acc)),
        secret_values,
    );
    let prime = _mm256_set1_epi32(P32_1 as i32);
    let low = _mm256_mul_epu32(mixed, prime);
    let high = _mm256_slli_epi64::<32>(_mm256_mul_epu32(_mm256_srli_epi64::<32>(mixed), prime));

    _mm256_add_epi64(low, high)
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) fn initial() -> Accumulator {
    let initial = AlignedAccumulator(initial_accumulator());

    Accumulator {
        // SAFETY: The accumulator owns eight u64 lanes; bounded lane indices and its declared
        // alignment cover the vector access.
        low: unsafe { _mm256_load_si256(initial.0.as_ptr().cast()) },
        // SAFETY: The accumulator owns eight u64 lanes; bounded lane indices and its declared
        // alignment cover the vector access.
        high: unsafe { _mm256_load_si256(initial.0.as_ptr().add(4).cast()) },
    }
}

/// # Safety
/// The kernel CPU features must be available. Each data or secret stripe must cover 64 readable
/// bytes at the supplied offset, and accumulator storage must remain exclusively borrowed.
#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn accumulate_registers(
    acc: &mut Accumulator,
    data: *const u8,
    secret: *const u8,
) {
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    acc.low = unsafe { accumulate_vector(acc.low, data, secret) };
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    acc.high = unsafe { accumulate_vector(acc.high, data.add(32), secret.add(32)) };
}

/// # Safety
/// The kernel CPU features must be available. Each data or secret stripe must cover 64 readable
/// bytes at the supplied offset, and accumulator storage must remain exclusively borrowed.
#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn scramble_registers(acc: &mut Accumulator, secret: *const u8) {
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    acc.low = unsafe { scramble_vector(acc.low, secret) };
    // SAFETY: The caller supplies a complete data/secret stripe, bounded lane offsets stay within
    // it, and dispatch enables the kernel features.
    acc.high = unsafe { scramble_vector(acc.high, secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
fn store(acc: Accumulator, output: &mut AlignedAccumulator) {
    // SAFETY: The accumulator owns eight u64 lanes; bounded lane indices and its declared alignment
    // cover the vector access.
    unsafe {
        _mm256_store_si256(output.0.as_mut_ptr().cast(), acc.low);
        _mm256_store_si256(output.0.as_mut_ptr().add(4).cast(), acc.high);
    }
}

#[inline]
#[target_feature(enable = "avx2")]
pub(super) fn finish(acc: Accumulator) -> [u64; 8] {
    let mut output = AlignedAccumulator([0; 8]);

    store(acc, &mut output);

    output.0
}

#[inline]
#[target_feature(enable = "avx2")]
fn reduce_chains(
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

/// # Safety
/// AVX2 must be available. The caller must provide four complete 64-byte stripes beginning at
/// first_stripe and every corresponding 64-byte secret window.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_group(
    [acc0, acc1, acc2, acc3]: [&mut Accumulator; 4],
    data: *const u8,
    secret: *const u8,
    first_stripe: usize,
) {
    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    unsafe {
        accumulate_registers(
            acc0,
            data.add(first_stripe * 64),
            secret.add(first_stripe * 8),
        );
        accumulate_registers(
            acc1,
            data.add((first_stripe + 1) * 64),
            secret.add((first_stripe + 1) * 8),
        );
        accumulate_registers(
            acc2,
            data.add((first_stripe + 2) * 64),
            secret.add((first_stripe + 2) * 8),
        );
        accumulate_registers(
            acc3,
            data.add((first_stripe + 3) * 64),
            secret.add((first_stripe + 3) * 8),
        );
    }
}

/// # Safety
/// AVX2 must be available. Data must cover a complete 1024-byte block and the secret must cover 192
/// readable bytes.
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
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe {
            accumulate_group(
                [&mut acc0, &mut acc1, &mut acc2, &mut acc3],
                data,
                secret,
                stripe,
            )
        };
    }

    reduce_chains(acc0, acc1, acc2, acc3)
}

/// # Safety
/// AVX2 must be available. Data must cover tail_stripes complete 64-byte stripes, last must cover
/// the final 64 bytes, and the secret must cover 192 readable bytes.
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
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
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
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe { accumulate_registers(&mut acc0, data.add(stripe * 64), secret.add(stripe * 8)) };
    }

    if remaining >= 2 {
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe {
            accumulate_registers(
                &mut acc1,
                data.add((stripe + 1) * 64),
                secret.add((stripe + 1) * 8),
            )
        };
    }

    if remaining == 3 {
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe {
            accumulate_registers(
                &mut acc2,
                data.add((stripe + 2) * 64),
                secret.add((stripe + 2) * 8),
            )
        };
    }

    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    let last_secret = unsafe { secret.add(121) };

    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    unsafe { accumulate_registers(&mut acc3, last, last_secret) };

    reduce_chains(acc0, acc1, acc2, acc3)
}

#[target_feature(enable = "avx2")]
pub(in crate::xxhash::long_inputs) fn accumulate(
    input: LongInput<'_>,
    secret: &Secret,
) -> [u64; 8] {
    let data = input.as_bytes();
    let secret = secret.as_bytes();
    let schedule = build_long_input_schedule(input);
    let mut acc = initial();

    for block in 0..schedule.full_blocks() {
        let offset = block * 1024;

        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        acc = unsafe { accumulate_block_chains(acc, data.as_ptr().add(offset), secret.as_ptr()) };
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        let secret_ptr = unsafe { secret.as_ptr().add(128) };

        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe { scramble_registers(&mut acc, secret_ptr) };
    }

    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    let tail = unsafe { data.as_ptr().add(schedule.tail_offset()) };
    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    let last = unsafe { data.as_ptr().add(schedule.last_offset()) };

    // The final stripe uses a distinct secret and forms the fourth independent
    // update when the regular tail already contains at least three stripes.
    if schedule.tail_stripes() >= 3 {
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        acc = unsafe {
            accumulate_tail_chains(acc, tail, secret.as_ptr(), schedule.tail_stripes(), last)
        };
    } else {
        for stripe in 0..schedule.tail_stripes() {
            // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
            // 192-byte secret, including the final overlapping stripe.
            let input = unsafe { tail.add(stripe * 64) };
            // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
            // 192-byte secret, including the final overlapping stripe.
            let secret_ptr = unsafe { secret.as_ptr().add(stripe * 8) };
            // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
            // 192-byte secret, including the final overlapping stripe.
            unsafe { accumulate_registers(&mut acc, input, secret_ptr) };
        }

        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        let secret_ptr = unsafe { secret.as_ptr().add(121) };
        // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the
        // 192-byte secret, including the final overlapping stripe.
        unsafe { accumulate_registers(&mut acc, last, secret_ptr) };
    }

    finish(acc)
}

#[target_feature(enable = "avx2")]
pub(in crate::xxhash::long_inputs) fn hash_128_medium(
    input: LongInput<'_>,
    secret: &Secret,
) -> [u64; 2] {
    debug_assert!(input.len() <= 1024);

    // Two chains keep medium-input accumulation and finalization in one frame
    // without the register pressure of the four-chain bulk kernel.
    let data = input.as_bytes();
    let mut acc0 = initial();
    let zero = _mm256_setzero_si256();
    let mut acc1 = Accumulator {
        low: zero,
        high: zero,
    };
    let mut stripe = 0;

    // SAFETY: The long-input schedule keeps all 64-byte stripes within the input and the 192-byte
    // secret, including the final overlapping stripe.
    unsafe {
        while stripe + 2 <= input.regular_stripes() {
            accumulate_registers(
                &mut acc0,
                data.as_ptr().add(stripe * 64),
                secret.as_bytes().as_ptr().add(stripe * 8),
            );
            accumulate_registers(
                &mut acc1,
                data.as_ptr().add((stripe + 1) * 64),
                secret.as_bytes().as_ptr().add((stripe + 1) * 8),
            );
            stripe += 2;
        }

        if stripe < input.regular_stripes() {
            accumulate_registers(
                &mut acc0,
                data.as_ptr().add(stripe * 64),
                secret.as_bytes().as_ptr().add(stripe * 8),
            );
        }

        accumulate_registers(
            &mut acc1,
            data.as_ptr().add(data.len() - 64),
            secret.as_bytes().as_ptr().add(121),
        );
    }

    acc0.low = _mm256_add_epi64(acc0.low, acc1.low);
    acc0.high = _mm256_add_epi64(acc0.high, acc1.high);
    finalize_long_128(input.len(), secret, finish(acc0))
}
