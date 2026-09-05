use super::{LongInput, Secret, build_long_input_schedule, initial_accumulator};
use crate::xxhash::primitives::P32_1;

#[inline(always)]
unsafe fn read_u64_le(ptr: *const u8, offset: usize) -> u64 {
    u64::from_le(unsafe { ptr.add(offset).cast::<u64>().read_unaligned() })
}

#[inline(always)]
unsafe fn accumulate_stripe(
    acc: &mut [u64; 8],
    data: &[u8],
    secret: &Secret,
    data_offset: usize,
    secret_offset: usize,
) {
    debug_assert!(data_offset <= data.len() - 64);
    debug_assert!(secret_offset <= secret.as_bytes().len() - 64);

    let data = data.as_ptr();
    let secret = secret.as_bytes().as_ptr();

    for lane in 0..8 {
        // `LongSchedule` keeps each data stripe in bounds, and every secret
        // offset passed by `accumulate` leaves a complete 64-byte stripe.
        let value = unsafe { read_u64_le(data, data_offset + lane * 8) };
        let mixed = value ^ unsafe { read_u64_le(secret, secret_offset + lane * 8) };
        acc[lane ^ 1] = acc[lane ^ 1].wrapping_add(value);
        acc[lane] = acc[lane].wrapping_add((mixed as u32 as u64).wrapping_mul(mixed >> 32));
    }
}

#[inline(always)]
fn scramble(acc: &mut [u64; 8], secret: &Secret) {
    let secret = secret.as_bytes().as_ptr();

    for (lane, value) in acc.iter_mut().enumerate() {
        *value ^= *value >> 47;
        // A `Secret` is 192 bytes, so all eight fixed-offset loads are valid.
        *value ^= unsafe { read_u64_le(secret, 128 + lane * 8) };
        *value = value.wrapping_mul(P32_1);
    }
}

pub(super) fn accumulate(input: LongInput<'_>, secret: &Secret) -> [u64; 8] {
    let data = input.as_bytes();
    let schedule = build_long_input_schedule(input);
    let mut acc = initial_accumulator();

    for block in 0..schedule.full_blocks() {
        let offset = block * 1024;

        for stripe in 0..16 {
            unsafe { accumulate_stripe(&mut acc, data, secret, offset + stripe * 64, stripe * 8) };
        }

        scramble(&mut acc, secret);
    }

    for stripe in 0..schedule.tail_stripes() {
        unsafe {
            accumulate_stripe(
                &mut acc,
                data,
                secret,
                schedule.tail_offset() + stripe * 64,
                stripe * 8,
            )
        };
    }

    unsafe { accumulate_stripe(&mut acc, data, secret, schedule.last_offset(), 121) };

    acc
}
