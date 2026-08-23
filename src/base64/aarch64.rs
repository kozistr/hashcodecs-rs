//! AArch64 Base64 backend facade.

#[cfg(test)]
use super::Base64Error;

mod decode;
mod encode;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use decode::DECODE_ERROR_CHECK_INTERVAL;
pub(super) use decode::{decode_neon, decode_neon_transactional};
pub(super) use encode::encode_neon;
