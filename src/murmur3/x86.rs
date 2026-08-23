#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::primitives::{read_u32_le, read_u64_le};
use super::x86_32::mix_x86_32_hash;
use super::x86_128::{mix_x86_128_body_scalar, mix_x86_128_hashes};

const C1: u32 = 0xcc9e_2d51;
const C2: u32 = 0x1b87_3593;

#[target_feature(enable = "avx2")]
pub(super) unsafe fn mix_x86_32_body_avx2(key: &[u8], hash: &mut u32) {
    debug_assert!(key.len().is_multiple_of(4));
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
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        value = mix_x86_32_hash(value, block);
        offset += 4;
    }
    *hash = value;
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
pub(super) unsafe fn mix_x86_32_body_sse41(key: &[u8], hash: &mut u32) {
    debug_assert!(key.len().is_multiple_of(4));
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
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        value = mix_x86_32_hash(value, block);
        offset += 4;
    }
    *hash = value;
}

#[target_feature(enable = "sse4.1")]
#[inline]
fn premix_sse41(blocks: __m128i) -> __m128i {
    let blocks = _mm_mullo_epi32(blocks, _mm_set1_epi32(C1 as i32));
    let blocks = _mm_or_si128(_mm_slli_epi32::<15>(blocks), _mm_srli_epi32::<17>(blocks));
    _mm_mullo_epi32(blocks, _mm_set1_epi32(C2 as i32))
}

const X86_128_C1: [u32; 4] = [0x239b_961b, 0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93];
const X86_128_C2: [u32; 4] = [0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93, 0x239b_961b];

