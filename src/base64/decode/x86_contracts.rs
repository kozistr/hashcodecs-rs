//! Contracts shared by the x86 decoding kernels.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::{MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE};

use super::avx2::{
    decode_indices_32_mixed, decode_indices_32_standard, decode_indices_32_urlsafe, store_24_exact,
    store_24_padded,
};
use super::ssse3::{
    decode_indices_16_mixed, decode_indices_16_standard, decode_indices_16_urlsafe, store_12_exact,
    store_12_padded,
};

pub(crate) struct StandardDecoder;
pub(crate) struct UrlSafeDecoder;
pub(crate) struct MixedDecoder;
pub(crate) struct ExactStore;
pub(crate) struct PaddedStore;

pub(crate) trait Decoder {
    fn decode_table() -> &'static [u8; 256];
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i);
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i);
}

pub(crate) trait Store {
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
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &STANDARD_DECODE
    }

    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_standard(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_standard(input) }
    }
}

impl Decoder for UrlSafeDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &URLSAFE_DECODE
    }

    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_urlsafe(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_urlsafe(input) }
    }
}

impl Decoder for MixedDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &MIXED_DECODE
    }

    #[inline(always)]
    unsafe fn decode_indices_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_mixed(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_mixed(input) }
    }
}
