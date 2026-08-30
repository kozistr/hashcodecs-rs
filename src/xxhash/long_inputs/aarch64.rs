//! AArch64 long-input accumulation kernel.

use std::arch::aarch64::*;

use super::super::primitives::P32_1;
use super::{LongInput, Secret, build_long_input_schedule, initial_accumulator};

#[inline(always)]
unsafe fn compiler_guard(mut value: uint64x2_t) -> uint64x2_t {
    unsafe {
        std::arch::asm!(
            "/* {value:v} */",
            value = inout(vreg) value,
            options(nomem, nostack, preserves_flags),
        )
    };
    value
}

#[inline(always)]
unsafe fn accumulate_stripe(acc: &mut [u64; 8], data: *const u8, secret: *const u8) {
    for vector in (0..4).step_by(2) {
        let byte_offset = vector * 16;
        let next_byte_offset = byte_offset + 16;
        let acc_offset = vector * 2;

        unsafe {
            let input_lo = vreinterpretq_u64_u8(vld1q_u8(data.add(byte_offset)));
            let input_hi = vreinterpretq_u64_u8(vld1q_u8(data.add(next_byte_offset)));
            let key_lo = vreinterpretq_u64_u8(vld1q_u8(secret.add(byte_offset)));
            let key_hi = vreinterpretq_u64_u8(vld1q_u8(secret.add(next_byte_offset)));

            let keyed_lo = veorq_u64(input_lo, key_lo);
            let keyed_hi = veorq_u64(input_hi, key_hi);
            let unzipped = vuzpq_u32(
                vreinterpretq_u32_u64(keyed_lo),
                vreinterpretq_u32_u64(keyed_hi),
            );
            let low_words = unzipped.0;
            let high_words = unzipped.1;

            let swapped_lo = vextq_u64::<1>(input_lo, input_lo);
            let swapped_hi = vextq_u64::<1>(input_hi, input_hi);
            // Keep LLVM from lengthening the multiply dependency chain by
            // folding the swapped-data addition into the accumulator first.
            let sum_lo = compiler_guard(vmlal_u32(
                swapped_lo,
                vget_low_u32(low_words),
                vget_low_u32(high_words),
            ));
            let sum_hi = compiler_guard(vmlal_high_u32(swapped_hi, low_words, high_words));

            let acc_ptr = acc.as_mut_ptr().add(acc_offset);
            vst1q_u64(acc_ptr, vaddq_u64(vld1q_u64(acc_ptr), sum_lo));
            vst1q_u64(acc_ptr.add(2), vaddq_u64(vld1q_u64(acc_ptr.add(2)), sum_hi));
        }
    }
}

#[inline(always)]
unsafe fn scramble(acc: &mut [u64; 8], secret: *const u8) {
    let prime = unsafe { vdup_n_u32(P32_1 as u32) };

    for vector in 0..4 {
        let byte_offset = vector * 16;
        let acc_offset = vector * 2;

        unsafe {
            let acc_offset_ptr = acc.as_mut_ptr().add(acc_offset);

            let value = vld1q_u64(acc_offset_ptr);
            let key = vreinterpretq_u64_u8(vld1q_u8(secret.add(byte_offset)));
            let mixed = veorq_u64(veorq_u64(value, vshrq_n_u64::<47>(value)), key);

            let low_product = vmull_u32(vmovn_u64(mixed), prime);
            let high_product = vshlq_n_u64::<32>(vmull_u32(vshrn_n_u64::<32>(mixed), prime));

            vst1q_u64(acc_offset_ptr, vaddq_u64(low_product, high_product))
        }
    }
}

#[target_feature(enable = "neon")]
/// # Safety
/// The caller must have detected NEON support. This kernel is only compiled
/// for little-endian AArch64, matching XXH3's lane byte order.
pub(super) unsafe fn accumulate(input: LongInput<'_>, secret: &Secret) -> [u64; 8] {
    let data = input.as_bytes();
    let secret = secret.as_bytes();
    let schedule = build_long_input_schedule(input);
    let mut acc = initial_accumulator();

    for block in 0..schedule.full_blocks() {
        let offset = block * 1024;
        for stripe in 0..16 {
            unsafe {
                accumulate_stripe(
                    &mut acc,
                    data.as_ptr().add(offset + stripe * 64),
                    secret.as_ptr().add(stripe * 8),
                )
            };
        }
        unsafe { scramble(&mut acc, secret.as_ptr().add(128)) };
    }

    for stripe in 0..schedule.tail_stripes() {
        unsafe {
            accumulate_stripe(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset() + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }

    unsafe {
        accumulate_stripe(
            &mut acc,
            data.as_ptr().add(schedule.last_offset()),
            secret.as_ptr().add(121),
        )
    };

    acc
}
