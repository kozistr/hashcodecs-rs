//! Runtime dispatch for shared Base64 byte scanning and translation.

use std::sync::OnceLock;

#[cfg(target_arch = "aarch64")]
mod aarch64;
pub(super) mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

pub(super) use scalar::is_lenient_symbol;

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

    scalar::symbol_count(input, altchars)
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

    scalar::alphanumeric_prefix
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_symbol_prefix_for_x86(avx2: bool, sse2: bool) -> SymbolPrefix {
    if avx2 {
        return x86::symbol_prefix_avx2;
    }

    if sse2 {
        return x86::symbol_prefix_sse2;
    }

    scalar::symbol_prefix
}

fn select_alphanumeric_prefix() -> AlphanumericPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_alphanumeric_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    scalar::alphanumeric_prefix
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
    scalar::symbol_prefix
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

    scalar::translate
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
    scalar::translate
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
    fn select_x86_kernels() {
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
                scalar::alphanumeric_prefix as AlphanumericPrefix,
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
            (false, false, scalar::symbol_prefix as SymbolPrefix),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_symbol_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (true, true, super::x86::translate_avx2 as TranslateBytes),
            (false, true, super::x86::translate_sse2 as TranslateBytes),
            (false, false, scalar::translate as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }
}
