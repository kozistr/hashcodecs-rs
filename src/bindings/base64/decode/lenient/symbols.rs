#[inline(always)]
pub(in crate::bindings::base64::decode) fn is_lenient_symbol(
    byte: u8,
    altchars: Option<[u8; 2]>,
) -> bool {
    byte.wrapping_sub(b'A') <= b'Z' - b'A'
        || byte.wrapping_sub(b'a') <= b'z' - b'a'
        || byte.wrapping_sub(b'0') <= b'9' - b'0'
        || matches!(byte, b'+' | b'/')
        || altchars.is_some_and(|[plus, slash]| byte == plus || byte == slash)
}

fn symbol_count_scalar(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    input
        .iter()
        .filter(|&&byte| is_lenient_symbol(byte, altchars))
        .count()
}

pub(in crate::bindings::base64::decode) unsafe fn alphanumeric_prefix_scalar(
    input: &[u8],
) -> usize {
    input
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(input.len())
}

pub(in crate::bindings::base64::decode) unsafe fn symbol_prefix_scalar(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
    input
        .iter()
        .position(|&byte| !is_lenient_symbol(byte, altchars))
        .unwrap_or(input.len())
}

#[cfg(test)]
pub(in crate::bindings::base64::decode) unsafe fn translate_bytes_scalar(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { translate_scalar(input, source0, target0, source1, target1) };
}

pub(in crate::bindings::base64::decode) unsafe fn translate_scalar(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    for byte in input {
        if *byte == source0 {
            *byte = target0;
        } else if *byte == source1 {
            *byte = target1;
        }
    }
}

pub(in crate::bindings::base64::decode) fn lenient_symbol_count(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            return unsafe { super::symbols_x86::symbol_count_avx2(input, altchars) };
        }
        if input.len() >= 16 && std::is_x86_feature_detected!("sse2") {
            return unsafe { super::symbols_x86::symbol_count_sse2(input, altchars) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if input.len() >= 16 {
            return unsafe { super::symbols_aarch64::symbol_count(input, altchars) };
        }
    }

    symbol_count_scalar(input, altchars)
}

pub(in crate::bindings::base64::decode) type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;
pub(in crate::bindings::base64::decode) type SymbolPrefix =
    unsafe fn(&[u8], Option<[u8; 2]>) -> usize;

#[derive(Clone, Copy)]
pub(in crate::bindings::base64::decode) struct DecodeByteKernels {
    pub(in crate::bindings::base64::decode) scanner: AlphanumericPrefix,
    pub(in crate::bindings::base64::decode) symbol_prefix: SymbolPrefix,
    pub(in crate::bindings::base64::decode) translate: TranslateBytes,
}

static DECODE_BYTE_KERNELS: OnceLock<DecodeByteKernels> = OnceLock::new();

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_alphanumeric_prefix_for_x86(avx2: bool, sse2: bool) -> AlphanumericPrefix {
    if avx2 {
        return super::symbols_x86::alphanumeric_prefix_avx2;
    }
    if sse2 {
        return super::symbols_x86::alphanumeric_prefix_sse2;
    }
    alphanumeric_prefix_scalar
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_symbol_prefix_for_x86(avx2: bool, sse2: bool) -> SymbolPrefix {
    if avx2 {
        return super::symbols_x86::symbol_prefix_avx2;
    }
    if sse2 {
        return super::symbols_x86::symbol_prefix_sse2;
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
    return super::symbols_aarch64::symbol_prefix;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    symbol_prefix_scalar
}

pub(in crate::bindings::base64::decode) type TranslateBytes = unsafe fn(&mut [u8], u8, u8, u8, u8);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_translate_bytes_for_x86(avx2: bool, sse2: bool) -> TranslateBytes {
    if avx2 {
        return super::symbols_x86::translate_avx2;
    }
    if sse2 {
        return super::symbols_x86::translate_sse2;
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
    return super::symbols_aarch64::translate;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    translate_scalar
}

pub(in crate::bindings::base64::decode) fn decode_byte_kernels() -> &'static DecodeByteKernels {
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
                super::super::symbols_x86::alphanumeric_prefix_avx2 as AlphanumericPrefix,
            ),
            (
                false,
                true,
                super::super::symbols_x86::alphanumeric_prefix_sse2 as AlphanumericPrefix,
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
            (
                true,
                true,
                super::super::symbols_x86::symbol_prefix_avx2 as SymbolPrefix,
            ),
            (
                false,
                true,
                super::super::symbols_x86::symbol_prefix_sse2 as SymbolPrefix,
            ),
            (false, false, symbol_prefix_scalar as SymbolPrefix),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_symbol_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (
                true,
                true,
                super::super::symbols_x86::translate_avx2 as TranslateBytes,
            ),
            (
                false,
                true,
                super::super::symbols_x86::translate_sse2 as TranslateBytes,
            ),
            (false, false, translate_scalar as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }
}
use std::sync::OnceLock;
