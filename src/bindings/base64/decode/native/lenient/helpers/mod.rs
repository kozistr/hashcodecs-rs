#[cfg(target_arch = "aarch64")]
mod aarch64;
mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(in crate::bindings::base64::decode::native) mod x86;

#[cfg(test)]
pub(in crate::bindings::base64::decode::native) use scalar::translate as translate_bytes_scalar;
pub(in crate::bindings::base64::decode::native) use scalar::{
    alphanumeric_prefix as alphanumeric_prefix_scalar, is_lenient_symbol,
};

pub(in crate::bindings::base64::decode::native) fn lenient_symbol_count(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
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

pub(in crate::bindings::base64::decode::native) type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;

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

pub(in crate::bindings::base64::decode::native) fn select_alphanumeric_prefix() -> AlphanumericPrefix
{
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_alphanumeric_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    alphanumeric_prefix_scalar
}

pub(in crate::bindings::base64::decode::native) type TranslateBytes =
    unsafe fn(&mut [u8], u8, u8, u8, u8);

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

pub(in crate::bindings::base64::decode::native) fn select_translate_bytes() -> TranslateBytes {
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

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;

    #[test]
    fn x86_backend_selectors_cover_each_dispatch_tier() {
        for (avx2, sse2, expected) in [
            (
                true,
                true,
                x86::alphanumeric_prefix_avx2 as AlphanumericPrefix,
            ),
            (
                false,
                true,
                x86::alphanumeric_prefix_sse2 as AlphanumericPrefix,
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
            (true, true, x86::translate_avx2 as TranslateBytes),
            (false, true, x86::translate_sse2 as TranslateBytes),
            (false, false, scalar::translate as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }
}
