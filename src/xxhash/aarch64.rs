use std::arch::aarch64::*;

use super::{P32_1, initial_accumulator, long_schedule};

#[inline(always)]
unsafe fn accumulate_neon(acc: &mut [u64; 8], data: *const u8, secret: *const u8) {
    for vector in 0..4 {
        let byte_offset = vector * 16;
        let acc_offset = vector * 2;

        unsafe {
            let input = vreinterpretq_u64_u8(vld1q_u8(data.add(byte_offset)));
            let key = vreinterpretq_u64_u8(vld1q_u8(secret.add(byte_offset)));

            let keyed = veorq_u64(input, key);
            let product = vmull_u32(vmovn_u64(keyed), vshrn_n_u64::<32>(keyed));
            let swapped = vextq_u64::<1>(input, input);

            let acc_offset_ptr = acc.as_mut_ptr().add(acc_offset);
            let old = vld1q_u64(acc_offset_ptr);

            vst1q_u64(acc_offset_ptr, vaddq_u64(vaddq_u64(old, swapped), product))
        }
    }
}

#[inline(always)]
unsafe fn scramble_neon(acc: &mut [u64; 8], secret: *const u8) {
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
/// The caller must have detected NEON support. `data` must be in XXH3 long
/// mode and `secret` must contain at least 192 bytes.
pub(super) unsafe fn long_accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    let schedule = long_schedule(data.len());
    let mut acc = initial_accumulator();

    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        for stripe in 0..16 {
            unsafe {
                accumulate_neon(
                    &mut acc,
                    data.as_ptr().add(offset + stripe * 64),
                    secret.as_ptr().add(stripe * 8),
                )
            };
        }
        unsafe { scramble_neon(&mut acc, secret.as_ptr().add(128)) };
    }

    for stripe in 0..schedule.tail_stripes {
        unsafe {
            accumulate_neon(
                &mut acc,
                data.as_ptr().add(schedule.tail_offset + stripe * 64),
                secret.as_ptr().add(stripe * 8),
            )
        };
    }

    unsafe {
        accumulate_neon(
            &mut acc,
            data.as_ptr().add(schedule.last_offset),
            secret.as_ptr().add(121),
        )
    };

    acc
}
