#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
use std::{arch::asm, hint::black_box};

use super::Base64Error;
#[cfg(not(coverage))]
use super::{MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE};

pub(super) struct StandardDecoder;
pub(super) struct UrlSafeDecoder;
pub(super) struct MixedDecoder;
pub(super) struct ExactStore;
pub(super) struct PaddedStore;

// Streaming stores avoid evicting the caller's input and other hot data when
// encoding large one-shot buffers. Keep the threshold conservative because
// write-allocate stores are faster for buffers that are likely to be reused.
const NT_STORE_MIN_LEN: usize = 4 << 20;

#[cfg(target_arch = "x86_64")]
struct EncodeAvx2Constants {
    reshuffle: __m256i,
    align_mul: __m256i,
    field_mask: __m256i,
    field_mul: __m256i,
    translate: __m256i,
    c51: __m256i,
    c25: __m256i,
}

pub(super) trait Decoder {
    #[cfg(not(coverage))]
    fn decode_table() -> &'static [u8; 256];
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i);
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i);
}

pub(super) trait Store {
    unsafe fn store_12(output: *mut u8, value: __m128i);
    unsafe fn store_24(output: *mut u8, value: __m256i);
}

impl Store for ExactStore {
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        unsafe { store_12_exact(output, value) };
    }

    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        unsafe { store_24_exact(output, value) };
    }
}

impl Store for PaddedStore {
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        unsafe { store_12_padded(output, value) };
    }

    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        unsafe { store_24_padded(output, value) };
    }
}

impl Decoder for StandardDecoder {
    #[cfg(not(coverage))]
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &STANDARD_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_standard(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_standard(input) }
    }
}

impl Decoder for UrlSafeDecoder {
    #[cfg(not(coverage))]
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &URLSAFE_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_urlsafe(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_urlsafe(input) }
    }
}

