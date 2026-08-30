#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::{X86_32_C1, X86_32_C2, mix_x86_32_hash};
use crate::murmur3::block_buffer::FullBlocks;
use crate::murmur3::dispatch::Backend;
use crate::murmur3::primitives::read_u32_le;

#[target_feature(enable = "avx2")]
unsafe fn mix_x86_32_body_avx2(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    let key = blocks.as_bytes();
    let mut value = *hash;
    let mut offset = 0;
    let mut mixed = [0_u32; 32];

    while offset + 128 <= key.len() {
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
            value = mix_x86_32_hash(value, block);
        }
        offset += 128;
    }
    while offset + 32 <= key.len() {
        let blocks = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
        let blocks = premix_avx2(blocks);
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), blocks) };
        for &block in &mixed[..8] {
            value = mix_x86_32_hash(value, block);
        }
        offset += 32;
    }
    while offset < key.len() {
        let block = read_u32_le(key, offset)
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
        value = mix_x86_32_hash(value, block);
        offset += 4;
    }
    *hash = value;
}

#[target_feature(enable = "avx2")]
#[inline]
fn premix_avx2(blocks: __m256i) -> __m256i {
    let blocks = _mm256_mullo_epi32(blocks, _mm256_set1_epi32(X86_32_C1 as i32));
    let blocks = _mm256_or_si256(
        _mm256_slli_epi32::<15>(blocks),
        _mm256_srli_epi32::<17>(blocks),
    );
    _mm256_mullo_epi32(blocks, _mm256_set1_epi32(X86_32_C2 as i32))
}

#[target_feature(enable = "sse4.1")]
unsafe fn mix_x86_32_body_sse41(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    let key = blocks.as_bytes();
    let mut value = *hash;
    let mut offset = 0;
    let mut mixed = [0_u32; 16];

    while offset + 64 <= key.len() {
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
            value = mix_x86_32_hash(value, block);
        }
        offset += 64;
    }
    while offset + 16 <= key.len() {
        let blocks = unsafe { _mm_loadu_si128(key.as_ptr().add(offset).cast()) };
        let blocks = premix_sse41(blocks);
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), blocks) };
        for &block in &mixed[..4] {
            value = mix_x86_32_hash(value, block);
        }
        offset += 16;
    }
    while offset < key.len() {
        let block = read_u32_le(key, offset)
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
        value = mix_x86_32_hash(value, block);
        offset += 4;
    }
    *hash = value;
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn premix_sse41(blocks: __m128i) -> __m128i {
    let blocks = _mm_mullo_epi32(blocks, _mm_set1_epi32(X86_32_C1 as i32));
    let blocks = _mm_or_si128(_mm_slli_epi32::<15>(blocks), _mm_srli_epi32::<17>(blocks));
    _mm_mullo_epi32(blocks, _mm_set1_epi32(X86_32_C2 as i32))
}

#[inline(always)]
/// # Safety
/// The current CPU must support the selected backend.
pub(in crate::murmur3) unsafe fn try_mix_x86_32_body(
    blocks: FullBlocks<'_, 4>,
    hash: &mut u32,
    backend: Backend,
) -> bool {
    match backend {
        Backend::Avx2 => {
            unsafe { mix_x86_32_body_avx2(blocks, hash) };
            true
        }
        Backend::Sse41 => {
            unsafe { mix_x86_32_body_sse41(blocks, hash) };
            true
        }
        Backend::Scalar => false,
    }
}
