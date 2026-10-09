//! SSE4.1 decoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::sse_kernels::decode_kernels;
use super::ssse3::{pack_16_indices, store_12_exact, store_12_padded};
use super::x86_contracts::{Decoder, Store};

decode_kernels!(
    "ssse3,sse4.1",
    errors => _mm_testz_si128(errors, errors) == 0
);
