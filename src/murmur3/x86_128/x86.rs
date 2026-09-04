#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::{mem::MaybeUninit, slice};

use super::{X86_128_C1, X86_128_C2, mix_x86_128_body_scalar, mix_x86_128_hashes};
use crate::murmur3::block_buffer::FullBlocks;
use crate::murmur3::dispatch::Backend;

#[target_feature(enable = "avx2")]
unsafe fn mix_x86_128_body_avx2(blocks: FullBlocks<'_, 16>, hashes: &mut [u32; 4]) {
    let input = blocks.as_bytes();
    let c1 = _mm256_setr_epi32(
        X86_128_C1[0] as i32,
        X86_128_C1[1] as i32,
        X86_128_C1[2] as i32,
        X86_128_C1[3] as i32,
        X86_128_C1[0] as i32,
        X86_128_C1[1] as i32,
        X86_128_C1[2] as i32,
        X86_128_C1[3] as i32,
    );
    let c2 = _mm256_setr_epi32(
        X86_128_C2[0] as i32,
        X86_128_C2[1] as i32,
        X86_128_C2[2] as i32,
        X86_128_C2[3] as i32,
        X86_128_C2[0] as i32,
        X86_128_C2[1] as i32,
        X86_128_C2[2] as i32,
        X86_128_C2[3] as i32,
    );
    let rotate_left = _mm256_setr_epi32(15, 16, 17, 18, 15, 16, 17, 18);
    let rotate_right = _mm256_setr_epi32(17, 16, 15, 14, 17, 16, 15, 14);
    let mut mixed = MaybeUninit::<[u32; 64]>::uninit();
    let mixed = mixed.as_mut_ptr().cast::<u32>();
    let mut offset = 0;

    while offset + 256 <= input.len() {
        for vector in 0..8 {
            let values =
                unsafe { _mm256_loadu_si256(input.as_ptr().add(offset + vector * 32).cast()) };
            let values = premix_x86_128_avx2(values, c1, c2, rotate_left, rotate_right);
            unsafe { _mm256_storeu_si256(mixed.add(vector * 8).cast(), values) };
        }
        // All 64 words are initialized by the eight stores above.
        mix_x86_128_blocks(hashes, unsafe { slice::from_raw_parts(mixed, 64) });
        offset += 256;
    }
    while offset + 128 <= input.len() {
        for vector in 0..4 {
            let values =
                unsafe { _mm256_loadu_si256(input.as_ptr().add(offset + vector * 32).cast()) };
            let values = premix_x86_128_avx2(values, c1, c2, rotate_left, rotate_right);
            unsafe { _mm256_storeu_si256(mixed.add(vector * 8).cast(), values) };
        }
        // The first four vector stores initialize exactly 32 words.
        mix_x86_128_blocks(hashes, unsafe { slice::from_raw_parts(mixed, 32) });
        offset += 128;
    }
    while offset + 32 <= input.len() {
        let values = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset).cast()) };
        let values = premix_x86_128_avx2(values, c1, c2, rotate_left, rotate_right);
        unsafe { _mm256_storeu_si256(mixed.cast(), values) };
        // This store initializes the first eight words.
        mix_x86_128_blocks(hashes, unsafe { slice::from_raw_parts(mixed, 8) });
        offset += 32;
    }
    let remaining = FullBlocks::new(&input[offset..]).expect("SIMD leaves complete blocks");
    mix_x86_128_body_scalar(remaining, hashes);
}

#[target_feature(enable = "avx2")]
#[inline]
fn premix_x86_128_avx2(
    blocks: __m256i,
    c1: __m256i,
    c2: __m256i,
    rotate_left: __m256i,
    rotate_right: __m256i,
) -> __m256i {
    let blocks = _mm256_mullo_epi32(blocks, c1);
    let blocks = _mm256_or_si256(
        _mm256_sllv_epi32(blocks, rotate_left),
        _mm256_srlv_epi32(blocks, rotate_right),
    );
    _mm256_mullo_epi32(blocks, c2)
}

