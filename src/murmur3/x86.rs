#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::{finish_x86_32, mix_x86_32_hash, read_u32_le};

const C1: u32 = 0xcc9e_2d51;
const C2: u32 = 0x1b87_3593;

#[target_feature(enable = "avx2")]
pub(super) unsafe fn murmur3_x86_32_avx2(key: &[u8], seed: u32) -> u32 {
    let block_end = key.len() & !3;
    let mut hash = seed;
    let mut offset = 0;
    let mut mixed = [0_u32; 32];

    while offset + 128 <= block_end {
        let first = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
        let second = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset + 32).cast()) };
        let third = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset + 64).cast()) };
        let fourth = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset + 96).cast()) };
        let first = premix_avx2(first);
        let second = premix_avx2(second);
        let third = premix_avx2(third);
        let fourth = premix_avx2(fourth);
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), first) };
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(8).cast(), second) };
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(16).cast(), third) };
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(24).cast(), fourth) };
        for &block in &mixed {
            hash = mix_x86_32_hash(hash, block);
        }
        offset += 128;
    }
    while offset + 32 <= block_end {
        let blocks = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
        let blocks = premix_avx2(blocks);
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), blocks) };
        for &block in &mixed[..8] {
            hash = mix_x86_32_hash(hash, block);
        }
        offset += 32;
    }
    while offset < block_end {
        let block = read_u32_le(key, offset)
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        hash = mix_x86_32_hash(hash, block);
        offset += 4;
    }
    finish_x86_32(key, hash, offset)
}

#[target_feature(enable = "avx2")]
#[inline]
fn premix_avx2(blocks: __m256i) -> __m256i {
    let blocks = _mm256_mullo_epi32(blocks, _mm256_set1_epi32(C1 as i32));
    let blocks = _mm256_or_si256(
        _mm256_slli_epi32::<15>(blocks),
        _mm256_srli_epi32::<17>(blocks),
    );
    _mm256_mullo_epi32(blocks, _mm256_set1_epi32(C2 as i32))
}

#[target_feature(enable = "sse4.1")]
pub(super) unsafe fn murmur3_x86_32_sse41(key: &[u8], seed: u32) -> u32 {
    let block_end = key.len() & !3;
    let mut hash = seed;
    let mut offset = 0;
    let mut mixed = [0_u32; 16];

    while offset + 64 <= block_end {
        let first = unsafe { _mm_loadu_si128(key.as_ptr().add(offset).cast()) };
        let second = unsafe { _mm_loadu_si128(key.as_ptr().add(offset + 16).cast()) };
        let third = unsafe { _mm_loadu_si128(key.as_ptr().add(offset + 32).cast()) };
        let fourth = unsafe { _mm_loadu_si128(key.as_ptr().add(offset + 48).cast()) };
        let first = premix_sse41(first);
        let second = premix_sse41(second);
        let third = premix_sse41(third);
        let fourth = premix_sse41(fourth);
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), first) };
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(4).cast(), second) };
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(8).cast(), third) };
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(12).cast(), fourth) };
        for &block in &mixed {
            hash = mix_x86_32_hash(hash, block);
        }
        offset += 64;
    }
    while offset + 16 <= block_end {
        let blocks = unsafe { _mm_loadu_si128(key.as_ptr().add(offset).cast()) };
        let blocks = premix_sse41(blocks);
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), blocks) };
        for &block in &mixed[..4] {
            hash = mix_x86_32_hash(hash, block);
        }
        offset += 16;
    }
    while offset < block_end {
        let block = read_u32_le(key, offset)
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        hash = mix_x86_32_hash(hash, block);
        offset += 4;
    }
    finish_x86_32(key, hash, offset)
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn premix_sse41(blocks: __m128i) -> __m128i {
    let blocks = _mm_mullo_epi32(blocks, _mm_set1_epi32(C1 as i32));
    let blocks = _mm_or_si128(_mm_slli_epi32::<15>(blocks), _mm_srli_epi32::<17>(blocks));
    _mm_mullo_epi32(blocks, _mm_set1_epi32(C2 as i32))
}