impl Decoder for MixedDecoder {
    #[cfg(not(coverage))]
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &MIXED_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_mixed(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_mixed(input) }
    }
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn encode_ssse3<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    let mut source = 0;
    let mut destination = 0;
    while source + 52 <= input.len() {
        let first = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source)) };
        let second = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 12)) };
        let third = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 24)) };
        let fourth = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 36)) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), first) };
        unsafe { _mm_storeu_si128(output.add(destination + 16).cast(), second) };
        unsafe { _mm_storeu_si128(output.add(destination + 32).cast(), third) };
        unsafe { _mm_storeu_si128(output.add(destination + 48).cast(), fourth) };
        source += 48;
        destination += 64;
    }
    // Loading a vector reads 16 bytes, so leave enough bytes for the load.
    while source + 16 <= input.len() {
        let encoded = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source)) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };
        source += 12;
        destination += 16;
    }
    source
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn encode_avx2<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    if input.len() < 32 {
        return unsafe { encode_ssse3::<URLSAFE>(input, output) };
    }

    let first = unsafe { encode_24_first::<URLSAFE>(input.as_ptr()) };
    unsafe { _mm256_storeu_si256(output.cast(), first) };

    // Later loads start four bytes before the block they encode. Keeping the
    // actual load offset avoids forming a pointer before the start of `input`.
    let mut load_offset = 20;
    let mut destination = 32;
    #[cfg(target_arch = "x86_64")]
    if input.len() >= 64 * 1024 {
        // A group needs four 32-byte shifted loads for 96 logical input bytes.
        // `input.len() - load_offset - 8` is an equivalent division form of
        // the final-load bound `load_offset + 104 <= input.len()`.
        let groups = (input.len() - load_offset - 8) / 96;
        if groups != 0 {
            let use_streaming_stores =
                input.len() >= NT_STORE_MIN_LEN && output.align_offset(16) == 0;
            if use_streaming_stores {
                unsafe {
                    encode_96_shifted_nt::<URLSAFE>(
                        input.as_ptr().add(load_offset),
                        output.add(destination),
                        groups,
                    )
                };
            } else {
                unsafe {
                    encode_96_shifted_asm::<URLSAFE>(
                        input.as_ptr().add(load_offset),
                        output.add(destination),
                        groups,
                    )
                };
            }
            load_offset += groups * 96;
            destination += groups * 128;
        }
    }
    // The helper's fixed call and register-save costs outweigh its scheduling
    // benefit on small inputs. This loop is also the 32-bit x86 fallback.
    while load_offset + 104 <= input.len() {
        let first = unsafe { encode_24_shifted::<URLSAFE>(input.as_ptr().add(load_offset)) };
        let second = unsafe { encode_24_shifted::<URLSAFE>(input.as_ptr().add(load_offset + 24)) };
        let third = unsafe { encode_24_shifted::<URLSAFE>(input.as_ptr().add(load_offset + 48)) };
        let fourth = unsafe { encode_24_shifted::<URLSAFE>(input.as_ptr().add(load_offset + 72)) };
        unsafe { _mm256_storeu_si256(output.add(destination).cast(), first) };
        unsafe { _mm256_storeu_si256(output.add(destination + 32).cast(), second) };
        unsafe { _mm256_storeu_si256(output.add(destination + 64).cast(), third) };
        unsafe { _mm256_storeu_si256(output.add(destination + 96).cast(), fourth) };
        load_offset += 96;
        destination += 128;
    }
    while load_offset + 32 <= input.len() {
        let encoded = unsafe { encode_24_shifted::<URLSAFE>(input.as_ptr().add(load_offset)) };
        unsafe { _mm256_storeu_si256(output.add(destination).cast(), encoded) };
        load_offset += 24;
        destination += 32;
    }

    // The shifted load is four bytes behind the logical source position.
    let source = load_offset + 4;
    // Keep the final SIMD block VEX-encoded. Entering the legacy-encoded
    // SSSE3 helper after YMM work can incur an AVX-to-SSE transition penalty.
    if source + 16 <= input.len() {
        let encoded = unsafe { encode_12_avx2::<URLSAFE>(input.as_ptr().add(source)) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };
        source + 12
    } else {
        source
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_shifted_nt<const URLSAFE: bool>(
    mut input: *const u8,
    mut output: *mut u8,
    mut groups: usize,
) {
    let constants = encode_avx2_constants::<URLSAFE>();
    while groups >= 2 {
        // Load the whole unrolled group before doing any arithmetic. This
        // keeps the independent multiply/shuffle chains in flight together.
        let first = unsafe { _mm256_loadu_si256(input.cast()) };
        let second = unsafe { _mm256_loadu_si256(input.add(24).cast()) };
        let third = unsafe { _mm256_loadu_si256(input.add(48).cast()) };
        let fourth = unsafe { _mm256_loadu_si256(input.add(72).cast()) };
        let fifth = unsafe { _mm256_loadu_si256(input.add(96).cast()) };
        let sixth = unsafe { _mm256_loadu_si256(input.add(120).cast()) };
        let seventh = unsafe { _mm256_loadu_si256(input.add(144).cast()) };
        let eighth = unsafe { _mm256_loadu_si256(input.add(168).cast()) };
        let first = encode_avx2_value(first, &constants);
        let second = encode_avx2_value(second, &constants);
        let third = encode_avx2_value(third, &constants);
        let fourth = encode_avx2_value(fourth, &constants);
        let fifth = encode_avx2_value(fifth, &constants);
        let sixth = encode_avx2_value(sixth, &constants);
        let seventh = encode_avx2_value(seventh, &constants);
        let eighth = encode_avx2_value(eighth, &constants);
        unsafe {
            store_32_nt(output, first);
            store_32_nt(output.add(32), second);
            store_32_nt(output.add(64), third);
            store_32_nt(output.add(96), fourth);
            store_32_nt(output.add(128), fifth);
            store_32_nt(output.add(160), sixth);
            store_32_nt(output.add(192), seventh);
            store_32_nt(output.add(224), eighth);
        }
        input = unsafe { input.add(192) };
        output = unsafe { output.add(256) };
        groups -= 2;
    }
    if groups != 0 {
        let first = unsafe { _mm256_loadu_si256(input.cast()) };
        let second = unsafe { _mm256_loadu_si256(input.add(24).cast()) };
        let third = unsafe { _mm256_loadu_si256(input.add(48).cast()) };
        let fourth = unsafe { _mm256_loadu_si256(input.add(72).cast()) };
        let first = encode_avx2_value(first, &constants);
        let second = encode_avx2_value(second, &constants);
        let third = encode_avx2_value(third, &constants);
        let fourth = encode_avx2_value(fourth, &constants);
        unsafe {
            store_32_nt(output, first);
            store_32_nt(output.add(32), second);
            store_32_nt(output.add(64), third);
            store_32_nt(output.add(96), fourth);
        }
    }
    // Streaming stores are weakly ordered with respect to later loads/stores.
    _mm_sfence();
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
fn encode_avx2_constants<const URLSAFE: bool>() -> EncodeAvx2Constants {
    let translate = if URLSAFE {
        _mm256_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -17, 32, 0, 0, 65, 71, -4, -4, -4, -4,
            -4, -4, -4, -4, -4, -4, -17, 32, 0, 0,
        )
    } else {
        _mm256_setr_epi8(
            65, 71, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -19, -16, 0, 0, 65, 71, -4, -4, -4, -4,
            -4, -4, -4, -4, -4, -4, -19, -16, 0, 0,
        )
    };
    EncodeAvx2Constants {
        reshuffle: _mm256_set_epi8(
            10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1, 14, 15, 13, 14, 11, 12, 10, 11, 8,
            9, 7, 8, 5, 6, 4, 5,
        ),
        align_mul: black_box(_mm256_set1_epi32(0x0010_0001)),
        field_mask: _mm256_set1_epi32(0x003f_03f0),
        field_mul: black_box(_mm256_set1_epi32(0x0100_0010)),
        translate,
        c51: _mm256_set1_epi8(51),
        c25: _mm256_set1_epi8(25),
    }
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
fn encode_avx2_value(input: __m256i, constants: &EncodeAvx2Constants) -> __m256i {
    let shuffled = _mm256_shuffle_epi8(input, constants.reshuffle);
    let aligned = _mm256_srli_epi16(_mm256_mullo_epi16(shuffled, constants.align_mul), 10);
    let fields = _mm256_mullo_epi16(
        _mm256_and_si256(shuffled, constants.field_mask),
        constants.field_mul,
    );
    let indices = _mm256_or_si256(aligned, fields);
    let lut_index = _mm256_sub_epi8(
        _mm256_subs_epu8(indices, constants.c51),
        _mm256_cmpgt_epi8(indices, constants.c25),
    );
    _mm256_add_epi8(indices, _mm256_shuffle_epi8(constants.translate, lut_index))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn store_32_nt(output: *mut u8, value: __m256i) {
    unsafe {
        _mm_stream_si128(output.cast(), _mm256_castsi256_si128(value));
        _mm_stream_si128(output.add(16).cast(), _mm256_extracti128_si256(value, 1));
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_shifted_asm<const URLSAFE: bool>(
    input: *const u8,
    output: *mut u8,
    groups: usize,
) {
    let shuffle = _mm256_setr_epi8(
        5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7,
        10, 9, 11, 10,
    );
    let higher_mask = _mm256_set1_epi32(0x0fc0_fc00);
    let higher_multiplier = _mm256_set1_epi32(0x0400_0040);
    let lower_mask = _mm256_set1_epi32(0x003f_03f0);
    let lower_multiplier = _mm256_set1_epi32(0x0100_0010);
    let reduction_base = _mm256_set1_epi8(51);
    let lower_bound = _mm256_set1_epi8(25);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        b'A' as i8,
        (b'a' - 26) as i8,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        if URLSAFE { -17 } else { -19 },
        if URLSAFE { 32 } else { -16 },
        0,
        0,
    ));

    // Keep the actual branch target aligned, rather than placing an alignment
    // directive in a Rust loop where LLVM's backedge label precedes the NOPs.
    // Eight input constants plus eight early-clobber outputs occupy all YMM
    // registers on x86-64, preventing accidental input/output register aliasing.
    unsafe {
        asm!(
            ".p2align 5",
            "2:",
            "vmovdqu {value0}, [{input}]",
            "vmovdqu {value1}, [{input} + 24]",
            "vmovdqu {value2}, [{input} + 48]",
            "vmovdqu {value3}, [{input} + 72]",
            "vpshufb {value0}, {value0}, {shuffle}",
            "vpshufb {value1}, {value1}, {shuffle}",
            "vpshufb {value2}, {value2}, {shuffle}",
            "vpshufb {value3}, {value3}, {shuffle}",
            "vpand {temporary0}, {value0}, {higher_mask}",
            "vpand {temporary1}, {value1}, {higher_mask}",
            "vpand {temporary2}, {value2}, {higher_mask}",
            "vpand {temporary3}, {value3}, {higher_mask}",
            "vpmulhuw {temporary0}, {temporary0}, {higher_multiplier}",
            "vpmulhuw {temporary1}, {temporary1}, {higher_multiplier}",
            "vpmulhuw {temporary2}, {temporary2}, {higher_multiplier}",
            "vpmulhuw {temporary3}, {temporary3}, {higher_multiplier}",
            "vpand {value0}, {value0}, {lower_mask}",
            "vpand {value1}, {value1}, {lower_mask}",
            "vpand {value2}, {value2}, {lower_mask}",
            "vpand {value3}, {value3}, {lower_mask}",
            "vpmullw {value0}, {value0}, {lower_multiplier}",
            "vpmullw {value1}, {value1}, {lower_multiplier}",
            "vpmullw {value2}, {value2}, {lower_multiplier}",
            "vpmullw {value3}, {value3}, {lower_multiplier}",
            "vpor {value0}, {value0}, {temporary0}",
            "vpor {value1}, {value1}, {temporary1}",
            "vpor {value2}, {value2}, {temporary2}",
            "vpor {value3}, {value3}, {temporary3}",
            // Translate two independent vectors at a time so both dependency
            // chains remain available to the scheduler.
            "vpsubusb {temporary0}, {value0}, {reduction_base}",
            "vpsubusb {temporary1}, {value1}, {reduction_base}",
            "vpcmpgtb {temporary2}, {value0}, {lower_bound}",
            "vpcmpgtb {temporary3}, {value1}, {lower_bound}",
            "vpsubb {temporary0}, {temporary0}, {temporary2}",
            "vpsubb {temporary1}, {temporary1}, {temporary3}",
            "vpshufb {temporary0}, {offsets}, {temporary0}",
            "vpshufb {temporary1}, {offsets}, {temporary1}",
            "vpaddb {value0}, {value0}, {temporary0}",
            "vpaddb {value1}, {value1}, {temporary1}",
            "vpsubusb {temporary0}, {value2}, {reduction_base}",
            "vpsubusb {temporary1}, {value3}, {reduction_base}",
            "vpcmpgtb {temporary2}, {value2}, {lower_bound}",
            "vpcmpgtb {temporary3}, {value3}, {lower_bound}",
            "vpsubb {temporary0}, {temporary0}, {temporary2}",
            "vpsubb {temporary1}, {temporary1}, {temporary3}",
            "vpshufb {temporary0}, {offsets}, {temporary0}",
            "vpshufb {temporary1}, {offsets}, {temporary1}",
            "vpaddb {value2}, {value2}, {temporary0}",
            "vpaddb {value3}, {value3}, {temporary1}",
            "vmovdqu [{output}], {value0}",
            "vmovdqu [{output} + 32], {value1}",
            "vmovdqu [{output} + 64], {value2}",
            "vmovdqu [{output} + 96], {value3}",
            "add {input}, 96",
            "add {output}, 128",
            "dec {groups}",
            "jnz 2b",
            input = inout(reg) input => _,
            output = inout(reg) output => _,
            groups = inout(reg) groups => _,
            shuffle = in(ymm_reg) shuffle,
            higher_mask = in(ymm_reg) higher_mask,
            higher_multiplier = in(ymm_reg) higher_multiplier,
            lower_mask = in(ymm_reg) lower_mask,
            lower_multiplier = in(ymm_reg) lower_multiplier,
            reduction_base = in(ymm_reg) reduction_base,
            lower_bound = in(ymm_reg) lower_bound,
            offsets = in(ymm_reg) offsets,
            value0 = out(ymm_reg) _,
            value1 = out(ymm_reg) _,
            value2 = out(ymm_reg) _,
            value3 = out(ymm_reg) _,
            temporary0 = out(ymm_reg) _,
            temporary1 = out(ymm_reg) _,
            temporary2 = out(ymm_reg) _,
            temporary3 = out(ymm_reg) _,
            options(nostack)
        );
    }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_avx2<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 128 <= input.len() {
        let (first, first_error) = unsafe { A::decode_32(input.as_ptr().add(source)) };
        let (second, second_error) = unsafe { A::decode_32(input.as_ptr().add(source + 32)) };
        let (third, third_error) = unsafe { A::decode_32(input.as_ptr().add(source + 64)) };
        let (fourth, fourth_error) = unsafe { A::decode_32(input.as_ptr().add(source + 96)) };
        let errors = _mm256_or_si256(
            _mm256_or_si256(first_error, second_error),
            _mm256_or_si256(third_error, fourth_error),
        );
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        // The padded stores write four bytes into the following block's output,
        // where the next store replaces them. This is safe for exact-output
        // callers because each of these blocks has a complete successor.
        unsafe { store_24_padded(output.add(destination), pack_32(first)) };
        unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
        unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        if source + 160 <= input.len() {
            unsafe { store_24_padded(output.add(destination + 72), pack_32(fourth)) };
        } else {
            unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };
        }
        source += 128;
        destination += 96;
    }
    while source + 32 <= input.len() {
        let (indices, errors) = unsafe { A::decode_32(input.as_ptr().add(source)) };
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        if source + 64 <= input.len() {
            // A complete following block provides enough in-bounds output for
            // the four-byte overlap, which that block replaces.
            unsafe { store_24_padded(output.add(destination), pack_32(indices)) };
        } else {
            unsafe { S::store_24(output.add(destination), pack_32(indices)) };
        }
        source += 32;
        destination += 24;
    }
    // At most one 16-byte block remains after the AVX2 loops. Decode it
    // directly so the bulk SSSE3 entry point does not sit on the AVX2 hot path.
    if source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_ssse3<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
        let errors = _mm_or_si128(
            _mm_or_si128(first_errors, second_errors),
            _mm_or_si128(third_errors, fourth_errors),
        );
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(first)) };
        unsafe { S::store_12(output.add(destination + 12), pack_16_indices(second)) };
        unsafe { S::store_12(output.add(destination + 24), pack_16_indices(third)) };
        unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3,sse4.1")]
pub(super) unsafe fn decode_sse41<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
        let errors = _mm_or_si128(
            _mm_or_si128(first_errors, second_errors),
            _mm_or_si128(third_errors, fourth_errors),
        );
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(first)) };
        unsafe { S::store_12(output.add(destination + 12), pack_16_indices(second)) };
        unsafe { S::store_12(output.add(destination + 24), pack_16_indices(third)) };
        unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3")]