#[target_feature(enable = "sse4.1")]
unsafe fn mix_x86_128_body_sse41(blocks: FullBlocks<'_, 16>, hashes: &mut [u32; 4]) {
    let input = blocks.as_bytes();
    let mut mixed = MaybeUninit::<[u32; 64]>::uninit();
    let mixed = mixed.as_mut_ptr().cast::<u32>();
    let mut offset = 0;

    while offset + 256 <= input.len() {
        for group in 0..4 {
            unsafe {
                premix_x86_128_group_sse41(
                    input.as_ptr().add(offset + group * 64),
                    mixed.add(group * 16),
                )
            };
        }
        for group in 0..4 {
            // Each group helper initializes its corresponding 16-word range.
            mix_x86_128_transposed_group(hashes, unsafe {
                slice::from_raw_parts(mixed.add(group * 16), 16)
            });
        }
        offset += 256;
    }
    while offset + 64 <= input.len() {
        unsafe { premix_x86_128_group_sse41(input.as_ptr().add(offset), mixed) };
        // The group helper initializes the first 16 words.
        mix_x86_128_transposed_group(hashes, unsafe { slice::from_raw_parts(mixed, 16) });
        offset += 64;
    }
    let remaining = FullBlocks::new(&input[offset..]).expect("SIMD leaves complete blocks");
    mix_x86_128_body_scalar(remaining, hashes);
}

#[target_feature(enable = "sse4.1")]
#[inline]
unsafe fn premix_x86_128_group_sse41(input: *const u8, output: *mut u32) {
    let row0 = unsafe { _mm_loadu_si128(input.cast()) };
    let row1 = unsafe { _mm_loadu_si128(input.add(16).cast()) };
    let row2 = unsafe { _mm_loadu_si128(input.add(32).cast()) };
    let row3 = unsafe { _mm_loadu_si128(input.add(48).cast()) };
    let low01 = _mm_unpacklo_epi32(row0, row1);
    let high01 = _mm_unpackhi_epi32(row0, row1);
    let low23 = _mm_unpacklo_epi32(row2, row3);
    let high23 = _mm_unpackhi_epi32(row2, row3);
    let block1 = premix_x86_128_lane_sse41::<15, 17>(
        _mm_unpacklo_epi64(low01, low23),
        X86_128_C1[0],
        X86_128_C2[0],
    );
    let block2 = premix_x86_128_lane_sse41::<16, 16>(
        _mm_unpackhi_epi64(low01, low23),
        X86_128_C1[1],
        X86_128_C2[1],
    );
    let block3 = premix_x86_128_lane_sse41::<17, 15>(
        _mm_unpacklo_epi64(high01, high23),
        X86_128_C1[2],
        X86_128_C2[2],
    );
    let block4 = premix_x86_128_lane_sse41::<18, 14>(
        _mm_unpackhi_epi64(high01, high23),
        X86_128_C1[3],
        X86_128_C2[3],
    );
    unsafe { _mm_storeu_si128(output.cast(), block1) };
    unsafe { _mm_storeu_si128(output.add(4).cast(), block2) };
    unsafe { _mm_storeu_si128(output.add(8).cast(), block3) };
    unsafe { _mm_storeu_si128(output.add(12).cast(), block4) };
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn premix_x86_128_lane_sse41<const LEFT: i32, const RIGHT: i32>(
    blocks: __m128i,
    c1: u32,
    c2: u32,
) -> __m128i {
    let blocks = _mm_mullo_epi32(blocks, _mm_set1_epi32(c1 as i32));
    let blocks = _mm_or_si128(
        _mm_slli_epi32::<LEFT>(blocks),
        _mm_srli_epi32::<RIGHT>(blocks),
    );
    _mm_mullo_epi32(blocks, _mm_set1_epi32(c2 as i32))
}

#[inline(always)]
fn mix_x86_128_transposed_group(hashes: &mut [u32; 4], mixed: &[u32]) {
    for index in 0..4 {
        mix_x86_128_hashes(
            hashes,
            mixed[index],
            mixed[4 + index],
            mixed[8 + index],
            mixed[12 + index],
        );
    }
}

#[inline(always)]
fn mix_x86_128_blocks(hashes: &mut [u32; 4], blocks: &[u32]) {
    for block in blocks.as_chunks::<4>().0 {
        mix_x86_128_hashes(hashes, block[0], block[1], block[2], block[3]);
    }
}

#[inline(always)]
/// # Safety
/// The current CPU must support the selected backend.
pub(in crate::murmur3) unsafe fn mix_x86_128_body(
    blocks: FullBlocks<'_, 16>,
    hashes: &mut [u32; 4],
    backend: Backend,
) {
    match backend {
        Backend::Avx2 => {
            unsafe { mix_x86_128_body_avx2(blocks, hashes) };
        }
        Backend::Sse41 => {
            unsafe { mix_x86_128_body_sse41(blocks, hashes) };
        }
        Backend::Scalar => mix_x86_128_body_scalar(blocks, hashes),
    }
}
