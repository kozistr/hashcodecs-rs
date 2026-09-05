//! Runtime dispatch for shared Base64 byte scanning and translation.

use std::sync::OnceLock;

#[cfg(target_arch = "aarch64")]
mod aarch64;
pub(super) mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

pub(super) use scalar::is_lenient_symbol;
use scalar::{alphanumeric_prefix_scalar, symbol_count_scalar};
#[cfg(not(target_arch = "aarch64"))]
use scalar::{symbol_prefix_scalar, translate_scalar};

pub(super) fn lenient_symbol_count(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            return unsafe { x86::symbol_count_avx2(input, altchars) };
        }
        if input.len() >= 16 && std::is_x86_feature_detected!("sse2") {
            return unsafe { x86::symbol_count_sse2(input, altchars) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if input.len() >= 16 {
            return unsafe { aarch64::symbol_count(input, altchars) };
        }
    }

    symbol_count_scalar(input, altchars)
}

pub(super) type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;
pub(super) type SymbolPrefix = unsafe fn(&[u8], Option<[u8; 2]>) -> usize;

#[derive(Clone, Copy)]
pub(super) struct DecodeByteKernels {
    pub(super) scanner: AlphanumericPrefix,
    pub(super) symbol_prefix: SymbolPrefix,
    pub(super) translate: TranslateBytes,
}

static DECODE_BYTE_KERNELS: OnceLock<DecodeByteKernels> = OnceLock::new();

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_alphanumeric_prefix_for_x86(avx2: bool, sse2: bool) -> AlphanumericPrefix {
    if avx2 {
        return x86::alphanumeric_prefix_avx2;
    }
    if sse2 {
        return x86::alphanumeric_prefix_sse2;
    }
    alphanumeric_prefix_scalar
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_symbol_prefix_for_x86(avx2: bool, sse2: bool) -> SymbolPrefix {
    if avx2 {
        return x86::symbol_prefix_avx2;
    }
    if sse2 {
        return x86::symbol_prefix_sse2;
    }
    symbol_prefix_scalar
}

fn select_alphanumeric_prefix() -> AlphanumericPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_alphanumeric_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    alphanumeric_prefix_scalar
}

fn select_symbol_prefix() -> SymbolPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_symbol_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(target_arch = "aarch64")]
    return aarch64::symbol_prefix;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    symbol_prefix_scalar
}

pub(super) type TranslateBytes = unsafe fn(&mut [u8], u8, u8, u8, u8);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_translate_bytes_for_x86(avx2: bool, sse2: bool) -> TranslateBytes {
    if avx2 {
        return x86::translate_avx2;
    }
    if sse2 {
        return x86::translate_sse2;
    }
    translate_scalar
}

fn select_translate_bytes() -> TranslateBytes {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_translate_bytes_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(target_arch = "aarch64")]
    return aarch64::translate;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    translate_scalar
}

pub(super) fn decode_byte_kernels() -> &'static DecodeByteKernels {
    DECODE_BYTE_KERNELS.get_or_init(|| DecodeByteKernels {
        scanner: select_alphanumeric_prefix(),
        symbol_prefix: select_symbol_prefix(),
        translate: select_translate_bytes(),
    })
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;

    #[test]
    fn x86_backend_selectors_cover_each_dispatch_tier() {
        for (avx2, sse2, expected) in [
            (
                true,
                true,
                super::x86::alphanumeric_prefix_avx2 as AlphanumericPrefix,
            ),
            (
                false,
                true,
                super::x86::alphanumeric_prefix_sse2 as AlphanumericPrefix,
            ),
            (
                false,
                false,
                alphanumeric_prefix_scalar as AlphanumericPrefix,
            ),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_alphanumeric_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (true, true, super::x86::symbol_prefix_avx2 as SymbolPrefix),
            (false, true, super::x86::symbol_prefix_sse2 as SymbolPrefix),
            (false, false, symbol_prefix_scalar as SymbolPrefix),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_symbol_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (true, true, super::x86::translate_avx2 as TranslateBytes),
            (false, true, super::x86::translate_sse2 as TranslateBytes),
            (false, false, translate_scalar as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }
}

pub(super) fn translate_bytes(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let kernels = decode_byte_kernels();
    unsafe { (kernels.translate)(input, source0, target0, source1, target1) };
}
