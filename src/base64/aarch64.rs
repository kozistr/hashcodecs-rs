//! AArch64 Base64 backend facade.

use super::Base64Error;

mod decode;
mod encode;
#[cfg(test)]
mod tests;

pub(super) use decode::{DECODE_ERROR_CHECK_INTERVAL, decode_neon, decode_neon_transactional};
pub(super) use encode::encode_neon;
