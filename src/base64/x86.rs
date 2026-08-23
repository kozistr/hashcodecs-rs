//! x86 Base64 backend facade.

#[cfg(not(any(kani, miri)))]
mod cache;
mod decode;
mod encode;

pub(super) use decode::{
    Decoder, ExactStore, MixedDecoder, PaddedStore, StandardDecoder, Store, UrlSafeDecoder,
    decode_avx2, decode_avx512, decode_sse41, decode_ssse3,
};
pub(super) use encode::{
    Avx2StoreMode, encode_avx2, encode_avx2_with_store, encode_avx512, encode_ssse3,
};

#[cfg(test)]
pub(in crate::base64) use decode::AVX512_DECODE_SHUFFLE;
#[cfg(test)]
pub(in crate::base64) use encode::{AVX512_ENCODE_SHUFFLE, AVX512_MULTISHIFT_SHIFTS};

#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
pub(in crate::base64) use cache::{cached_input_limit, use_streaming_stores};

#[cfg(all(target_arch = "x86_64", any(kani, miri)))]
#[inline]
pub(in crate::base64) const fn cached_input_limit() -> Option<usize> {
    None
}

#[cfg(all(target_arch = "x86_64", any(kani, miri)))]
#[inline]
pub(in crate::base64) const fn use_streaming_stores(
    _cached_input_limit: Option<usize>,
    _input_len: usize,
    _output: *mut u8,
) -> bool {
    false
}
