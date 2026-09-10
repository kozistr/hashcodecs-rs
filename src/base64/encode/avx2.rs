//! AVX2 encoding kernel.

use std::arch::asm;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;
#[cfg(target_arch = "x86_64")]
use std::hint::black_box;

#[cfg(feature = "python")]
use super::WrappedOutput;
use super::ssse3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::base64) enum Avx2StoreMode {
    Cached,
    Streaming,
}

#[cfg(target_arch = "x86_64")]
type Encode96Kernel = unsafe fn(*const u8, *mut u8, usize, __m256i);

#[cfg(target_arch = "x86_64")]
struct EncodeAvx2Constants {
    reshuffle: __m256i,
    align_mul: __m256i,
    field_mask: __m256i,
    field_mul: __m256i,
    translate: __m256i,
    last_lowercase_index: __m256i,
    last_uppercase_index: __m256i,
}

#[target_feature(enable = "avx2")]
pub(in crate::base64) unsafe fn encode_avx2_with_store<const URLSAFE: bool>(
    input: &[u8],
    output: *mut u8,
    store_mode: Avx2StoreMode,
) -> usize {
    if input.len() < 32 {
        return unsafe { ssse3::encode::<URLSAFE>(input, output) };
    }

    #[cfg(target_arch = "x86_64")]
    unsafe {
        encode_avx2_with_offsets(
            input,
            output,
            store_mode,
            standard_offsets_256::<URLSAFE>(),
            encode_96_shifted_asm::<URLSAFE>,
        )
    }

    #[cfg(target_arch = "x86")]
    unsafe {
        encode_avx2_with_offsets(input, output, store_mode, standard_offsets_256::<URLSAFE>())
    }
}

#[cfg(feature = "python")]
#[target_feature(enable = "avx2")]
pub(in crate::base64) unsafe fn encode_custom(
    input: &[u8],
    output: *mut u8,
    offsets: &[i8; 16],
    store_mode: Avx2StoreMode,
) -> usize {
    if input.len() < 32 {
        return unsafe { ssse3::encode_custom(input, output, offsets) };
    }

    let offsets = _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) });
    #[cfg(target_arch = "x86_64")]
    unsafe {
        encode_avx2_with_offsets(
            input,
            output,
            store_mode,
            offsets,
            encode_96_shifted_asm_custom,
        )
    }

    #[cfg(target_arch = "x86")]
    unsafe {
        encode_avx2_with_offsets(input, output, store_mode, offsets)
    }
}

