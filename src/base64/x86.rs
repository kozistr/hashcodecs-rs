//! x86 Base64 backend facade.

mod decode;
mod encode;

pub(super) use decode::{
    Decoder, ExactStore, MixedDecoder, PaddedStore, StandardDecoder, Store, UrlSafeDecoder,
    decode_avx2, decode_sse41, decode_ssse3,
};
pub(super) use encode::{encode_avx2, encode_ssse3};