#[target_feature(enable = "avx2")]
pub(super) unsafe fn mix_x86_128_body_avx2(key: &[u8], hashes: &mut [u32; 4]) {
    debug_assert!(key.len().is_multiple_of(16));
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
    let mut mixed = [0_u32; 64];
    let mut offset = 0;

    while offset + 256 <= key.len() {
        for vector in 0..8 {
            let blocks =
                unsafe { _mm256_loadu_si256(key.as_ptr().add(offset + vector * 32).cast()) };
            let blocks = premix_x86_128_avx2(blocks, c1, c2, rotate_left, rotate_right);
            unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(vector * 8).cast(), blocks) };
        }
        mix_x86_128_blocks(hashes, &mixed);
        offset += 256;
    }
    while offset + 128 <= key.len() {
        for vector in 0..4 {
            let blocks =
                unsafe { _mm256_loadu_si256(key.as_ptr().add(offset + vector * 32).cast()) };
            let blocks = premix_x86_128_avx2(blocks, c1, c2, rotate_left, rotate_right);
            unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().add(vector * 8).cast(), blocks) };
        }
        mix_x86_128_blocks(hashes, &mixed[..32]);
        offset += 128;
    }
    while offset + 32 <= key.len() {
        let blocks = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
        let blocks = premix_x86_128_avx2(blocks, c1, c2, rotate_left, rotate_right);
        unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), blocks) };
        mix_x86_128_blocks(hashes, &mixed[..8]);
        offset += 32;
    }
    mix_x86_128_body_scalar(&key[offset..], hashes);
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
pub(super) unsafe fn mix_x86_128_body_sse41(key: &[u8], hashes: &mut [u32; 4]) {
    debug_assert!(key.len().is_multiple_of(16));
    let mut mixed = [0_u32; 64];
    let mut offset = 0;

    while offset + 256 <= key.len() {
        for group in 0..4 {
            unsafe {
                premix_x86_128_group_sse41(
                    key.as_ptr().add(offset + group * 64),
                    mixed.as_mut_ptr().add(group * 16),
                )
            };
        }
        for group in 0..4 {
            mix_x86_128_transposed_group(hashes, &mixed[group * 16..group * 16 + 16]);
        }
        offset += 256;
    }
    while offset + 64 <= key.len() {
        unsafe { premix_x86_128_group_sse41(key.as_ptr().add(offset), mixed.as_mut_ptr()) };
        mix_x86_128_transposed_group(hashes, &mixed[..16]);
        offset += 64;
    }
    mix_x86_128_body_scalar(&key[offset..], hashes);
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

const X64_128_C1: u64 = 0x87c3_7b91_1142_53d5;
const X64_128_C2: u64 = 0x4cf5_ad43_2745_937f;

macro_rules! define_x64_128_avx2_kernel {
    ($name:ident, $features:literal) => {
        #[target_feature(enable = $features)]
        pub(super) unsafe fn $name(key: &[u8], hashes: [u64; 2]) -> [u64; 2] {
            debug_assert!(key.len().is_multiple_of(16));
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
                    let blocks = unsafe {
                        _mm256_loadu_si256(key.as_ptr().add(offset + vector * 32).cast())
                    };
                    let blocks = premix_x64_128_avx2(blocks, c1, c2, rotate_left, rotate_right);
                    unsafe {
                        _mm256_storeu_si256(mixed.as_mut_ptr().add(vector * 4).cast(), blocks)
                    };
                }
                mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed);
                offset += 128;
            }
            while offset + 32 <= key.len() {
                let blocks = unsafe { _mm256_loadu_si256(key.as_ptr().add(offset).cast()) };
                let blocks = premix_x64_128_avx2(blocks, c1, c2, rotate_left, rotate_right);
                unsafe { _mm256_storeu_si256(mixed.as_mut_ptr().cast(), blocks) };
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
pub(super) unsafe fn mix_x64_128_body_sse41(key: &[u8], hashes: [u64; 2]) -> [u64; 2] {
    debug_assert!(key.len().is_multiple_of(16));
    let c1 = _mm_set_epi64x(X64_128_C2 as i64, X64_128_C1 as i64);
    let c2 = _mm_set_epi64x(X64_128_C1 as i64, X64_128_C2 as i64);
    let mut hash1 = hashes[0];
    let mut hash2 = hashes[1];
    let mut mixed = [0_u64; 8];
    let mut offset = 0;

    while offset + 64 <= key.len() {
        for vector in 0..4 {
            let blocks = unsafe { _mm_loadu_si128(key.as_ptr().add(offset + vector * 16).cast()) };
            let blocks = premix_x64_128_sse41(blocks, c1, c2);
            unsafe { _mm_storeu_si128(mixed.as_mut_ptr().add(vector * 2).cast(), blocks) };
        }
        mix_x64_128_blocks(&mut hash1, &mut hash2, &mixed);
        offset += 64;
    }
    while offset < key.len() {
        let blocks = unsafe { _mm_loadu_si128(key.as_ptr().add(offset).cast()) };
        let blocks = premix_x64_128_sse41(blocks, c1, c2);
        unsafe { _mm_storeu_si128(mixed.as_mut_ptr().cast(), blocks) };
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
fn mix_x64_128_hashes(hash1: &mut u64, hash2: &mut u64, block1: u64, block2: u64) {
    *hash1 ^= block1;
    *hash1 = hash1
        .rotate_left(27)
        .wrapping_add(*hash2)
        .wrapping_mul(5)
        .wrapping_add(0x52dc_e729);
    *hash2 ^= block2;
    *hash2 = hash2
        .rotate_left(31)
        .wrapping_add(*hash1)
        .wrapping_mul(5)
        .wrapping_add(0x3849_5ab5);
}
#[inline(always)]
/// # Safety
/// The selected backend must be supported by the current CPU.
pub(super) unsafe fn try_mix_x86_32_body(
    key: &[u8],
    hash: &mut u32,
    backend: super::dispatch::Backend,
) -> bool {
    match backend {
        super::dispatch::Backend::Avx2 => {
            unsafe { mix_x86_32_body_avx2(key, hash) };
            true
        }
        super::dispatch::Backend::Sse41 => {
            unsafe { mix_x86_32_body_sse41(key, hash) };
            true
        }
        super::dispatch::Backend::Scalar => false,
    }
}
#[inline(always)]
/// # Safety
/// The selected backend and BMI2 flag must describe features available on the
/// current CPU.
pub(super) unsafe fn try_mix_x64_128_body(
    key: &[u8],
    hashes: &mut [u64; 2],
    backend: super::dispatch::Backend,
    bmi2: bool,
) -> bool {
    match backend {
        super::dispatch::Backend::Avx2 => {
            *hashes = if bmi2 {
                unsafe { mix_x64_128_body_avx2_bmi2(key, *hashes) }
            } else {
                unsafe { mix_x64_128_body_avx2(key, *hashes) }
            };
            true
        }
        super::dispatch::Backend::Sse41 => {
            *hashes = unsafe { mix_x64_128_body_sse41(key, *hashes) };
            true
        }
        super::dispatch::Backend::Scalar => false,
    }
}

#[inline(always)]
/// # Safety
/// The selected backend must be supported by the current CPU.
pub(super) unsafe fn try_mix_x86_128_body(
    key: &[u8],
    hashes: &mut [u32; 4],
    backend: super::dispatch::Backend,
) -> bool {
    match backend {
        super::dispatch::Backend::Avx2 => {
            unsafe { mix_x86_128_body_avx2(key, hashes) };
            true
        }
        super::dispatch::Backend::Sse41 => {
            unsafe { mix_x86_128_body_sse41(key, hashes) };
            true
        }
        super::dispatch::Backend::Scalar => false,
    }
}