#[target_feature(enable = "avx2")]
#[inline]
unsafe fn encode_avx2_with_offsets(
    input: &[u8],
    output: *mut u8,
    store_mode: Avx2StoreMode,
    offsets: __m256i,
    #[cfg(target_arch = "x86_64")] encode_96: Encode96Kernel,
) -> usize {
    #[cfg(target_arch = "x86")]
    let _ = store_mode;

    let first = unsafe { encode_24_first(input.as_ptr(), offsets) };
    unsafe { _mm256_storeu_si256(output.cast(), first) };

    // Later loads start four bytes before the block they encode. Keeping the
    // actual load offset avoids forming a pointer before the start of `input`.
    let mut load_offset = 20;
    let mut destination = 32;
    #[cfg(target_arch = "x86_64")]
    let mut used_streaming_stores = false;

    // A group needs four 32-byte shifted loads for 96 logical input bytes.
    // `input.len() - load_offset - 8` is an equivalent division form of the
    // final-load bound `load_offset + 104 <= input.len()`.
    #[cfg(target_arch = "x86_64")]
    {
        let groups = (input.len() - load_offset - 8) / 96;
        // Amortize the helper's fixed call and register-save costs across at
        // least two groups. Shorter prefixes use the inline 24-byte loop.
        if groups >= 2 {
            if store_mode == Avx2StoreMode::Streaming {
                used_streaming_stores = true;
                unsafe {
                    if output.align_offset(32) == 0 {
                        encode_96_shifted::<StreamingStore256>(
                            input.as_ptr().add(load_offset),
                            output.add(destination),
                            groups,
                            offsets,
                        )
                    } else {
                        encode_96_shifted::<StreamingStore128>(
                            input.as_ptr().add(load_offset),
                            output.add(destination),
                            groups,
                            offsets,
                        )
                    }
                };
            } else {
                unsafe {
                    encode_96(
                        input.as_ptr().add(load_offset),
                        output.add(destination),
                        groups,
                        offsets,
                    )
                };
            }

            load_offset += groups * 96;
            destination += groups * 128;
        }
    }

    // The assembly helper is unavailable on 32-bit x86.
    #[cfg(target_arch = "x86")]
    while load_offset + 104 <= input.len() {
        let first = unsafe { encode_24_shifted(input.as_ptr().add(load_offset), offsets) };
        let second = unsafe { encode_24_shifted(input.as_ptr().add(load_offset + 24), offsets) };
        let third = unsafe { encode_24_shifted(input.as_ptr().add(load_offset + 48), offsets) };
        let fourth = unsafe { encode_24_shifted(input.as_ptr().add(load_offset + 72), offsets) };

        unsafe { _mm256_storeu_si256(output.add(destination).cast(), first) };
        unsafe { _mm256_storeu_si256(output.add(destination + 32).cast(), second) };
        unsafe { _mm256_storeu_si256(output.add(destination + 64).cast(), third) };
        unsafe { _mm256_storeu_si256(output.add(destination + 96).cast(), fourth) };

        load_offset += 96;
        destination += 128;
    }

    while load_offset + 32 <= input.len() {
        let encoded = unsafe { encode_24_shifted(input.as_ptr().add(load_offset), offsets) };
        unsafe { _mm256_storeu_si256(output.add(destination).cast(), encoded) };

        load_offset += 24;
        destination += 32;
    }

    // The shifted load is four bytes behind the logical source position.
    let source = load_offset + 4;

    // Keep the final SIMD block VEX-encoded. Entering the legacy-encoded
    // SSSE3 helper after YMM work can incur an AVX-to-SSE transition penalty.
    let consumed = if source + 16 <= input.len() {
        let encoded =
            unsafe { encode_12_avx2(input.as_ptr().add(source), _mm256_castsi256_si128(offsets)) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };

        source + 12
    } else {
        source
    };

    #[cfg(target_arch = "x86_64")]
    if used_streaming_stores {
        // Streaming stores are weakly ordered with respect to later
        // loads/stores. Fence after the cached tail has also completed.
        _mm_sfence();
    }

    consumed
}

#[cfg(feature = "python")]
#[target_feature(enable = "avx2")]
pub(in crate::base64) unsafe fn encode_wrapped<const URLSAFE: bool>(
    input: &[u8],
    output: &mut WrappedOutput,
) -> usize {
    if input.len() < 32 {
        return unsafe { ssse3::encode_wrapped::<URLSAFE>(input, output) };
    }

    unsafe { encode_wrapped_with_offsets(input, output, standard_offsets_256::<URLSAFE>()) }
}

#[cfg(feature = "python")]
#[target_feature(enable = "avx2")]
pub(in crate::base64) unsafe fn encode_wrapped_custom(
    input: &[u8],
    output: &mut WrappedOutput,
    offsets: &[i8; 16],
) -> usize {
    if input.len() < 32 {
        return unsafe { ssse3::encode_wrapped_custom(input, output, offsets) };
    }

    let offsets = _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) });
    unsafe { encode_wrapped_with_offsets(input, output, offsets) }
}

#[cfg(feature = "python")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn encode_wrapped_with_offsets(
    input: &[u8],
    output: &mut WrappedOutput,
    offsets: __m256i,
) -> usize {
    let first = unsafe { encode_24_first(input.as_ptr(), offsets) };
    unsafe { write_wrapped_32(output, first) };

    let mut load_offset = 20;
    while load_offset + 104 <= input.len() {
        for offset in [0, 24, 48, 72] {
            let encoded =
                unsafe { encode_24_shifted(input.as_ptr().add(load_offset + offset), offsets) };
            unsafe { write_wrapped_32(output, encoded) };
        }
        load_offset += 96;
    }

    while load_offset + 32 <= input.len() {
        let encoded = unsafe { encode_24_shifted(input.as_ptr().add(load_offset), offsets) };
        unsafe { write_wrapped_32(output, encoded) };
        load_offset += 24;
    }

    let source = load_offset + 4;
    if source + 16 <= input.len() {
        let encoded =
            unsafe { encode_12_avx2(input.as_ptr().add(source), _mm256_castsi256_si128(offsets)) };
        unsafe { output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(encoded)) };
        source + 12
    } else {
        source
    }
}

