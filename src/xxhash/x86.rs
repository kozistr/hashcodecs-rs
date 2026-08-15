#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::sync::OnceLock;

use super::{P32_1, SECRET, initial_accumulator, long_schedule};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Backend {
    Scalar,
    Ssse3,
    Sse41,
    Avx2,
    Avx512,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

#[repr(align(64))]
#[derive(Clone, Copy)]
struct AlignedAccumulator([u64; 8]);

#[derive(Clone, Copy)]
struct Avx2Accumulator {
    low: __m256i,
    high: __m256i,
}

#[inline]
pub(super) fn backend() -> Backend {
    *BACKEND.get_or_init(|| {
        if std::is_x86_feature_detected!("avx512f") {
            Backend::Avx512
        } else if std::is_x86_feature_detected!("avx2") {
            Backend::Avx2
        } else if std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("ssse3")
        {
            Backend::Sse41
        } else if std::is_x86_feature_detected!("ssse3") {
            Backend::Ssse3
        } else {
            Backend::Scalar
        }
    })
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support.
pub(super) unsafe fn init_secret_avx2(seed: u64) -> [u8; 192] {
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
unsafe fn accumulate_vector_avx2(acc: __m256i, data: *const u8, secret: *const u8) -> __m256i {
    let input = unsafe { _mm256_loadu_si256(data.cast()) };
    let key = unsafe { _mm256_loadu_si256(secret.cast()) };
    let keyed = _mm256_xor_si256(input, key);
    let product = _mm256_mul_epu32(keyed, _mm256_srli_epi64::<32>(keyed));
    let swapped = _mm256_shuffle_epi32::<0x4e>(input);
    _mm256_add_epi64(_mm256_add_epi64(acc, swapped), product)
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn scramble_vector_avx2(acc: __m256i, secret: *const u8) -> __m256i {
    let key = unsafe { _mm256_loadu_si256(secret.cast()) };
    let mixed = _mm256_xor_si256(_mm256_xor_si256(acc, _mm256_srli_epi64::<47>(acc)), key);
    let prime = _mm256_set1_epi32(P32_1 as i32);
    let low = _mm256_mul_epu32(mixed, prime);
    let high = _mm256_slli_epi64::<32>(_mm256_mul_epu32(_mm256_srli_epi64::<32>(mixed), prime));
    _mm256_add_epi64(low, high)
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn initial_avx2() -> Avx2Accumulator {
    let initial = AlignedAccumulator(initial_accumulator());
    Avx2Accumulator {
        low: unsafe { _mm256_load_si256(initial.0.as_ptr().cast()) },
        high: unsafe { _mm256_load_si256(initial.0.as_ptr().add(4).cast()) },
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn accumulate_registers_avx2(acc: &mut Avx2Accumulator, data: *const u8, secret: *const u8) {
    acc.low = unsafe { accumulate_vector_avx2(acc.low, data, secret) };
    acc.high = unsafe { accumulate_vector_avx2(acc.high, data.add(32), secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn scramble_registers_avx2(acc: &mut Avx2Accumulator, secret: *const u8) {
    acc.low = unsafe { scramble_vector_avx2(acc.low, secret) };
    acc.high = unsafe { scramble_vector_avx2(acc.high, secret.add(32)) };
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_avx2(acc: Avx2Accumulator, output: &mut AlignedAccumulator) {
    unsafe {
        _mm256_store_si256(output.0.as_mut_ptr().cast(), acc.low);
        _mm256_store_si256(output.0.as_mut_ptr().add(4).cast(), acc.high);
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn finish_avx2(acc: Avx2Accumulator) -> [u64; 8] {
    let mut output = AlignedAccumulator([0; 8]);
    unsafe { store_avx2(acc, &mut output) };
    output.0
}

#[target_feature(enable = "avx2")]
/// # Safety
/// The caller must have detected AVX2 support. `data` must be in XXH3 long
/// mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn long_accumulate_avx2(data: &[u8], secret: &[u8]) -> [u64; 8] {
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
            low = unsafe { accumulate_vector_avx2(low, input, key) };
            high = unsafe { accumulate_vector_avx2(high, input.add(32), key.add(32)) };
        }
        let key = unsafe { secret.as_ptr().add(128) };
        low = unsafe { scramble_vector_avx2(low, key) };
        high = unsafe { scramble_vector_avx2(high, key.add(32)) };
    }
    for stripe in 0..schedule.tail_stripes {
        let input = unsafe { data.as_ptr().add(schedule.tail_offset + stripe * 64) };
        let key = unsafe { secret.as_ptr().add(stripe * 8) };
        low = unsafe { accumulate_vector_avx2(low, input, key) };
        high = unsafe { accumulate_vector_avx2(high, input.add(32), key.add(32)) };
    }
    let input = unsafe { data.as_ptr().add(schedule.last_offset) };
    let key = unsafe { secret.as_ptr().add(121) };
    low = unsafe { accumulate_vector_avx2(low, input, key) };
    high = unsafe { accumulate_vector_avx2(high, input.add(32), key.add(32)) };

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
pub(super) unsafe fn long_accumulate_batch4_avx2(data: [&[u8]; 4], secret: &[u8]) -> [[u64; 8]; 4] {
    let mut acc0 = unsafe { initial_avx2() };
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
                accumulate_registers_avx2(&mut acc0, data[0].as_ptr().add(input_offset), key);
                accumulate_registers_avx2(&mut acc1, data[1].as_ptr().add(input_offset), key);
                accumulate_registers_avx2(&mut acc2, data[2].as_ptr().add(input_offset), key);
                accumulate_registers_avx2(&mut acc3, data[3].as_ptr().add(input_offset), key);
            }
        }
        let key = unsafe { secret.as_ptr().add(128) };
        unsafe {
            scramble_registers_avx2(&mut acc0, key);
            scramble_registers_avx2(&mut acc1, key);
            scramble_registers_avx2(&mut acc2, key);
            scramble_registers_avx2(&mut acc3, key);
        }
    }

    for stripe in 0..schedule.tail_stripes {
        let input_offset = schedule.tail_offset + stripe * 64;
        let key = unsafe { secret.as_ptr().add(stripe * 8) };
        unsafe {
            accumulate_registers_avx2(&mut acc0, data[0].as_ptr().add(input_offset), key);
            accumulate_registers_avx2(&mut acc1, data[1].as_ptr().add(input_offset), key);
            accumulate_registers_avx2(&mut acc2, data[2].as_ptr().add(input_offset), key);
            accumulate_registers_avx2(&mut acc3, data[3].as_ptr().add(input_offset), key);
        }
    }
    let input_offset = schedule.last_offset;
    let key = unsafe { secret.as_ptr().add(121) };
    unsafe {
        accumulate_registers_avx2(&mut acc0, data[0].as_ptr().add(input_offset), key);
        accumulate_registers_avx2(&mut acc1, data[1].as_ptr().add(input_offset), key);
        accumulate_registers_avx2(&mut acc2, data[2].as_ptr().add(input_offset), key);
        accumulate_registers_avx2(&mut acc3, data[3].as_ptr().add(input_offset), key);
    }
    unsafe {
        [
            finish_avx2(acc0),
            finish_avx2(acc1),
            finish_avx2(acc2),
            finish_avx2(acc3),
        ]
    }
}

#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn accumulate_sse(acc: &mut AlignedAccumulator, data: *const u8, secret: *const u8) {
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
unsafe fn scramble_sse(acc: &mut AlignedAccumulator, secret: *const u8) {
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
/// The caller must have detected SSSE3 support. `data` must be in XXH3 long
/// mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn long_accumulate_ssse3(data: &[u8], secret: &[u8]) -> [u64; 8] {
    let schedule = long_schedule(data.len());
    let mut acc = AlignedAccumulator(initial_accumulator());
    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        for stripe in 0..16 {
            unsafe {
                accumulate_sse(
                    &mut acc,
                    data.as_ptr().add(offset + stripe * 64),
                    secret.as_ptr().add(stripe * 8),
                )
            };
        }
        unsafe { scramble_sse(&mut acc, secret.as_ptr().add(128)) };
    }
    for stripe in 0..schedule.tail_stripes {
        unsafe {
            accumulate_sse(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }
    unsafe {
        accumulate_sse(
            &mut acc,
            data.as_ptr().add(schedule.last_offset),
            secret.as_ptr().add(121),
        )
    };
    acc.0
}

#[target_feature(enable = "sse4.1,ssse3")]
/// # Safety
/// The caller must have detected SSE4.1 and SSSE3 support. `data` must be in
/// XXH3 long mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn long_accumulate_sse41(data: &[u8], secret: &[u8]) -> [u64; 8] {
    unsafe { long_accumulate_ssse3(data, secret) }
}

#[target_feature(enable = "avx512f")]
/// # Safety
/// The caller must have detected AVX-512F support. `data` must be in XXH3 long
/// mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn long_accumulate_avx512(data: &[u8], secret: &[u8]) -> [u64; 8] {
    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn accumulate(acc: &mut AlignedAccumulator, data: *const u8, secret: *const u8) {
        let input = unsafe { _mm512_loadu_si512(data.cast()) };
        let key = unsafe { _mm512_loadu_si512(secret.cast()) };
        let keyed = _mm512_xor_si512(input, key);
        let product = _mm512_mul_epu32(keyed, _mm512_srli_epi64::<32>(keyed));
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
        let key = unsafe { _mm512_loadu_si512(secret.cast()) };
        let mixed = _mm512_xor_si512(_mm512_xor_si512(value, _mm512_srli_epi64::<47>(value)), key);
        let prime = _mm512_set1_epi32(P32_1 as i32);
        let low = _mm512_mul_epu32(mixed, prime);
        let high = _mm512_slli_epi64::<32>(_mm512_mul_epu32(_mm512_srli_epi64::<32>(mixed), prime));
        unsafe { _mm512_store_si512(acc.0.as_mut_ptr().cast(), _mm512_add_epi64(low, high)) };
    }

    let schedule = long_schedule(data.len());
    let mut acc = AlignedAccumulator(initial_accumulator());
    for block in 0..schedule.full_blocks {
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
    for stripe in 0..schedule.tail_stripes {
        unsafe {
            accumulate(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }
    unsafe {
        accumulate(
            &mut acc,
            data.as_ptr().add(schedule.last_offset),
            secret.as_ptr().add(121),
        )
    };
    acc.0
}

#[cfg(test)]
pub(super) fn backend_supported(backend: Backend) -> bool {
    match backend {
        Backend::Scalar => true,
        Backend::Ssse3 => std::is_x86_feature_detected!("ssse3"),
        Backend::Sse41 => {
            std::is_x86_feature_detected!("sse4.1") && std::is_x86_feature_detected!("ssse3")
        }
        Backend::Avx2 => std::is_x86_feature_detected!("avx2"),
        Backend::Avx512 => std::is_x86_feature_detected!("avx512f"),
    }
}
