//! x86 Base64 backend facade.

pub(super) mod avx512;
#[cfg(not(any(kani, miri)))]
mod cache;
mod decode;
mod encode;

pub(super) use decode::{
    Decoder, ExactStore, MixedDecoder, PaddedStore, StandardDecoder, Store, UrlSafeDecoder,
    decode_avx2, decode_sse41, decode_ssse3,
};
pub(super) use encode::{Avx2StoreMode, encode_avx2, encode_avx2_with_store, encode_ssse3};

#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
pub(in crate::base64) use cache::{cached_input_limit, use_streaming_stores};