#[cfg(feature = "python")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn write_wrapped_32(output: &mut WrappedOutput, value: __m256i) {
    unsafe {
        output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(
            _mm256_castsi256_si128(value),
        ));
        output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(
            _mm256_extracti128_si256::<1>(value),
        ));
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(unused_unsafe)]
unsafe fn encode_96_shifted<Store: StreamingStore>(
    mut input: *const u8,
    mut output: *mut u8,
    mut groups: usize,
    offsets: __m256i,
) {
    let constants = encode_avx2_constants(offsets);
    while groups >= 2 {
        let first = unsafe { encode_96_values(_mm256_loadu_si256(input.cast()), &constants) };
        let second =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(24).cast()), &constants) };
        let third =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(48).cast()), &constants) };
        let fourth =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(72).cast()), &constants) };

        unsafe {
            Store::store(output, first);
            Store::store(output.add(32), second);
            Store::store(output.add(64), third);
            Store::store(output.add(96), fourth);
        }

        let fifth =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(96).cast()), &constants) };
        let sixth =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(120).cast()), &constants) };
        let seventh =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(144).cast()), &constants) };
        let eighth =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(168).cast()), &constants) };

        unsafe {
            Store::store(output.add(128), fifth);
            Store::store(output.add(160), sixth);
            Store::store(output.add(192), seventh);
            Store::store(output.add(224), eighth);
        }

        input = unsafe { input.add(192) };
        output = unsafe { output.add(256) };

        groups -= 2;
    }

    if groups != 0 {
        let first = unsafe { encode_96_values(_mm256_loadu_si256(input.cast()), &constants) };
        let second =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(24).cast()), &constants) };
        let third =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(48).cast()), &constants) };
        let fourth =
            unsafe { encode_96_values(_mm256_loadu_si256(input.add(72).cast()), &constants) };

        unsafe {
            Store::store(output, first);
            Store::store(output.add(32), second);
            Store::store(output.add(64), third);
            Store::store(output.add(96), fourth);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2")]
fn encode_avx2_constants(translate: __m256i) -> EncodeAvx2Constants {
    EncodeAvx2Constants {
        reshuffle: _mm256_set_epi8(
            10, 11, 9, 10, 7, 8, 6, 7, 4, 5, 3, 4, 1, 2, 0, 1, 14, 15, 13, 14, 11, 12, 10, 11, 8,
            9, 7, 8, 5, 6, 4, 5,
        ),
        align_mul: black_box(_mm256_set1_epi32(0x0010_0001)),
        field_mask: _mm256_set1_epi32(0x003f_03f0),
        field_mul: black_box(_mm256_set1_epi32(0x0100_0010)),
        translate,
        last_lowercase_index: _mm256_set1_epi8(51),
        last_uppercase_index: _mm256_set1_epi8(25),
    }
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_values(input: __m256i, constants: &EncodeAvx2Constants) -> __m256i {
    let shuffled = _mm256_shuffle_epi8(input, constants.reshuffle);
    let aligned = _mm256_srli_epi16(_mm256_mullo_epi16(shuffled, constants.align_mul), 10);
    let fields = _mm256_mullo_epi16(
        _mm256_and_si256(shuffled, constants.field_mask),
        constants.field_mul,
    );

    let indices = _mm256_or_si256(aligned, fields);
    let lut_index = _mm256_sub_epi8(
        _mm256_subs_epu8(indices, constants.last_lowercase_index),
        _mm256_cmpgt_epi8(indices, constants.last_uppercase_index),
    );

    _mm256_add_epi8(indices, _mm256_shuffle_epi8(constants.translate, lut_index))
}

#[cfg(target_arch = "x86_64")]
trait StreamingStore {
    unsafe fn store(output: *mut u8, value: __m256i);
}

#[cfg(target_arch = "x86_64")]
struct StreamingStore128;

#[cfg(target_arch = "x86_64")]
impl StreamingStore for StreamingStore128 {
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn store(output: *mut u8, value: __m256i) {
        unsafe {
            _mm_stream_si128(output.cast(), _mm256_castsi256_si128(value));
            _mm_stream_si128(output.add(16).cast(), _mm256_extracti128_si256(value, 1));
        }
    }
}

#[cfg(target_arch = "x86_64")]
struct StreamingStore256;

#[cfg(target_arch = "x86_64")]
impl StreamingStore for StreamingStore256 {
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn store(output: *mut u8, value: __m256i) {
        unsafe { _mm256_stream_si256(output.cast(), value) };
    }
}

#[cfg(target_arch = "x86_64")]
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_shifted_asm<const URLSAFE: bool>(
    input: *const u8,
    output: *mut u8,
    groups: usize,
    _offsets: __m256i,
) {
    let offsets = standard_offsets_256::<URLSAFE>();

    unsafe { encode_96_shifted_asm_inner(input, output, groups, offsets) };
}

#[cfg(all(feature = "python", target_arch = "x86_64"))]
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_shifted_asm_custom(
    input: *const u8,
    output: *mut u8,
    groups: usize,
    offsets: __m256i,
) {
    unsafe { encode_96_shifted_asm_inner(input, output, groups, offsets) };
}

#[cfg(target_arch = "x86_64")]
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn encode_96_shifted_asm_inner(
    input: *const u8,
    output: *mut u8,
    groups: usize,
    offsets: __m256i,
) {
    let shuffle = _mm256_setr_epi8(
        5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7,
        10, 9, 11, 10,
    );

    let align_multiplier = _mm256_set1_epi32(0x0010_0001);
    let lower_mask = _mm256_set1_epi32(0x003f_03f0);
    let lower_multiplier = _mm256_set1_epi32(0x0100_0010);
    let reduction_base = _mm256_set1_epi8(51);
    let lower_bound = _mm256_set1_epi8(25);

    // Keep the actual branch target aligned, rather than placing an alignment
    // directive in a Rust loop where LLVM's backedge label precedes the NOPs.
    // Seven input constants plus eight early-clobber outputs leave one YMM
    // register free while preventing accidental input/output register aliasing.
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
            "vpmullw {temporary0}, {value0}, {align_multiplier}",
            "vpmullw {temporary1}, {value1}, {align_multiplier}",
            "vpmullw {temporary2}, {value2}, {align_multiplier}",
            "vpmullw {temporary3}, {value3}, {align_multiplier}",
            "vpsrlw {temporary0}, {temporary0}, 10",
            "vpsrlw {temporary1}, {temporary1}, 10",
            "vpsrlw {temporary2}, {temporary2}, 10",
            "vpsrlw {temporary3}, {temporary3}, 10",
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
            align_multiplier = in(ymm_reg) align_multiplier,
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
unsafe fn encode_12_avx2(input: *const u8, offsets: __m128i) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);

    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact_avx2_128(higher, _mm_set1_epi32(0x0400_0040)) };
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = unsafe { mullo_epi16_exact_avx2_128(lower, _mm_set1_epi32(0x0100_0010)) };

    ascii_from_indices_avx2_128(_mm_or_si128(higher, lower), offsets)
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
unsafe fn encode_24_first(input: *const u8, offsets: __m256i) -> __m256i {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let shifted = _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 0, 1, 2, 3, 4, 5, 6));

    encode_24_shifted_value(shifted, offsets)
}

