#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::{X64_128_C1, X64_128_C2, mix_x64_128_hashes};
use crate::murmur3::block_buffer::FullBlocks;
use crate::murmur3::dispatch::Backend;
use crate::murmur3::primitives::read_u64_le;

macro_rules! define_x64_128_avx2_kernel {
    ($name:ident, $features:literal) => {
        #[target_feature(enable = $features)]
        unsafe fn $name(blocks: FullBlocks<'_, 16>, hashes: [u64; 2]) -> [u64; 2] {
            let key = blocks.as_bytes();
            let c1 = _mm256_setr_epi64x(
                X64_128_C1 as i64,
                X64_128_C2 as i64,
                X64_128_C1 as i64,
                X64_128_C2 as i64,
            );
            let c2 = _mm256_setr_epi64x(
                X64_128_C2 as i64,
                X64_128_C1 as i64,
                X64_128_C2 as i64,
                X64_128_C1 as i64,
            );
            let rotate_left = _mm256_setr_epi64x(31, 33, 31, 33);
            let rotate_right = _mm256_setr_epi64x(33, 31, 33, 31);
            let mut hash1 = hashes[0];
            let mut hash2 = hashes[1];
            let mut mixed = [0_u64; 16];
            let mut offset = 0;

            while offset + 128 <= key.len() {
                for vector in 0..4 {
                    let input = unsafe {
                        _mm256_loadu_si256(key.as_ptr().add(offset + vector * 32).cast())
                    };
                    let input = premix_x64_128_avx2(input, c1, c2, rotate_left, rotate_right);
                    unsafe {
                        _mm256_storeu_si256(mixed.as_mut_ptr().add(vector * 4).cast(), input)
                    };
                }
                mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed);
                offset += 128;
            }
            while offset + 32 <= key.len() {
                let input = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
                let input = premix_x64_128_avx2(input, c1, c2, rotate_left, rotate_right);
                unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), input) };
                mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed[..4]);
                offset += 32;
            }
            if offset < key.len() {
                let value1 = read_u64_le(key, offset);
                let value2 = read_u64_le(key, offset + 8);
                let block1 = value1
                    .wrapping_mul(X64_128_C1)
                    .rotate_left(31)
                    .wrapping_mul(X64_128_C2);
                let block2 = value2
                    .wrapping_mul(X64_128_C2)
                    .rotate_left(33)
                    .wrapping_mul(X64_128_C1);
                mix_x64_128_hashes(&mut hash1, &mut hash2, block1, block2);
            }
            [hash1, hash2]
        }
    };
}

define_x64_128_avx2_kernel!(mix_x64_128_body_avx2, "avx2");
define_x64_128_avx2_kernel!(mix_x64_128_body_avx2_bmi2, "avx2,bmi2");

#[target_feature(enable = "avx2")]
#[inline]
fn premix_x64_128_avx2(
    blocks: __m256i,
    c1: __m256i,
    c2: __m256i,
    rotate_left: __m256i,
    rotate_right: __m256i,
) -> __m256i {
    let blocks = mullo_epi64_avx2(blocks, c1);
    let blocks = _mm256_or_si256(
        _mm256_sllv_epi64(blocks, rotate_left),
        _mm256_srlv_epi64(blocks, rotate_right),
    );
    mullo_epi64_avx2(blocks, c2)
}

#[target_feature(enable = "avx2")]
#[inline]
fn mullo_epi64_avx2(left: __m256i, right: __m256i) -> __m256i {
    let low = _mm256_mul_epu32(left, right);
    let left_high = _mm256_srli_epi64::<32>(left);
    let right_high = _mm256_srli_epi64::<32>(right);
    let cross = _mm256_add_epi64(
        _mm256_mul_epu32(left_high, right),
        _mm256_mul_epu32(left, right_high),
    );
    _mm256_add_epi64(low, _mm256_slli_epi64::<32>(cross))
}

#[target_feature(enable = "sse4.1")]
unsafe fn mix_x64_128_body_sse41(blocks: FullBlocks<'_, 16>, hashes: [u64; 2]) -> [u64; 2] {
    let key = blocks.as_bytes();
    let c1 = _mm_set_epi64x(X64_128_C2 as i64, X64_128_C1 as i64);
    let c2 = _mm_set_epi64x(X64_128_C1 as i64, X64_128_C2 as i64);
    let mut hash1 = hashes[0];
    let mut hash2 = hashes[1];
    let mut mixed = [0_u64; 8];
    let mut offset = 0;

    while offset + 64 <= key.len() {
        for vector in 0..4 {
            let input = unsafe { _mm_loadu_si128(key.as_ptr().add(offset + vector * 16).cast()) };
            let input = premix_x64_128_sse41(input, c1, c2);
            unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(vector * 2).cast(), input) };
        }
        mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed);
        offset += 64;
    }
    while offset < key.len() {
        let input = unsafe { _mm_loadu_si128(key.as_ptr().add(offset).cast()) };
        let input = premix_x64_128_sse41(input, c1, c2);
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), input) };
        mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed[..2]);
        offset += 16;
    }
    [hash1, hash2]
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn premix_x64_128_sse41(blocks: __m128i, c1: __m128i, c2: __m128i) -> __m128i {
    let blocks = mullo_epi64_sse41(blocks, c1);
    let rotate31 = _mm_or_si128(_mm_slli_epi64::<31>(blocks), _mm_srli_epi64::<33>(blocks));
    let rotate33 = _mm_or_si128(_mm_slli_epi64::<33>(blocks), _mm_srli_epi64::<31>(blocks));
    let blocks = _mm_blend_epi16::<0xf0>(rotate31, rotate33);
    mullo_epi64_sse41(blocks, c2)
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn mullo_epi64_sse41(left: __m128i, right: __m128i) -> __m128i {
    let low = _mm_mul_epu32(left, right);
    let left_high = _mm_srli_epi64::<32>(left);
    let right_high = _mm_srli_epi64::<32>(right);
    let cross = _mm_add_epi64(
        _mm_mul_epu32(left_high, right),
        _mm_mul_epu32(left, right_high),
    );
    _mm_add_epi64(low, _mm_slli_epi64::<32>(cross))
}

#[inline(always)]
fn mix_x64_128_blocks(hash1: &mut u64, hash2: &mut u64, blocks: &[u64]) {
    for block in blocks.as_chunks::<2>().0 {
        mix_x64_128_hashes(hash1, hash2, block[0], block[1]);
    }
}

#[inline(always)]
/// # Safety
/// The selected backend and BMI2 flag must describe features available on the
/// current CPU.
pub(in crate::murmur3) unsafe fn try_mix_x64_128_body(
    blocks: FullBlocks<'_, 16>,
    hashes: &mut [u64; 2],
    backend: Backend,
    bmi2: bool,
) -> bool {
    match backend {
        Backend::Avx2 => {
            *hashes = if bmi2 {
                unsafe { mix_x64_128_body_avx2_bmi2(blocks, *hashes) }
            } else {
                unsafe { mix_x64_128_body_avx2(blocks, *hashes) }
            };
            true
        }
        Backend::Sse41 => {
            *hashes = unsafe { mix_x64_128_body_sse41(blocks, *hashes) };
            true
        }
        Backend::Scalar => false,
    }
}
