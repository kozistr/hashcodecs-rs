//! SSE4.1 decoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::ssse3::pack_16_indices;
use super::x86_contracts::{Decoder, Store};

#[target_feature(enable = "ssse3,sse4.1")]
pub(crate) unsafe fn decode_sse41<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
        let errors = _mm_or_si128(
            _mm_or_si128(first_errors, second_errors),
            _mm_or_si128(third_errors, fourth_errors),
        );
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(first)) };
        unsafe { S::store_12(output.add(destination + 12), pack_16_indices(second)) };
        unsafe { S::store_12(output.add(destination + 24), pack_16_indices(third)) };
        unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}
