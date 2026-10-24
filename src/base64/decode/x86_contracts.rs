//! Contracts shared by the x86 decoding kernels.

#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

use super::super::{MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE};

#[cfg(any(feature = "python", test))]
use super::avx2::decode_indices_32_standard_validated;
use super::avx2::{
    decode_indices_32_mixed, decode_indices_32_standard, decode_indices_32_urlsafe, store_24_exact,
    store_24_padded,
};
#[cfg(any(feature = "python", test))]
use super::ssse3::decode_indices_16_standard_validated;
use super::ssse3::{
    decode_indices_16_mixed, decode_indices_16_standard, decode_indices_16_urlsafe, store_12_exact,
    store_12_padded,
};

pub(crate) struct StandardDecoder;
#[cfg(any(feature = "python", test))]
pub(crate) struct ValidatedStandardDecoder;
pub(crate) struct UrlSafeDecoder;
pub(crate) struct MixedDecoder;
pub(crate) struct ExactStore;
pub(crate) struct PaddedStore;

pub(crate) trait Decoder {
    const CHECK_INPUT: bool = true;

    #[inline(always)]
    fn rejects_input(has_errors: bool) -> bool {
        Self::CHECK_INPUT && has_errors
    }

    fn decode_table() -> &'static [u8; 256];

    /// # Safety
    /// AVX2 must be available and input must cover 32 readable bytes.
    /// Decoders with CHECK_INPUT disabled require previously validated symbols.
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i);

    /// # Safety
    /// SSSE3 must be available and input must cover 16 readable bytes.
    /// Decoders with CHECK_INPUT disabled require previously validated symbols.
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i);
}

pub(crate) trait Store {
    /// # Safety
    /// SSSE3 must be available. ExactStore requires 12 writable bytes;
    /// PaddedStore requires 16 writable bytes, including its overlap.
    unsafe fn store_12(output: *mut u8, value: __m128i);

    /// # Safety
    /// AVX2 must be available. ExactStore requires 24 writable bytes;
    /// PaddedStore requires 28 writable bytes, including its overlap.
    unsafe fn store_24(output: *mut u8, value: __m256i);
}

impl Store for ExactStore {
    /// # Safety
    /// The kernel CPU features must be available and output must cover every decoded lane written
    /// by this helper.
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        // SAFETY: The Store contract reserves the full exact or padded store width; the calling
        // kernel has enabled the required instruction set.
        unsafe { store_12_exact(output, value) };
    }

    /// # Safety
    /// The kernel CPU features must be available and output must cover every decoded lane written
    /// by this helper.
    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        // SAFETY: The Store contract reserves the full exact or padded store width; the calling
        // kernel has enabled the required instruction set.
        unsafe { store_24_exact(output, value) };
    }
}

impl Store for PaddedStore {
    /// # Safety
    /// The kernel CPU features must be available and output must cover every decoded lane written
    /// by this helper.
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        // SAFETY: The Store contract reserves the full exact or padded store width; the calling
        // kernel has enabled the required instruction set.
        unsafe { store_12_padded(output, value) };
    }

    /// # Safety
    /// The kernel CPU features must be available and output must cover every decoded lane written
    /// by this helper.
    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        // SAFETY: The Store contract reserves the full exact or padded store width; the calling
        // kernel has enabled the required instruction set.
        unsafe { store_24_padded(output, value) };
    }
}

impl Decoder for StandardDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &STANDARD_DECODE
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 32 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_32_standard(input) }
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 16 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_16_standard(input) }
    }
}

#[cfg(any(feature = "python", test))]
impl Decoder for ValidatedStandardDecoder {
    const CHECK_INPUT: bool = false;

    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &STANDARD_DECODE
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 32 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_32_standard_validated(input) }
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 16 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_16_standard_validated(input) }
    }
}

impl Decoder for UrlSafeDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &URLSAFE_DECODE
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 32 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_32_urlsafe(input) }
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 16 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_16_urlsafe(input) }
    }
}

impl Decoder for MixedDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &MIXED_DECODE
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 32 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_32_mixed(input) }
    }

    /// # Safety
    /// The caller must enable the kernel CPU features and provide 16 readable input bytes.
    /// Validated variants require previously checked alphabet symbols.
    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        // SAFETY: The Decoder contract supplies a complete vector of readable input and the calling
        // kernel has enabled the required instruction set.
        unsafe { decode_indices_16_mixed(input) }
    }
}
