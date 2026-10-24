#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

use super::{X86_32_C1, X86_32_C2, mix_body_scalar, mix_hash};
use crate::murmur3::block_buffer::FullBlocks;
use crate::murmur3::dispatch::Backend;
use crate::murmur3::primitives::read_u32_le;

#[target_feature(enable = "avx2")]
fn mix_body_avx2(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    let input = blocks.as_bytes();
    let mut value = *hash;
    let mut offset = 0;
    let mut mixed = [0_u32; 32];

    while offset + 128 <= input.len() {
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let first = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let second = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset + 32).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let third = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset + 64).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let fourth = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset + 96).cast()) };
        let first = premix_avx2(first);
        let second = premix_avx2(second);
        let third = premix_avx2(third);
        let fourth = premix_avx2(fourth);
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), first) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(8).cast(), second) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(16).cast(), third) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(24).cast(), fourth) };

        for &block in &mixed {
            value = mix_hash(value, block);
        }

        offset += 128;
    }

    while offset + 32 <= input.len() {
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let blocks = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset).cast()) };
        let blocks = premix_avx2(blocks);
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), blocks) };

        for &block in &mixed[..8] {
            value = mix_hash(value, block);
        }

        offset += 32;
    }

    while offset < input.len() {
        let block = read_u32_le(input, offset)
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
        value = mix_hash(value, block);
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
fn mix_body_sse41(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    let input = blocks.as_bytes();
    let mut value = *hash;
    let mut offset = 0;
    let mut mixed = [0_u32; 16];

    while offset + 64 <= input.len() {
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let first = unsafe { _mm_loadu_si128(input.as_ptr().add(offset).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let second = unsafe { _mm_loadu_si128(input.as_ptr().add(offset + 16).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let third = unsafe { _mm_loadu_si128(input.as_ptr().add(offset + 32).cast()) };
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let fourth = unsafe { _mm_loadu_si128(input.as_ptr().add(offset + 48).cast()) };
        let first = premix_sse41(first);
        let second = premix_sse41(second);
        let third = premix_sse41(third);
        let fourth = premix_sse41(fourth);
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), first) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(4).cast(), second) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(8).cast(), third) };
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(12).cast(), fourth) };

        for &block in &mixed {
            value = mix_hash(value, block);
        }

        offset += 64;
    }

    while offset + 16 <= input.len() {
        // SAFETY: The loop guard leaves a complete vector inside FullBlocks; unaligned loads accept
        // the input slice's alignment.
        let blocks = unsafe { _mm_loadu_si128(input.as_ptr().add(offset).cast()) };
        let blocks = premix_sse41(blocks);
        // SAFETY: The fixed scratch array contains the full vector store, and every element is
        // initialized before scalar mixing reads it.
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), blocks) };

        for &block in &mixed[..4] {
            value = mix_hash(value, block);
        }

        offset += 16;
    }

    while offset < input.len() {
        let block = read_u32_le(input, offset)
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
        value = mix_hash(value, block);
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
pub(in crate::murmur3) unsafe fn mix_body(
    blocks: FullBlocks<'_, 4>,
    hash: &mut u32,
    backend: Backend,
) {
    match backend {
        Backend::Avx2 => {
            // SAFETY: Dispatch checks the required CPU features, and FullBlocks guarantees complete
            // algorithm blocks for this kernel.
            unsafe { mix_body_avx2(blocks, hash) };
        }
        Backend::Sse41 => {
            // SAFETY: Dispatch checks the required CPU features, and FullBlocks guarantees complete
            // algorithm blocks for this kernel.
            unsafe { mix_body_sse41(blocks, hash) };
        }
        Backend::Scalar => mix_body_scalar(blocks, hash),
    }
}