unsafe fn encode_12<const URLSAFE: bool>(input: *const u8) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);
    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact_ssse3(higher, _mm_set1_epi32(0x0400_0040)) };
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));
    ascii_from_indices::<URLSAFE>(_mm_or_si128(higher, lower))
}

#[target_feature(enable = "avx2")]
unsafe fn encode_12_avx2<const URLSAFE: bool>(input: *const u8) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);
    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact_avx2_128(higher, _mm_set1_epi32(0x0400_0040)) };
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = unsafe { mullo_epi16_exact_avx2_128(lower, _mm_set1_epi32(0x0100_0010)) };
    ascii_from_indices_avx2_128::<URLSAFE>(_mm_or_si128(higher, lower))
}

// LLVM can expand this constant multiply into a long widen/shift/pack
// sequence. Keep the single SSE2 instruction on the SSSE3 fallback path.
#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn mulhi_epu16_exact_ssse3(mut value: __m128i, multiplier: __m128i) -> __m128i {
    unsafe {
        asm!(
            "pmulhuw {value}, {multiplier}",
            value = inout(xmm_reg) value,
            multiplier = in(xmm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }
    value
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn mulhi_epu16_exact_avx2_128(mut value: __m128i, multiplier: __m128i) -> __m128i {
    unsafe {
        asm!(
            "vpmulhuw {value}, {value}, {multiplier}",
            value = inout(xmm_reg) value,
            multiplier = in(xmm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }
    value
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn mullo_epi16_exact_avx2_128(mut value: __m128i, multiplier: __m128i) -> __m128i {
    unsafe {
        asm!(
            "vpmullw {value}, {value}, {multiplier}",
            value = inout(xmm_reg) value,
            multiplier = in(xmm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }
    value
}

#[target_feature(enable = "avx2")]
unsafe fn encode_24_first<const URLSAFE: bool>(input: *const u8) -> __m256i {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let shifted = _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 0, 1, 2, 3, 4, 5, 6));
    encode_24_shifted_value::<URLSAFE>(shifted)
}

#[target_feature(enable = "avx2")]
unsafe fn encode_24_shifted<const URLSAFE: bool>(input: *const u8) -> __m256i {
    let shifted = unsafe { _mm256_loadu_si256(input.cast()) };
    encode_24_shifted_value::<URLSAFE>(shifted)
}

#[target_feature(enable = "avx2")]
fn encode_24_shifted_value<const URLSAFE: bool>(shifted: __m256i) -> __m256i {
    // The low lane's payload starts four bytes into the vector; the high
    // lane's payload starts at byte zero. This arrangement lets every block
    // after the first avoid a cross-lane VPERMD.
    let shuffle = _mm256_setr_epi8(
        5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7,
        10, 9, 11, 10,
    );
    let value = _mm256_shuffle_epi8(shifted, shuffle);

    let higher = _mm256_and_si256(value, _mm256_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact(higher, _mm256_set1_epi32(0x0400_0040)) };
    let lower = _mm256_and_si256(value, _mm256_set1_epi32(0x003f_03f0));
    let lower = unsafe { mullo_epi16_exact(lower, _mm256_set1_epi32(0x0100_0010)) };
    ascii_from_indices_avx2::<URLSAFE>(_mm256_or_si256(higher, lower))
}

// LLVM can strength-reduce these alternating word multipliers into a much
// longer widen/shift/pack sequence. Keep the native AVX2 instructions: each
// operation is one instruction and has exactly the intrinsic's wrapping
// 16-bit semantics.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn mulhi_epu16_exact(mut value: __m256i, multiplier: __m256i) -> __m256i {
    unsafe {
        asm!(
            "vpmulhuw {value}, {value}, {multiplier}",
            value = inout(ymm_reg) value,
            multiplier = in(ymm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }
    value
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn mullo_epi16_exact(mut value: __m256i, multiplier: __m256i) -> __m256i {
    unsafe {
        asm!(
            "vpmullw {value}, {value}, {multiplier}",
            value = inout(ymm_reg) value,
            multiplier = in(ymm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }
    value
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_standard(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
        0x10,
    );
    let low_classes = _mm_setr_epi8(
        0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b, 0x1b,
        0x1a,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    let offsets = _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    (
        _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_urlsafe(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b,
        0x33,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let offsets = _mm_setr_epi8(0, 0, 17, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let correction = _mm_and_si128(underscore, _mm_set1_epi8(33));
    (_mm_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_mixed(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b,
        0x32,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    let offsets = _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices));
    let dash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let corrections = _mm_or_si128(
        _mm_and_si128(dash, _mm_set1_epi8(-2)),
        _mm_and_si128(underscore, _mm_set1_epi8(33)),
    );
    (_mm_add_epi8(indices, corrections), errors)
}

#[target_feature(enable = "ssse3")]
fn classify_ascii_ssse3(
    value: __m128i,
    high_classes: __m128i,
    low_classes: __m128i,
) -> (__m128i, __m128i) {
    // Invalid high/low nibble pairs share a class bit; valid pairs produce zero.
    let mask = _mm_set1_epi8(0x0f);
    let high_nibbles = _mm_and_si128(_mm_srli_epi16(value, 4), mask);
    let low_nibbles = _mm_and_si128(value, mask);
    let high_matches = _mm_shuffle_epi8(high_classes, high_nibbles);
    let low_matches = _mm_shuffle_epi8(low_classes, low_nibbles);
    (high_nibbles, _mm_and_si128(high_matches, low_matches))
}

#[target_feature(enable = "ssse3")]
fn errors_are_zero_ssse3(errors: __m128i) -> bool {
    _mm_movemask_epi8(_mm_cmpeq_epi8(errors, _mm_setzero_si128())) == 0xffff
}

#[target_feature(enable = "ssse3")]
fn pack_16_indices(indices: __m128i) -> __m128i {
    let merged = _mm_maddubs_epi16(indices, _mm_set1_epi32(0x0140_0140));
    let packed = _mm_madd_epi16(merged, _mm_set1_epi32(0x0001_1000));
    let shuffle = _mm_setr_epi8(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1);
    _mm_shuffle_epi8(packed, shuffle)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_standard(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x2a, 0x2b, 0x2b, 0x2b,
        0x2a,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    (
        _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b,
        0x33,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 0, 17, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    let indices = _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let correction = _mm256_and_si256(underscore, _mm256_set1_epi8(33));
    (_mm256_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b,
        0x32,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    let indices = _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices));
    let dash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
    let underscore = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let corrections = _mm256_or_si256(
        _mm256_and_si256(dash, _mm256_set1_epi8(-2)),
        _mm256_and_si256(underscore, _mm256_set1_epi8(33)),
    );
    (_mm256_add_epi8(indices, corrections), errors)
}

#[target_feature(enable = "avx2")]
fn classify_ascii_avx2(
    value: __m256i,
    high_classes: __m128i,
    low_classes: __m128i,
) -> (__m256i, __m256i) {
    // Invalid high/low nibble pairs share a class bit; valid pairs produce zero.
    let mask = _mm256_set1_epi8(0x0f);
    let high_nibbles = _mm256_and_si256(_mm256_srli_epi16(value, 4), mask);
    let low_nibbles = _mm256_and_si256(value, mask);
    let high_matches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(high_classes), high_nibbles);
    let low_matches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(low_classes), low_nibbles);
    (high_nibbles, _mm256_and_si256(high_matches, low_matches))
}

#[target_feature(enable = "avx2")]
fn pack_32(indices: __m256i) -> __m256i {
    let merged = _mm256_maddubs_epi16(indices, _mm256_set1_epi32(0x0140_0140));
    let packed = _mm256_madd_epi16(merged, _mm256_set1_epi32(0x0001_1000));
    let shuffle = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1,
    ));
    _mm256_shuffle_epi8(packed, shuffle)
}

#[target_feature(enable = "ssse3")]
fn ascii_from_indices<const URLSAFE: bool>(indices: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let lower = _mm_cmpgt_epi8(indices, _mm_set1_epi8(25));
    let reduced = _mm_sub_epi8(reduced, lower);
    let offsets = _mm_setr_epi8(
        b'A' as i8,
        (b'a' - 26) as i8,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        if URLSAFE { -17 } else { -19 },
        if URLSAFE { 32 } else { -16 },
        0,
        0,
    );
    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "avx2")]
fn ascii_from_indices_avx2_128<const URLSAFE: bool>(indices: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let lower = _mm_cmpgt_epi8(indices, _mm_set1_epi8(25));
    let reduced = _mm_sub_epi8(reduced, lower);
    let offsets = _mm_setr_epi8(
        b'A' as i8,
        (b'a' - 26) as i8,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        if URLSAFE { -17 } else { -19 },
        if URLSAFE { 32 } else { -16 },
        0,
        0,
    );
    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "avx2")]
fn ascii_from_indices_avx2<const URLSAFE: bool>(indices: __m256i) -> __m256i {
    let reduced = _mm256_subs_epu8(indices, _mm256_set1_epi8(51));
    let lower = _mm256_cmpgt_epi8(indices, _mm256_set1_epi8(25));
    let reduced = _mm256_sub_epi8(reduced, lower);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        b'A' as i8,
        (b'a' - 26) as i8,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        if URLSAFE { -17 } else { -19 },
        if URLSAFE { 32 } else { -16 },
        0,
        0,
    ));
    _mm256_add_epi8(_mm256_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "ssse3")]
unsafe fn store_12_exact(output: *mut u8, value: __m128i) {
    unsafe { _mm_storel_epi64(output.cast(), value) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(value, 8));
    unsafe { output.add(8).cast::<i32>().write_unaligned(remaining) };
}

#[target_feature(enable = "avx2")]
unsafe fn store_24_exact(output: *mut u8, value: __m256i) {
    let lower = _mm256_castsi256_si128(value);
    let upper = _mm256_extracti128_si256(value, 1);
    // The first store's four lane-padding bytes are replaced by the second.
    unsafe { _mm_storeu_si128(output.cast(), lower) };
    unsafe { _mm_storel_epi64(output.add(12).cast(), upper) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(upper, 8));
    unsafe { output.add(20).cast::<i32>().write_unaligned(remaining) };
}

#[target_feature(enable = "ssse3")]
unsafe fn store_12_padded(output: *mut u8, value: __m128i) {
    unsafe { _mm_storeu_si128(output.cast(), value) };
}

#[target_feature(enable = "avx2")]
unsafe fn store_24_padded(output: *mut u8, value: __m256i) {
    unsafe { _mm_storeu_si128(output.cast(), _mm256_castsi256_si128(value)) };
    unsafe { _mm_storeu_si128(output.add(12).cast(), _mm256_extracti128_si256(value, 1)) };
}