#[target_feature(enable = "avx2")]
unsafe fn encode_24_shifted(input: *const u8, offsets: __m256i) -> __m256i {
    let shifted = unsafe { _mm256_loadu_si256(input.cast()) };

    encode_24_shifted_value(shifted, offsets)
}

#[target_feature(enable = "avx2")]
fn encode_24_shifted_value(shifted: __m256i, offsets: __m256i) -> __m256i {
    // The low lane's payload starts four bytes into the vector.
    // The high lane's payload starts at byte zero. This arrangement lets every block
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

    ascii_from_indices_avx2(_mm256_or_si256(higher, lower), offsets)
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

#[target_feature(enable = "avx2")]
fn ascii_from_indices_avx2_128(indices: __m128i, offsets: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let lower = _mm_cmpgt_epi8(indices, _mm_set1_epi8(25));
    let reduced = _mm_sub_epi8(reduced, lower);
    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "avx2")]
fn standard_offsets_128<const URLSAFE: bool>() -> __m128i {
    _mm_setr_epi8(
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
    )
}

#[target_feature(enable = "avx2")]
fn standard_offsets_256<const URLSAFE: bool>() -> __m256i {
    _mm256_broadcastsi128_si256(standard_offsets_128::<URLSAFE>())
}

#[target_feature(enable = "avx2")]
fn ascii_from_indices_avx2(indices: __m256i, offsets: __m256i) -> __m256i {
    let reduced = _mm256_subs_epu8(indices, _mm256_set1_epi8(51));
    let lower = _mm256_cmpgt_epi8(indices, _mm256_set1_epi8(25));
    let reduced = _mm256_sub_epi8(reduced, lower);

    _mm256_add_epi8(_mm256_shuffle_epi8(offsets, reduced), indices)
}
